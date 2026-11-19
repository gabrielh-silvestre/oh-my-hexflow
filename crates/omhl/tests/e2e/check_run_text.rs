use crate::support;

use std::fs;

use omhl::adapters::text::{AgentVocabulary, HumanVocabulary, SYMBOL_OK, State, Vocabulary};
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
run = ["sh", "-c", "printf 'SECRET-OUT-%s\\n' 7a1; printf 'SECRET-ERR-%s\\n' 7a1 >&2; exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const SUITE_RED: &str = r#"blocking = true
how = "Runs the suite."
concern = "test"
variant = "suite"
run = ["sh", "-c", "exit 101"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const FORMAT_GREEN: &str = r#"blocking = true
how = "Formats the code."
concern = "style"
variant = "format"
run = ["true"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const SOFT_RED: &str = r#"blocking = false
how = "Fails without blocking."
run = ["sh", "-c", "exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const SLOW: &str = r#"blocking = true
how = "Never ends in time."
run = ["sleep", "5"]
timeout_secs = 1
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

/// Apaga o arquivo do check e commita: o check segue no banco, órfão, com a árvore limpa.
fn drop_check(p: &Project, slug: &str) {
    fs::remove_file(p.dir().join(format!(".omhl/checks/{slug}.toml")))
        .expect("the check file is deleted");
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "drop check"]);
}

/// O `error` da linha JSON de falha.
fn failure_error(stderr: &str) -> Value {
    let mut value: Value = serde_json::from_str(stderr.trim_end()).expect("stderr is JSON");

    value["error"].take()
}

fn run_pr(p: &Project, format: &str) -> support::child::Output {
    omhl_in(
        p,
        &["check", "run", "--event", "before-pr", "--format", format],
    )
    .run()
}

fn without_context(mut error: Value) -> Value {
    error
        .as_object_mut()
        .expect("error is an object")
        .remove("context");
    for detail in error["details"].as_array_mut().expect("details") {
        detail
            .as_object_mut()
            .expect("detail is an object")
            .remove("context");
    }

    error
}

#[test]
fn it_should_keep_every_pre_existing_failure_json_field_unchanged_with_format_json() {
    let p = cli_repo(&[("red", NOISY_RED)]);

    let output = run_pr(&p, "json");

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert_eq!(
        without_context(failure_error(&output.stderr)),
        json!({
            "code": "checks-failed",
            "message": "a blocking check failed",
            "details": [{
                "code": "failed",
                "message": "omhl byproduct checks.red work-items.w1 --tail 40",
                "path": "checks.red",
            }],
        })
    );
}

#[test]
fn it_should_carry_the_exit_code_and_run_fields_of_a_failed_check_in_the_failure_json() {
    let p = cli_repo(&[("suite", SUITE_RED)]);

    let output = run_pr(&p, "json");

    let context = &failure_error(&output.stderr)["details"][0]["context"];
    assert_eq!(context["blocking"], json!(true));
    assert_eq!(context["concern"], json!("test"));
    assert_eq!(context["variant"], json!("suite"));
    assert_eq!(context["run"], json!(["sh", "-c", "exit 101"]));
    assert_eq!(context["timeout_secs"], json!(60));
    assert_eq!(context["exit_code"], json!(101));
    assert!(context["duration_ms"].is_u64(), "{context}");
    assert_eq!(context["timed_out"], json!(false));
    assert_eq!(context["truncated"], json!(false));
}

#[test]
fn it_should_carry_the_event_work_item_and_summary_once_in_the_failure_json() {
    let p = cli_repo(&[("suite", SUITE_RED), ("green", GREEN)]);

    let output = run_pr(&p, "json");

    assert_eq!(
        failure_error(&output.stderr)["context"],
        json!({
            "event": "before-pr",
            "work_item": "work-items.w1",
            "summary": { "planned": 2, "passed": 1, "failed": 1, "blocking_failed": 1, "not_run": 0 },
        })
    );
}

#[test]
fn it_should_count_blocking_failures_apart_in_the_failure_json_summary() {
    let p = cli_repo(&[("suite", SUITE_RED), ("soft", SOFT_RED), ("green", GREEN)]);

    let output = run_pr(&p, "json");

    assert_eq!(
        failure_error(&output.stderr)["context"]["summary"],
        json!({ "planned": 3, "passed": 1, "failed": 2, "blocking_failed": 1, "not_run": 0 })
    );
}

#[test]
fn it_should_split_the_failure_text_header_between_blocking_and_non_blocking() {
    let p = cli_repo(&[("suite", SUITE_RED), ("soft", SOFT_RED), ("green", GREEN)]);

    let output = run_pr(&p, "agent");

    assert_eq!(
        output.stderr.lines().next(),
        Some(
            "✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 1 non-blocking failed, 1 passed, 0 not run"
        )
    );
    assert!(output.stderr.contains(
        "    how to read the output: omhl byproduct checks.soft work-items.w1 --tail 40"
    ));
}

#[test]
fn it_should_leave_exit_code_out_of_the_failure_json_when_the_check_timed_out() {
    let p = cli_repo(&[("slow", SLOW)]);

    let output = run_pr(&p, "json");

    let context = &failure_error(&output.stderr)["details"][0]["context"];
    assert_eq!(context["timed_out"], json!(true));
    assert_eq!(context["timeout_secs"], json!(1));
    assert!(
        context.get("exit_code").is_none(),
        "no exit_code: {context}"
    );
}

#[test]
fn it_should_count_an_orphaned_check_as_not_run_in_the_failure_summary() {
    let p = cli_repo(&[("suite", SUITE_RED), ("gone", GREEN)]);
    drop_check(&p, "gone");

    let output = run_pr(&p, "json");

    assert_eq!(
        failure_error(&output.stderr)["context"]["summary"],
        json!({ "planned": 2, "passed": 0, "failed": 1, "blocking_failed": 1, "not_run": 1 })
    );
}

#[test]
fn it_should_leave_work_item_out_of_the_failure_json_on_develop() {
    let p = cli_repo(&[("suite", SUITE_RED)]);
    git(&p, &["switch", "-q", "-c", "develop"]);

    let output = run_pr(&p, "json");

    let error = failure_error(&output.stderr);
    assert_eq!(error["context"]["event"], json!("before-pr"));
    assert!(
        error["context"].get("work_item").is_none(),
        "no work_item: {error}"
    );
}

#[test]
fn it_should_open_the_failure_text_with_event_work_item_and_summary() {
    let p = cli_repo(&[("suite", SUITE_RED), ("gone", GREEN)]);
    drop_check(&p, "gone");

    let output = run_pr(&p, "agent");

    assert_eq!(
        output.stderr.lines().next(),
        Some("✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 0 passed, 1 not run")
    );
}

#[test]
fn it_should_open_the_failure_text_without_a_work_item_on_develop() {
    let p = cli_repo(&[("suite", SUITE_RED)]);
    git(&p, &["switch", "-q", "-c", "develop"]);

    let output = run_pr(&p, "agent");

    assert_eq!(
        output.stderr.lines().next(),
        Some("✘ BLOCKED · before-pr · no work item · 1 blocking failed, 0 passed, 0 not run")
    );
}

#[test]
fn it_should_describe_a_failed_check_in_the_text_with_its_exit_code_and_command() {
    let p = cli_repo(&[("suite", SUITE_RED)]);

    let output = run_pr(&p, "agent");

    let lines: Vec<&str> = output.stderr.lines().collect();
    assert!(
        lines[1]
            .starts_with("  ✘ checks.suite · test/suite · blocking · timeout 60s · exit 101 · "),
        "{lines:?}"
    );
    assert_eq!(lines[2], "    command: sh -c exit 101");
}

#[test]
fn it_should_describe_a_timed_out_check_in_the_text_without_an_exit_code() {
    let p = cli_repo(&[("slow", SLOW)]);

    let output = run_pr(&p, "agent");

    let line = output.stderr.lines().nth(1).expect("the failed check line");
    assert!(
        line.starts_with("  ✘ checks.slow · blocking · timeout 1s · timed out · "),
        "{line}"
    );
    assert!(!line.contains("exit"), "{line}");
}

#[test]
fn it_should_carry_concern_and_variant_in_each_success_result_that_declares_them() {
    let p = cli_repo(&[("format", FORMAT_GREEN), ("green", GREEN)]);

    let output = run_pr(&p, "json");

    let results = &normalized(&output.stdout)["results"];
    assert_eq!(results[0]["concern"], json!("style"));
    assert_eq!(results[0]["variant"], json!("format"));
    assert!(results[1].get("concern").is_none(), "{}", results[1]);
}

#[test]
fn it_should_show_concern_and_variant_next_to_the_id_in_the_success_text() {
    let p = cli_repo(&[("format", FORMAT_GREEN)]);

    let output = run_pr(&p, "agent");

    let line = output.stdout.lines().nth(1).expect("the check line");
    assert!(
        line.starts_with("  ✔ checks.format style/format passed "),
        "{line}"
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
fn it_should_accept_the_three_known_formats() {
    let p = cli_repo(&[("green", GREEN)]);

    let codes: Vec<Option<i32>> = ["agent", "human", "json"]
        .map(|format| run_pr(&p, format).code)
        .to_vec();

    assert_eq!(codes, vec![Some(0); 3]);
}

#[test]
fn it_should_reject_the_removed_text_format_with_a_usage_exit() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = run_pr(&p, "text");

    assert_eq!(output.code, Some(2));
    assert_eq!(output.stdout, "");
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
        lines[1].starts_with("  ✔ checks.green passed "),
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
        output.stderr.starts_with("✘ BLOCKED · before-pr · "),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("  ✘ checks.red · blocking · timeout 60s · exit 1 · "),
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
fn it_should_print_the_output_tail_of_a_failed_check_on_develop() {
    let p = cli_repo(&[("red", NOISY_RED)]);
    git(&p, &["switch", "-q", "-c", "develop"]);

    let output = run_pr(&p, "agent");

    assert_eq!(output.code, Some(1));
    assert!(
        output.stderr.contains(
            "    output tail:\n      stdout | SECRET-OUT-7a1\n      stderr | SECRET-ERR-7a1\n"
        ),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_warn_in_the_success_text_that_an_orphaned_check_did_not_run() {
    let p = cli_repo(&[("green", GREEN), ("gone", GREEN)]);
    drop_check(&p, "gone");

    let output = run_pr(&p, "agent");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        output
            .stdout
            .contains("⚠ check-orphaned: checks.gone did not run. the check file is missing in this working tree; "),
        "{}",
        output.stdout
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

const WARNING: &str = r#"blocking = true
how = "Exits 1, which the expectation accepts with a warning."
run = ["sh", "-c", "exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }, { exit = 1, outcome = "passed_with_warning" }]

[when]
event = "before-pr"
"#;

const MANUAL: &str = r#"blocking = false
how = "Review it by hand."

[when]
event = "before-merge"
"#;

const GHOST: &str = r#"blocking = true
how = "The binary does not exist."
run = ["omhl-ghost-binary"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const STAGER: &str = r#"blocking = true
how = "Stages a file behind the snapshot."
run = ["sh", "-c", "echo n > n && git add n"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-commit"
"#;

/// Saída 1, stdout vazio, o código do erro e o do detalhe escritos no stderr, e sem controle.
fn assert_text_refusal(output: &support::child::Output, code: &str, detail_code: &str) {
    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    assert!(
        output.stderr.starts_with(&format!("✘ {code} · ")),
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains(detail_code), "{}", output.stderr);
    assert!(!output.stderr.contains('\u{1b}'), "{:?}", output.stderr);
}

#[test]
fn it_should_tell_a_passed_with_warning_check_from_a_passed_one_in_the_success_text() {
    let p = cli_repo(&[("warn", WARNING), ("green", GREEN)]);

    let output = run_pr(&p, "agent");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");
    let lines: Vec<&str> = output.stdout.lines().collect();
    assert!(
        lines[1].starts_with("  ✔ checks.green passed "),
        "{lines:?}"
    );
    assert!(
        lines[2].starts_with("  ⚠ checks.warn passed_with_warning "),
        "{lines:?}"
    );
    assert!(lines[2].ends_with("ms (exit 1)"), "{lines:?}");
}

#[test]
fn it_should_print_check_not_found_as_text_for_an_unknown_check() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(&p, &["check", "run", "--check", "checks.nope"]).run();

    assert_text_refusal(&output, "check-not-found", "check-not-found");
}

#[test]
fn it_should_print_check_not_mechanical_as_text_for_a_check_without_a_command() {
    let p = cli_repo(&[("review", MANUAL)]);

    let output = omhl_in(&p, &["check", "run", "--check", "checks.review"]).run();

    assert_text_refusal(&output, "check-not-mechanical", "check-not-mechanical");
}

#[test]
fn it_should_print_spec_stale_as_text_when_the_committed_file_differs_from_the_stored_definition() {
    let p = cli_repo(&[("green", GREEN)]);
    ExtTree::at(p.dir()).check(
        "green",
        &GREEN.replace(r#"run = ["true"]"#, r#"run = ["false"]"#),
    );
    git(&p, &["add", "-A"]);
    git(&p, &["commit", "-q", "-m", "edit green"]);

    let output = omhl_in(&p, &["check", "run", "--event", "before-pr"]).run();

    assert_text_refusal(&output, "check-not-run", "spec-stale");
    assert!(output.stderr.contains("omhl load"), "{}", output.stderr);
}

#[test]
fn it_should_print_no_marker_as_text_outside_a_marked_project() {
    let bare = tempfile::TempDir::new().expect("bare directory");
    let xdg = tempfile::TempDir::new().expect("data directory");

    let output = omhl(&["check", "run", "--event", "before-pr"])
        .cwd(bare.path())
        .xdg(xdg.path())
        .run();

    assert_text_refusal(&output, "no-marker", "rebase");
}

#[test]
fn it_should_print_push_input_invalid_as_text_for_a_malformed_pre_push_line() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(&p, &["check", "run", "--event", "before-pr", "--pre-push"])
        .stdin("garbage\n")
        .run();

    assert_text_refusal(&output, "push-input-invalid", "push-input-invalid");
}

#[test]
fn it_should_print_worktree_dirty_as_text_for_an_untracked_file() {
    let p = cli_repo(&[("green", GREEN)]);
    fs::write(p.dir().join("u.txt"), "u\n").expect("untracked file");

    let output = run_pr(&p, "agent");

    assert_text_refusal(&output, "worktree-dirty", "worktree-dirty");
}

#[test]
fn it_should_print_branch_protected_as_text_on_main() {
    let p = cli_repo(&[("green", GREEN)]);
    git(&p, &["switch", "-q", "main"]);

    let output = run_pr(&p, "agent");

    assert_text_refusal(&output, "branch-protected", "branch-protected");
}

#[test]
fn it_should_print_check_not_run_as_text_when_the_binary_of_the_check_does_not_exist() {
    let p = cli_repo(&[("ghost", GHOST)]);

    let output = run_pr(&p, "agent");

    assert_text_refusal(&output, "check-not-run", "spawn-failed");
    assert!(
        output.stderr.contains("kind: spawn-not-found"),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_print_attestation_refused_as_text_when_a_check_stages_a_file_behind_the_snapshot() {
    let p = cli_repo(&[("stager", STAGER)]);

    let output = omhl_in(&p, &["check", "run", "--event", "before-commit"]).run();

    assert_text_refusal(&output, "attestation-refused", "snapshot-tree-mismatch");
}

const SPACED_RED: &str = r#"blocking = true
how = "Prints a spaced line and fails."
run = ["sh", "-c", "printf '  a   b  \\n'; exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

#[test]
fn it_should_keep_the_interior_spaces_of_the_command_output_in_the_agent_text() {
    let p = cli_repo(&[("spaced", SPACED_RED)]);
    git(&p, &["switch", "-q", "-c", "develop"]);

    let output = run_pr(&p, "agent");

    assert_eq!(output.code, Some(1));
    assert!(
        output.stderr.contains("      stdout |   a   b\n"),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_trim_only_the_end_of_the_command_output_line_in_the_agent_text() {
    let p = cli_repo(&[("spaced", SPACED_RED)]);
    git(&p, &["switch", "-q", "-c", "develop"]);

    let output = run_pr(&p, "agent");

    assert!(
        output.stderr.lines().all(|line| line == line.trim_end()),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_keep_the_command_output_line_untouched_in_the_human_text() {
    let p = cli_repo(&[("spaced", SPACED_RED)]);
    git(&p, &["switch", "-q", "-c", "develop"]);

    let output = run_pr(&p, "human");

    assert_eq!(output.code, Some(1));
    assert!(
        output.stderr.contains("      stdout |   a   b  \n"),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_keep_the_two_space_gap_between_the_check_columns_in_the_human_text() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = run_pr(&p, "human");

    assert!(
        output
            .stdout
            .contains(&format!("\n  {SYMBOL_OK} checks.green  passed  ")),
        "{}",
        output.stdout
    );
}

#[test]
fn it_should_open_the_success_text_of_both_layouts_with_the_ok_symbol_of_the_vocabulary() {
    let p = cli_repo(&[("green", GREEN)]);
    let symbol = AgentVocabulary.symbol(State::Ok);

    let agent = run_pr(&p, "agent");
    let human = run_pr(&p, "human");

    assert!(agent.stdout.starts_with(&format!("{symbol} before-pr")));
    assert!(human.stdout.starts_with(&format!("{symbol} before-pr")));
}

#[test]
fn it_should_open_the_failure_text_of_both_layouts_with_the_error_symbol_of_the_vocabulary() {
    let p = cli_repo(&[("red", NOISY_RED)]);
    let symbol = HumanVocabulary.symbol(State::Error);

    let agent = run_pr(&p, "agent");
    let human = run_pr(&p, "human");

    assert!(agent.stderr.starts_with(&format!("{symbol} BLOCKED")));
    assert!(human.stderr.starts_with(&format!("{symbol} BLOCKED")));
}
