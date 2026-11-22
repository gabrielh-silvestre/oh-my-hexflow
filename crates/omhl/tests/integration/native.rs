use crate::support;

use hexlog::{
    AppendItem, AppendRequest, Catalog, DataViolation, EndpointStatus, ErrorCode, Hexlog,
    HexlogError, ItemBody, NativeNode, NativeNodes, Origin, RelationQuery, RelationView,
    SemanticId, Snapshot, SnapshotKind,
};
use omhl::graphs;
use omhl::native::{NativeBase, NativeCheck, NativeGuideline};
use omhl::pr_gate::{REQUIRED_EVENTS, required_checks};
use omhl::probe::{ProbeOutcome, TreeSource};
use serde_json::{Value, json};
use support::omhl::{OmhlHarness, check_data, seed_pull_request};
use support::pr_gate::{attest, expected_outcomes, gate_of, outcomes, required_pairs, snapshots};
use support::raw::RawHarness;
use support::{FakeProbe, author, rty, sid, snap, tenant, tname};

const CHECK_IDS: [&str; 3] = [
    "checks.run-tests",
    "checks.ci-green-before-merge",
    "checks.verify-review-comment-anchoring",
];
const GUIDELINE_IDS: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];

const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const LINT_DOCS: &str = "checks.lint-docs";
// Exceção documentada (ADR 0003): o evento `before-publishing-review-comments` deste check nativo está fora do
// enum de eventos do tipo `check`; ele não passa pelo catálogo e não tem schema próprio.
const REVIEW_CHECK_EVENT_EXCEPTION: &str = "checks.verify-review-comment-anchoring";

/// Os cinco ids nativos na ordem do contrato, com o tipo de cada um.
fn native_ids_with_types() -> Vec<(&'static str, &'static str)> {
    let checks = CHECK_IDS.iter().map(|id| (*id, "check"));
    let guidelines = GUIDELINE_IDS.iter().map(|id| (*id, "guideline"));

    checks.chain(guidelines).collect()
}

fn catalog() -> Catalog {
    graphs::catalog().expect("the embedded catalogs build")
}

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn guideline_data(text: &str) -> Value {
    json!({ "text": text })
}

fn node_item(id: &str, type_name: &str, data: Value) -> AppendItem {
    AppendItem {
        body: ItemBody::Node {
            id: sid(id),
            type_name: tname(type_name),
            data,
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

fn project_check_item(id: &str) -> AppendItem {
    node_item(
        id,
        "check",
        check_data("before-commit", "run the linter", true),
    )
}

fn after_item(from: &str, to: &str) -> AppendItem {
    AppendItem {
        body: ItemBody::Relation {
            relation_type: rty("after"),
            from: sid(from),
            to: sid(to),
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

fn request(items: Vec<AppendItem>) -> AppendRequest {
    AppendRequest {
        tenant: tenant("acme"),
        author: author(),
        key: None,
        items,
    }
}

/// Abre o `hexlog` real com o catalog e a base nativa reais e já cria o tenant.
fn open<'a>(harness: &'a RawHarness, native: &'a NativeBase) -> Hexlog<'a> {
    let mut hexlog = harness.hexlog(catalog(), native);
    hexlog
        .ensure_tenant(&tenant("acme"))
        .expect("the tenant is created");

    hexlog
}

fn refusal(hexlog: &mut Hexlog<'_>, items: Vec<AppendItem>) -> HexlogError {
    hexlog
        .append(&request(items))
        .expect_err("the append is refused")
}

/// Código do erro e, de cada detalhe, o par (código, ponteiro).
fn refusal_summary(error: &HexlogError) -> (ErrorCode, Vec<(&str, &str)>) {
    let details = error
        .details
        .iter()
        .map(|detail| (detail.code.as_str(), detail.path.as_str()))
        .collect();

    (error.code, details)
}

fn after_relations(hexlog: &mut Hexlog<'_>) -> Vec<RelationView> {
    let query = RelationQuery {
        relation_type: Some(rty("after")),
        from: None,
        to: None,
        include_retired: false,
    };

    hexlog
        .relations(&tenant("acme"), &query)
        .expect("the relations are read")
}

/// Tudo que o nativo expõe, copiado para comparar antes e depois de uma escrita do projeto.
type NativeContent = (
    Vec<NativeCheck>,
    Vec<NativeGuideline>,
    Vec<Option<NativeNode>>,
    Vec<(SemanticId, SemanticId)>,
);

fn native_content(native: &NativeBase) -> NativeContent {
    let looked_up = native_ids_with_types()
        .iter()
        .map(|(id, _)| native.lookup(&sid(id)))
        .collect();

    (
        native.checks().to_vec(),
        native.guidelines().to_vec(),
        looked_up,
        native.native_relations(&rty("after")),
    )
}

#[test]
fn it_should_validate_every_native_check_but_the_review_one_against_the_check_type() {
    let native = NativeBase;
    let catalog = catalog();

    let rejected: Vec<(&str, Vec<DataViolation>)> = native
        .checks()
        .iter()
        .filter_map(|check| {
            let data = check_data(check.event, check.how, check.blocking);
            catalog
                .validate_data(&tname("check"), &data)
                .err()
                .map(|violations| (check.id, violations))
        })
        .collect();

    assert_eq!(
        rejected,
        vec![(
            REVIEW_CHECK_EVENT_EXCEPTION,
            vec![DataViolation {
                pointer: "/when/event".to_owned(),
                code: "enum".to_owned()
            }]
        )]
    );
}

#[test]
fn it_should_validate_every_native_guideline_against_the_guideline_type() {
    let native = NativeBase;
    let catalog = catalog();

    let rejected: Vec<(&str, Vec<DataViolation>)> = native
        .guidelines()
        .iter()
        .filter_map(|guideline| {
            let data = guideline_data(guideline.text);
            catalog
                .validate_data(&tname("guideline"), &data)
                .err()
                .map(|violations| (guideline.id, violations))
        })
        .collect();

    assert_eq!(rejected, Vec::new());
}

#[test]
fn it_should_refuse_a_tenant_node_with_each_native_id() {
    let harness = RawHarness::new();
    let native = NativeBase;
    let mut hexlog = open(&harness, &native);
    let expected = (
        ErrorCode::ReservedId,
        vec![("reserved-id", "/items/0/body/id")],
    );

    let errors: Vec<HexlogError> = native_ids_with_types()
        .iter()
        .map(|(id, type_name)| {
            let data = match *type_name {
                "check" => check_data("before-commit", "run the linter", true),
                _ => guideline_data("a project guideline"),
            };
            refusal(&mut hexlog, vec![node_item(id, type_name, data)])
        })
        .collect();

    let summaries: Vec<_> = errors.iter().map(refusal_summary).collect();
    assert_eq!(summaries, vec![expected; 5]);
}

#[test]
fn it_should_accept_a_project_check_linked_after_a_native_check() {
    let harness = RawHarness::new();
    let native = NativeBase;
    let mut hexlog = open(&harness, &native);
    let items = vec![
        project_check_item(LINT_DOCS),
        after_item(LINT_DOCS, RUN_TESTS),
    ];

    hexlog
        .append(&request(items))
        .expect("the project check and the relation are accepted");

    let relation = after_relations(&mut hexlog);
    let view: Vec<_> = relation
        .iter()
        .map(|r| {
            (
                r.from_origin,
                r.to_origin,
                r.to_status,
                r.native_catalog.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        view,
        vec![(
            Origin::Tenant,
            Origin::Native,
            EndpointStatus::Current,
            Some("1.0.0")
        )]
    );
}

#[test]
fn it_should_accept_a_project_check_linked_before_a_native_check() {
    let harness = RawHarness::new();
    let native = NativeBase;
    let mut hexlog = open(&harness, &native);
    let items = vec![
        project_check_item(LINT_DOCS),
        after_item(RUN_TESTS, LINT_DOCS),
    ];

    hexlog
        .append(&request(items))
        .expect("the project check and the relation are accepted");

    let relation = after_relations(&mut hexlog);
    let origins: Vec<_> = relation
        .iter()
        .map(|r| (r.from_origin, r.to_origin))
        .collect();
    assert_eq!(origins, vec![(Origin::Native, Origin::Tenant)]);
}

#[test]
fn it_should_not_change_the_native_content_when_the_project_links_to_it() {
    let harness = RawHarness::new();
    let native = NativeBase;
    let mut hexlog = open(&harness, &native);
    let before = native_content(&native);

    hexlog
        .append(&request(vec![
            project_check_item(LINT_DOCS),
            after_item(LINT_DOCS, RUN_TESTS),
        ]))
        .expect("the project check and the relation are accepted");

    let after = native_content(&native);
    assert_eq!(after, before);
}

#[test]
fn it_should_refuse_a_relation_between_two_native_nodes() {
    let harness = RawHarness::new();
    let native = NativeBase;
    let mut hexlog = open(&harness, &native);

    let error = refusal(&mut hexlog, vec![after_item(CI_GREEN, RUN_TESTS)]);

    assert_eq!(
        refusal_summary(&error),
        (
            ErrorCode::InvalidRecord,
            vec![("native-only-relation", "/items/0/body")]
        )
    );
}

#[test]
fn it_should_reject_a_cycle_that_runs_through_the_native_relation() {
    let harness = RawHarness::new();
    let native = NativeBase;
    let mut hexlog = open(&harness, &native);
    hexlog
        .append(&request(vec![
            project_check_item(LINT_DOCS),
            after_item(RUN_TESTS, LINT_DOCS),
        ]))
        .expect("the first relation is accepted");

    let error = refusal(&mut hexlog, vec![after_item(LINT_DOCS, CI_GREEN)]);

    assert_eq!(error.code, ErrorCode::CycleRejected);
}

#[test]
fn it_should_keep_the_native_checks_on_git_commit_snapshots() {
    let probe = FakeProbe::clean(snap('a'))
        .with_tree(TreeSource::Index, Ok(ProbeOutcome::Reproducible(tree('c'))));
    let harness = OmhlHarness::new(probe);
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let at_head = snapshots(&snap('a'), &tree('c'));
    attest(&mut omhl, &tree('c'), RUN_TESTS, "passed", true);
    attest(&mut omhl, &tree('c'), CI_GREEN, "passed", true);
    let on_the_tree = gate_of(&mut omhl, &at_head);
    attest(&mut omhl, &snap('a'), RUN_TESTS, "passed", true);
    attest(&mut omhl, &snap('a'), CI_GREEN, "passed", true);

    let on_the_commit = gate_of(&mut omhl, &at_head);

    assert_eq!(
        outcomes(&on_the_tree),
        expected_outcomes(&[
            (RUN_TESTS, false),
            (CI_GREEN, false),
            ("no-open-contradiction", true),
            ("decisions-anchored", true),
            ("anchors-confirmed", true),
        ])
    );
    assert_eq!(
        outcomes(&on_the_commit),
        expected_outcomes(&[
            (RUN_TESTS, true),
            (CI_GREEN, true),
            ("no-open-contradiction", true),
            ("decisions-anchored", true),
            ("anchors-confirmed", true),
        ])
    );
}

#[test]
fn it_should_keep_the_native_review_check_in_the_catalog_and_out_of_the_required_events() {
    let checks = NativeBase.checks();
    let ids: Vec<&str> = checks.iter().map(|check| check.id).collect();
    let review = checks
        .iter()
        .find(|check| check.id == "checks.verify-review-comment-anchoring")
        .expect("the review check is in the catalog");

    let required = required_checks(checks, &[]);

    assert_eq!(ids, CHECK_IDS.to_vec());
    assert_eq!(
        (review.event, review.blocking),
        ("before-publishing-review-comments", true)
    );
    assert!(!REQUIRED_EVENTS.contains(&"before-publishing-review-comments"));
    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, SnapshotKind::GitCommit),
            (CI_GREEN, SnapshotKind::GitCommit),
        ])
    );
}
