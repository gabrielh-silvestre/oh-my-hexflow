//! Texto do `omhl check run` para agentes: lê só o `Value` que o JSON já mostra, nunca o domínio.

use serde_json::Value;

const SECRET_WARNING: &str = "The output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.";
const NON_BLOCKING_PREFIX: &str = "non-blocking: ";

/// `value` é o sucesso do comando; a linha final não leva quebra.
pub fn render_success(value: &Value) -> String {
    let event = text(value, "event");
    if text(value, "push") == "nothing-to-attest" {
        return format!("✔ {event} · nothing to attest in this push; no check ran");
    }

    let mut lines = vec![success_header(value, &event)];
    lines.extend(items(value, "results").map(result_line));
    lines.extend(items(value, "warnings").map(|warning| warning_line(warning, &event)));
    lines.push(format!(
        "Nothing blocked. Next: {}.",
        next_step_after_success(&event)
    ));

    lines.join("\n")
}

/// `value` é o `{"error": {...}}` de `AppError::to_json`; a linha final não leva quebra.
pub fn render_error(value: &Value) -> String {
    let error = &value["error"];
    let code = text(error, "code");
    let blocked = code == "checks-failed";
    let mut lines = vec![if blocked {
        blocked_header(&error["context"])
    } else {
        format!("✘ {code} · {}", text(error, "message"))
    }];
    lines.extend(items(error, "details").flat_map(detail_lines));
    if blocked || items(error, "details").any(|detail| !detail["context"]["output_tail"].is_null())
    {
        lines.push(SECRET_WARNING.to_owned());
    }
    lines.push(format!("Next: {}.", next_step_after_error(&code)));

    lines.join("\n")
}

/// Run, hint, cause e cauda vêm do TOML, do SO ou do comando: cada campo fica numa linha só e sem controle.
/// Quebras de linha viram espaço; os demais controles e os caracteres de formato (zero-width, bidi) somem.
fn one_line(raw: &str) -> String {
    raw.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}' => Some(' '),
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

fn text(value: &Value, key: &str) -> String {
    one_line(value[key].as_str().unwrap_or_default())
}

fn opt_text(value: &Value, key: &str) -> Option<String> {
    value[key].as_str().map(one_line)
}

fn items<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value[key].as_array().into_iter().flatten()
}

fn success_header(value: &Value, event: &str) -> String {
    let work_item = opt_text(value, "work_item").unwrap_or_else(|| "no work item".to_owned());
    let recorded = if value["recorded"] == true {
        "recorded"
    } else {
        "not recorded"
    };
    let snapshot = &value["snapshot"];
    let snapshot = match opt_text(snapshot, "id") {
        Some(id) => format!(
            " ({} {})",
            text(snapshot, "kind"),
            id.chars().take(7).collect::<String>()
        ),
        None => String::new(),
    };

    format!("✔ {event} · {work_item} · {recorded}{snapshot}")
}

/// `context` é o do erro `checks-failed`: evento, work item (ausente na `develop`) e resumo.
fn blocked_header(context: &Value) -> String {
    let summary = &context["summary"];
    let count = |key: &str| summary[key].as_u64().unwrap_or_default();

    let blocking_failed = count("blocking_failed");
    let non_blocking_failed = count("failed").saturating_sub(blocking_failed);
    let non_blocking = if non_blocking_failed == 0 {
        String::new()
    } else {
        format!("{non_blocking_failed} non-blocking failed, ")
    };

    format!(
        "✘ BLOCKED · {} · {} · {blocking_failed} blocking failed, {non_blocking}{} passed, {} not run",
        text(context, "event"),
        opt_text(context, "work_item").unwrap_or_else(|| "no work item".to_owned()),
        count("passed"),
        count("not_run")
    )
}

/// Os fatos curtos do check reprovado, depois do id: `test/suite · blocking · exit 101 · 2.6s`.
fn failed_check_line(path: &str, context: &Value) -> String {
    let mut parts = vec![path.to_owned()];
    if let Some(concern) = opt_text(context, "concern") {
        parts.push(format!("{concern}/{}", text(context, "variant")));
    }
    parts.push(
        if context["blocking"] == false {
            "non-blocking"
        } else {
            "blocking"
        }
        .to_owned(),
    );
    if let Some(timeout_secs) = context["timeout_secs"].as_u64() {
        parts.push(format!("timeout {timeout_secs}s"));
    }
    if let Some(exit_code) = context["exit_code"].as_i64() {
        parts.push(format!("exit {exit_code}"));
    }
    if context["timed_out"] == true {
        parts.push("timed out".to_owned());
    }
    if let Some(duration_ms) = context["duration_ms"].as_u64() {
        parts.push(duration_label(duration_ms));
    }
    if context["truncated"] == true {
        parts.push("truncated".to_owned());
    }

    format!("  ✘ {}", parts.join(" · "))
}

fn duration_label(duration_ms: u64) -> String {
    if duration_ms < 1000 {
        return format!("{duration_ms}ms");
    }
    let tenths = (duration_ms + 50) / 100;

    format!("{}.{}s", tenths / 10, tenths % 10)
}

fn result_line(result: &Value) -> String {
    let outcome = text(result, "outcome");
    let icon = match outcome.as_str() {
        "passed" => "✔",
        "passed_with_warning" => "⚠",
        _ => "✘",
    };
    let mut notes = Vec::new();
    if result["blocking"] == false {
        notes.push("non-blocking".to_owned());
    }
    if outcome != "passed"
        && let Some(exit_code) = result["exit_code"].as_i64()
    {
        notes.push(format!("exit {exit_code}"));
    }
    if result["timed_out"] == true {
        notes.push("timed out".to_owned());
    }
    if result["truncated"] == true {
        notes.push("truncated".to_owned());
    }
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    };

    let concern = match opt_text(result, "concern") {
        Some(concern) => format!("  {concern}/{}", text(result, "variant")),
        None => String::new(),
    };

    format!(
        "  {icon} {}{concern}  {outcome}  {}ms{notes}",
        text(result, "check"),
        result["duration_ms"].as_u64().unwrap_or_default()
    )
}

fn warning_line(warning: &Value, event: &str) -> String {
    let code = text(warning, "code");
    match code.as_str() {
        "no-checks-for-event" => format!(
            "⚠ {code}: no check is planned for {event}, so nothing ran and this is not a pass. Next: add a mechanical check for this event if one is expected."
        ),
        "check-orphaned" => format!(
            "⚠ {code}: {} did not run. {}",
            text(warning, "check"),
            text(warning, "hint")
        ),
        _ => format!("⚠ {code}"),
    }
}

fn next_step_after_success(event: &str) -> &'static str {
    match event {
        "before-commit" => "go on with the commit",
        "before-pr" => "go on with the push",
        "before-merge" => "go on with the merge",
        _ => "go on",
    }
}

fn next_step_after_error(code: &str) -> &'static str {
    match code {
        "checks-failed" => {
            "read the output with the command above, fix the cause, then run the same command again"
        }
        _ => {
            "follow the hint above if there is one, fix the cause, then run the same command again"
        }
    }
}

fn detail_lines(detail: &Value) -> Vec<String> {
    let code = text(detail, "code");
    let message = text(detail, "message");
    let path = text(detail, "path");
    let context = &detail["context"];
    let mut lines = Vec::new();

    if code == "failed" {
        lines.push(failed_check_line(&path, context));
        if let Some(run) = context["run"].as_array() {
            let run: Vec<String> = run.iter().filter_map(Value::as_str).map(one_line).collect();
            lines.push(format!("    command: {}", run.join(" ")));
        }
        lines.push(format!(
            "    how to read the output: {}",
            message
                .strip_prefix(NON_BLOCKING_PREFIX)
                .unwrap_or(&message)
        ));
    } else {
        lines.push(if path.is_empty() {
            format!("  - {code}")
        } else {
            format!("  - {code} at {path}")
        });
        if message != code {
            lines.push(format!("    {message}"));
        }
    }
    for (label, value) in [
        (
            "kind",
            opt_text(context, "kind").filter(|kind| *kind != code),
        ),
        ("check", opt_text(context, "check")),
        ("cause", opt_text(context, "os_error")),
        ("hint", opt_text(context, "hint")),
        ("next step", opt_text(detail, "next_step")),
    ] {
        if let Some(value) = value {
            lines.push(format!("    {label}: {value}"));
        }
    }
    if !context["output_tail"].is_null() {
        lines.extend(tail_lines(&context["output_tail"]));
    }

    lines
}

fn tail_lines(tail: &Value) -> Vec<String> {
    let mut lines = vec!["    output tail:".to_owned()];
    for stream in ["stdout", "stderr"] {
        lines.extend(items(tail, stream).map(|line| {
            format!(
                "      {stream} | {}",
                one_line(line.as_str().unwrap_or_default())
            )
        }));
    }
    if lines.len() == 1 {
        lines.push("      (empty)".to_owned());
    }

    lines
}

#[cfg(test)]
mod tests;
