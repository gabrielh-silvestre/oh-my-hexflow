use crate::support;

use hexlog::{Snapshot, SnapshotKind};
use omhl::facade::Omhl;
use omhl::pr_gate::GateSnapshots;
use omhl::probe::{ProbeOutcome, TreeSource};
use omhl::workflow::config::NativeConfig;
use serde_json::json;
use support::omhl::{OmhlHarness, PR_7, RUN_TESTS, acme, append_ok_as, check_data, node_item};
use support::pr_gate::{
    ANCHORS_CONFIRMED, CI_GREEN, DECISIONS_ANCHORED, NO_OPEN_CONTRADICTION, VERIFY_ANCHORING,
    all_native_enabled, attest, native_enabled, outcomes, relation_item, snapshots,
};
use support::{FakeProbe, author, loader_author, sid, snap};

const BASE_LABELS: [&str; 3] = [NO_OPEN_CONTRADICTION, DECISIONS_ANCHORED, ANCHORS_CONFIRMED];
const LINT: &str = "checks.lint-docs";
const NOTES: &str = "checks.notes";

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn at_head() -> GateSnapshots {
    snapshots(&snap('a'), &tree('c'))
}

fn harness() -> OmhlHarness {
    OmhlHarness::new(
        FakeProbe::clean(snap('a'))
            .with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree('c')))),
    )
}

fn open_pull_request(harness: &OmhlHarness) -> Omhl<'_> {
    let mut omhl = harness.open();
    support::omhl::seed_pull_request(&mut omhl);

    omhl
}

fn seed_project_check(omhl: &mut Omhl<'_>, id: &str, event: &str, blocking: bool, run: bool) {
    let mut data = check_data(event, "run it", blocking);
    if run {
        data["run"] = json!(["true"]);
        data["timeout_secs"] = json!(60);
        data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);
    }
    append_ok_as(omhl, loader_author(), vec![node_item(id, "check", data)]);
}

type Outcomes = Vec<(String, bool)>;

fn gate_outcomes(omhl: &mut Omhl<'_>, enabled: &NativeConfig) -> Outcomes {
    let report = omhl
        .evaluate_pr_gate(
            &acme(),
            &sid(PR_7),
            &at_head(),
            enabled,
            &omhl::facade::WorkRequirements::none(),
        )
        .expect("the gate is evaluated");

    outcomes(&report)
}

fn close_outcomes(omhl: &mut Omhl<'_>, enabled: &NativeConfig) -> (Outcomes, bool) {
    let closed = omhl
        .close_work_item(
            &acme(),
            &sid(PR_7),
            &at_head(),
            enabled,
            &omhl::facade::WorkRequirements::none(),
            &author(),
        )
        .expect("the work item is closed");

    (outcomes(&closed.report), closed.report.passed)
}

/// `gate-pr` e depois `close` sobre o mesmo estado: o `close` só avalia o gate e grava.
fn gate_and_close(omhl: &mut Omhl<'_>, enabled: &NativeConfig) -> [Outcomes; 2] {
    let gate = gate_outcomes(omhl, enabled);
    let (closed, _) = close_outcomes(omhl, enabled);

    [gate, closed]
}

/// Banco novo com o pull request, o `seed` aplicado e `gate-pr` seguido de `close`.
fn scenario(seed: impl FnOnce(&mut Omhl<'_>), enabled: &NativeConfig) -> [Outcomes; 2] {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    seed(&mut omhl);

    gate_and_close(&mut omhl, enabled)
}

fn labels(outcomes: &Outcomes) -> Vec<&str> {
    outcomes.iter().map(|(label, _)| label.as_str()).collect()
}

fn failing(label: &str) -> (String, bool) {
    (label.to_owned(), false)
}

// CA-005: o que `gate-pr` e `close` perguntam sobre os nativos

#[test]
fn it_should_ask_no_native_question_in_gate_pr_and_close_when_none_is_enabled() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[]));

    for asked in [gate, closed] {
        assert_eq!(labels(&asked), BASE_LABELS);
    }
}

#[test]
fn it_should_ask_only_the_run_tests_question_in_gate_pr_and_close_when_only_run_tests_is_enabled() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[RUN_TESTS]));

    for asked in [gate, closed] {
        assert_eq!(
            labels(&asked),
            [
                RUN_TESTS,
                NO_OPEN_CONTRADICTION,
                DECISIONS_ANCHORED,
                ANCHORS_CONFIRMED
            ]
        );
    }
}

#[test]
fn it_should_ask_only_the_ci_green_question_in_gate_pr_and_close_when_only_ci_green_is_enabled() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[CI_GREEN]));

    for asked in [gate, closed] {
        assert_eq!(
            labels(&asked),
            [
                CI_GREEN,
                NO_OPEN_CONTRADICTION,
                DECISIONS_ANCHORED,
                ANCHORS_CONFIRMED
            ]
        );
    }
}

fn without(asked: &Outcomes, label: &str) -> Outcomes {
    asked
        .iter()
        .filter(|(asked_label, _)| asked_label != label)
        .cloned()
        .collect()
}

fn attested_ci_green(omhl: &mut Omhl<'_>) {
    attest(omhl, &snap('a'), CI_GREEN, "passed", true);
}

#[test]
fn it_should_ask_the_same_ci_green_question_with_and_without_run_tests_enabled() {
    let both = scenario(attested_ci_green, &native_enabled(&[RUN_TESTS, CI_GREEN]));
    let alone = scenario(attested_ci_green, &native_enabled(&[CI_GREEN]));

    for (with_both, with_ci_green_only) in both.iter().zip(&alone) {
        assert_eq!(&without(with_both, RUN_TESTS), with_ci_green_only);
    }
}

fn two_project_checks(omhl: &mut Omhl<'_>) {
    seed_project_check(omhl, LINT, "before-merge", true, true);
    seed_project_check(omhl, NOTES, "before-commit", true, false);
}

#[test]
fn it_should_ask_the_same_project_check_questions_with_the_natives_enabled_and_disabled() {
    let off = scenario(two_project_checks, &native_enabled(&[]));
    let on = scenario(two_project_checks, &all_native_enabled());

    for (with_natives, without_natives) in on.iter().zip(&off) {
        let project_part = without(&without(with_natives, RUN_TESTS), CI_GREEN);
        assert_eq!(&project_part, without_natives);
    }
}

// CA-021: um check do projeto não substitui o nativo ligado

#[derive(Clone, Copy)]
enum Own {
    MechanicalBlocking,
    BlockingWithoutRun,
    NotBlocking,
}

fn seed_own(omhl: &mut Omhl<'_>, event: &str, own: Own) {
    let (blocking, run) = match own {
        Own::MechanicalBlocking => (true, true),
        Own::BlockingWithoutRun => (true, false),
        Own::NotBlocking => (false, false),
    };
    seed_project_check(omhl, "checks.own", event, blocking, run);
}

fn assert_native_beside_own(native: &str, event: &str, own: Own, own_is_asked: bool) {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    seed_own(&mut omhl, event, own);

    let [gate, closed] = gate_and_close(&mut omhl, &all_native_enabled());

    for asked in [gate, closed] {
        let labels = labels(&asked);
        assert!(labels.contains(&native), "{native} missing in {labels:?}");
        assert_eq!(labels.contains(&"checks.own"), own_is_asked, "{labels:?}");
    }
}

#[test]
fn it_should_ask_run_tests_beside_a_mechanical_blocking_before_commit_check() {
    assert_native_beside_own(RUN_TESTS, "before-commit", Own::MechanicalBlocking, true);
}

#[test]
fn it_should_ask_run_tests_beside_a_blocking_before_commit_check_without_run() {
    assert_native_beside_own(RUN_TESTS, "before-commit", Own::BlockingWithoutRun, true);
}

#[test]
fn it_should_ask_only_run_tests_beside_a_non_blocking_before_commit_check() {
    assert_native_beside_own(RUN_TESTS, "before-commit", Own::NotBlocking, false);
}

#[test]
fn it_should_ask_ci_green_beside_a_mechanical_blocking_before_merge_check() {
    assert_native_beside_own(CI_GREEN, "before-merge", Own::MechanicalBlocking, true);
}

#[test]
fn it_should_ask_ci_green_beside_a_blocking_before_merge_check_without_run() {
    assert_native_beside_own(CI_GREEN, "before-merge", Own::BlockingWithoutRun, true);
}

#[test]
fn it_should_ask_only_ci_green_beside_a_non_blocking_before_merge_check() {
    assert_native_beside_own(CI_GREEN, "before-merge", Own::NotBlocking, false);
}

#[test]
fn it_should_not_let_the_own_attestation_answer_the_native_question() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    seed_own(&mut omhl, "before-commit", Own::BlockingWithoutRun);
    attest(&mut omhl, &snap('a'), "checks.own", "passed", true);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[RUN_TESTS]));

    for asked in [gate, closed] {
        assert!(asked.contains(&failing(RUN_TESTS)), "{asked:?}");
        assert!(
            asked.contains(&("checks.own".to_owned(), true)),
            "{asked:?}"
        );
    }
}

#[test]
fn it_should_not_let_the_native_attestation_answer_the_own_question() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    seed_own(&mut omhl, "before-commit", Own::BlockingWithoutRun);
    attest(&mut omhl, &snap('a'), RUN_TESTS, "passed", true);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[RUN_TESTS]));

    for asked in [gate, closed] {
        assert!(
            asked.contains(&("checks.own".to_owned(), false)),
            "{asked:?}"
        );
        assert!(asked.contains(&(RUN_TESTS.to_owned(), true)), "{asked:?}");
    }
}

// CA-022: o `verify-review-comment-anchoring` nunca é perguntado

#[test]
fn it_should_not_ask_the_review_anchoring_check_when_it_is_the_only_one_enabled() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[VERIFY_ANCHORING]));

    for asked in [gate, closed] {
        assert_eq!(labels(&asked), BASE_LABELS);
    }
}

#[test]
fn it_should_ask_only_run_tests_and_ci_green_when_the_three_natives_are_enabled() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let [gate, closed] = gate_and_close(&mut omhl, &all_native_enabled());

    for asked in [gate, closed] {
        assert_eq!(
            labels(&asked),
            [
                RUN_TESTS,
                CI_GREEN,
                NO_OPEN_CONTRADICTION,
                DECISIONS_ANCHORED,
                ANCHORS_CONFIRMED
            ]
        );
    }
}

// CA-006: o gate aprova sem hook de git

#[test]
fn it_should_pass_the_gate_with_the_three_method_questions_when_nothing_is_enabled() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let gate = omhl
        .evaluate_pr_gate(
            &acme(),
            &sid(PR_7),
            &at_head(),
            &native_enabled(&[]),
            &omhl::facade::WorkRequirements::none(),
        )
        .expect("the gate is evaluated");
    let (closed, closed_passed) = close_outcomes(&mut omhl, &native_enabled(&[]));

    assert_eq!(labels(&outcomes(&gate)), BASE_LABELS);
    assert!(gate.passed);
    assert_eq!(labels(&closed), BASE_LABELS);
    assert!(closed_passed);
}

fn assert_native_fails_before_attest_and_passes_after(native: &'static str) {
    let enabled = native_enabled(&[native]);
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    let before = gate_outcomes(&mut omhl, &enabled);
    attest(&mut omhl, &snap('a'), native, "passed", true);
    let after = gate_outcomes(&mut omhl, &enabled);
    let (_, closed_passed) = close_outcomes(&mut omhl, &enabled);

    assert!(before.contains(&failing(native)), "{before:?}");
    assert!(after.iter().all(|(_, passed)| *passed), "{after:?}");
    assert!(closed_passed);
}

#[test]
fn it_should_fail_the_gate_before_the_run_tests_attestation_and_pass_it_after() {
    assert_native_fails_before_attest_and_passes_after(RUN_TESTS);
}

#[test]
fn it_should_fail_the_gate_before_the_ci_green_attestation_and_pass_it_after() {
    assert_native_fails_before_attest_and_passes_after(CI_GREEN);
}

#[test]
fn it_should_record_the_failed_gate_when_close_runs_with_an_enabled_native_unattested() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);

    let (asked, passed) = close_outcomes(&mut omhl, &native_enabled(&[RUN_TESTS]));

    assert!(asked.contains(&failing(RUN_TESTS)), "{asked:?}");
    assert!(!passed);
}

// CA-007: o histórico que cita um nativo desligado segue válido

fn seed_native_history(omhl: &mut Omhl<'_>) {
    for native in support::pr_gate::ALL_NATIVE_CHECKS {
        attest(omhl, &snap('a'), native, "passed", true);
    }
    seed_project_check(omhl, LINT, "before-merge", true, false);
    append_ok_as(
        omhl,
        loader_author(),
        vec![relation_item("after", LINT, CI_GREEN)],
    );
}

#[test]
fn it_should_evaluate_gate_pr_and_close_over_the_history_of_disabled_natives() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    seed_native_history(&mut omhl);

    let [gate, closed] = gate_and_close(&mut omhl, &native_enabled(&[]));

    for asked in [gate, closed] {
        assert_eq!(
            asked,
            vec![
                (LINT.to_owned(), false),
                (NO_OPEN_CONTRADICTION.to_owned(), true),
                (DECISIONS_ANCHORED.to_owned(), true),
                (ANCHORS_CONFIRMED.to_owned(), true),
            ]
        );
    }
}

#[test]
fn it_should_accept_a_new_attestation_of_a_disabled_native_without_asking_about_it() {
    let harness = harness();
    let mut omhl = open_pull_request(&harness);
    attest(&mut omhl, &snap('a'), RUN_TESTS, "passed", true);

    let asked = gate_outcomes(&mut omhl, &native_enabled(&[]));

    assert_eq!(labels(&asked), BASE_LABELS);
}
