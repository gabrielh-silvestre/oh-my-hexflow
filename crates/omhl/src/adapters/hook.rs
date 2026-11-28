use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::app::ProcessRunner;
use crate::app::action::{Action, Caller};
use crate::app::error::AppError;
use crate::app::input::{absolute_path, read_json_input};
use crate::app::runtime::Runtime;
use crate::app::{
    AgentRoleEnv, EditInput, LanguageRungs, Replacement, RoleVerdict, SpecLockEvent, Tool, Verdict,
    agent_role as check_agent_role, language_line, resolve_language, settings_language,
    spec_lock as check_spec_lock, user_settings_language,
};
use crate::app::{
    BashEvent, GuardOutcome, MergeToolEvent, PUSH_WARNING_TIMEOUT, ProviderAccess, WriteToolEvent,
    branch_birth as check_branch_birth, command_guard as check_command_guard, heal_branch_marks,
    merge_tool_guard as check_mcp_merge_guard, write_tool_warning as check_mcp_write_warning,
};
use crate::app::{ExtensionKind, Group};
use crate::project::Env;

/// Nunca bloqueia: uma falha de leitura vai ao stderr, e a rede do `session-start` cobre a marca que faltou.
pub fn branch_birth(stdin: &mut impl Read, err: &mut impl Write) -> ExitCode {
    let result = read_json_input(stdin)
        .and_then(|input| bash_event(&input))
        .and_then(|event| check_branch_birth(&event));
    if let Err(error) = result {
        let _ = writeln!(err, "{}", error.to_json());
    }

    ExitCode::SUCCESS
}

pub fn command_guard(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
) -> ExitCode {
    let runner = ProcessRunner::new();
    let gh = ProviderAccess {
        runner: &runner,
        timeout: PUSH_WARNING_TIMEOUT,
    };
    pre_tool_use(stdin, out, err, |input| {
        check_command_guard(&bash_event(input)?, &gh).map(|outcome| guard_output(&outcome))
    })
}

pub fn mcp_merge_guard(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
) -> ExitCode {
    let mut failed = false;
    let code = pre_tool_use(stdin, out, err, |input| {
        let event = MergeToolEvent {
            cwd: absolute_path(&input["cwd"])?,
        };
        check_mcp_merge_guard(&event)
            .map(|outcome| guard_output(&outcome))
            .inspect_err(|_| failed = true)
    });

    // Falha fechada: o script troca a saída não zero por 2, que bloqueia o merge.
    match failed {
        true => ExitCode::from(1),
        false => code,
    }
}

pub fn mcp_write_warning(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
) -> ExitCode {
    let runner = ProcessRunner::new();
    let gh = ProviderAccess {
        runner: &runner,
        timeout: PUSH_WARNING_TIMEOUT,
    };
    pre_tool_use(stdin, out, err, |input| {
        let field = |name: &str| input["tool_input"][name].as_str().map(str::to_owned);
        let event = WriteToolEvent {
            cwd: absolute_path(&input["cwd"])?,
            owner: field("owner"),
            repo: field("repo"),
            branch: field("branch"),
        };
        check_mcp_write_warning(&event, &gh).map(|outcome| guard_output(&outcome))
    })
}

fn bash_event(input: &Value) -> Result<BashEvent, AppError> {
    Ok(BashEvent {
        cwd: absolute_path(&input["cwd"])?,
        command: input["tool_input"]["command"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Sem negação nem aviso, nada sai no stdout.
fn guard_output(outcome: &GuardOutcome) -> Option<Value> {
    let mut decision = json!({});
    if let Some(reason) = &outcome.deny {
        decision["permissionDecision"] = json!("deny");
        decision["permissionDecisionReason"] = json!(reason);
    }
    if !outcome.context.is_empty() {
        decision["additionalContext"] = json!(outcome.context.join("\n"));
    }

    (outcome.deny.is_some() || !outcome.context.is_empty()).then_some(decision)
}

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

#[cfg(target_os = "macos")]
const MANAGED_SETTINGS: &str = "/Library/Application Support/ClaudeCode/managed-settings.json";
#[cfg(not(target_os = "macos"))]
const MANAGED_SETTINGS: &str = "/etc/claude-code/managed-settings.json";

/// Nunca bloqueia nem falha: sem idioma, ou com settings ilegíveis, sai mudo com 0.
pub fn subagent_start(stdin: &mut impl Read, out: &mut impl Write, env: &Env<'_>) -> ExitCode {
    let cwd = read_json_input(stdin)
        .ok()
        .and_then(|input| input["cwd"].as_str().map(PathBuf::from));
    let project = non_empty(env.claude.project_dir.as_deref())
        .map(PathBuf::from)
        .or(cwd.filter(|dir| dir.is_absolute()));
    let user = non_empty(env.claude.config_dir.as_deref())
        .map(PathBuf::from)
        .or_else(|| non_empty(env.home).map(|home| Path::new(home).join(".claude")));
    let project_file = |name: &str| project.as_ref().map(|dir| dir.join(".claude").join(name));
    let managed = settings_language(Some(Path::new(MANAGED_SETTINGS)));
    let local = settings_language(project_file("settings.local.json").as_deref());
    let shared = settings_language(project_file("settings.json").as_deref());
    let personal = user_settings_language(user.map(|dir| dir.join("settings.json")).as_deref());
    let language = resolve_language(&LanguageRungs {
        plugin_option: env
            .claude
            .plugin_option_language
            .as_deref()
            .and_then(OsStr::to_str),
        managed: managed.as_deref(),
        local: local.as_deref(),
        project: shared.as_deref(),
        user: personal.as_deref(),
    });

    if let Some(language) = language {
        let _ = writeln!(out, "{}", language_output("SubagentStart", &language));
    }

    ExitCode::SUCCESS
}

fn non_empty(value: Option<&OsStr>) -> Option<&OsStr> {
    value.filter(|text| !text.is_empty())
}

fn language_output(event: &str, language: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "additionalContext": language_line(language),
        }
    })
}

/// Só a recusa de `--format agent|human` sai com 2, e vem antes de o hook rodar; a negação vai no JSON, e um erro interno antes de reconhecer a spec libera a escrita, com a linha de erro no stderr; depois do reconhecimento, a falha de git ou de leitura nega com `spec-lock-unavailable`. Quem decide o código de saída que bloqueia é o `omhl-hook.sh`, cujo ramo sem o binário sai com `exit 0` fora de projeto marcado (`docs/directives-flow.md`). Além das specs, nega a mudança do `specs.dir` no `workflow.toml` principal com spec congelada (`spec-dir-frozen`), qualquer edição sob um `.git` de projeto omhl (`git-dir-protected`) e a edição que deixaria o marcador do projeto inválido (`marker-protected`).
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
    // A rede da marca de nascimento: uma falha aqui não tira o contexto da sessão.
    let _ = heal_branch_marks(&cwd);
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
    let notices: Vec<String> = [runtime.git_notice(), app.git_hooks_notice()]
        .into_iter()
        .flatten()
        .collect();
    for line in &notices {
        text.push_str(&format!("\n{line}\n"));
    }
    let git_notice = (!notices.is_empty()).then(|| notices.join("\n"));

    Ok(Some((text, git_notice)))
}

fn extension_notice(checked: &Result<Value, AppError>) -> Option<String> {
    match checked {
        Ok(report) => {
            let notes: Vec<String> = [pending_note(report), orphan_note(report)]
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
