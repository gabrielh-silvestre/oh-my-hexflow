use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::CommandFactory;
use serde_json::{Value, json};

use super::cli::Cli;
use crate::app::action::{Action, Caller};
use crate::app::error::AppError;
use crate::app::input::read_json_input;
use crate::app::runtime::Runtime;
use crate::app::{
    AgentRoleEnv, RoleVerdict, Verdict, agent_role as check_agent_role,
    spec_lock as check_spec_lock,
};
use crate::app::{ExtensionKind, INVALID_WORKFLOW_CONFIG};
use crate::project::Env;

/// Without a marker the hook stays silent; any failure leaves stdout empty so no partial context is injected.
pub fn session_start(
    stdin: &mut impl Read,
    out: &mut impl Write,
    err: &mut impl Write,
    env: &Env<'_>,
) -> ExitCode {
    match context_for_session(stdin, env) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(context)) => {
            let output = json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": context,
                }
            });
            let _ = writeln!(out, "{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let _ = writeln!(err, "{}", error.to_json());
            ExitCode::from(1)
        }
    }
}

/// Nunca sai com 2: a negação vai no JSON e qualquer erro interno libera a escrita, com a linha de erro no stderr.
pub fn spec_lock(stdin: &mut impl Read, out: &mut impl Write, err: &mut impl Write) -> ExitCode {
    pre_tool_use(stdin, out, err, |input| {
        Ok(match check_spec_lock(input)? {
            Verdict::Allow => None,
            Verdict::Deny { reason, .. } => {
                Some(json!({ "permissionDecision": "deny", "permissionDecisionReason": reason }))
            }
        })
    })
}

/// Mesmo contrato do `spec_lock`; só o Bash. O que passa segue o fluxo normal de permissão; só o comando dentro da matriz recebe `allow`.
pub fn agent_role(stdin: &mut impl Read, out: &mut impl Write, err: &mut impl Write) -> ExitCode {
    let plugin_root = std::env::var_os("CLAUDE_PLUGIN_ROOT").map(PathBuf::from);
    let cli = Cli::command();
    // Os comandos com subcomando vêm do clap: o hook não mantém uma segunda lista de folhas.
    let groups: Vec<&str> = cli
        .get_subcommands()
        .filter(|command| command.has_subcommands())
        .map(clap::Command::get_name)
        .collect();
    let env = AgentRoleEnv {
        plugin_root: plugin_root.as_deref(),
        groups: &groups,
    };

    pre_tool_use(stdin, out, err, |input| {
        Ok(match check_agent_role(input, &env) {
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

/// `decide` devolve os campos da decisão (sem o nome do evento); `None` deixa o stdout vazio.
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

fn context_for_session(stdin: &mut impl Read, env: &Env<'_>) -> Result<Option<String>, AppError> {
    let cwd = read_cwd(stdin)?;
    let runtime = match Runtime::discover(Path::new(&cwd), None, env) {
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

    Ok(Some(text))
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

fn read_cwd(stdin: &mut impl Read) -> Result<String, AppError> {
    match read_json_input(stdin)?.get_mut("cwd").map(Value::take) {
        Some(Value::String(cwd)) if Path::new(&cwd).is_absolute() => Ok(cwd),
        _ => Err(AppError::invalid_hook_input()),
    }
}
