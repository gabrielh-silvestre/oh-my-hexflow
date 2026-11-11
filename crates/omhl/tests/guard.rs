mod support;

use hexlog::{
    AppendItem, AppendOutcome, AppendRequest, Author, FieldName, ListFilter, PageRequest, Scalar,
    Snapshot, SnapshotKind,
};
use omhl::error::{OmhlError, RuleCode};
use omhl::facade::{Omhl, OmhlDeps};
use omhl::guard::attestation_id;
use omhl::ports::git::GitProbe;
use omhl::probe::{ProbeOutcome, TreeSource};
use serde_json::{Value, json};
use support::git::GitRepo;
use support::omhl::{
    LINT_DOCS, OmhlHarness, PR_7, RUN_TESTS, acme, append_ok, attestation_data, attestation_id_for,
    attestation_item, check_data, closed_with_gate, decision_data, node_item, premise_data,
    pull_request_data, refusal, request, retirement_item, revision_item, seed_pull_request,
    with_snapshot, work_item_data,
};
use support::pr_gate::close;
use support::raw::RawHarness;
use support::{
    FakeProbe, executor_attestation_data, executor_author, loader_author, sid, snap, snap64, tname,
};

const ITEM_ID_POINTER: &str = "/items/0/body/id";
const SNAPSHOT_POINTER: &str = "/items/0/snapshot";
const WORK_ITEM_POINTER: &str = "/items/0/body/data/work_item";
const FMT: &str = "checks.fmt";
const LINT: &str = "checks.lint";
const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const HEAD_TREE: &str = "08585692ce06452da6f82ae66b90d98b55536fca";
const STATUS_POINTER: &str = "/items/0/body/data/status";

fn rule(code: RuleCode, pointer: &str) -> OmhlError {
    OmhlError::rule(code, pointer)
}

fn page() -> PageRequest {
    PageRequest {
        limit: 10,
        after_seq: None,
    }
}

fn filter_with(where_eq: Vec<(FieldName, Scalar)>) -> ListFilter {
    ListFilter {
        type_name: None,
        id_prefix: None,
        snapshot: None,
        where_eq,
    }
}

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

/// Sonda de `pre-commit`: o índice reproduz `tree('c')` e a working tree está suja para o `probe()`.
fn staged_tree_probe() -> FakeProbe {
    FakeProbe::dirty().with_tree(TreeSource::Index, Ok(ProbeOutcome::Reproducible(tree('c'))))
}

/// Check com `run`, como o `load` o grava.
fn mechanical_check(event: &str, run: &[&str], blocking: bool) -> Value {
    let mut data = check_data(event, "run it", blocking);
    data["run"] = json!(run);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);

    data
}

fn append_as(
    omhl: &mut Omhl<'_>,
    author: Author,
    items: Vec<AppendItem>,
) -> Result<AppendOutcome, OmhlError> {
    omhl.append(&AppendRequest {
        author,
        ..request(items)
    })
}

fn refusal_as_executor(omhl: &mut Omhl<'_>, items: Vec<AppendItem>) -> OmhlError {
    append_as(omhl, executor_author(), items).expect_err("the append is refused")
}

/// Pull request `PR_7` aberto e o check `check` gravado pelo loader.
fn seed_pull_request_and_check(omhl: &mut Omhl<'_>, check: &str, event: &str) {
    seed_pull_request(omhl);
    let node = node_item(check, "check", mechanical_check(event, &["true"], true));
    append_as(omhl, loader_author(), vec![node]).expect("the check is accepted");
}

/// Attestation `passed` de um check com `run`, no id derivado do snapshot.
fn executor_attestation(snapshot: &Snapshot, check: &str) -> AppendItem {
    let id = attestation_id_for(snapshot, check, PR_7);
    let data = executor_attestation_data(check, PR_7, "passed", EMPTY_SHA);

    with_snapshot(node_item(&id, "attestation", data), snapshot)
}

fn attestation_count(omhl: &mut Omhl<'_>) -> usize {
    let filter = ListFilter {
        type_name: Some(tname("attestation")),
        ..filter_with(Vec::new())
    };

    omhl.list(&acme(), &filter, &page())
        .expect("the list is read")
        .items
        .len()
}

/// `count` pares `f0 = 0`, `f1 = 1`, ... em campos que nenhum node tem.
fn int_pairs(count: i64) -> Vec<(FieldName, Scalar)> {
    (0..count)
        .map(|i| {
            let name = FieldName::parse(&format!("f{i}")).expect("valid field name");

            (name, Scalar::Int(i))
        })
        .collect()
}

/// Dois pares: `f0 = 1` e `f1 = text`.
fn pairs_with_text(text: &str) -> Vec<(FieldName, Scalar)> {
    vec![
        (
            FieldName::parse("f0").expect("valid field name"),
            Scalar::Int(1),
        ),
        (
            FieldName::parse("f1").expect("valid field name"),
            Scalar::Str(text.to_owned()),
        ),
    ]
}

#[test]
fn it_should_accept_a_work_item_and_a_pull_request_created_open() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let work_item = node_item("work-items.wi-1", "work-item", work_item_data("Ship it"));
    let pull_request = node_item(PR_7, "pull-request", pull_request_data("Add gate"));

    let result = omhl.append(&request(vec![work_item, pull_request]));

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(2));
}

#[test]
fn it_should_refuse_a_work_item_created_closed() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let closed = closed_with_gate(work_item_data("Ship it"));

    let error = refusal(
        &mut omhl,
        vec![node_item("work-items.wi-1", "work-item", closed)],
    );

    assert_eq!(error, rule(RuleCode::UseCloseWorkItem, STATUS_POINTER));
}

#[test]
fn it_should_refuse_closing_a_work_item_through_append() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let base = seed_pull_request(&mut omhl);
    let closed = closed_with_gate(pull_request_data("Add gate"));

    let error = refusal(
        &mut omhl,
        vec![revision_item(PR_7, "pull-request", closed, &base)],
    );

    assert_eq!(error, rule(RuleCode::UseCloseWorkItem, STATUS_POINTER));
}

#[test]
fn it_should_accept_a_revision_that_keeps_the_work_item_open() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let base = seed_pull_request(&mut omhl);
    let retitled = pull_request_data("Add the gate");

    let result = omhl.append(&request(vec![revision_item(
        PR_7,
        "pull-request",
        retitled,
        &base,
    )]));

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_derive_the_attestation_id_from_snapshot_check_and_work_item() {
    let expected = format!(
        "attestations.{}.{}.checks.run-tests.work-items.pr-7",
        "a".repeat(32),
        "a".repeat(8)
    );

    let id = attestation_id(&snap('a'), &sid(RUN_TESTS), &sid(PR_7));

    assert_eq!(id, Ok(sid(&expected)));
}

#[test]
fn it_should_split_a_sha256_snapshot_into_two_segments_of_32() {
    let expected = format!(
        "attestations.{}.{}.checks.run-tests.work-items.pr-7",
        "b".repeat(32),
        "b".repeat(32)
    );

    let id = attestation_id(&snap64('b'), &sid(RUN_TESTS), &sid(PR_7));

    assert_eq!(id, Ok(sid(&expected)));
}

#[test]
fn it_should_refuse_an_attestation_id_above_200_characters() {
    let check = sid(&format!("checks.{}", "c".repeat(63)));
    let work_item = sid(&format!("work-items.{}", "w".repeat(63)));

    let id = attestation_id(&snap64('b'), &check, &work_item);

    assert_eq!(id, Err(rule(RuleCode::AttestationIdTooLong, "")));
}

#[test]
fn it_should_accept_an_attestation_at_the_clean_head_for_a_native_check() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('a'), RUN_TESTS, PR_7);

    let result = omhl.append(&request(vec![attestation]));

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_accept_an_attestation_for_a_project_check() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let project_check = node_item(
        LINT_DOCS,
        "check",
        check_data("before-commit", "run the linter", true),
    );
    append_ok(&mut omhl, vec![project_check]);
    let attestation = attestation_item(&snap('a'), LINT_DOCS, PR_7);

    let result = omhl.append(&request(vec![attestation]));

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_refuse_an_attestation_whose_id_does_not_match_its_data() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let id = attestation_id_for(&snap('a'), RUN_TESTS, "work-items.pr-8");
    let attestation = with_snapshot(
        node_item(
            &id,
            "attestation",
            attestation_data(RUN_TESTS, PR_7, "passed", true),
        ),
        &snap('a'),
    );

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(
        error,
        rule(RuleCode::AttestationIdMismatch, ITEM_ID_POINTER)
    );
}

#[test]
fn it_should_refuse_an_attestation_when_the_snapshot_differs_from_the_probe_head() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('b'), RUN_TESTS, PR_7);

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::SnapshotNotHead, SNAPSHOT_POINTER));
}

#[test]
fn it_should_refuse_an_attestation_when_the_tree_is_dirty() {
    let harness = OmhlHarness::new(FakeProbe::dirty());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('a'), RUN_TESTS, PR_7);

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::SnapshotDirty, SNAPSHOT_POINTER));
}

#[test]
fn it_should_refuse_an_attestation_when_the_probe_fails() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('a'), RUN_TESTS, PR_7);

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::ProbeFailed, SNAPSHOT_POINTER));
}

#[test]
fn it_should_refuse_an_attestation_for_a_missing_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    let attestation = attestation_item(&snap('a'), RUN_TESTS, "work-items.pr-9");

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::WorkItemNotFound, WORK_ITEM_POINTER));
}

#[test]
fn it_should_refuse_an_attestation_for_an_unknown_check() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('a'), "checks.unknown", PR_7);

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(
        error,
        rule(RuleCode::CheckNotFound, "/items/0/body/data/check")
    );
}

#[test]
fn it_should_refuse_changing_the_outcome_of_an_attestation() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('a'), RUN_TESTS, PR_7);
    let id = attestation_id_for(&snap('a'), RUN_TESTS, PR_7);
    let base = append_ok(&mut omhl, vec![attestation]).records[0]
        .id
        .clone();
    let failed = revision_item(
        &id,
        "attestation",
        attestation_data(RUN_TESTS, PR_7, "failed", false),
        &base,
    );

    let error = refusal(&mut omhl, vec![with_snapshot(failed, &snap('a'))]);

    assert_eq!(
        error,
        rule(RuleCode::OutcomeImmutable, "/items/0/body/data/outcome")
    );
}

#[test]
fn it_should_accept_an_attestation_revision_that_only_changes_the_note() {
    let mut harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let id = attestation_id_for(&snap('a'), RUN_TESTS, PR_7);
    let base = {
        let mut omhl = harness.open();
        seed_pull_request(&mut omhl);
        let attestation = attestation_item(&snap('a'), RUN_TESTS, PR_7);

        append_ok(&mut omhl, vec![attestation]).records[0]
            .id
            .clone()
    };
    harness.set_probe(FakeProbe::failing());
    let mut omhl = harness.open();
    let mut data = attestation_data(RUN_TESTS, PR_7, "passed", true);
    data["note"] = "rechecked on CI".into();
    let revision = with_snapshot(revision_item(&id, "attestation", data, &base), &snap('a'));

    let result = omhl.append(&request(vec![revision]));

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_refuse_reactivating_a_retired_attestation() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let id = attestation_id_for(&snap('a'), RUN_TESTS, PR_7);
    let created = append_ok(
        &mut omhl,
        vec![attestation_item(&snap('a'), RUN_TESTS, PR_7)],
    );
    let retired = append_ok(
        &mut omhl,
        vec![retirement_item(&id, &created.records[0].id)],
    );
    let reactivation = with_snapshot(
        revision_item(
            &id,
            "attestation",
            attestation_data(RUN_TESTS, PR_7, "passed", true),
            &retired.records[0].id,
        ),
        &snap('a'),
    );

    let error = refusal(&mut omhl, vec![reactivation]);

    assert_eq!(
        error,
        rule(RuleCode::ReactivationUnsupported, "/items/0/base")
    );
}

#[test]
fn it_should_refuse_reactivating_a_retired_decision() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let created = append_ok(
        &mut omhl,
        vec![node_item(
            "decisions.d-1",
            "decision",
            decision_data("use sqlite", None),
        )],
    );
    let retired = append_ok(
        &mut omhl,
        vec![retirement_item("decisions.d-1", &created.records[0].id)],
    );
    let reactivation = revision_item(
        "decisions.d-1",
        "decision",
        decision_data("use sqlite", None),
        &retired.records[0].id,
    );

    let error = refusal(&mut omhl, vec![reactivation]);

    assert_eq!(
        error,
        rule(RuleCode::ReactivationUnsupported, "/items/0/base")
    );
}

#[test]
fn it_should_refuse_reactivating_a_retired_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let created = append_ok(
        &mut omhl,
        vec![node_item(
            "premises.p-1",
            "premise",
            premise_data("tests are fast", None),
        )],
    );
    let retired = append_ok(
        &mut omhl,
        vec![retirement_item("premises.p-1", &created.records[0].id)],
    );
    let reactivation = revision_item(
        "premises.p-1",
        "premise",
        premise_data("tests are fast", None),
        &retired.records[0].id,
    );

    let error = refusal(&mut omhl, vec![reactivation]);

    assert_eq!(
        error,
        rule(RuleCode::ReactivationUnsupported, "/items/0/base")
    );
}

#[test]
fn it_should_refuse_a_node_id_outside_the_namespace_of_its_type() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let misplaced_check = node_item(
        "lint.docs",
        "check",
        check_data("before-commit", "run the linter", true),
    );
    let misplaced_decision = node_item("checks.x", "decision", decision_data("use sqlite", None));

    let errors = [
        refusal(&mut omhl, vec![misplaced_check]),
        refusal(&mut omhl, vec![misplaced_decision]),
    ];

    assert_eq!(
        errors,
        [
            rule(RuleCode::IdNamespace, ITEM_ID_POINTER),
            rule(RuleCode::IdNamespace, ITEM_ID_POINTER)
        ]
    );
}

#[test]
fn it_should_refuse_reactivating_a_retired_check() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let data = check_data("before-commit", "run the linter", true);
    let created = append_ok(&mut omhl, vec![node_item(LINT_DOCS, "check", data.clone())]);
    let retired = append_ok(
        &mut omhl,
        vec![retirement_item(LINT_DOCS, &created.records[0].id)],
    );
    let reactivation = revision_item(LINT_DOCS, "check", data, &retired.records[0].id);

    let error = refusal(&mut omhl, vec![reactivation]);

    assert_eq!(
        error,
        rule(RuleCode::ReactivationUnsupported, "/items/0/base")
    );
}

#[test]
fn it_should_refuse_a_premise_or_a_decision_linked_to_a_missing_work_item() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let premise = node_item(
        "premises.p-1",
        "premise",
        premise_data("tests are fast", Some("work-items.pr-9")),
    );
    let decision = node_item(
        "decisions.d-1",
        "decision",
        decision_data("use sqlite", Some("work-items.pr-9")),
    );

    let errors = [
        refusal(&mut omhl, vec![premise]),
        refusal(&mut omhl, vec![decision]),
    ];

    assert_eq!(
        errors,
        [
            rule(RuleCode::WorkItemNotFound, WORK_ITEM_POINTER),
            rule(RuleCode::WorkItemNotFound, WORK_ITEM_POINTER)
        ]
    );
}

#[test]
fn it_should_accept_a_decision_linked_to_an_open_work_item() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let decision = node_item(
        "decisions.d-1",
        "decision",
        decision_data("use sqlite", Some(PR_7)),
    );

    let result = omhl.append(&request(vec![decision]));

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_refuse_more_than_eight_where_eq_pairs_and_accept_eight() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();

    let nine = omhl
        .list(&acme(), &filter_with(int_pairs(9)), &page())
        .map(|found| found.items.len());
    let eight = omhl
        .list(&acme(), &filter_with(int_pairs(8)), &page())
        .map(|found| found.items.len());

    assert_eq!(
        (nine, eight),
        (
            Err(rule(RuleCode::WhereEqTooMany, "/filter/where_eq")),
            Ok(0)
        )
    );
}

#[test]
fn it_should_refuse_a_where_eq_string_above_256_characters_and_accept_256() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    // "é" ocupa 2 bytes: o teto conta caracteres, não bytes.
    let at_ceiling = "é".repeat(256);
    let above_ceiling = "é".repeat(257);

    let above = omhl
        .list(
            &acme(),
            &filter_with(pairs_with_text(&above_ceiling)),
            &page(),
        )
        .map(|found| found.items.len());
    let at = omhl
        .list(&acme(), &filter_with(pairs_with_text(&at_ceiling)), &page())
        .map(|found| found.items.len());

    assert_eq!(
        (above, at),
        (
            Err(rule(RuleCode::WhereEqValueTooLong, "/filter/where_eq/1")),
            Ok(0)
        )
    );
}

#[test]
fn it_should_pass_a_premise_append_through_without_consulting_the_probe() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let data = premise_data("tests are fast", None);
    let premise = node_item("premises.p-1", "premise", data.clone());

    let outcome = append_ok(&mut omhl, vec![premise]);
    let stored = omhl
        .get(&acme(), &sid("premises.p-1"))
        .expect("the node is read")
        .map(|node| node.data);

    assert_eq!(
        (outcome.replayed, outcome.records.len(), stored),
        (false, 1, Some(data))
    );
}

#[test]
fn it_should_check_every_item_of_the_batch_before_writing_any() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let head_before = omhl.chain_head(&acme()).expect("the head is read");
    let valid = node_item(
        "premises.p-1",
        "premise",
        premise_data("tests are fast", None),
    );
    let misplaced = node_item(
        "lint.docs",
        "check",
        check_data("before-commit", "run the linter", true),
    );

    let error = refusal(&mut omhl, vec![valid, misplaced]);
    let head_after = omhl.chain_head(&acme()).expect("the head is read");

    assert_eq!(
        (error, head_after),
        (rule(RuleCode::IdNamespace, "/items/1/body/id"), head_before)
    );
}

#[test]
fn it_should_refuse_an_attestation_whose_check_is_a_native_guideline() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation = attestation_item(&snap('a'), "guidelines.commit-policy", PR_7);

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(
        error,
        rule(RuleCode::CheckNotFound, "/items/0/body/data/check")
    );
}

#[test]
fn it_should_refuse_reopening_a_closed_work_item_through_a_retirement() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let closed = close(&mut omhl, PR_7, &snap('a'));
    let retired = append_ok(
        &mut omhl,
        vec![retirement_item(PR_7, &closed.outcome.records[0].id)],
    );
    let reopening = revision_item(
        PR_7,
        "pull-request",
        pull_request_data("Add gate"),
        &retired.records[0].id,
    );

    let error = refusal(&mut omhl, vec![reopening]);

    assert_eq!(
        error,
        rule(RuleCode::ReactivationUnsupported, "/items/0/base")
    );
}

#[test]
fn it_should_refuse_an_attestation_whose_work_item_id_is_not_a_work_item() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    append_ok(
        &mut omhl,
        vec![node_item(
            "decisions.d1",
            "decision",
            decision_data("use sqlite", None),
        )],
    );
    let attestation = attestation_item(&snap('a'), RUN_TESTS, "decisions.d1");

    let error = refusal(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::WorkItemNotFound, WORK_ITEM_POINTER));
}

#[test]
fn it_should_accept_a_git_tree_attestation_for_the_staged_tree() {
    let harness = OmhlHarness::new(staged_tree_probe());
    let mut omhl = harness.open();
    seed_pull_request_and_check(&mut omhl, FMT, "before-commit");
    let attestation = executor_attestation(&tree('c'), FMT);

    let result = append_as(&mut omhl, executor_author(), vec![attestation]);

    assert_eq!(result.map(|outcome| outcome.records.len()), Ok(1));
}

#[test]
fn it_should_refuse_a_git_tree_attestation_for_another_tree() {
    let harness = OmhlHarness::new(staged_tree_probe());
    let mut omhl = harness.open();
    seed_pull_request_and_check(&mut omhl, FMT, "before-commit");
    let attestation = executor_attestation(&tree('d'), FMT);

    let error = refusal_as_executor(&mut omhl, vec![attestation]);

    assert_eq!(
        (error, attestation_count(&mut omhl)),
        (rule(RuleCode::SnapshotTreeMismatch, SNAPSHOT_POINTER), 0)
    );
}

#[test]
fn it_should_accept_a_git_tree_attestation_for_the_head_tree_when_the_index_equals_it() {
    let repo = GitRepo::init();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    repo.git(&["switch", "-c", "feat/wi-w1__x"]);
    let git_probe = GitProbe::new(repo.path());
    let raw = RawHarness::new();
    let deps = OmhlDeps {
        clock: &raw.clock,
        ids: &raw.ids,
        probe: &git_probe,
    };
    let mut omhl = Omhl::open(&raw.config(), deps).expect("omhl opens");
    omhl.ensure_tenant(&acme()).expect("the tenant is created");
    seed_pull_request_and_check(&mut omhl, FMT, "before-commit");
    let head_tree = Snapshot::new(SnapshotKind::GitTree, HEAD_TREE).expect("valid snapshot");
    let attestation = executor_attestation(&head_tree, FMT);

    let result = append_as(&mut omhl, executor_author(), vec![attestation]);

    assert_eq!(
        (
            result.map(|outcome| outcome.records.len()),
            repo.git(&["rev-parse", "HEAD^{tree}"]).trim().to_owned()
        ),
        (Ok(1), HEAD_TREE.to_owned())
    );
}

#[test]
fn it_should_refuse_a_git_tree_attestation_when_the_working_tree_differs_from_the_index() {
    let probe = FakeProbe::clean(snap('a')).with_tree(TreeSource::Index, Ok(ProbeOutcome::Dirty));
    let harness = OmhlHarness::new(probe);
    let mut omhl = harness.open();
    seed_pull_request_and_check(&mut omhl, FMT, "before-commit");
    let attestation = executor_attestation(&tree('c'), FMT);

    let error = refusal_as_executor(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::SnapshotDirty, SNAPSHOT_POINTER));
}

#[test]
fn it_should_keep_refusing_a_git_commit_attestation_on_a_dirty_tree() {
    let probe =
        FakeProbe::dirty().with_tree(TreeSource::Index, Ok(ProbeOutcome::Reproducible(tree('a'))));
    let harness = OmhlHarness::new(probe);
    let mut omhl = harness.open();
    seed_pull_request_and_check(&mut omhl, LINT, "before-pr");
    let attestation = executor_attestation(&snap('a'), LINT);

    let error = refusal_as_executor(&mut omhl, vec![attestation]);

    assert_eq!(error, rule(RuleCode::SnapshotDirty, SNAPSHOT_POINTER));
}

#[test]
fn it_should_answer_snapshot_tree_mismatch_with_its_own_code() {
    let harness = OmhlHarness::new(staged_tree_probe());
    let mut omhl = harness.open();
    seed_pull_request_and_check(&mut omhl, FMT, "before-commit");
    let attestation = executor_attestation(&tree('d'), FMT);

    let error = refusal_as_executor(&mut omhl, vec![attestation]);

    assert_eq!(
        (
            error.to_string(),
            RuleCode::SnapshotTreeMismatch.as_str(),
            RuleCode::SnapshotTreeMismatch != RuleCode::SnapshotNotHead
        ),
        (
            "snapshot-tree-mismatch at /items/0/snapshot".to_owned(),
            "snapshot-tree-mismatch",
            true
        )
    );
}
