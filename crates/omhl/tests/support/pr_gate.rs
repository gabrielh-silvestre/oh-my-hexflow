use ::omhl::facade::{ClosedWorkItem, Omhl};
use ::omhl::gate::{GateReport, QuestionReport};
use ::omhl::native::NativeCheck;
use ::omhl::pr_gate::GateSnapshots;
use ::omhl::workflow::config::{NativeConfig, WorkflowConfig};
use hexlog::{
    AppendItem, AppendOutcome, ItemBody, Lineage, NodeView, RecordId, SemanticId, Snapshot,
    SnapshotKind,
};
use serde_json::{Value, json};

use super::omhl::{
    PR_7, RUN_TESTS, acme, append_ok, append_ok_as, attestation_data, attestation_id_for,
    check_data, decision_data, node_item, premise_data, with_snapshot,
};
use super::{DEFAULT_INSTANT, author, loader_author, role_author, rty, sid, tname};

pub const CI_GREEN: &str = "checks.ci-green-before-merge";
pub const FEATURE_1: &str = "work-items.feature-1";
pub const PREMISE: &str = "premises.sqlite-is-fast";
pub const DECISION: &str = "decisions.use-sqlite";
pub const ANCHOR: &str = "premises.users-pay-monthly";
pub const CRITIC_DECISION: &str = "decisions.use-postgres";
pub const NO_OPEN_CONTRADICTION: &str = "no-open-contradiction";
pub const DECISIONS_ANCHORED: &str = "decisions-anchored";
pub const ANCHORS_CONFIRMED: &str = "anchors-confirmed";

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

pub const VERIFY_ANCHORING: &str = "checks.verify-review-comment-anchoring";
/// O fixture padrão das camadas que não escolhem liga todos: o nativo desligado some do gate e dos testes.
pub const ALL_NATIVE_CHECKS: [&str; 3] = [RUN_TESTS, CI_GREEN, VERIFY_ANCHORING];

pub fn native_enabled(ids: &[&'static str]) -> NativeConfig {
    NativeConfig {
        checks: ids.iter().copied().collect(),
        // A tabela que um projeto sem `workflow.toml` lê: a dos padrões do binário.
        prefixes: WorkflowConfig::default().prefixes,
    }
}

pub fn all_native_enabled() -> NativeConfig {
    native_enabled(&ALL_NATIVE_CHECKS)
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

pub fn mechanical_check_data(event: &str, how: &str, blocking: bool) -> Value {
    let mut data = check_data(event, how, blocking);
    data["run"] = json!(["true"]);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{"exit": 0, "outcome": "passed"}]);

    data
}

/// Como `project_check`, mas com `run`, `timeout_secs` e `expect` e gravado pelo `loader`.
pub fn mechanical_project_check(id: &str, event: &str, blocking: bool, shadowed: bool) -> NodeView {
    NodeView {
        data: mechanical_check_data(event, "run it", blocking),
        author: loader_author(),
        ..project_check(id, event, blocking, shadowed)
    }
}

/// O que `required_checks` devolve, escrito por ids de texto: `(id, kind)` na ordem dada.
pub fn required_pairs(pairs: &[(&str, SnapshotKind)]) -> Vec<(SemanticId, SnapshotKind)> {
    pairs.iter().map(|(id, kind)| (sid(id), *kind)).collect()
}

pub fn snapshots(commit: &Snapshot, tree: &Snapshot) -> GateSnapshots {
    GateSnapshots {
        head_commit: commit.clone(),
        head_tree: tree.clone(),
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

/// Premissa confirmada por uma pessoa, com `confirmed_by`.
pub fn confirmed_premise_data(text: &str) -> Value {
    let mut data = premise_data(text, None);
    data["status"] = json!("confirmed");
    data["confirmed_by"] = json!({
        "by": "person:ana",
        "question": "Is this premise right?",
        "answer": "Yes, it is.",
    });

    data
}

/// Apoia `DECISION` em `ANCHOR`, uma premissa confirmada com `confirmed_by`. Devolve o record da relation `rests-on`.
pub fn anchor_decision(omhl: &mut Omhl<'_>) -> RecordId {
    append_ok(
        omhl,
        vec![node_item(
            ANCHOR,
            "premise",
            confirmed_premise_data("Users pay monthly"),
        )],
    );

    append_ok(omhl, vec![relation_item("rests-on", DECISION, ANCHOR)]).records[0]
        .id
        .clone()
}

/// Decision de `work-items.pr-7` contradita por uma premise, e ancorada em `ANCHOR`. Devolve o record da relation `contradicts`.
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
    anchor_decision(omhl);

    append_ok(omhl, vec![relation_item("contradicts", PREMISE, DECISION)]).records[0]
        .id
        .clone()
}

/// `DECISION` do architect e `CRITIC_DECISION` do critic, ambas de `work-items.pr-7` e ancoradas em `ANCHOR`
/// (a do critic só se `critic_anchored`); o critic grava `contradicts` da sua para a do architect, a única
/// direção que a matriz deixa. Devolve o record da relation `contradicts`. O work item já deve existir.
pub fn seed_critic_contradiction(omhl: &mut Omhl<'_>, critic_anchored: bool) -> RecordId {
    let architect = role_author("omhl:architect");
    let critic = role_author("omhl:critic");
    append_ok(
        omhl,
        vec![node_item(
            ANCHOR,
            "premise",
            confirmed_premise_data("Users pay monthly"),
        )],
    );
    append_ok_as(
        omhl,
        architect.clone(),
        vec![node_item(
            DECISION,
            "decision",
            decision_data("Use SQLite", Some(PR_7)),
        )],
    );
    append_ok_as(
        omhl,
        architect,
        vec![relation_item("rests-on", DECISION, ANCHOR)],
    );
    append_ok_as(
        omhl,
        critic.clone(),
        vec![node_item(
            CRITIC_DECISION,
            "decision",
            decision_data("Use Postgres", Some(PR_7)),
        )],
    );
    if critic_anchored {
        append_ok_as(
            omhl,
            critic.clone(),
            vec![relation_item("rests-on", CRITIC_DECISION, ANCHOR)],
        );
    }

    append_ok_as(
        omhl,
        critic,
        vec![relation_item("contradicts", CRITIC_DECISION, DECISION)],
    )
    .records[0]
        .id
        .clone()
}

// Leitura

pub fn gate_of(omhl: &mut Omhl<'_>, snapshots: &GateSnapshots) -> GateReport {
    gate_with(omhl, snapshots, &all_native_enabled())
}

pub fn gate_with(
    omhl: &mut Omhl<'_>,
    snapshots: &GateSnapshots,
    enabled: &NativeConfig,
) -> GateReport {
    omhl.evaluate_pr_gate(
        &acme(),
        &sid(PR_7),
        snapshots,
        enabled,
        &omhl::facade::WorkRequirements::none(),
    )
    .expect("the gate is evaluated")
}

pub fn close(omhl: &mut Omhl<'_>, work_item: &str, snapshots: &GateSnapshots) -> ClosedWorkItem {
    omhl.close_work_item(
        &acme(),
        &sid(work_item),
        snapshots,
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    )
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

/// `outcomes` esperado, escrito por ids de texto.
pub fn expected_outcomes(expected: &[(&str, bool)]) -> Vec<(String, bool)> {
    expected
        .iter()
        .map(|(label, passed)| ((*label).to_owned(), *passed))
        .collect()
}
