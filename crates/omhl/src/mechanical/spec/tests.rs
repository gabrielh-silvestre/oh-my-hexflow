use crate::mechanical::spec::{
    CheckSpec, Outcome, SpecError, StreamPattern, is_mechanical, snapshot_for, spec_digest,
};
use crate::test_support::omhl::check_data;
use hexlog::SnapshotKind;
use serde_json::{Value, json};

fn passed_on_zero() -> Value {
    json!([{"exit": 0, "outcome": "passed"}])
}

fn run_data(event: &str, expect: Value) -> Value {
    let mut data = check_data(event, "Run the linter.", true);
    data["run"] = json!(["cargo", "clippy"]);
    data["timeout_secs"] = json!(120);
    data["expect"] = expect;

    data
}

fn lint_data() -> Value {
    let mut data = check_data("before-commit", "Run clippy on the workspace.", true);
    data["concern"] = json!("style");
    data["variant"] = json!("lint");
    data["run"] = json!([
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked"
    ]);
    data["timeout_secs"] = json!(600);
    data["expect"] = json!([
        {"exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning"},
        {"exit": 0, "outcome": "passed"},
    ]);

    data
}

fn units(data: &Value) -> usize {
    hexlog::chain::canonical(data)
        .expect("canonical json")
        .encode_utf16()
        .count()
}

fn big_data(total_units: usize) -> Value {
    let mut data = check_data("before-pr", &"h".repeat(8_000), true);
    data["run"] = json!(vec!["r".repeat(1_000); 5]);
    data["timeout_secs"] = json!(600);
    data["expect"] = json!([{"exit": 0, "stdout": "", "outcome": "passed"}]);
    data["expect"][0]["stdout"] = json!("p".repeat(total_units - units(&data)));
    assert_eq!(units(&data), total_units, "big_data size");

    data
}

fn error(path: &str, code: &'static str) -> SpecError {
    SpecError {
        path: path.to_owned(),
        code,
    }
}

fn run_with_last_item(last: &str) -> Value {
    let mut data = run_data("before-pr", passed_on_zero());
    data["run"] = json!(vec!["r".repeat(1_000); 5]);
    data["run"]
        .as_array_mut()
        .expect("run is a list")
        .push(json!(last));

    data
}

#[test]
fn it_should_decode_a_text_only_check_as_no_spec() {
    let data = check_data("before-commit", "Review the diff by hand.", true);

    let result = CheckSpec::from_data(&data);

    assert!(matches!(result, Ok(None)));
}

#[test]
fn it_should_reject_a_run_on_an_event_that_no_flag_or_hook_can_run() {
    let deploy = run_data("before-deploy", passed_on_zero());
    let review = run_data("before-publishing-review-comments", passed_on_zero());

    let deploy_result = CheckSpec::from_data(&deploy);
    let review_result = CheckSpec::from_data(&review);

    assert_eq!(
        deploy_result.unwrap_err(),
        error("/when/event", "event-unsupported")
    );
    assert_eq!(
        review_result.unwrap_err(),
        error("/when/event", "event-unsupported")
    );
}

#[test]
fn it_should_decode_a_text_only_check_on_the_native_review_event_as_no_spec() {
    let data = check_data(
        "before-publishing-review-comments",
        "Verify the anchors.",
        true,
    );

    let result = CheckSpec::from_data(&data);

    assert!(matches!(result, Ok(None)));
}

#[test]
fn it_should_decode_the_lint_check_of_the_report() {
    let data = lint_data();

    let spec = CheckSpec::from_data(&data)
        .expect("decodes")
        .expect("has run");

    assert_eq!(
        spec.run,
        [
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked"
        ]
    );
    assert_eq!(spec.timeout_secs, 600);
    assert_eq!(spec.expect.len(), 2);
    assert_eq!(spec.expect[0].exit, 0);
    assert!(matches!(
        &spec.expect[0].pattern,
        Some(StreamPattern::Stderr(r)) if r.as_str() == "(?m)^warning"
    ));
    assert_eq!(spec.expect[0].outcome, Outcome::PassedWithWarning);
    assert_eq!(spec.expect[1].exit, 0);
    assert!(spec.expect[1].pattern.is_none());
    assert_eq!(spec.expect[1].outcome, Outcome::Passed);
}

#[test]
fn it_should_reject_a_run_with_an_empty_argv() {
    let mut data = run_data("before-pr", passed_on_zero());
    data["run"] = json!([]);

    let result = CheckSpec::from_data(&data);

    assert_eq!(result.unwrap_err(), error("/run", "run-empty"));
}

#[test]
fn it_should_reject_a_timeout_of_zero() {
    let mut data = run_data("before-pr", passed_on_zero());
    data["timeout_secs"] = json!(0);

    let result = CheckSpec::from_data(&data);

    assert_eq!(
        result.unwrap_err(),
        error("/timeout_secs", "timeout-out-of-range")
    );
}

#[test]
fn it_should_reject_an_invalid_regex_without_echoing_it() {
    let data = run_data(
        "before-pr",
        json!([{"exit": 0, "stdout": "(unclosed", "outcome": "passed"}]),
    );

    let result = CheckSpec::from_data(&data);

    let spec_error = result.unwrap_err();
    assert_eq!(spec_error, error("/expect/0/stdout", "regex-invalid"));
    assert!(!format!("{spec_error:?}").contains("unclosed"));
}

#[test]
fn it_should_reject_a_regex_whose_compiled_size_exceeds_the_limit() {
    let too_big = run_data(
        "before-pr",
        json!([{"exit": 0, "stderr": "\\w{100}", "outcome": "passed"}]),
    );
    let fits = run_data(
        "before-pr",
        json!([{"exit": 0, "stderr": "\\w{10}", "outcome": "passed"}]),
    );

    let too_big_result = CheckSpec::from_data(&too_big);
    let fits_result = CheckSpec::from_data(&fits);

    assert_eq!(
        too_big_result.unwrap_err(),
        error("/expect/0/stderr", "regex-too-large")
    );
    assert!(matches!(fits_result, Ok(Some(_))));
}

#[test]
fn it_should_reject_a_rule_without_exit() {
    let data = run_data(
        "before-pr",
        json!([{"exit": 0, "outcome": "passed"}, {"outcome": "passed"}]),
    );

    let result = CheckSpec::from_data(&data);

    assert_eq!(result.unwrap_err(), error("/expect/1/exit", "exit-missing"));
}

#[test]
fn it_should_reject_a_check_whose_data_exceeds_the_aggregate_size_limit() {
    let data = big_data(15_001);

    let result = CheckSpec::from_data(&data);

    assert_eq!(result.unwrap_err(), error("", "data-too-large"));
}

#[test]
fn it_should_accept_a_check_just_under_the_aggregate_size_limit() {
    let data = big_data(15_000);

    let result = CheckSpec::from_data(&data);

    assert!(matches!(result, Ok(Some(_))));
}

#[test]
fn it_should_not_apply_the_aggregate_size_limit_to_a_text_only_check() {
    let data = check_data("before-commit", &"\"".repeat(7_600), true);

    let result = CheckSpec::from_data(&data);

    assert!(matches!(result, Ok(None)));
}

#[test]
fn it_should_reject_a_run_whose_canonical_size_exceeds_its_limit() {
    let over = run_with_last_item(&"s".repeat(982));
    let at_limit = run_with_last_item(&"s".repeat(981));

    let over_result = CheckSpec::from_data(&over);
    let at_limit_result = CheckSpec::from_data(&at_limit);

    assert_eq!(over_result.unwrap_err(), error("/run", "run-too-large"));
    assert!(matches!(at_limit_result, Ok(Some(_))));
}

#[test]
fn it_should_derive_git_tree_for_a_before_commit_check_with_run() {
    let data = run_data("before-commit", passed_on_zero());

    let kind = snapshot_for(&data);

    assert_eq!(kind, SnapshotKind::GitTree);
}

#[test]
fn it_should_derive_git_commit_for_a_before_pr_check_with_run() {
    let data = run_data("before-pr", passed_on_zero());

    let kind = snapshot_for(&data);

    assert_eq!(kind, SnapshotKind::GitCommit);
}

#[test]
fn it_should_derive_git_commit_for_a_before_merge_check_with_run() {
    let data = run_data("before-merge", passed_on_zero());

    let kind = snapshot_for(&data);

    assert_eq!(kind, SnapshotKind::GitCommit);
}

#[test]
fn it_should_derive_git_commit_for_a_before_commit_check_that_is_only_text() {
    let data = check_data("before-commit", "Review the diff by hand.", true);

    let kind = snapshot_for(&data);

    assert_eq!(kind, SnapshotKind::GitCommit);
}

#[test]
fn it_should_reject_a_rule_with_both_a_stdout_and_a_stderr_regex() {
    let data = run_data(
        "before-pr",
        json!([{"exit": 0, "stdout": "ok", "stderr": "warn", "outcome": "passed"}]),
    );

    let result = CheckSpec::from_data(&data);

    assert_eq!(result.unwrap_err(), error("/expect/0", "rule-two-streams"));
}

#[test]
fn it_should_keep_the_spec_digest_when_only_unrelated_fields_change() {
    let base = run_data("before-pr", passed_on_zero());
    let mut other = base.clone();
    other["blocking"] = json!(false);
    other["concern"] = json!("style");
    other["how"] = json!("Another text.");

    assert_eq!(spec_digest(&other), spec_digest(&base));
}

#[test]
fn it_should_change_the_spec_digest_when_run_changes() {
    let base = run_data("before-pr", passed_on_zero());
    let mut other = base.clone();
    other["run"] = json!(["true"]);

    assert_ne!(spec_digest(&other), spec_digest(&base));
}

#[test]
fn it_should_change_the_spec_digest_when_timeout_secs_changes() {
    let base = run_data("before-pr", passed_on_zero());
    let mut other = base.clone();
    other["timeout_secs"] = json!(121);

    assert_ne!(spec_digest(&other), spec_digest(&base));
}

#[test]
fn it_should_change_the_spec_digest_when_expect_changes() {
    let base = run_data("before-pr", passed_on_zero());
    let other = run_data("before-pr", json!([{"exit": 0, "outcome": "failed"}]));

    assert_ne!(spec_digest(&other), spec_digest(&base));
}

#[test]
fn it_should_treat_a_check_as_mechanical_when_run_is_present_whatever_its_value() {
    let values = [json!(["cargo", "test"]), json!([]), Value::Null];

    for run in values {
        let data = json!({ "run": run });

        assert!(is_mechanical(&data), "run: {run}");
    }
}

#[test]
fn it_should_treat_a_check_without_run_as_text_only() {
    let data = json!({ "when": { "event": "before-pr" } });

    assert!(!is_mechanical(&data));
}
