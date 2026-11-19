use crate::mechanical::plan::{
    CheckNode, CheckResult, PlannedCheck, RunSummary, plan_run, summarize,
};
use crate::mechanical::spec::Outcome;
use crate::test_support::omhl::check_data;
use crate::test_support::sid as id;
use hexlog::{SemanticId, SnapshotKind};
use serde_json::{Value, json};

const LINT_RUN: [&str; 5] = ["cargo", "clippy", "--", "-D", "warnings"];

fn run_data(event: &str, blocking: bool) -> Value {
    let mut data = check_data(event, "Runs the linter.", blocking);
    data["run"] = json!(LINT_RUN);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);

    data
}

// Regra com stdout e stderr juntos: o `from_data` recusa com "rule-two-streams".
fn two_streams_data(event: &str, blocking: bool) -> Value {
    let mut data = run_data(event, blocking);
    data["expect"] = json!([{ "exit": 0, "stdout": "ok", "stderr": "warn", "outcome": "passed" }]);

    data
}

fn text_data(event: &str, blocking: bool) -> Value {
    check_data(event, "Review the diff by hand.", blocking)
}

fn node(id_text: &str, data: Value, shadowed: bool) -> CheckNode {
    CheckNode {
        id: id(id_text),
        data,
        shadowed,
    }
}

fn result(id_text: &str, outcome: Outcome, blocking: bool) -> CheckResult {
    CheckResult {
        id: id(id_text),
        outcome,
        blocking,
    }
}

fn plan_ids(plan: &[PlannedCheck]) -> Vec<SemanticId> {
    plan.iter().map(|planned| planned.id.clone()).collect()
}

#[test]
fn it_should_select_only_checks_of_the_event() {
    let nodes = [
        node("checks.a", run_data("before-commit", true), false),
        node("checks.b", run_data("before-pr", true), false),
        node("checks.c", run_data("before-merge", true), false),
    ];

    let plan = plan_run("before-pr", &nodes, &[]);

    assert_eq!(plan_ids(&plan), [id("checks.b")]);
}

#[test]
fn it_should_skip_text_only_checks() {
    let nodes = [
        node("checks.a", text_data("before-pr", true), false),
        node("checks.b", run_data("before-pr", true), false),
    ];

    let plan = plan_run("before-pr", &nodes, &[]);

    assert_eq!(plan_ids(&plan), [id("checks.b")]);
}

#[test]
fn it_should_skip_shadowed_checks() {
    let nodes = [
        node("checks.a", run_data("before-pr", true), true),
        node("checks.b", run_data("before-pr", true), false),
    ];

    let plan = plan_run("before-pr", &nodes, &[]);

    assert_eq!(plan_ids(&plan), [id("checks.b")]);
}

#[test]
fn it_should_keep_the_blocking_flag_per_planned_check() {
    let nodes = [
        node("checks.a", run_data("before-pr", true), false),
        node("checks.b", run_data("before-pr", false), false),
    ];

    let plan = plan_run("before-pr", &nodes, &[]);

    let flags: Vec<_> = plan
        .iter()
        .map(|planned| (planned.id.clone(), planned.blocking))
        .collect();
    assert_eq!(flags, [(id("checks.a"), true), (id("checks.b"), false)]);
}

#[test]
fn it_should_carry_the_derived_snapshot_kind_in_each_planned_check() {
    let commit_nodes = [
        node("checks.a", run_data("before-commit", true), false),
        node("checks.b", two_streams_data("before-commit", true), false),
    ];
    let pr_nodes = [node("checks.c", run_data("before-pr", true), false)];

    let commit_plan = plan_run("before-commit", &commit_nodes, &[]);
    let pr_plan = plan_run("before-pr", &pr_nodes, &[]);

    let snapshots = |plan: &[PlannedCheck]| -> Vec<(SemanticId, SnapshotKind)> {
        plan.iter()
            .map(|planned| (planned.id.clone(), planned.snapshot))
            .collect()
    };
    assert_eq!(
        snapshots(&commit_plan),
        [
            (id("checks.a"), SnapshotKind::GitTree),
            (id("checks.b"), SnapshotKind::GitTree)
        ]
    );
    assert_eq!(
        snapshots(&pr_plan),
        [(id("checks.c"), SnapshotKind::GitCommit)]
    );
}

#[test]
fn it_should_keep_a_check_whose_stored_spec_is_invalid_in_the_plan_with_its_error() {
    let nodes = [
        node("checks.a", run_data("before-pr", true), false),
        node("checks.b", two_streams_data("before-pr", true), false),
        node("checks.c", run_data("before-pr", true), false),
    ];
    let after = [(id("checks.a"), id("checks.c"))];

    let plan = plan_run("before-pr", &nodes, &after);

    assert_eq!(
        plan_ids(&plan),
        [id("checks.b"), id("checks.c"), id("checks.a")]
    );
    let invalid = &plan[0];
    assert_eq!(
        invalid
            .spec
            .as_ref()
            .expect_err("stored spec is invalid")
            .code,
        "rule-two-streams"
    );
    assert!(invalid.blocking);
    for valid in &plan[1..] {
        let spec = valid.spec.as_ref().expect("stored spec is valid");
        assert_eq!(spec.run, LINT_RUN);
        assert_eq!(spec.timeout_secs, 60);
    }
}

#[test]
fn it_should_block_the_exit_when_a_blocking_check_failed() {
    let results = [
        result("checks.a", Outcome::Passed, true),
        result("checks.b", Outcome::Failed, true),
        result("checks.c", Outcome::Failed, false),
    ];

    let blocked = summarize(&results).blocked;

    assert_eq!(blocked, vec![id("checks.b")]);
}

#[test]
fn it_should_not_block_the_exit_when_only_a_non_blocking_check_failed() {
    let results = [
        result("checks.a", Outcome::Passed, true),
        result("checks.b", Outcome::Failed, false),
    ];

    let blocked = summarize(&results).blocked;

    assert!(blocked.is_empty());
}

#[test]
fn it_should_not_block_the_exit_when_a_check_passed_with_warning() {
    let results = [result("checks.a", Outcome::PassedWithWarning, true)];

    let blocked = summarize(&results).blocked;

    assert!(blocked.is_empty());
}

#[test]
fn it_should_list_only_the_blocking_checks_that_failed_in_the_summary() {
    let results = [
        result("checks.a", Outcome::Passed, true),
        result("checks.b", Outcome::Failed, false),
        result("checks.c", Outcome::PassedWithWarning, true),
        result("checks.d", Outcome::Failed, true),
    ];

    let summary = summarize(&results);

    assert_eq!(
        summary,
        RunSummary {
            blocked: vec![id("checks.d")],
        }
    );
}

#[test]
fn it_should_not_block_when_no_check_ran() {
    let summary = summarize(&[]);

    assert_eq!(summary, RunSummary { blocked: vec![] });
}
