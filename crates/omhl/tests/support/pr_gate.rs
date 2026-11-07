use ::omhl::facade::{ClosedWorkItem, Omhl};
use ::omhl::gate::{GateReport, QuestionReport};
use ::omhl::native::NativeCheck;
use hexlog::{
    AppendItem, AppendOutcome, ItemBody, Lineage, NodeView, RecordId, SemanticId, Snapshot,
};
use serde_json::{Value, json};

use super::omhl::{
    PR_7, RUN_TESTS, acme, append_ok, attestation_data, attestation_id_for, check_data,
    decision_data, node_item, premise_data, with_snapshot,
};
use super::{DEFAULT_INSTANT, author, rty, sid, tname};

pub const CI_GREEN: &str = "checks.ci-green-before-merge";
pub const FEATURE_1: &str = "work-items.feature-1";
pub const PREMISE: &str = "premises.sqlite-is-fast";
pub const DECISION: &str = "decisions.use-sqlite";
pub const NO_OPEN_CONTRADICTION: &str = "no-open-contradiction";

const RECORD_ID: &str = "acme:0192a000-0000-7000-8000-000000000001";

// Entradas dos testes puros

pub fn sids(ids: &[&str]) -> Vec<SemanticId> {
    ids.iter().map(|id| sid(id)).collect()
}

pub fn native_check(id: &'static str, event: &'static str, blocking: bool) -> NativeCheck {
    NativeCheck {
        id,
        event,
        how: "run it",
        blocking,
    }
}

/// Os dois checks que o gate cobra por padrão, na ordem de `NativeBase::checks()`.
pub fn default_natives() -> Vec<NativeCheck> {
    vec![
        native_check(RUN_TESTS, "before-commit", true),
        native_check(CI_GREEN, "before-merge", true),
    ]
}

/// `NodeView` de um check do projeto, montado por literal.
pub fn project_check(id: &str, event: &str, blocking: bool, shadowed: bool) -> NodeView {
    NodeView {
        id: sid(id),
        type_name: tname("check"),
        record_id: RecordId::parse(RECORD_ID).expect("valid record id"),
        seq: 1,
        at: DEFAULT_INSTANT.to_owned(),
        data: check_data(event, "run it", blocking),
        snapshot: None,
        external_id: None,
        author: author(),
        shadowed,
    }
}

// Escrita pelo `Omhl`

pub fn relation_item(relation_type: &str, from: &str, to: &str) -> AppendItem {
    AppendItem {
        body: ItemBody::Relation {
            relation_type: rty(relation_type),
            from: sid(from),
            to: sid(to),
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

pub fn relation_retirement_item(
    relation_type: &str,
    from: &str,
    to: &str,
    base: &RecordId,
) -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: Lineage::Relation {
                relation_type: rty(relation_type),
                from: sid(from),
                to: sid(to),
            },
        },
        base: Some(base.clone()),
        snapshot: None,
        external_id: None,
    }
}

/// Atestado criado em `snapshot` com o id derivado.
pub fn attest(
    omhl: &mut Omhl<'_>,
    snapshot: &Snapshot,
    check: &str,
    outcome: &str,
    ok: bool,
) -> AppendOutcome {
    let id = attestation_id_for(snapshot, check, PR_7);
    let item = with_snapshot(
        node_item(
            &id,
            "attestation",
            attestation_data(check, PR_7, outcome, ok),
        ),
        snapshot,
    );

    append_ok(omhl, vec![item])
}

/// Atestado `passed` dos dois checks nativos que o gate cobra, para `work-items.pr-7`.
pub fn attest_required(omhl: &mut Omhl<'_>, snapshot: &Snapshot) {
    attest(omhl, snapshot, RUN_TESTS, "passed", true);
    attest(omhl, snapshot, CI_GREEN, "passed", true);
}

/// Decision de `work-items.pr-7` contradita por uma premise. Devolve o record da relation `contradicts`.
pub fn seed_contradicted_decision(omhl: &mut Omhl<'_>) -> RecordId {
    append_ok(
        omhl,
        vec![
            node_item(PREMISE, "premise", premise_data("SQLite is fast", None)),
            node_item(
                DECISION,
                "decision",
                decision_data("Use SQLite", Some(PR_7)),
            ),
        ],
    );

    append_ok(omhl, vec![relation_item("contradicts", PREMISE, DECISION)]).records[0]
        .id
        .clone()
}

// Leitura

pub fn gate_of(omhl: &mut Omhl<'_>, snapshot: &Snapshot) -> GateReport {
    omhl.evaluate_pr_gate(&acme(), &sid(PR_7), snapshot)
        .expect("the gate is evaluated")
}

pub fn close(omhl: &mut Omhl<'_>, work_item: &str, snapshot: &Snapshot) -> ClosedWorkItem {
    omhl.close_work_item(&acme(), &sid(work_item), snapshot, &author())
        .expect("the work item is closed")
}

/// `data` corrente do node.
pub fn data_of(omhl: &mut Omhl<'_>, id: &str) -> Value {
    omhl.get(&acme(), &sid(id))
        .expect("the node is read")
        .expect("the node exists")
        .data
}

pub fn head_seq(omhl: &mut Omhl<'_>) -> u64 {
    omhl.chain_head(&acme()).expect("the head is read").seq
}

pub fn question<'r>(report: &'r GateReport, label: &str) -> &'r QuestionReport {
    report
        .questions
        .iter()
        .find(|question| question.label == label)
        .expect("a question with this label exists")
}

/// `(label, passed)` de cada pergunta, na ordem do relatório.
pub fn outcomes(report: &GateReport) -> Vec<(String, bool)> {
    report
        .questions
        .iter()
        .map(|question| (question.label.clone(), question.passed))
        .collect()
}

/// `data` esperada de um work item fechado: a aberta, com `status` e o objeto `gate`.
pub fn closed_data(open: Value, passed: bool, chain_head_seq: u64, snapshot: &Snapshot) -> Value {
    let mut data = open;
    data["status"] = json!("closed");
    data["gate"] = json!({
        "name": "pull-request",
        "passed": passed,
        "chain_head_seq": chain_head_seq,
        "snapshot": { "kind": "git-commit", "id": snapshot.id() },
    });

    data
}
