//! Texto do `omhl check run`: lê só o `Value` que o JSON já mostra, nunca o domínio.

use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Join, Line, Marker, items, one_line, opt_text, text};
use super::error::{self, ErrorFacts};
#[cfg(test)]
use super::{BaseRenderer, Renderer, layout};

pub(super) const SECRET_WARNING: &str = "The output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.";
const NON_BLOCKING_PREFIX: &str = "non-blocking: ";

/// Sem rótulo: o cabeçalho do `check run` abre pelo evento. Sem curinga e tudo `optional` exceto `event` (sucesso)
/// e `code`/`message` da raiz (erro). O `nothing-to-attest` só tem `event`, `push` e `results: []`; `exit_code` do
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
        Entry::new(Region::Data, "id", Shape::Hash(7))
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

    /// `value` é o sucesso do comando.
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

    /// O contexto do erro não é impresso (só o cabeçalho `BLOCKED` o usa); sobra o aviso de segredos.
    fn failure_extras(&self, error: &Value, facts: &ErrorFacts) -> Vec<Line> {
        let shows_output = facts.code == "checks-failed"
            || items(error, "details").any(|detail| !detail["context"]["output_tail"].is_null());

        match shows_output {
            true => vec![Line::Raw {
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
            id.chars().take(7).collect::<String>()
        ),
        None => String::new(),
    };

    vec![event, work_item, format!("{recorded}{snapshot}")]
}

/// `context` é o do erro `checks-failed`: evento, work item (ausente na `develop`) e resumo.
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
        symbol: "✘",
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

/// Os fatos curtos do check reprovado, depois do id: `test/suite · blocking · exit 101 · 2.6s`.
fn failed_check_line(path: &str, context: &Value) -> Line {
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
    push_if_true(&mut parts, context, "timed_out", "timed out");
    if let Some(duration_ms) = context["duration_ms"].as_u64() {
        parts.push(duration_label(duration_ms));
    }
    push_if_true(&mut parts, context, "truncated", "truncated");

    Line::Item {
        depth: 1,
        marker: Marker::Symbol("✘"),
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
    push_if_true(&mut notes, result, "timed_out", "timed out");
    push_if_true(&mut notes, result, "truncated", "truncated");
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
        marker: Marker::Symbol(icon),
        parts,
        join: Join::Gap,
    }
}

fn warning_line(warning: &Value, event: &str) -> Line {
    let code = text(warning, "code");
    let message = match code.as_str() {
        "no-checks-for-event" => format!(
            "{code}: no check is planned for {event}, so nothing ran and this is not a pass. Next: add a mechanical check for this event if one is expected."
        ),
        "check-orphaned" => format!(
            "{code}: {} did not run. {}",
            text(warning, "check"),
            text(warning, "hint")
        ),
        _ => code,
    };

    Line::Note {
        symbol: "⚠",
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

fn field(label: &str, value: String) -> Line {
    Line::Field {
        depth: 2,
        label: label.to_owned(),
        value,
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
            lines.push(field("command", run.join(" ")));
        }
        lines.push(field(
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
            lines.push(Line::Raw {
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
            lines.push(field(label, value));
        }
    }
    if !context["output_tail"].is_null() {
        lines.extend(tail_lines(&context["output_tail"]));
    }

    lines
}

fn tail_lines(tail: &Value) -> Vec<Line> {
    let raw = |depth, text: String| Line::Raw { depth, text };
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
mod tests {
    use serde_json::json;

    use super::*;

    fn render_success(value: &Value) -> String {
        layout::agent(&BaseRenderer(CheckRun).success(value))
    }

    fn render_error(value: &Value) -> String {
        layout::agent(&BaseRenderer(CheckRun).failure(value))
    }

    const COMMIT: &str = "c43356d0b9a1e8f47d2a5b6c7e8f9a0b1c2d3e4f";

    fn result(check: &str, outcome: &str, blocking: bool, exit_code: i64, ms: u64) -> Value {
        json!({
            "blocking": blocking, "check": check, "duration_ms": ms, "exit_code": exit_code,
            "ok": outcome != "failed", "outcome": outcome, "timed_out": false, "truncated": false
        })
    }

    fn success(event: &str, results: Vec<Value>) -> Value {
        json!({
            "blocked": [], "event": event, "recorded": true, "results": Value::Array(results),
            "snapshot": { "id": COMMIT, "kind": "git-commit" }, "work_item": "work-items.w1"
        })
    }

    fn failed_detail(path: &str, message: &str) -> Value {
        json!({
            "code": "failed", "message": message, "path": path,
            "context": {
                "blocking": true, "concern": "test", "variant": "suite", "run": ["cargo", "test"],
                "timeout_secs": 60, "exit_code": 101, "duration_ms": 2600,
                "timed_out": false, "truncated": false
            }
        })
    }

    fn with_context(mut detail: Value, key: &str, value: Value) -> Value {
        detail["context"][key] = value;

        detail
    }

    fn error(code: &str, message: &str, details: Vec<Value>) -> Value {
        json!({ "error": { "code": code, "details": Value::Array(details), "message": message } })
    }

    fn checks_failed(details: Vec<Value>) -> Value {
        let mut value = error("checks-failed", "a blocking check failed", details);
        value["error"]["context"] = json!({
            "event": "before-pr", "work_item": "work-items.w1",
            "summary": { "planned": 3, "passed": 1, "failed": 1, "blocking_failed": 1, "not_run": 1 }
        });

        value
    }

    fn develop_failure(tail: Value) -> Value {
        let detail = with_context(
            failed_detail("checks.bad", "output in context.output_tail"),
            "output_tail",
            tail,
        );
        let mut value = checks_failed(vec![detail]);
        value["error"]["context"]
            .as_object_mut()
            .expect("context is an object")
            .remove("work_item");

        value
    }

    fn pre_check_errors() -> Vec<Value> {
        vec![
            error(
                "no-marker",
                "no .omhl.toml found in the project directory or any parent; a branch older than the marker needs a rebase onto main",
                vec![],
            ),
            error(
                "invalid-project-dir",
                "the project directory does not exist",
                vec![],
            ),
            error(
                "push-input-invalid",
                "push-input-invalid at ",
                vec![json!({
                    "code": "push-input-invalid", "message": "the pre-push input is invalid", "path": "",
                    "context": { "kind": "push-input-invalid", "event": "before-pr", "hint": "the pre-push input needs four fields per line; run git push again" }
                })],
            ),
        ]
    }

    fn refusals() -> Vec<Value> {
        vec![
            error(
                "check-not-found",
                "check-not-found at /check",
                vec![
                    json!({ "code": "check-not-found", "message": "check-not-found", "path": "/check" }),
                ],
            ),
            error(
                "worktree-dirty",
                "the working tree has uncommitted changes",
                vec![json!({
                    "code": "worktree-dirty", "message": "the working tree has uncommitted changes", "path": "",
                    "context": { "branch": "feat/wi-w1__x", "event": "before-pr", "kind": "worktree-dirty", "work_item": "work-items.w1" }
                })],
            ),
            error(
                "branch-protected",
                "the branch is protected",
                vec![json!({
                    "code": "branch-protected", "message": "the branch is protected", "path": "",
                    "context": { "kind": "push-to-main", "event": "before-pr", "rref": "refs/heads/main", "hint": "main receives no direct pushes; open a pull request" }
                })],
            ),
            error(
                "check-not-run",
                "a check could not run to a verdict",
                vec![json!({
                    "code": "spawn-failed", "message": "the command could not run", "path": "checks.ghost",
                    "context": {
                        "argv": ["omhl-ghost-binary"], "branch": "feat/wi-w1__x", "check": "checks.ghost", "cwd": "/repo",
                        "event": "before-pr", "kind": "spawn-not-found",
                        "hint": "`omhl-ghost-binary` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again",
                        "os_error": "omhl-ghost-binary did not start: No such file or directory (os error 2)",
                        "snapshot": { "id": COMMIT, "kind": "git-commit" }, "work_item": "work-items.w1"
                    }
                })],
            ),
            error(
                "attestation-refused",
                "the guard refused an attestation",
                vec![json!({
                    "code": "outcome-immutable", "message": "the guard refused the attestation", "path": "checks.ok",
                    "context": { "kind": "guard-refused", "check": "checks.ok", "hint": "this snapshot already has the stored outcome for this check; change the tree to get a new verdict" }
                })],
            ),
            error(
                "check-not-run",
                "a check could not run to a verdict",
                vec![json!({
                    "code": "spec-stale", "message": "the check file is invalid or differs from the stored definition", "path": "checks.ok",
                    "context": { "kind": "spec-stale", "hint": "the check file on disk is invalid or differs from the stored definition; fix the file and run omhl load, then run the same command again" }
                })],
            ),
            error(
                "attestation-not-recorded",
                "a verdict was computed but its attestation was not recorded",
                vec![json!({
                    "code": "not-run", "message": "the attestation was not recorded", "path": "checks.ok",
                    "context": { "kind": "busy", "hint": "the database stayed locked for about 74 seconds; the verdict above was computed but not recorded; run the same command again when the other process ends" }
                })],
            ),
        ]
    }

    fn every_rendering() -> Vec<String> {
        let ok = success(
            "before-pr",
            vec![result("checks.ok", "passed", true, 0, 12)],
        );
        let mut texts = vec![
            render_success(&ok),
            render_error(&checks_failed(vec![failed_detail(
                "checks.bad",
                "omhl byproduct checks.bad work-items.w1 --tail 40",
            )])),
            render_error(&develop_failure(
                json!({ "stdout": ["\u{1b}[31mred\u{1b}[0m"], "stderr": [] }),
            )),
        ];
        texts.extend(pre_check_errors().iter().map(render_error));
        texts.extend(refusals().iter().map(render_error));

        texts
    }

    #[test]
    fn it_should_open_the_success_text_with_event_work_item_and_snapshot_prefix() {
        let value = success(
            "before-pr",
            vec![result("checks.ok", "passed", true, 0, 12)],
        );

        let rendered = render_success(&value);

        assert_eq!(
            rendered.lines().next(),
            Some("✔ before-pr · work-items.w1 · recorded (git-commit c43356d)")
        );
    }

    #[test]
    fn it_should_render_one_line_per_check_in_json_order_telling_passed_from_passed_with_warning() {
        let value = success(
            "before-pr",
            vec![
                result("checks.fmt", "passed", true, 0, 12),
                result("checks.lint", "passed_with_warning", true, 2, 1500),
            ],
        );

        let rendered = render_success(&value);

        assert_eq!(
            rendered,
            "✔ before-pr · work-items.w1 · recorded (git-commit c43356d)\n  ✔ checks.fmt  passed  12ms\n  ⚠ checks.lint  passed_with_warning  1500ms (exit 2)\nNothing blocked. Next: go on with the push."
        );
    }

    #[test]
    fn it_should_flag_a_non_blocking_failure_in_a_successful_run() {
        let mut soft = result("checks.soft", "failed", false, 1, 12);
        soft["truncated"] = json!(true);
        let value = success("before-pr", vec![soft]);

        let rendered = render_success(&value);

        assert!(
            rendered.contains("  ✘ checks.soft  failed  12ms (non-blocking, exit 1, truncated)")
        );
    }

    #[test]
    fn it_should_flag_a_timed_out_check() {
        let mut slow = result("checks.slow", "failed", false, 0, 60000);
        slow["timed_out"] = json!(true);
        let value = success("before-pr", vec![slow]);

        let rendered = render_success(&value);

        assert!(rendered.contains("(non-blocking, exit 0, timed out)"));
    }

    #[test]
    fn it_should_name_the_next_step_of_each_event_on_the_last_line() {
        let last = |event: &str| {
            render_success(&success(event, vec![]))
                .lines()
                .last()
                .map(str::to_owned)
        };

        assert_eq!(
            last("before-commit").as_deref(),
            Some("Nothing blocked. Next: go on with the commit.")
        );
        assert_eq!(
            last("before-merge").as_deref(),
            Some("Nothing blocked. Next: go on with the merge.")
        );
    }

    #[test]
    fn it_should_warn_with_an_action_when_the_event_has_no_check() {
        let value = json!({
            "blocked": [], "checks_planned": 0, "event": "before-merge", "recorded": true,
            "results": [], "snapshot": null, "warnings": [{ "code": "no-checks-for-event" }],
            "work_item": "work-items.w1"
        });

        let rendered = render_success(&value);

        assert_eq!(
            rendered,
            "✔ before-merge · work-items.w1 · recorded\n⚠ no-checks-for-event: no check is planned for before-merge, so nothing ran and this is not a pass. Next: add a mechanical check for this event if one is expected.\nNothing blocked. Next: go on with the merge."
        );
    }

    #[test]
    fn it_should_warn_with_the_check_and_its_hint_when_a_check_is_orphaned() {
        let mut value = success("before-pr", vec![]);
        value["warnings"] = json!([{
            "code": "check-orphaned", "check": "checks.gone", "hint": "the check file is missing in this working tree; rebase it onto main"
        }]);

        let rendered = render_success(&value);

        assert!(rendered.contains(
            "⚠ check-orphaned: checks.gone did not run. the check file is missing in this working tree; rebase it onto main"
        ));
    }

    #[test]
    fn it_should_say_in_one_line_that_a_push_had_nothing_to_attest() {
        let value = json!({ "event": "before-pr", "push": "nothing-to-attest", "results": [] });

        let rendered = render_success(&value);

        assert_eq!(
            rendered,
            "✔ before-pr · nothing to attest in this push; no check ran"
        );
    }

    #[test]
    fn it_should_report_a_blocked_event_with_the_failed_check_and_how_to_read_its_output() {
        let value = checks_failed(vec![failed_detail(
            "checks.bad",
            "omhl byproduct checks.bad work-items.w1 --tail 40",
        )]);

        let rendered = render_error(&value);

        assert_eq!(
            rendered,
            "✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 1 passed, 1 not run\n  ✘ checks.bad · test/suite · blocking · timeout 60s · exit 101 · 2.6s\n    command: cargo test\n    how to read the output: omhl byproduct checks.bad work-items.w1 --tail 40\nThe output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.\nNext: read the output with the command above, fix the cause, then run the same command again."
        );
    }

    #[test]
    fn it_should_label_a_non_blocking_failed_check_and_drop_the_json_prefix() {
        let soft = with_context(
            failed_detail(
                "checks.soft",
                "non-blocking: omhl byproduct checks.soft work-items.w1 --tail 40",
            ),
            "blocking",
            json!(false),
        );
        let value = checks_failed(vec![soft]);

        let rendered = render_error(&value);

        assert!(rendered.contains("  ✘ checks.soft · test/suite · non-blocking · "));
        assert!(
            rendered
                .contains("    how to read the output: omhl byproduct checks.soft work-items.w1")
        );
    }

    #[test]
    fn it_should_say_timed_out_instead_of_an_exit_code_when_the_check_ran_out_of_time() {
        let mut slow = with_context(
            failed_detail("checks.slow", "omhl byproduct checks.slow w1 --tail 40"),
            "timed_out",
            json!(true),
        );
        slow["context"]
            .as_object_mut()
            .expect("context")
            .remove("exit_code");
        let value = checks_failed(vec![slow]);

        let rendered = render_error(&value);

        assert!(rendered.contains(
            "  ✘ checks.slow · test/suite · blocking · timeout 60s · timed out · 2.6s\n"
        ));
    }

    #[test]
    fn it_should_flag_a_truncated_output_on_the_failed_check_line() {
        let big = with_context(
            failed_detail("checks.big", "omhl byproduct checks.big w1 --tail 40"),
            "truncated",
            json!(true),
        );
        let value = checks_failed(vec![big]);

        let rendered = render_error(&value);

        assert!(rendered.contains("blocking · timeout 60s · exit 101 · 2.6s · truncated\n"));
    }

    #[test]
    fn it_should_open_the_blocked_header_without_a_work_item_on_develop() {
        let value = develop_failure(json!({ "stdout": [], "stderr": [] }));

        let rendered = render_error(&value);

        assert_eq!(
            rendered.lines().next(),
            Some("✘ BLOCKED · before-pr · no work item · 1 blocking failed, 1 passed, 1 not run")
        );
    }

    #[test]
    fn it_should_split_the_blocked_header_between_blocking_and_non_blocking_failures() {
        let mut value = checks_failed(vec![failed_detail(
            "checks.bad",
            "omhl byproduct checks.bad w1",
        )]);
        value["error"]["context"]["summary"] =
            json!({ "planned": 3, "passed": 1, "failed": 2, "blocking_failed": 1, "not_run": 0 });

        let rendered = render_error(&value);

        assert_eq!(
            rendered.lines().next(),
            Some(
                "✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 1 non-blocking failed, 1 passed, 0 not run"
            )
        );
    }

    #[test]
    fn it_should_omit_the_non_blocking_count_from_the_blocked_header_when_it_is_zero() {
        let mut value = checks_failed(vec![failed_detail(
            "checks.bad",
            "omhl byproduct checks.bad w1",
        )]);
        value["error"]["context"]["summary"] =
            json!({ "planned": 1, "passed": 0, "failed": 1, "blocking_failed": 1, "not_run": 0 });

        let rendered = render_error(&value);

        assert_eq!(
            rendered.lines().next(),
            Some("✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 0 passed, 0 not run")
        );
    }

    #[test]
    fn it_should_show_milliseconds_for_a_check_that_took_less_than_a_second() {
        let quick = with_context(
            failed_detail("checks.quick", "omhl byproduct checks.quick w1 --tail 40"),
            "duration_ms",
            json!(40),
        );
        let value = checks_failed(vec![quick]);

        let rendered = render_error(&value);

        assert!(rendered.contains("exit 101 · 40ms\n"));
    }

    #[test]
    fn it_should_show_concern_and_variant_next_to_the_id_of_a_passed_check() {
        let mut format = result("checks.fmt", "passed", true, 0, 12);
        format["concern"] = json!("style");
        format["variant"] = json!("format");
        let value = success("before-pr", vec![format]);

        let rendered = render_success(&value);

        assert!(rendered.contains("\n  ✔ checks.fmt  style/format  passed  12ms\n"));
    }

    #[test]
    fn it_should_keep_the_cut_output_note_of_a_failed_check() {
        let value = checks_failed(vec![failed_detail(
            "checks.big",
            "output was cut at 4 MiB; the rule could not be checked; omhl byproduct checks.big work-items.w1 --tail 40",
        )]);

        let rendered = render_error(&value);

        assert!(
            rendered
                .contains("output was cut at 4 MiB; the rule could not be checked; omhl byproduct")
        );
    }

    #[test]
    fn it_should_show_the_output_tail_of_a_failed_check_on_develop_with_the_secret_warning() {
        let value = develop_failure(json!({ "stdout": ["first", "second"], "stderr": ["boom"] }));

        let rendered = render_error(&value);

        assert!(rendered.contains(
            "    output tail:\n      stdout | first\n      stdout | second\n      stderr | boom\n"
        ));
        assert!(rendered.contains(SECRET_WARNING));
    }

    #[test]
    fn it_should_say_the_tail_is_empty_when_the_failed_check_printed_nothing() {
        let value = develop_failure(json!({ "stdout": [], "stderr": [] }));

        let rendered = render_error(&value);

        assert!(rendered.contains("    output tail:\n      (empty)\n"));
    }

    #[test]
    fn it_should_write_the_error_code_detail_code_message_path_and_hint_of_every_refusal() {
        let expected = [
            ("check-not-found", "check-not-found", "/check", ""),
            (
                "worktree-dirty",
                "worktree-dirty",
                "",
                "the working tree has uncommitted changes",
            ),
            (
                "branch-protected",
                "branch-protected",
                "",
                "main receives no direct pushes; open a pull request",
            ),
            (
                "check-not-run",
                "spawn-failed",
                "checks.ghost",
                "`omhl-ghost-binary` was not found",
            ),
            (
                "attestation-refused",
                "outcome-immutable",
                "checks.ok",
                "change the tree to get a new verdict",
            ),
            ("check-not-run", "spec-stale", "checks.ok", "run omhl load"),
            (
                "attestation-not-recorded",
                "not-run",
                "checks.ok",
                "run the same command again when the other process ends",
            ),
        ];

        for (value, (code, detail_code, path, fragment)) in refusals().iter().zip(expected) {
            let rendered = render_error(value);

            assert!(rendered.contains(code), "{code}: {rendered}");
            assert!(rendered.contains(detail_code), "{detail_code}: {rendered}");
            assert!(rendered.contains(path), "{path}: {rendered}");
            assert!(rendered.contains(fragment), "{fragment}: {rendered}");
        }
    }

    #[test]
    fn it_should_show_the_spawn_cause_and_kind_of_a_check_that_could_not_run() {
        let value = &refusals()[3];

        let rendered = render_error(value);

        assert!(rendered.contains("    kind: spawn-not-found\n"));
        assert!(rendered.contains(
            "    cause: omhl-ghost-binary did not start: No such file or directory (os error 2)\n"
        ));
    }

    #[test]
    fn it_should_render_errors_raised_before_any_check_with_code_and_message() {
        let expected = [
            (
                "no-marker",
                "a branch older than the marker needs a rebase onto main",
            ),
            (
                "invalid-project-dir",
                "the project directory does not exist",
            ),
            (
                "push-input-invalid",
                "the pre-push input needs four fields per line",
            ),
        ];

        for (value, (code, fragment)) in pre_check_errors().iter().zip(expected) {
            let rendered = render_error(value);

            assert!(rendered.starts_with(&format!("✘ {code} · ")), "{rendered}");
            assert!(rendered.contains(fragment), "{fragment}: {rendered}");
        }
    }

    #[test]
    fn it_should_never_emit_the_escape_byte() {
        for rendered in every_rendering() {
            assert!(!rendered.contains('\u{1b}'), "{rendered:?}");
        }
    }

    #[test]
    fn it_should_strip_the_escape_byte_from_run_hint_cause_and_message() {
        let with_escape_run = with_context(
            failed_detail("checks.bad", "omhl byproduct checks.bad w1 --tail 40"),
            "run",
            json!(["sh", "-c", "\u{1b}[31mx"]),
        );
        let with_escape_hint_and_cause = with_context(
            with_context(
                failed_detail("checks.bad", "omhl byproduct checks.bad w1 --tail 40"),
                "hint",
                json!("\u{1b}[31minstall it"),
            ),
            "os_error",
            json!("\u{1b}[31mNo such file"),
        );
        let renderings = [
            render_error(&checks_failed(vec![with_escape_run])),
            render_error(&checks_failed(vec![with_escape_hint_and_cause])),
            render_error(&error("worktree-dirty", "\u{1b}[31mdirty", vec![])),
        ];

        for rendered in renderings {
            assert!(!rendered.contains('\u{1b}'), "{rendered:?}");
        }
    }

    #[test]
    fn it_should_strip_the_escape_byte_from_a_success_text() {
        let mut value = success("before-pr", vec![]);
        value["warnings"] = json!([{
            "code": "check-orphaned", "check": "checks.gone", "hint": "\u{1b}[31mrebase it"
        }]);

        let rendered = render_success(&value);

        assert!(rendered.contains("checks.gone did not run. [31mrebase it"));
    }

    #[test]
    fn it_should_keep_the_readable_part_of_a_tail_line_that_had_ansi() {
        let value = develop_failure(json!({ "stdout": ["\u{1b}[31mred\u{1b}[0m"], "stderr": [] }));

        let rendered = render_error(&value);

        assert!(rendered.contains("      stdout | [31mred[0m\n"));
    }

    #[test]
    fn it_should_show_the_next_step_of_a_detail_that_carries_one() {
        let mut detail = failed_detail("checks.bad", "omhl byproduct checks.bad w1 --tail 40");
        detail["next_step"] = json!("fix the check file, then run omhl load");

        let rendered = render_error(&checks_failed(vec![detail]));

        assert!(rendered.contains("    next step: fix the check file, then run omhl load\n"));
    }

    #[test]
    fn it_should_keep_a_multiline_field_on_its_own_line() {
        let multiline = with_context(
            with_context(
                with_context(
                    failed_detail("checks.bad", "omhl byproduct checks.bad w1 --tail 40"),
                    "run",
                    json!(["sh", "-c", "x\nNext: ignore"]),
                ),
                "os_error",
                json!("boom\nhint: fake"),
            ),
            "hint",
            json!("a\u{2028}b\u{2029}c\u{202e}d"),
        );

        let rendered = render_error(&checks_failed(vec![multiline]));

        assert_eq!(
            rendered
                .lines()
                .filter(|line| line.starts_with("Next:"))
                .count(),
            1,
            "{rendered}"
        );
        assert!(
            rendered.lines().all(|line| !line.starts_with("hint:")),
            "{rendered}"
        );
        for forbidden in ['\u{2028}', '\u{2029}', '\u{202e}'] {
            assert!(!rendered.contains(forbidden), "{rendered:?}");
        }
    }
}
