mod support;

use serde_json::{Value, json};
use support::child::{Cmd, omhl};
use support::ext::ExtTree;
use support::git::git_in;
use support::project::Project;

const WORK_ITEM_APPEND: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.w1","type":"work-item","data":{"title":"t","status":"open"}}}]}"#;

const GREEN: &str = r#"blocking = true
how = "Always passes."
run = ["true"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const NOISY_RED: &str = r#"blocking = true
how = "Always fails."
run = ["sh", "-c", "printf 'SECRET-OUT-7a1\\n'; printf 'SECRET-ERR-7a1\\n' >&2; exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

fn git(p: &Project, args: &[&str]) {
    let output = git_in(p.dir(), args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn omhl_in(p: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(p.dir()).xdg(p.xdg())
}

/// Repositório git limpo na branch `feat/wi-w1__x`, com o work item `w1` e os checks carregados.
fn cli_repo(checks: &[(&str, &str)]) -> Project {
    let p = Project::new("acme");
    git(&p, &["init", "-q", "-b", "main"]);
    git(&p, &["config", "user.name", "Test Author"]);
    git(&p, &["config", "user.email", "author@example.com"]);
    let tree = ExtTree::at(p.dir());
    for (slug, toml) in checks {
        tree.check(slug, toml);
    }
    git(&p, &["add", "-A"]);
    git(&p, &["commit", "-q", "-m", "base"]);
    git(&p, &["switch", "-q", "-c", "feat/wi-w1__x"]);

    let appended = omhl_in(&p, &["append"]).stdin(WORK_ITEM_APPEND).run();
    assert_eq!(appended.code, Some(0), "append stderr: {}", appended.stderr);
    let loaded = omhl_in(&p, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);

    p
}

/// A linha JSON com cada `results[i].duration_ms` conferido como inteiro e trocado por `0`.
fn normalized(line: &str) -> Value {
    let mut value: Value = serde_json::from_str(line.trim_end()).expect("the line is JSON");
    if let Some(results) = value["results"].as_array_mut() {
        for result in results {
            assert!(result["duration_ms"].is_u64(), "integer duration: {result}");
            result["duration_ms"] = json!(0);
        }
    }
    value
}

#[test]
fn it_should_keep_the_success_json_line_unchanged_with_format_json() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--event", "before-pr", "--format", "json"],
    )
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout.lines().count(),
        1,
        "stdout: {}",
        output.stdout
    );
    assert_eq!(
        normalized(&output.stdout)["results"],
        json!([{
            "check": "checks.green", "outcome": "passed", "ok": true, "blocking": true,
            "exit_code": 0, "duration_ms": 0, "timed_out": false, "truncated": false,
        }])
    );
}

#[test]
fn it_should_keep_the_failure_json_line_unchanged_with_format_json() {
    let p = cli_repo(&[("red", NOISY_RED)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--event", "before-pr", "--format", "json"],
    )
    .run();

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert_eq!(
        serde_json::from_str::<Value>(output.stderr.trim_end()).expect("stderr is JSON"),
        json!({"error": {
            "code": "checks-failed",
            "message": "a blocking check failed",
            "details": [{
                "code": "failed",
                "message": "omhl byproduct checks.red work-items.w1 --tail 40",
                "path": "checks.red",
            }],
        }})
    );
}

#[test]
fn it_should_keep_the_warning_json_line_unchanged_with_format_json() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(
        &p,
        &[
            "check",
            "run",
            "--event",
            "before-merge",
            "--format",
            "json",
        ],
    )
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        normalized(&output.stdout),
        json!({
            "blocked": [], "checks_planned": 0, "event": "before-merge", "recorded": true,
            "results": [], "snapshot": null, "warnings": [{ "code": "no-checks-for-event" }],
            "work_item": "work-items.w1",
        })
    );
}

#[test]
fn it_should_reject_an_unknown_format_with_a_usage_exit() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--event", "before-pr", "--format", "xml"],
    )
    .run();

    assert_eq!(output.code, Some(2));
    assert_eq!(output.stdout, "");
}

#[test]
fn it_should_accept_both_known_formats() {
    let p = cli_repo(&[("green", GREEN)]);

    let text = omhl_in(
        &p,
        &["check", "run", "--event", "before-pr", "--format", "text"],
    )
    .run();
    let json = omhl_in(
        &p,
        &["check", "run", "--event", "before-pr", "--format", "json"],
    )
    .run();

    assert_eq!((text.code, json.code), (Some(0), Some(0)));
}

#[test]
fn it_should_print_the_success_text_on_stdout_by_default() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(&p, &["check", "run", "--event", "before-pr"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");
    let lines: Vec<&str> = output.stdout.lines().collect();
    assert!(
        lines[0].starts_with("✔ before-pr · work-items.w1 · recorded (git-commit "),
        "{lines:?}"
    );
    assert!(
        lines[1].starts_with("  ✔ checks.green  passed  "),
        "{lines:?}"
    );
    assert_eq!(
        lines.last().copied(),
        Some("Nothing blocked. Next: go on with the push.")
    );
}

#[test]
fn it_should_print_the_failure_text_on_stderr_without_the_check_output() {
    let p = cli_repo(&[("red", NOISY_RED)]);

    let output = omhl_in(&p, &["check", "run", "--event", "before-pr"]).run();

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert!(
        output.stderr.starts_with("✘ BLOCKED · checks-failed · "),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("  ✘ checks.red · blocking\n"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("omhl byproduct checks.red work-items.w1 --tail 40"),
        "{}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("SECRET-OUT-7a1"),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_print_an_error_raised_before_any_check_as_text_on_stderr() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(
        &p,
        &[
            "--project-dir",
            "/nonexistent-omhl-dir",
            "check",
            "run",
            "--event",
            "before-pr",
        ],
    )
    .run();

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert!(
        output.stderr.starts_with("✘ invalid-project-dir · "),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_never_put_the_escape_byte_in_the_text_output() {
    let p = cli_repo(&[("red", NOISY_RED)]);

    let failure = omhl_in(&p, &["check", "run", "--event", "before-pr"]).run();
    let success = omhl_in(&p, &["check", "run", "--event", "before-merge"]).run();

    assert!(!failure.stderr.contains('\u{1b}'));
    assert!(!success.stdout.contains('\u{1b}'));
}
