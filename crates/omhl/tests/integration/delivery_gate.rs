use crate::support;

use hexlog::{
    Author, FieldName, ListFilter, Scalar, Snapshot, SnapshotKind, SnapshotKind::GitCommit,
};
use omhl::app::RUNNABLE_EVENTS;
use omhl::delivery_gate::{
    CloseRefusal, GATE_NAME, GateSnapshots, REQUIRED_EVENTS, required_checks,
};
use omhl::error::{OmhlError, RuleCode};
use omhl::facade::{CloseError, Omhl, OmhlDeps};
use omhl::gate::{GateQuestion, GateReport, Question, QuestionClass};
use omhl::mechanical::spec_digest;
use omhl::probe::{ProbeOutcome, TreeSource};
use omhl::workflow::config::NativeConfig;
use serde_json::json;
use support::delivery_gate::{
    ANCHOR, ANCHORS_CONFIRMED, CI_GREEN, CONFIG_LOOSENING_REVIEW, CRITIC_DECISION, DECISION,
    DECISIONS_ANCHORED, FEATURE_1, NO_OPEN_CONTRADICTION, PREMISE, all_native_enabled,
    anchor_decision, attest, attest_required, close, closed_data, data_of, default_natives,
    expected_outcomes, gate_of, gate_with_review, head_seq, mechanical_project_check,
    native_enabled, outcomes, project_check, question, relation_item, relation_retirement_item,
    required_pairs, seed_contradicted_decision, seed_critic_contradiction, sids, snapshots,
};
use support::legacy::seed_unconfirmed_guideline;
use support::omhl::{
    OmhlHarness, PR_7, RUN_TESTS, acme, append_as, append_ok, append_ok_as, attestation_id_for,
    attestation_item, check_data, decision_data, node_item, premise_data, pull_request_data,
    refusal, request, retirement_item, revision_item, seed_pull_request, with_snapshot,
    work_item_data,
};
use support::raw::RawHarness;
use support::{
    FakeProbe, author, executor_attestation_data, executor_author, loader_author, role_author, sid,
    snap,
};

/// Hash do conteúdo vazio, o `byproduct` das attestations escritas à mão.
const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn rule(code: RuleCode, pointer: &str) -> OmhlError {
    OmhlError::rule(code, pointer)
}

fn field(name: &str) -> FieldName {
    FieldName::parse(name).expect("valid field name")
}

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

/// O commit `snap('a')` e a árvore `tree('c')` de um HEAD limpo.
fn at_head() -> GateSnapshots {
    snapshots(&snap('a'), &tree('c'))
}

/// Sonda limpa: commit `snap('a')`, e a árvore `tree('c')` na fonte `Head`, a que o fechamento consulta.
fn head_probe() -> FakeProbe {
    FakeProbe::clean(snap('a'))
        .with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree('c'))))
}

/// Como `head_probe`, e a mesma árvore na fonte `Index`, a que a guarda consulta ao gravar a attestation.
fn flow_probe() -> FakeProbe {
    head_probe().with_tree(TreeSource::Index, Ok(ProbeOutcome::Reproducible(tree('c'))))
}

/// Atesta o mecânico e os dois nativos exigidos; o gate fecha verde porque um check do projeto não substitui o nativo ligado.
fn seed_per_check_flow(omhl: &mut Omhl<'_>) {
    seed_pull_request(omhl);
    let fmt = mechanical_project_check("checks.fmt", "before-commit", true, false);
    append_ok_as(
        omhl,
        loader_author(),
        vec![node_item("checks.fmt", "check", fmt.data.clone())],
    );
    let id = attestation_id_for(&tree('c'), "checks.fmt", PR_7);
    let mut data = executor_attestation_data("checks.fmt", PR_7, "passed", EMPTY_SHA);
    data["spec_digest"] = json!(spec_digest(&fmt.data));
    let attestation = node_item(&id, "attestation", data);
    append_ok_as(
        omhl,
        executor_author(),
        vec![with_snapshot(attestation, &tree('c'))],
    );
    attest(omhl, &snap('a'), RUN_TESTS, "passed", true);
    attest(omhl, &snap('a'), CI_GREEN, "passed", true);
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
    let while_active = gate_of(&mut omhl, &at_head()).questions.len();
    append_as(
        &mut omhl,
        loader_author(),
        vec![retirement_item("checks.lint-docs", &created.records[0].id)],
    )
    .expect("the loader retires the check");

    let after_retirement = gate_of(&mut omhl, &at_head()).questions.len();

    assert_eq!((while_active, after_retirement), (6, 5));
}

#[test]
fn it_should_pass_when_every_required_check_has_an_ok_attestation_at_the_snapshot() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(&mut omhl, &snap('a'), RUN_TESTS, "passed", true);
    attest(&mut omhl, &snap('a'), CI_GREEN, "passed_with_warning", true);

    let report = gate_of(&mut omhl, &at_head());

    assert!(report.passed);
    assert_eq!(report.gate, GATE_NAME);
    assert_eq!(
        outcomes(&report),
        vec![
            (RUN_TESTS.to_owned(), true),
            (CI_GREEN.to_owned(), true),
            (NO_OPEN_CONTRADICTION.to_owned(), true),
            (DECISIONS_ANCHORED.to_owned(), true),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_fail_and_name_the_check_without_an_attestation() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(&mut omhl, &snap('a'), CI_GREEN, "passed", true);

    let report = gate_of(&mut omhl, &at_head());

    assert!(!report.passed);
    assert!(!question(&report, RUN_TESTS).passed);
}

#[test]
fn it_should_fail_when_the_attestation_belongs_to_another_snapshot() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('b')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('b'));

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        outcomes(&report),
        vec![
            (RUN_TESTS.to_owned(), false),
            (CI_GREEN.to_owned(), false),
            (NO_OPEN_CONTRADICTION.to_owned(), true),
            (DECISIONS_ANCHORED.to_owned(), true),
            (ANCHORS_CONFIRMED.to_owned(), true),
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

    let report = gate_of(&mut omhl, &at_head());

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

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        outcomes(&report),
        vec![
            (RUN_TESTS.to_owned(), true),
            (CI_GREEN.to_owned(), true),
            (NO_OPEN_CONTRADICTION.to_owned(), false),
            (DECISIONS_ANCHORED.to_owned(), true),
            (ANCHORS_CONFIRMED.to_owned(), true),
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

    let report = gate_of(&mut omhl, &at_head());

    assert!(report.passed);
}

#[test]
fn it_should_pass_the_no_open_contradiction_gate_after_the_verifier_retires_the_contradicts() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let contradiction = seed_contradicted_decision(&mut omhl);
    append_ok_as(
        &mut omhl,
        role_author("omhl:verifier"),
        vec![relation_retirement_item(
            "contradicts",
            PREMISE,
            DECISION,
            &contradiction,
        )],
    );

    let report = gate_of(&mut omhl, &at_head());

    assert!(question(&report, NO_OPEN_CONTRADICTION).passed);
}

#[test]
fn it_should_accept_a_rests_on_from_a_critic_decision_to_a_confirmed_premise() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_critic_contradiction(&mut omhl, false);
    let anchor = relation_item("rests-on", CRITIC_DECISION, ANCHOR);

    let result = append_as(&mut omhl, role_author("omhl:critic"), vec![anchor]);

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_pass_decisions_anchored_with_a_critic_decision_that_contradicts_an_architect_decision()
{
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_critic_contradiction(&mut omhl, true);

    let report = gate_of(&mut omhl, &at_head());

    assert!(question(&report, DECISIONS_ANCHORED).passed);
}

#[test]
fn it_should_fail_decisions_anchored_when_a_critic_decision_has_no_rests_on() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_critic_contradiction(&mut omhl, false);

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        question(&report, DECISIONS_ANCHORED).failing,
        sids(&[CRITIC_DECISION])
    );
}

#[test]
fn it_should_refuse_evaluating_the_gate_for_an_unknown_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();

    let result = omhl.evaluate_delivery_gate(
        &acme(),
        &sid("work-items.pr-9"),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
    );

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

    gate_of(&mut omhl, &at_head());

    let after = omhl.chain_head(&acme()).expect("the head is read");
    assert_eq!(after, before);
}

// Fechamento do work item

#[test]
fn it_should_close_a_work_item_recording_the_passing_result_snapshot_and_chain_head_seq() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    let head_before = head_seq(&mut omhl);

    let closed = close(&mut omhl, PR_7, &at_head());

    assert_eq!(
        data_of(&mut omhl, PR_7),
        closed_data(pull_request_data("Add gate"), true, head_before, &snap('a'))
    );
    assert_eq!(closed.outcome.records[0].seq, head_before + 1);
}

#[test]
fn it_should_refuse_closing_when_the_gate_fails_without_force() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let head_before = head_seq(&mut omhl);

    let result = omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    );

    assert!(matches!(
        result,
        Err(CloseError::Refused(CloseRefusal::Refused(_)))
    ));
    assert_eq!(data_of(&mut omhl, PR_7)["status"], json!("open"));
    assert_eq!(head_seq(&mut omhl), head_before);
}

#[test]
fn it_should_refuse_the_gate_when_the_head_changed_after_the_requirements_were_read() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    let stale = omhl.chain_head(&acme()).expect("the head is read");
    seed_pull_request(&mut omhl);

    let result = omhl.evaluate_delivery_gate(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements {
            head: Some(stale),
            atoms: Vec::new(),
            evaluated: NativeConfig::default(),
        },
        false,
    );

    assert_eq!(result.err(), Some(rule(RuleCode::GateInconsistent, "")));
}

#[test]
fn it_should_refuse_closing_an_unknown_work_item() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();

    let result = omhl.close_work_item(
        &acme(),
        &sid("work-items.pr-9"),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &support::author(),
            note: None,
        },
    );

    assert_eq!(
        result.err(),
        Some(omhl::facade::CloseError::Omhl(rule(
            RuleCode::WorkItemNotFound,
            "/work_item"
        )))
    );
}

#[test]
fn it_should_refuse_closing_a_work_item_already_closed() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    close(&mut omhl, PR_7, &at_head());

    let result = omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &support::author(),
            note: None,
        },
    );

    assert_eq!(
        result.err(),
        Some(omhl::facade::CloseError::Omhl(rule(
            RuleCode::InvalidTransition,
            "/work_item"
        )))
    );
}

#[test]
fn it_should_refuse_closing_a_pull_request_and_a_plain_work_item_the_same_way() {
    let harness = OmhlHarness::new(head_probe());
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
    let head_before = head_seq(&mut omhl);

    let pull_request = close_without_force(&mut omhl, PR_7);
    let work_item = close_without_force(&mut omhl, FEATURE_1);

    assert!(matches!(
        pull_request,
        Err(CloseError::Refused(CloseRefusal::Refused(_)))
    ));
    assert!(matches!(
        work_item,
        Err(CloseError::Refused(CloseRefusal::Refused(_)))
    ));
    assert_eq!(data_of(&mut omhl, PR_7)["status"], json!("open"));
    assert_eq!(data_of(&mut omhl, FEATURE_1)["status"], json!("open"));
    assert_eq!(head_seq(&mut omhl), head_before);
}

fn close_without_force(
    omhl: &mut Omhl<'_>,
    work_item: &str,
) -> Result<omhl::facade::ClosedWorkItem, CloseError> {
    omhl.close_work_item(
        &acme(),
        &sid(work_item),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    )
}

#[test]
fn it_should_refuse_reopening_a_closed_work_item() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    let closed = close(&mut omhl, PR_7, &at_head());
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
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    close(&mut omhl, PR_7, &at_head());

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
    let harness = OmhlHarness::new(head_probe());
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
    close(&mut omhl, PR_7, &at_head());

    let report = omhl
        .verify_chain(&acme(), None)
        .expect("the chain is verified");

    assert!(report.ok);
    assert!(report.breaks.is_empty());
}

// Tetos de `where_eq` em `evaluate`

#[test]
fn it_should_refuse_an_evaluation_with_more_than_eight_where_eq_pairs() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let nine_pairs = (0..9)
        .map(|i| (field(&format!("f{i}")), Scalar::Int(i)))
        .collect();
    let questions = vec![GateQuestion::new(
        "too-many-pairs",
        Question::Occurred {
            select: ListFilter {
                type_name: None,
                id_prefix: None,
                snapshot: None,
                where_eq: nine_pairs,
            },
            min: 1,
        },
        QuestionClass::Warn,
    )];

    let result = omhl.evaluate(&acme(), "custom-gate", &questions);

    assert_eq!(
        result.err(),
        Some(rule(
            RuleCode::WhereEqTooMany,
            "/questions/0/filter/where_eq"
        ))
    );
}

// Sonda e autor no fechamento

#[test]
fn it_should_refuse_closing_when_the_snapshot_is_not_the_probe_head() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let head_before = omhl.chain_head(&acme()).expect("the head is read");

    let result = omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &snapshots(&snap('b'), &tree('c')),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    );
    let head_after = omhl.chain_head(&acme()).expect("the head is read");

    assert_eq!(
        (result.err(), head_after),
        (
            Some(omhl::facade::CloseError::Omhl(rule(
                RuleCode::SnapshotNotHead,
                "/snapshot"
            ))),
            head_before
        )
    );
}

#[test]
fn it_should_refuse_closing_when_the_tree_is_dirty() {
    let harness = OmhlHarness::new(FakeProbe::dirty());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let head_before = omhl.chain_head(&acme()).expect("the head is read");

    let result = omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    );
    let head_after = omhl.chain_head(&acme()).expect("the head is read");

    assert_eq!(
        (result.err(), head_after),
        (
            Some(omhl::facade::CloseError::Omhl(rule(
                RuleCode::SnapshotDirty,
                "/snapshot"
            ))),
            head_before
        )
    );
}

#[test]
fn it_should_record_the_caller_author_on_the_closing_revision() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let closer = Author {
        agent: "closer".to_owned(),
        model: None,
        client: "closing-client".to_owned(),
    };

    attest_required(&mut omhl, &snap('a'));
    omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &closer,
            note: None,
        },
    )
    .expect("the work item is closed");
    let stored = omhl
        .get(&acme(), &sid(PR_7))
        .expect("the node is read")
        .map(|node| node.author);

    assert_eq!(stored, Some(closer));
}

// Um snapshot por check

#[test]
fn it_should_require_the_three_events_in_the_pull_request_gate() {
    let notes = project_check("checks.review-notes", "before-push", true, false);

    let required = required_checks(
        &default_natives(),
        &all_native_enabled(),
        &NativeConfig::default(),
        &[notes],
        false,
    );

    assert_eq!(
        REQUIRED_EVENTS,
        ["before-commit", "before-push", "before-merge"]
    );
    assert_eq!(REQUIRED_EVENTS, RUNNABLE_EVENTS);
    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.review-notes", GitCommit),
        ])
    );
}

#[test]
fn it_should_close_a_work_item_with_the_per_check_snapshots() {
    let harness = OmhlHarness::new(flow_probe());
    let mut omhl = harness.open();
    seed_per_check_flow(&mut omhl);

    let closed = close(&mut omhl, PR_7, &at_head());

    assert!(closed.report.passed);
    assert_eq!(
        outcomes(&closed.report),
        expected_outcomes(&[
            (RUN_TESTS, true),
            (CI_GREEN, true),
            ("checks.fmt", true),
            ("spec-digest-match:checks.fmt", true),
            (NO_OPEN_CONTRADICTION, true),
            (DECISIONS_ANCHORED, true),
            (ANCHORS_CONFIRMED, true),
        ])
    );
}

#[test]
fn it_should_refuse_to_close_when_the_head_tree_cannot_be_probed() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let head_before = head_seq(&mut omhl);

    let result = omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        false,
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    );

    assert_eq!(
        result.err(),
        Some(omhl::facade::CloseError::Omhl(rule(
            RuleCode::ProbeFailed,
            "/snapshot"
        )))
    );
    assert_eq!(data_of(&mut omhl, PR_7)["status"], json!("open"));
    assert_eq!(head_seq(&mut omhl), head_before);
}

#[test]
fn it_should_keep_the_closing_record_snapshot_as_the_head_commit() {
    let harness = OmhlHarness::new(flow_probe());
    let mut omhl = harness.open();
    seed_per_check_flow(&mut omhl);
    let open = data_of(&mut omhl, PR_7);
    let head_before = head_seq(&mut omhl);

    close(&mut omhl, PR_7, &at_head());

    assert_eq!(
        data_of(&mut omhl, PR_7),
        closed_data(open, true, head_before, &snap('a'))
    );
}

// Perguntas de âncora

const UNANCHORED_NEXT_STEP: &str =
    "record a rests-on from the decision to a confirmed guideline or premise";
const UNCONFIRMED_NEXT_STEP: &str = "ask the human to confirm the target (premise: set status confirmed with confirmed_by) or rest the decision on a confirmed one";
const KEPT_GUIDELINE: &str = "guidelines.keep-it-simple";
const FORMER_NATIVE_GUIDELINE: &str = "guidelines.commit-policy";

fn seed_decision(omhl: &mut Omhl<'_>) {
    append_ok(
        omhl,
        vec![node_item(
            DECISION,
            "decision",
            decision_data("Use SQLite", Some(PR_7)),
        )],
    );
}

/// Premissa `confirmed` sem `confirmed_by` (o formato legado, que a guarda não grava mais) e a decisão apoiada nela.
/// Grava pelo `forge`, que reparte os ids com o `Omhl` aberto; `legacy.rs` abre outro `Hexlog` e repete o id.
fn seed_decision_on_a_legacy_premise(harness: &OmhlHarness, omhl: &mut Omhl<'_>) {
    seed_decision(omhl);
    let legacy = json!({
        "text": "Users pay monthly", "status": "confirmed", "kind": "business", "work_item": PR_7,
    });
    harness.forge(&request(vec![node_item(ANCHOR, "premise", legacy)]));
    append_ok(omhl, vec![relation_item("rests-on", DECISION, ANCHOR)]);
}

/// `(label, passed)` das duas últimas perguntas do relatório, as de âncora.
fn anchor_labels(report: &GateReport) -> Vec<(String, bool)> {
    let all = outcomes(report);

    all[all.len() - 2..].to_vec()
}

#[test]
fn it_should_append_the_anchor_questions_after_the_required_checks() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);

    let report = gate_of(&mut omhl, &at_head());

    let labels: Vec<String> = outcomes(&report)
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    assert_eq!(
        labels,
        vec![
            RUN_TESTS.to_owned(),
            CI_GREEN.to_owned(),
            NO_OPEN_CONTRADICTION.to_owned(),
            DECISIONS_ANCHORED.to_owned(),
            ANCHORS_CONFIRMED.to_owned(),
        ]
    );
}

#[test]
fn it_should_not_fail_for_a_loose_assumed_premise() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));
    append_ok(
        &mut omhl,
        vec![node_item(
            PREMISE,
            "premise",
            premise_data("SQLite is fast", Some(PR_7)),
        )],
    );

    let report = gate_of(&mut omhl, &at_head());

    assert!(report.passed);
}

#[test]
fn it_should_treat_a_legacy_confirmed_premise_without_confirmed_by_as_unconfirmed() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision_on_a_legacy_premise(&harness, &mut omhl);

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        question(&report, ANCHORS_CONFIRMED).failing,
        sids(&[DECISION])
    );
}

#[test]
fn it_should_leave_a_decision_without_rests_on_to_the_decisions_anchored_question_only() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        anchor_labels(&report),
        vec![
            (DECISIONS_ANCHORED.to_owned(), false),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_pass_both_anchor_questions_for_a_decision_resting_on_a_confirmed_premise() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);
    anchor_decision(&mut omhl);

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        anchor_labels(&report),
        vec![
            (DECISIONS_ANCHORED.to_owned(), true),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_pass_both_anchor_questions_for_a_decision_resting_on_a_guideline_written_by_the_loader()
 {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);
    let guideline = json!({ "text": "keep it simple", "kind": "business" });
    append_ok_as(
        &mut omhl,
        loader_author(),
        vec![node_item(KEPT_GUIDELINE, "guideline", guideline)],
    );
    append_ok(
        &mut omhl,
        vec![relation_item("rests-on", DECISION, KEPT_GUIDELINE)],
    );

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        anchor_labels(&report),
        vec![
            (DECISIONS_ANCHORED.to_owned(), true),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_fail_a_decision_resting_on_a_guideline_without_confirmation_written_by_another_client()
{
    let raw = RawHarness::new();
    seed_unconfirmed_guideline(&raw, KEPT_GUIDELINE, "business");
    let probe = FakeProbe::clean(snap('a'));
    let deps = OmhlDeps {
        clock: &raw.clock,
        ids: &raw.ids,
        probe: &probe,
        session: &crate::support::NO_SESSION,
    };
    let mut omhl = Omhl::open(&raw.config(), deps).expect("omhl opens");
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);
    append_ok(
        &mut omhl,
        vec![relation_item("rests-on", DECISION, KEPT_GUIDELINE)],
    );

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        question(&report, ANCHORS_CONFIRMED).failing,
        sids(&[DECISION])
    );
}

#[test]
fn it_should_pass_both_anchor_questions_for_a_decision_resting_on_a_loaded_former_native_guideline()
{
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);
    append_ok_as(
        &mut omhl,
        loader_author(),
        vec![node_item(
            FORMER_NATIVE_GUIDELINE,
            "guideline",
            json!({ "text": "ask before committing", "kind": "business" }),
        )],
    );
    append_ok(
        &mut omhl,
        vec![relation_item("rests-on", DECISION, FORMER_NATIVE_GUIDELINE)],
    );

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        anchor_labels(&report),
        vec![
            (DECISIONS_ANCHORED.to_owned(), true),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_fail_again_when_the_rests_on_of_a_decision_is_retired() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);
    let rests_on = anchor_decision(&mut omhl);
    append_ok(
        &mut omhl,
        vec![relation_retirement_item(
            "rests-on", DECISION, ANCHOR, &rests_on,
        )],
    );

    let report = gate_of(&mut omhl, &at_head());

    assert_eq!(
        anchor_labels(&report),
        vec![
            (DECISIONS_ANCHORED.to_owned(), false),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_fail_an_unanchored_decision_with_cause_and_next_step() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision(&mut omhl);

    let report = gate_of(&mut omhl, &at_head());

    let unanchored = question(&report, DECISIONS_ANCHORED);
    assert_eq!(
        (unanchored.failing.clone(), unanchored.next_step.clone()),
        (sids(&[DECISION]), Some(UNANCHORED_NEXT_STEP.to_owned()))
    );
}

#[test]
fn it_should_fail_an_unconfirmed_anchor_with_cause_and_next_step() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed_decision_on_a_legacy_premise(&harness, &mut omhl);

    let report = gate_of(&mut omhl, &at_head());

    let unconfirmed = question(&report, ANCHORS_CONFIRMED);
    assert_eq!(
        (unconfirmed.failing.clone(), unconfirmed.next_step.clone()),
        (sids(&[DECISION]), Some(UNCONFIRMED_NEXT_STEP.to_owned()))
    );
}

#[test]
fn it_should_not_attach_a_next_step_to_a_passing_question() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest_required(&mut omhl, &snap('a'));

    let report = gate_of(&mut omhl, &at_head());

    let next_steps: Vec<Option<String>> = report
        .questions
        .iter()
        .map(|entry| entry.next_step.clone())
        .collect();
    assert_eq!(next_steps, vec![None; 5]);
}

// A exigência da revisão de afrouxamento

#[test]
fn it_should_fail_the_review_question_as_refuse_when_it_is_required_and_unattested() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);

    let report = gate_with_review(&mut omhl, &at_head(), true);

    let review = question(&report, CONFIG_LOOSENING_REVIEW);
    assert_eq!(
        (review.passed, review.class),
        (false, Some(QuestionClass::Refuse))
    );
}

#[test]
fn it_should_pass_the_review_question_once_it_is_attested_at_the_head() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(
        &mut omhl,
        &snap('a'),
        CONFIG_LOOSENING_REVIEW,
        "passed",
        true,
    );

    let report = gate_with_review(&mut omhl, &at_head(), true);

    assert!(question(&report, CONFIG_LOOSENING_REVIEW).passed);
}

#[test]
fn it_should_fail_the_review_question_again_when_a_commit_follows_the_attestation() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    attest(
        &mut omhl,
        &snap('a'),
        CONFIG_LOOSENING_REVIEW,
        "passed",
        true,
    );
    let later_head = snapshots(&snap('b'), &tree('d'));

    let report = gate_with_review(&mut omhl, &later_head, true);

    assert!(!question(&report, CONFIG_LOOSENING_REVIEW).passed);
}

#[test]
fn it_should_leave_the_review_question_out_when_the_review_is_not_required() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);

    let report = gate_with_review(&mut omhl, &at_head(), false);

    let labels: Vec<&str> = report.questions.iter().map(|q| q.label.as_str()).collect();
    assert!(!labels.contains(&CONFIG_LOOSENING_REVIEW), "{labels:?}");
}

#[test]
fn it_should_refuse_closing_even_with_a_note_when_the_review_is_required_and_unattested() {
    let harness = OmhlHarness::new(head_probe());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let head_before = head_seq(&mut omhl);

    let result = omhl.close_work_item(
        &acme(),
        &sid(PR_7),
        &at_head(),
        &native_enabled(&[CONFIG_LOOSENING_REVIEW]),
        &omhl::facade::WorkRequirements::none(),
        true,
        &omhl::facade::Closing {
            author: &author(),
            note: Some("owner approved the override"),
        },
    );

    assert!(matches!(
        result,
        Err(CloseError::Refused(CloseRefusal::Refused(_)))
    ));
    assert_eq!(data_of(&mut omhl, PR_7)["status"], json!("open"));
    assert_eq!(head_seq(&mut omhl), head_before);
}
