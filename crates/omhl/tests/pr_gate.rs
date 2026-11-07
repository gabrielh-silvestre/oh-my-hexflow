mod support;

use hexlog::{FieldName, ListFilter, Scalar};
use omhl::error::{OmhlError, RuleCode};
use omhl::gate::{GateQuestion, Question};
use omhl::pr_gate::{GATE_NAME, pr_gate_questions, required_checks};
use support::omhl::{
    OmhlHarness, PR_7, RUN_TESTS, acme, append_ok, attestation_item, check_data, node_item,
    pull_request_data, refusal, retirement_item, revision_item, seed_pull_request, work_item_data,
};
use support::pr_gate::{
    CI_GREEN, DECISION, FEATURE_1, NO_OPEN_CONTRADICTION, PREMISE, attest, attest_required, close,
    closed_data, data_of, default_natives, gate_of, head_seq, native_check, outcomes,
    project_check, question, relation_retirement_item, seed_contradicted_decision, sids,
};
use support::{FakeProbe, sid, snap, tname};

fn rule(code: RuleCode, pointer: &str) -> OmhlError {
    OmhlError::rule(code, pointer)
}

fn field(name: &str) -> FieldName {
    FieldName::parse(name).expect("valid field name")
}

// Composição das perguntas e seleção dos checks (puros)

#[test]
fn it_should_build_the_pr_gate_as_one_occurred_per_required_check_then_one_no_open_contradiction() {
    let required = sids(&[RUN_TESTS, CI_GREEN]);
    let expected_occurred = |check: &str| GateQuestion {
        label: check.to_owned(),
        question: Question::Occurred {
            select: ListFilter {
                type_name: Some(tname("attestation")),
                id_prefix: None,
                snapshot: Some(snap('a')),
                where_eq: vec![
                    (field("check"), Scalar::Str(check.to_owned())),
                    (field("work_item"), Scalar::Str(PR_7.to_owned())),
                    (field("ok"), Scalar::Bool(true)),
                ],
            },
            min: 1,
        },
    };
    let expected = vec![
        expected_occurred(RUN_TESTS),
        expected_occurred(CI_GREEN),
        GateQuestion {
            label: NO_OPEN_CONTRADICTION.to_owned(),
            question: Question::NoOpenContradiction {
                of: Some(ListFilter {
                    type_name: Some(tname("decision")),
                    id_prefix: None,
                    snapshot: None,
                    where_eq: vec![(field("work_item"), Scalar::Str(PR_7.to_owned()))],
                }),
            },
        },
    ];

    let questions = pr_gate_questions(&required, &sid(PR_7), &snap('a'));

    assert_eq!(questions, expected);
}

#[test]
fn it_should_require_only_blocking_native_checks_with_before_commit_or_before_merge_events() {
    let natives = vec![
        native_check(RUN_TESTS, "before-commit", true),
        native_check(CI_GREEN, "before-merge", true),
        native_check(
            "checks.verify-review-comment-anchoring",
            "before-publishing-review-comments",
            true,
        ),
        native_check("checks.advisory", "before-commit", false),
    ];

    let required = required_checks(&natives, &[]);

    assert_eq!(required, sids(&[RUN_TESTS, CI_GREEN]));
}

#[test]
fn it_should_require_a_blocking_project_check_with_a_covered_event() {
    let project = [project_check(
        "checks.lint-docs",
        "before-merge",
        true,
        false,
    )];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(required, sids(&[RUN_TESTS, CI_GREEN, "checks.lint-docs"]));
}

#[test]
fn it_should_not_require_a_non_blocking_project_check() {
    let project = [project_check(
        "checks.lint-docs",
        "before-merge",
        false,
        false,
    )];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(required, sids(&[RUN_TESTS, CI_GREEN]));
}

#[test]
fn it_should_not_require_a_project_check_whose_event_is_not_covered() {
    let project = [project_check(
        "checks.lint-docs",
        "before-deploy",
        true,
        false,
    )];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(required, sids(&[RUN_TESTS, CI_GREEN]));
}

#[test]
fn it_should_not_require_a_shadowed_project_check() {
    // O nativo `run-tests` não é bloqueante; a sombra do projeto, bloqueante, não pode torná-lo obrigatório.
    let natives = vec![
        native_check(RUN_TESTS, "before-commit", false),
        native_check(CI_GREEN, "before-merge", true),
    ];
    let shadow = [project_check(RUN_TESTS, "before-commit", true, true)];

    let required = required_checks(&natives, &shadow);

    assert_eq!(required, sids(&[CI_GREEN]));
}

// Avaliação do gate sobre o `Hexlog` real

#[test]
fn it_should_not_require_a_retired_project_check() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let created = append_ok(
        &mut omhl,
        vec![node_item(
            "checks.lint-docs",
            "check",
            check_data("before-merge", "run it", true),
        )],
    );
    let while_active = gate_of(&mut omhl, &snap('a')).questions.len();
    append_ok(
        &mut omhl,
        vec![retirement_item("checks.lint-docs", &created.records[0].id)],
    );

    let after_retirement = gate_of(&mut omhl, &snap('a')).questions.len();

    assert_eq!((while_active, after_retirement), (4, 3));
}

#[test]
fn it_should_pass_when_every_required_check_has_an_ok_attestation_at_the_snapshot() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(&mut omhl, &snap('a'), RUN_TESTS, "passed", true);
    attest(&mut omhl, &snap('a'), CI_GREEN, "passed_with_warning", true);

    let report = gate_of(&mut omhl, &snap('a'));

    assert!(report.passed);
    assert_eq!(report.gate, GATE_NAME);
    assert_eq!(
        outcomes(&report),
        vec![
            (RUN_TESTS.to_owned(), true),
            (CI_GREEN.to_owned(), true),
            (NO_OPEN_CONTRADICTION.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_fail_and_name_the_check_without_an_attestation() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(&mut omhl, &snap('a'), CI_GREEN, "passed", true);

    let report = gate_of(&mut omhl, &snap('a'));

    assert!(!report.passed);
    assert!(!question(&report, RUN_TESTS).passed);
}

#[test]
fn it_should_fail_when_the_attestation_belongs_to_another_snapshot() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('b')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('b'));

    let report = gate_of(&mut omhl, &snap('a'));

    assert_eq!(
        outcomes(&report),
        vec![
            (RUN_TESTS.to_owned(), false),
            (CI_GREEN.to_owned(), false),
            (NO_OPEN_CONTRADICTION.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_fail_when_an_attestation_failed() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(&mut omhl, &snap('a'), RUN_TESTS, "failed", false);
    attest(&mut omhl, &snap('a'), CI_GREEN, "passed", true);

    let report = gate_of(&mut omhl, &snap('a'));

    assert!(!report.passed);
    assert!(!question(&report, RUN_TESTS).passed);
}

#[test]
fn it_should_fail_while_a_decision_of_the_work_item_is_contradicted() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    seed_contradicted_decision(&mut omhl);

    let report = gate_of(&mut omhl, &snap('a'));

    assert_eq!(
        outcomes(&report),
        vec![
            (RUN_TESTS.to_owned(), true),
            (CI_GREEN.to_owned(), true),
            (NO_OPEN_CONTRADICTION.to_owned(), false),
        ]
    );
    assert_eq!(
        question(&report, NO_OPEN_CONTRADICTION).failing,
        sids(&[DECISION])
    );
}

#[test]
fn it_should_pass_again_after_the_contradiction_is_retired() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    let contradiction = seed_contradicted_decision(&mut omhl);
    append_ok(
        &mut omhl,
        vec![relation_retirement_item(
            "contradicts",
            PREMISE,
            DECISION,
            &contradiction,
        )],
    );

    let report = gate_of(&mut omhl, &snap('a'));

    assert!(report.passed);
}

#[test]
fn it_should_refuse_evaluating_the_gate_for_an_unknown_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();

    let result = omhl.evaluate_pr_gate(&acme(), &sid("work-items.pr-9"), &snap('a'));

    assert_eq!(
        result.err(),
        Some(rule(RuleCode::WorkItemNotFound, "/work_item"))
    );
}

#[test]
fn it_should_not_write_anything_when_evaluating_the_gate() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let before = omhl.chain_head(&acme()).expect("the head is read");

    gate_of(&mut omhl, &snap('a'));

    let after = omhl.chain_head(&acme()).expect("the head is read");
    assert_eq!(after, before);
}

// Fechamento do work item

#[test]
fn it_should_close_a_work_item_recording_the_passing_result_snapshot_and_chain_head_seq() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    let head_before = head_seq(&mut omhl);

    let closed = close(&mut omhl, PR_7, &snap('a'));

    assert_eq!(
        data_of(&mut omhl, PR_7),
        closed_data(pull_request_data("Add gate"), true, head_before, &snap('a'))
    );
    assert_eq!(closed.outcome.records[0].seq, head_before + 1);
}

#[test]
fn it_should_close_even_when_the_gate_fails() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let head_before = head_seq(&mut omhl);

    let closed = close(&mut omhl, PR_7, &snap('a'));

    assert!(!closed.report.passed);
    assert_eq!(
        data_of(&mut omhl, PR_7),
        closed_data(
            pull_request_data("Add gate"),
            false,
            head_before,
            &snap('a')
        )
    );
}

#[test]
fn it_should_refuse_closing_an_unknown_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();

    let result = omhl.close_work_item(
        &acme(),
        &sid("work-items.pr-9"),
        &snap('a'),
        &support::author(),
    );

    assert_eq!(
        result.err(),
        Some(rule(RuleCode::WorkItemNotFound, "/work_item"))
    );
}

#[test]
fn it_should_refuse_closing_a_work_item_already_closed() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    close(&mut omhl, PR_7, &snap('a'));

    let result = omhl.close_work_item(&acme(), &sid(PR_7), &snap('a'), &support::author());

    assert_eq!(
        result.err(),
        Some(rule(RuleCode::InvalidTransition, "/work_item"))
    );
}

#[test]
fn it_should_close_a_pull_request_and_a_plain_work_item_the_same_way() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    append_ok(
        &mut omhl,
        vec![node_item(
            FEATURE_1,
            "work-item",
            work_item_data("Ship the feature"),
        )],
    );
    let pr_head = head_seq(&mut omhl);
    close(&mut omhl, PR_7, &snap('a'));
    let feature_head = head_seq(&mut omhl);

    close(&mut omhl, FEATURE_1, &snap('a'));

    assert_eq!(
        data_of(&mut omhl, PR_7),
        closed_data(pull_request_data("Add gate"), false, pr_head, &snap('a'))
    );
    assert_eq!(
        data_of(&mut omhl, FEATURE_1),
        closed_data(
            work_item_data("Ship the feature"),
            false,
            feature_head,
            &snap('a')
        )
    );
}

#[test]
fn it_should_refuse_reopening_a_closed_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let closed = close(&mut omhl, PR_7, &snap('a'));
    let reopening = revision_item(
        PR_7,
        "pull-request",
        pull_request_data("Add gate"),
        &closed.outcome.records[0].id,
    );

    let error = refusal(&mut omhl, vec![reopening]);

    assert_eq!(
        error,
        rule(RuleCode::InvalidTransition, "/items/0/body/data/status")
    );
}

#[test]
fn it_should_refuse_an_attestation_for_a_closed_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    close(&mut omhl, PR_7, &snap('a'));

    let error = refusal(
        &mut omhl,
        vec![attestation_item(&snap('a'), RUN_TESTS, PR_7)],
    );

    assert_eq!(
        error,
        rule(RuleCode::WorkItemClosed, "/items/0/body/data/work_item")
    );
}

#[test]
fn it_should_keep_the_chain_valid_after_the_full_flow() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    let contradiction = seed_contradicted_decision(&mut omhl);
    append_ok(
        &mut omhl,
        vec![relation_retirement_item(
            "contradicts",
            PREMISE,
            DECISION,
            &contradiction,
        )],
    );
    close(&mut omhl, PR_7, &snap('a'));

    let report = omhl
        .verify_chain(&acme(), None)
        .expect("the chain is verified");

    assert!(report.ok);
    assert!(report.breaks.is_empty());
}
