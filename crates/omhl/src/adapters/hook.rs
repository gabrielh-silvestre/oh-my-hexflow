use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::app::action::{Action, Caller};
use crate::app::error::AppError;
use crate::app::input::{INPUT_MAX_BYTES, parse_json_text};
use crate::app::runtime::Runtime;
use crate::project::Env;
use crate::project::marker::find_marker;

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

fn context_for_session(stdin: &mut impl Read, env: &Env<'_>) -> Result<Option<String>, AppError> {
    let cwd = read_cwd(stdin)?;
    if find_marker(Path::new(&cwd))?.is_none() {
        return Ok(None);
    }
    let runtime = Runtime::discover(Path::new(&cwd), None, env)?;
    let mut rendered = runtime.app()?.dispatch(&Caller::hook(), &Action::Render)?;

    match rendered["text"].take() {
        Value::String(text) => Ok(Some(text)),
        _ => Err(AppError::plain(
            "render-failed",
            "the render result has no text",
        )),
    }
}

/// Reads one byte past the limit so an oversized input is detected without reading the rest.
fn read_cwd(stdin: &mut impl Read) -> Result<String, AppError> {
    let mut bytes = Vec::new();
    stdin
        .take(INPUT_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::plain("input-unreadable", "input could not be read"))?;

    match parse_json_text(&bytes)?.get_mut("cwd").map(Value::take) {
        Some(Value::String(cwd)) => Ok(cwd),
        _ => Err(AppError::plain(
            "invalid-hook-input",
            "hook input must be a JSON object with a string cwd",
        )),
    }
}
