use std::iter::once;

use hexlog::{
    AppendItem, AppendRequest, Author, FieldName, ItemBody, ListFilter, NodeView, Scalar,
    SemanticId, Snapshot, SnapshotKind, TenantId, TypeName,
};
use serde_json::json;

use crate::gate::{GateQuestion, GateReport, Question};
use crate::mechanical::{RUNNABLE_EVENTS, snapshot_for, spec_digest};
use crate::native::NativeCheck;

pub const GATE_NAME: &str = "pull-request";
pub const REQUIRED_EVENTS: [&str; 3] = RUNNABLE_EVENTS;
/// Fonte única do kind de snapshot dos checks nativos.
pub const NATIVE_SNAPSHOT: SnapshotKind = SnapshotKind::GitCommit;

const NO_OPEN_CONTRADICTION_LABEL: &str = "no-open-contradiction";
/// Prefixo da pergunta que cobra, de um check com `run`, a attestation selada com a definição vigente.
const SPEC_DIGEST_LABEL: &str = "spec-digest-match";

/// Os dois snapshots que o gate usa: cada check é atestado no kind que o seu `snapshot_for` manda.
pub struct GateSnapshots {
    pub head_commit: Snapshot,
    pub head_tree: Snapshot,
}

/// Checks que o gate cobra, com o kind do snapshot de cada um: primeiro os nativos, depois os do projeto, sem repetir id.
pub fn required_checks(
    native: &[NativeCheck],
    tenant_checks: &[NodeView],
) -> Vec<(SemanticId, SnapshotKind)> {
    let projects: Vec<&NodeView> = tenant_checks
        .iter()
        .filter(|node| is_required_project_check(node))
        .collect();
    let natives = native
        .iter()
        .filter(|check| check.blocking && REQUIRED_EVENTS.contains(&check.event))
        .filter(|check| !is_replaced_by_mechanical_check(check, &projects))
        .filter_map(|check| SemanticId::parse(check.id).ok())
        .map(|id| (id, NATIVE_SNAPSHOT));
    let projects = projects
        .iter()
        .map(|node| (node.id.clone(), snapshot_for(&node.data)));
    let mut required: Vec<(SemanticId, SnapshotKind)> = Vec::new();
    for (id, kind) in natives.chain(projects) {
        if required.iter().all(|(existing, _)| *existing != id) {
            required.push((id, kind));
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

/// Um check mecânico (com `run`) do projeto, obrigatório no mesmo evento, dispensa o nativo daquele evento.
fn is_replaced_by_mechanical_check(native: &NativeCheck, projects: &[&NodeView]) -> bool {
    projects.iter().any(|node| {
        node.data.get("run").is_some() && node.data["when"]["event"].as_str() == Some(native.event)
    })
}

/// Digest da definição vigente de cada check do projeto, não sombreado, com `run`.
pub fn spec_digests(tenant_checks: &[NodeView]) -> Vec<(SemanticId, String)> {
    tenant_checks
        .iter()
        .filter(|node| !node.shadowed && node.data.get("run").is_some())
        .map(|node| (node.id.clone(), spec_digest(&node.data)))
        .collect()
}

/// Um `Occurred` por check obrigatório, na ordem; o check com `run` ganha logo depois a pergunta do selo
/// (a attestation com o `spec_digest` da definição vigente, que só o `check run` grava). Por fim, um `NoOpenContradiction`.
pub fn pr_gate_questions(
    required: &[(SemanticId, SnapshotKind)],
    work_item: &SemanticId,
    snapshots: &GateSnapshots,
    digests: &[(SemanticId, String)],
) -> Vec<GateQuestion> {
    required
        .iter()
        .flat_map(|(check, kind)| {
            let snapshot = match kind {
                SnapshotKind::GitTree => &snapshots.head_tree,
                _ => &snapshots.head_commit,
            };
            let sealed = digests
                .iter()
                .find(|(id, _)| id == check)
                .map(|(_, digest)| GateQuestion {
                    label: format!("{SPEC_DIGEST_LABEL}:{}", check.as_str()),
                    question: occurred(check, work_item, snapshot, Some(digest)),
                });
            let plain = GateQuestion {
                label: check.as_str().to_owned(),
                question: occurred(check, work_item, snapshot, None),
            };

            once(plain).chain(sealed)
        })
        .chain(once(contradiction_question(work_item)))
        .collect()
}

fn occurred(
    check: &SemanticId,
    work_item: &SemanticId,
    snapshot: &Snapshot,
    digest: Option<&String>,
) -> Question {
    Question::Occurred {
        select: attestation_filter(check, work_item, snapshot, digest),
        min: 1,
    }
}

fn attestation_filter(
    check: &SemanticId,
    work_item: &SemanticId,
    snapshot: &Snapshot,
    digest: Option<&String>,
) -> ListFilter {
    let mut where_eq = vec![
        pair("check", Scalar::Str(check.as_str().to_owned())),
        pair("work_item", Scalar::Str(work_item.as_str().to_owned())),
        pair("ok", Scalar::Bool(true)),
    ];
    if let Some(digest) = digest {
        where_eq.push(pair("spec_digest", Scalar::Str(digest.clone())));
    }

    ListFilter {
        type_name: Some(type_name("attestation")),
        id_prefix: None,
        snapshot: Some(snapshot.clone()),
        where_eq,
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

pub(crate) fn pair(name: &str, value: Scalar) -> (FieldName, Scalar) {
    let field = FieldName::parse(name).expect("the gate field names are valid constants");

    (field, value)
}
