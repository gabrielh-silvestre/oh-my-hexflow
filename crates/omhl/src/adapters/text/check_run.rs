//! Lê só o `Value` que o JSON já mostra, nunca o domínio.

use serde_json::Value;

use crate::app::byproduct_read_command;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Join, Line, Marker, field, items, one_line, opt_text, text};
use super::error::{self, ErrorFacts};
#[cfg(test)]
use super::vocab::{AgentVocabulary, NOTHING_BLOCKED, SYMBOL_ERROR, SYMBOL_OK, SYMBOL_WARNING};
use super::vocab::{
    BLOCKING, HASH_PREFIX_LEN, NEXT_LABEL, NON_BLOCKING, SECRET_WARNING, State, TIMED_OUT,
    TRUNCATED,
};
#[cfg(test)]
use super::{BaseRenderer, Renderer, layout};

const NON_BLOCKING_PREFIX: &str = "non-blocking: ";

/// O `nothing-to-attest` só tem `event`, `push` e `results: []`; `exit_code` do
/// sucesso é `Derived` porque o texto só o mostra quando o resultado não passou; a falha lista só o que o texto mostra,
/// e `Noise` é o que ele nunca mostra (`expected`, `byproduct` e `stored` vêm do `guard-refused`).
pub static PROFILE: Profile = Profile {
    label: "",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "event", Shape::Closed),
        Entry::new(Region::Root, "push", Shape::Derived).optional(),
        Entry::new(Region::Root, "work_item", Shape::Literal).optional(),
        Entry::new(Region::Root, "recorded", Shape::Derived).optional(),
        Entry::new(Region::Root, "checks_planned", Shape::Noise).optional(),
        Entry::new(Region::Data, "kind", Shape::Closed)
            .under("snapshot")
            .optional(),
        Entry::new(Region::Data, "id", Shape::Hash(HASH_PREFIX_LEN))
            .under("snapshot")
            .optional(),
        Entry::new(Region::Data, "check", Shape::Literal)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "outcome", Shape::Closed)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "ok", Shape::Derived)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "blocking", Shape::Derived)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "concern", Shape::Closed)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "variant", Shape::Closed)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "exit_code", Shape::Derived)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "duration_ms", Shape::Derived)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "timed_out", Shape::Derived)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "truncated", Shape::Derived)
            .under("results")
            .optional(),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "check", Shape::Literal)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "work_item", Shape::Literal)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "hint", Shape::Closed)
            .under("warnings")
            .optional(),
    ],
    failure: &[
        Entry::new(Region::Root, "code", Shape::Derived),
        Entry::new(Region::Root, "message", Shape::Derived),
        Entry::new(Region::Data, "event", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "work_item", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "planned", Shape::Noise)
            .under("summary")
            .optional(),
        Entry::new(Region::Data, "passed", Shape::Derived)
            .under("summary")
            .optional(),
        Entry::new(Region::Data, "failed", Shape::Derived)
            .under("summary")
            .optional(),
        Entry::new(Region::Data, "blocking_failed", Shape::Derived)
            .under("summary")
            .optional(),
        Entry::new(Region::Data, "not_run", Shape::Derived)
            .under("summary")
            .optional(),
        Entry::new(Region::Data, "path", Shape::Literal)
            .under("details")
            .optional(),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("details")
            .optional(),
        Entry::new(Region::Data, "message", Shape::Derived)
            .under("details")
            .optional(),
        Entry::new(Region::Data, "current", Shape::Noise)
            .under("details")
            .optional(),
        Entry::new(Region::Data, "retired", Shape::Noise)
            .under("details")
            .optional(),
        Entry::new(Region::Data, "next_step", Shape::Closed)
            .under("details")
            .optional(),
        Entry::new(Region::Data, "kind", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "check", Shape::Literal)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "os_error", Shape::Closed)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "hint", Shape::Closed)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "blocking", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "run", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "timeout_secs", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "concern", Shape::Closed)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "variant", Shape::Closed)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "exit_code", Shape::Literal)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "duration_ms", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "timed_out", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "truncated", Shape::Derived)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "stdout", Shape::Literal)
            .under("output_tail")
            .optional(),
        Entry::new(Region::Data, "stderr", Shape::Literal)
            .under("output_tail")
            .optional(),
        Entry::new(Region::Data, "branch", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "cwd", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "argv", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "rref", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "lref", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "expected", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "byproduct", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "stored", Shape::Noise)
            .under("context")
            .optional(),
        Entry::new(Region::Data, "kind", Shape::Noise)
            .under("snapshot")
            .optional(),
        Entry::new(Region::Data, "id", Shape::Noise)
            .under("snapshot")
            .optional(),
    ],
};

pub struct CheckRun;

impl Specialization for CheckRun {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let event = text(value, "event");
        if nothing_to_attest(value) {
            return vec![
                event,
                "nothing to attest in this push; no check ran".to_owned(),
            ];
        }

        success_summary(value, event)
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        if nothing_to_attest(value) {
            return Vec::new();
        }
        let event = text(value, "event");

        items(value, "results")
            .map(result_line)
            .chain(items(value, "warnings").map(|warning| warning_line(warning, &event)))
            .collect()
    }

    fn closing(&self, value: &Value) -> Closing {
        match nothing_to_attest(value) {
            true => Closing::None,
            false => {
                Closing::NothingBlocked(next_step_after_success(&text(value, "event")).to_owned())
            }
        }
    }

    fn failure_header(&self, error: &Value) -> Line {
        match text(error, "code") == "checks-failed" {
            true => blocked_header(&error["context"]),
            false => error::header(error),
        }
    }

    fn failure_detail_lines(&self, detail: &Value, _facts: &ErrorFacts) -> Vec<Line> {
        detail_lines(detail)
    }

    fn failure_extras(&self, error: &Value, facts: &ErrorFacts) -> Vec<Line> {
        let shows_output = facts.code == "checks-failed"
            || items(error, "details").any(|detail| !detail["context"]["output_tail"].is_null());

        match shows_output {
            true => vec![Line::Output {
                depth: 0,
                text: SECRET_WARNING.to_owned(),
            }],
            false => Vec::new(),
        }
    }

    fn failure_closing(&self, facts: &ErrorFacts) -> Closing {
        Closing::Next(next_step_after_error(&facts.code).to_owned())
    }
}

fn nothing_to_attest(value: &Value) -> bool {
    text(value, "push") == "nothing-to-attest"
}

fn success_summary(value: &Value, event: String) -> Vec<String> {
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
            id.chars().take(HASH_PREFIX_LEN).collect::<String>()
        ),
        None => String::new(),
    };

    vec![event, work_item, format!("{recorded}{snapshot}")]
}

fn blocked_header(context: &Value) -> Line {
    let summary = &context["summary"];
    let count = |key: &str| summary[key].as_u64().unwrap_or_default();

    let blocking_failed = count("blocking_failed");
    let non_blocking_failed = count("failed").saturating_sub(blocking_failed);
    let non_blocking = if non_blocking_failed == 0 {
        String::new()
    } else {
        format!("{non_blocking_failed} non-blocking failed, ")
    };

    Line::Header {
        state: State::Error,
        parts: vec![
            "BLOCKED".to_owned(),
            text(context, "event"),
            opt_text(context, "work_item").unwrap_or_else(|| "no work item".to_owned()),
            format!(
                "{blocking_failed} blocking failed, {non_blocking}{} passed, {} not run",
                count("passed"),
                count("not_run")
            ),
        ],
    }
}

fn failed_check_line(path: &str, context: &Value) -> Line {
    let mut parts = vec![path.to_owned()];
    if let Some(concern) = opt_text(context, "concern") {
        parts.push(format!("{concern}/{}", text(context, "variant")));
    }
    parts.push(
        if context["blocking"] == false {
            NON_BLOCKING
        } else {
            BLOCKING
        }
        .to_owned(),
    );
    if let Some(timeout_secs) = context["timeout_secs"].as_u64() {
        parts.push(format!("timeout {timeout_secs}s"));
    }
    if let Some(exit_code) = context["exit_code"].as_i64() {
        parts.push(format!("exit {exit_code}"));
    }
    push_if_true(&mut parts, context, "timed_out", TIMED_OUT);
    if let Some(duration_ms) = context["duration_ms"].as_u64() {
        parts.push(duration_label(duration_ms));
    }
    push_if_true(&mut parts, context, "truncated", TRUNCATED);

    Line::Item {
        depth: 1,
        marker: Marker::Symbol(State::Error),
        parts,
        join: Join::Dot,
    }
}

fn push_if_true(parts: &mut Vec<String>, object: &Value, key: &str, label: &str) {
    if object[key] == true {
        parts.push(label.to_owned());
    }
}

fn duration_label(duration_ms: u64) -> String {
    if duration_ms < 1000 {
        return format!("{duration_ms}ms");
    }
    let tenths = (duration_ms + 50) / 100;

    format!("{}.{}s", tenths / 10, tenths % 10)
}

fn result_line(result: &Value) -> Line {
    let outcome = text(result, "outcome");
    let state = match outcome.as_str() {
        "passed" => State::Ok,
        "passed_with_warning" => State::Warning,
        _ => State::Error,
    };
    let mut notes = Vec::new();
    if result["blocking"] == false {
        notes.push(NON_BLOCKING.to_owned());
    }
    if outcome != "passed"
        && let Some(exit_code) = result["exit_code"].as_i64()
    {
        notes.push(format!("exit {exit_code}"));
    }
    push_if_true(&mut notes, result, "timed_out", TIMED_OUT);
    push_if_true(&mut notes, result, "truncated", TRUNCATED);
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    };

    let mut parts = vec![text(result, "check")];
    if let Some(concern) = opt_text(result, "concern") {
        parts.push(format!("{concern}/{}", text(result, "variant")));
    }
    parts.push(outcome);
    parts.push(format!(
        "{}ms{notes}",
        result["duration_ms"].as_u64().unwrap_or_default()
    ));

    Line::Item {
        depth: 1,
        marker: Marker::Symbol(state),
        parts,
        join: Join::Gap,
    }
}

fn warning_line(warning: &Value, event: &str) -> Line {
    let code = text(warning, "code");
    let message = match code.as_str() {
        "no-checks-for-event" => format!(
            "{code}: no check is planned for {event}, so nothing ran and this is not a pass. {NEXT_LABEL} add a mechanical check for this event if one is expected."
        ),
        "check-orphaned" => format!(
            "{code}: {} did not run. {}",
            text(warning, "check"),
            text(warning, "hint")
        ),
        "output-stored" => format!(
            "{code}: the output of {} is stored. {NEXT_LABEL} {}",
            text(warning, "check"),
            byproduct_read_command(&text(warning, "check"), &text(warning, "work_item"))
        ),
        _ => code,
    };

    Line::Note {
        state: State::Warning,
        text: message,
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

fn detail_lines(detail: &Value) -> Vec<Line> {
    let code = text(detail, "code");
    let message = text(detail, "message");
    let path = text(detail, "path");
    let context = &detail["context"];
    let mut lines = Vec::new();

    if code == "failed" {
        lines.push(failed_check_line(&path, context));
        if let Some(run) = context["run"].as_array() {
            let run: Vec<String> = run.iter().filter_map(Value::as_str).map(one_line).collect();
            lines.push(field(2, "command", run.join(" ")));
        }
        lines.push(field(
            2,
            "how to read the output",
            message
                .strip_prefix(NON_BLOCKING_PREFIX)
                .unwrap_or(&message)
                .to_owned(),
        ));
    } else {
        lines.push(Line::Item {
            depth: 1,
            marker: Marker::Dash,
            parts: vec![if path.is_empty() {
                code.clone()
            } else {
                format!("{code} at {path}")
            }],
            join: Join::Dot,
        });
        if message != code {
            lines.push(Line::Output {
                depth: 2,
                text: message,
            });
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
            lines.push(field(2, label, value));
        }
    }
    if !context["output_tail"].is_null() {
        lines.extend(tail_lines(&context["output_tail"]));
    }

    lines
}

fn tail_lines(tail: &Value) -> Vec<Line> {
    let raw = |depth, text: String| Line::Output { depth, text };
    let mut lines = vec![raw(2, "output tail:".to_owned())];
    for stream in ["stdout", "stderr"] {
        lines.extend(items(tail, stream).map(|line| {
            raw(
                3,
                format!("{stream} | {}", one_line(line.as_str().unwrap_or_default())),
            )
        }));
    }
    if lines.len() == 1 {
        lines.push(raw(3, "(empty)".to_owned()));
    }

    lines
}

#[cfg(test)]
mod tests;
