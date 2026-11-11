mod support;

use ::omhl::mechanical::plan::{CheckResult, RunSummary, exit_blocked, summarize};
use ::omhl::mechanical::spec::Outcome;
use support::sid as id;

fn result(id_text: &str, outcome: Outcome, blocking: bool) -> CheckResult {
    CheckResult {
        id: id(id_text),
        outcome,
        blocking,
    }
}

#[test]
fn it_should_block_the_exit_when_a_blocking_check_failed() {
    let results = [
        result("checks.a", Outcome::Passed, true),
        result("checks.b", Outcome::Failed, true),
        result("checks.c", Outcome::Failed, false),
    ];

    let blocked = exit_blocked(&summarize(&results));

    assert!(blocked);
}

#[test]
fn it_should_not_block_the_exit_when_only_a_non_blocking_check_failed() {
    let results = [
        result("checks.a", Outcome::Passed, true),
        result("checks.b", Outcome::Failed, false),
    ];

    let blocked = exit_blocked(&summarize(&results));

    assert!(!blocked);
}

#[test]
fn it_should_not_block_the_exit_when_a_check_passed_with_warning() {
    let results = [result("checks.a", Outcome::PassedWithWarning, true)];

    let blocked = exit_blocked(&summarize(&results));

    assert!(!blocked);
}

#[test]
fn it_should_list_every_check_that_did_not_pass_in_the_summary() {
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
            not_passed: vec![id("checks.b"), id("checks.d")],
        }
    );
}

#[test]
fn it_should_not_block_when_no_check_ran() {
    let summary = summarize(&[]);

    assert_eq!(
        summary,
        RunSummary {
            blocked: vec![],
            not_passed: vec![],
        }
    );
    assert!(!exit_blocked(&summary));
}
