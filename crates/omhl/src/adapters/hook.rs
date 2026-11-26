use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::app::action::{Action, Caller};
use crate::app::error::AppError;
use crate::app::input::{absolute_path, read_json_input};
use crate::app::runtime::Runtime;
use crate::app::{
    AgentRoleEnv, EditInput, Replacement, RoleVerdict, SpecLockEvent, Tool, Verdict,
    agent_role as check_agent_role, spec_lock as check_spec_lock,
};
use crate::app::{ExtensionKind, Group, INVALID_WORKFLOW_CONFIG};
use crate::project::Env;

pub fn session_start(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
    env: &Env<'_>,
) -> ExitCode {
    match context_for_session(stdin, env) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some((context, git_notice))) => {
            let mut output = json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": context,
                }
            });
            // O `additionalContext` não aparece no chat, e quem corrige o git é a pessoa.
            if let Some(notice) = git_notice {
                output["systemMessage"] = json!(notice);
            }
            let _ = writeln!(out, "{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let _ = writeln!(err, "{}", error.to_json());
            ExitCode::from(1)
        }
    }
}

/// Só a recusa de `--format agent|human` sai com 2, e vem antes de o hook rodar; a negação vai no JSON, e um erro interno antes de reconhecer a spec libera a escrita, com a linha de erro no stderr; depois do reconhecimento, a falha de git ou de leitura nega com `spec-lock-unavailable` (o `|| exit 2` do `spec-lock` e o `|| exit 1` do `agent-role` no `omhl-hook.sh` valem só para o `omhl` presente e falho; sem o binário, o script bloqueia só num projeto marcado, e o `agent-role` e o `session-start` saem com `exit 0`). Além das specs, nega a mudança do `specs.dir` no `workflow.toml` principal com spec congelada (`spec-dir-frozen`), qualquer edição sob um `.git` de projeto omhl (`git-dir-protected`) e a edição que deixaria o marcador do projeto inválido (`marker-protected`).
pub fn spec_lock(stdin: &mut impl Read, out: &mut impl Write, err: &mut impl Write) -> ExitCode {
    let mut notices = Vec::new();
    let code = pre_tool_use(stdin, out, err, |input| {
        let outcome = check_spec_lock(spec_lock_event(input)?)?;
        notices = outcome.notices;

        Ok(match outcome.verdict {
            Verdict::Allow => None,
            Verdict::Deny { reason, .. } => {
                Some(json!({ "permissionDecision": "deny", "permissionDecisionReason": reason }))
            }
        })
    });
    // Os avisos acompanham a decisão: a negação vai no stdout e uma linha por aviso, no stderr.
    for notice in notices {
        let _ = writeln!(err, "{}", notice.to_json());
    }

    code
}

/// Ferramenta que não edita sai antes de ler o `file_path`: o evento dela não precisa dele.
fn spec_lock_event(input: &Value) -> Result<SpecLockEvent, AppError> {
    let Some(tool) = input["tool_name"].as_str().and_then(Tool::parse) else {
        return Ok(SpecLockEvent::NotAnEdit);
    };

    Ok(SpecLockEvent::Edit {
        tool,
        file_path: absolute_path(&input["tool_input"]["file_path"])?,
        input: edit_input(tool, &input["tool_input"]),
    })
}

/// O que a ferramenta vai gravar; campo ausente ou com tipo errado vira `Unreadable`, e quem decide vê o que fazer.
fn edit_input(tool: Tool, tool_input: &Value) -> EditInput {
    match tool {
        Tool::Write => tool_input["content"]
            .as_str()
            .map(|content| EditInput::Write {
                content: content.to_owned(),
            }),
        Tool::Edit => replacement(tool_input).map(EditInput::Edit),
        Tool::MultiEdit => tool_input["edits"]
            .as_array()
            .and_then(|edits| edits.iter().map(replacement).collect())
            .map(EditInput::MultiEdit),
    }
    .unwrap_or(EditInput::Unreadable)
}

fn replacement(value: &Value) -> Option<Replacement> {
    let replace_all = match &value["replace_all"] {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        _ => return None,
    };

    Some(Replacement {
        old: value["old_string"].as_str()?.to_owned(),
        new: value["new_string"].as_str()?.to_owned(),
        replace_all,
    })
}

/// Mesmo contrato do `spec_lock`; o Bash, o Monitor e a tool Agent. O que passa segue o fluxo normal de permissão; só o comando dentro da matriz recebe `allow`.
pub fn agent_role(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
    env: &Env<'_>,
    groups: &[Group],
) -> ExitCode {
    let role_env = AgentRoleEnv {
        plugin_root: env.plugin_root.map(Path::new),
        groups,
    };

    pre_tool_use(stdin, out, err, |input| {
        Ok(match check_agent_role(input, &role_env) {
            RoleVerdict::Pass => None,
            RoleVerdict::Deny { reason, .. } => {
                Some(json!({ "permissionDecision": "deny", "permissionDecisionReason": reason }))
            }
            RoleVerdict::Allow {
                reason,
                updated_input,
            } => Some(json!({
                "permissionDecision": "allow",
                "permissionDecisionReason": reason,
                "updatedInput": updated_input,
            })),
        })
    })
}

fn pre_tool_use(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
    decide: impl FnOnce(&Value) -> Result<Option<Value>, AppError>,
) -> ExitCode {
    match read_json_input(stdin).and_then(|input| decide(&input)) {
        Ok(None) => {}
        Ok(Some(mut decision)) => {
            decision["hookEventName"] = json!("PreToolUse");
            let _ = writeln!(out, "{}", json!({ "hookSpecificOutput": decision }));
        }
        Err(error) => {
            let _ = writeln!(err, "{}", error.to_json());
        }
    }

    ExitCode::SUCCESS
}

/// O contexto e, à parte, o aviso do `git`, que também já está dentro do contexto.
fn context_for_session(
    stdin: &mut impl Read,
    env: &Env<'_>,
) -> Result<Option<(String, Option<String>)>, AppError> {
    let cwd = absolute_path(&read_json_input(stdin)?["cwd"])?;
    let runtime = match Runtime::discover(&cwd, None, env) {
        Ok(runtime) => runtime,
        Err(error) if error.code() == "no-marker" => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut app = runtime.app()?;
    let caller = Caller::hook();
    let mut rendered = app.dispatch(&caller, &Action::Render)?;
    let Value::String(mut text) = rendered["text"].take() else {
        return Err(AppError::render_failed());
    };
    // A dry run never writes: the hook only tells what `omhl load` would do.
    let checked = app.dispatch(&caller, &Action::Load { dry_run: true });
    if let Some(line) = extension_notice(&checked) {
        text.push_str(&format!("\n{line}\n"));
    }
    let git_notice = runtime.git_notice();
    if let Some(line) = &git_notice {
        text.push_str(&format!("\n{line}\n"));
    }

    Ok(Some((text, git_notice)))
}

fn extension_notice(checked: &Result<Value, AppError>) -> Option<String> {
    match checked {
        Ok(report) => {
            let notes: Vec<String> = [
                pending_note(report),
                orphan_note(report),
                workflow_note(report),
            ]
            .into_iter()
            .flatten()
            .collect();

            (!notes.is_empty()).then(|| notes.join("\n"))
        }
        Err(error) => Some(warning(error)),
    }
}

fn pending_note(report: &Value) -> Option<String> {
    let pending: usize = ["created", "revised", "relations_created"]
        .iter()
        .map(|key| report[key].as_array().map_or(0, Vec::len))
        .sum();

    (pending > 0).then(|| {
        let tenant = report["tenant"].as_str().unwrap_or_default();

        format!(
            "NOTE: {pending} extension change(s) not loaded into tenant {tenant}; run omhl load"
        )
    })
}

/// The scan falls back to the defaults on an invalid workflow file; the agent still has to hear it.
fn workflow_note(report: &Value) -> Option<String> {
    report["warnings"]
        .as_array()?
        .iter()
        .any(|issue| issue["code"] == INVALID_WORKFLOW_CONFIG)
        .then(|| {
            "WARNING: .omhl/workflow.toml is invalid and the defaults apply (invalid-workflow-config); run omhl load --dry-run for details".to_owned()
        })
}

/// Only the checks: the load report also lists guideline orphans, which `omhl check retire` does not cover.
fn orphan_note(report: &Value) -> Option<String> {
    let orphans: Vec<&str> = report["orphans"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| id.starts_with(ExtensionKind::Check.id_prefix()))
        .collect();

    (!orphans.is_empty()).then(|| {
        format!(
            "NOTE: {} check(s) have no file in this working tree: {}; if this branch or worktree predates the check, rebase it onto main (or restore the file); omhl check retire <id> is permanent for every clone and branch (the id never comes back), so run it only after the file removal is merged",
            orphans.len(),
            orphans.join(", ")
        )
    })
}

/// Only codes and counts: file names and contents never reach the agent context.
fn warning(error: &AppError) -> String {
    let reasons = match error.code() {
        "extensions-invalid" => issue_counts(error),
        code => code.to_owned(),
    };

    format!("WARNING: extensions not checked ({reasons}); run omhl load --dry-run for details")
}

fn issue_counts(error: &AppError) -> String {
    let mut counts = BTreeMap::new();
    for code in error.detail_codes() {
        *counts.entry(code).or_insert(0_usize) += 1;
    }

    counts
        .iter()
        .map(|(code, count)| format!("{code} x{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}
