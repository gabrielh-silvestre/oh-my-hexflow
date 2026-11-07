use std::iter::once;

use hexlog::{
    AppendItem, AppendRequest, Author, FieldName, ItemBody, ListFilter, NodeView, Scalar,
    SemanticId, Snapshot, TenantId, TypeName,
};
use serde_json::json;

use crate::gate::{GateQuestion, GateReport, Question};
use crate::native::NativeCheck;

pub const GATE_NAME: &str = "pull-request";
pub const REQUIRED_EVENTS: [&str; 2] = ["before-commit", "before-merge"];

const NO_OPEN_CONTRADICTION_LABEL: &str = "no-open-contradiction";

/// Ids dos checks que o gate cobra: primeiro os nativos, depois os do projeto, sem repetir id.
pub fn required_checks(native: &[NativeCheck], tenant_checks: &[NodeView]) -> Vec<SemanticId> {
    let natives = native
        .iter()
        .filter(|check| check.blocking && REQUIRED_EVENTS.contains(&check.event))
        .filter_map(|check| SemanticId::parse(check.id).ok());
    let projects = tenant_checks
        .iter()
        .filter(|node| is_required_project_check(node))
        .map(|node| node.id.clone());
    let mut required = Vec::new();
    for id in natives.chain(projects) {
        if !required.contains(&id) {
            required.push(id);
        }
    }

    required
}

/// Um check do projeto sombreado por um nativo é ignorado: o nativo manda.
fn is_required_project_check(node: &NodeView) -> bool {
    let covered_event = node.data["when"]["event"]
        .as_str()
        .is_some_and(|event| REQUIRED_EVENTS.contains(&event));

    !node.shadowed && node.data["blocking"] == true && covered_event
}

/// Um `Occurred` por check obrigatório, na ordem, e depois um `NoOpenContradiction`.
pub fn pr_gate_questions(
    required: &[SemanticId],
    work_item: &SemanticId,
    snapshot: &Snapshot,
) -> Vec<GateQuestion> {
    required
        .iter()
        .map(|check| GateQuestion {
            label: check.as_str().to_owned(),
            question: Question::Occurred {
                select: attestation_filter(check, work_item, snapshot),
                min: 1,
            },
        })
        .chain(once(contradiction_question(work_item)))
        .collect()
}

fn attestation_filter(
    check: &SemanticId,
    work_item: &SemanticId,
    snapshot: &Snapshot,
) -> ListFilter {
    ListFilter {
        type_name: Some(type_name("attestation")),
        id_prefix: None,
        snapshot: Some(snapshot.clone()),
        where_eq: vec![
            pair("check", Scalar::Str(check.as_str().to_owned())),
            pair("work_item", Scalar::Str(work_item.as_str().to_owned())),
            pair("ok", Scalar::Bool(true)),
        ],
    }
}

fn contradiction_question(work_item: &SemanticId) -> GateQuestion {
    let decisions = ListFilter {
        type_name: Some(type_name("decision")),
        id_prefix: None,
        snapshot: None,
        where_eq: vec![pair(
            "work_item",
            Scalar::Str(work_item.as_str().to_owned()),
        )],
    };

    GateQuestion {
        label: NO_OPEN_CONTRADICTION_LABEL.to_owned(),
        question: Question::NoOpenContradiction {
            of: Some(decisions),
        },
    }
}

/// Filtro dos checks do projeto que o gate lê.
pub(crate) fn project_checks_filter() -> ListFilter {
    ListFilter {
        type_name: Some(type_name("check")),
        id_prefix: Some("checks".to_owned()),
        snapshot: None,
        where_eq: Vec::new(),
    }
}

/// Revision do work item com `status: closed` e o resultado do gate; vai direto ao `Hexlog`.
pub(crate) fn closing_request(
    tenant: &TenantId,
    author: &Author,
    current: NodeView,
    report: &GateReport,
    snapshot: &Snapshot,
) -> AppendRequest {
    let mut data = current.data;
    data["status"] = json!("closed");
    data["gate"] = json!({
        "name": GATE_NAME,
        "passed": report.passed,
        "chain_head_seq": report.chain_head.seq,
        "snapshot": snapshot,
    });
    let item = AppendItem {
        body: ItemBody::Node {
            id: current.id,
            type_name: current.type_name,
            data,
        },
        base: Some(current.record_id),
        snapshot: None,
        external_id: None,
    };

    AppendRequest {
        tenant: tenant.clone(),
        author: author.clone(),
        key: None,
        items: vec![item],
    }
}

fn type_name(name: &str) -> TypeName {
    TypeName::parse(name).expect("the gate type names are valid constants")
}

fn pair(name: &str, value: Scalar) -> (FieldName, Scalar) {
    let field = FieldName::parse(name).expect("the gate field names are valid constants");

    (field, value)
}
