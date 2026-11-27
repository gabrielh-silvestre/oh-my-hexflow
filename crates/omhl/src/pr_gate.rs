use std::iter::once;

use hexlog::{
    AppendItem, AppendRequest, Author, FieldName, ItemBody, ListFilter, NodeView, RelationTypeName,
    Scalar, SemanticId, Snapshot, SnapshotKind, TenantId, TypeName,
};
use serde_json::{Value, json};

use crate::extensions::plan::LOADER_CLIENT;
use crate::gate::{
    GateQuestion, GateReport, Predicate, Question, QuestionClass, QuestionReport, TargetCheck,
    TargetFact,
};
use crate::mechanical::{RUNNABLE_EVENTS, is_mechanical, snapshot_for, spec_digest};
use crate::native::{CI_GREEN_ID, NativeCheck};
use crate::workflow::config::{NativeConfig, RequireAtom};
use crate::workflow::gap::{is_confirmed_guideline, is_confirmed_premise};
use crate::workflow::retro::GateOverride;

pub const GATE_NAME: &str = "pull-request";
const ABANDONED_GATE_NAME: &str = "abandoned";
pub const REQUIRED_EVENTS: [&str; 3] = RUNNABLE_EVENTS;
pub const NATIVE_SNAPSHOT: SnapshotKind = SnapshotKind::GitCommit;

const NO_OPEN_CONTRADICTION_LABEL: &str = "no-open-contradiction";
const DECISIONS_ANCHORED_LABEL: &str = "decisions-anchored";
const ANCHORS_CONFIRMED_LABEL: &str = "anchors-confirmed";
const RESTS_ON: &str = "rests-on";
const SPEC_DIGEST_LABEL: &str = "spec-digest-match";

pub struct GateSnapshots {
    pub head_commit: Snapshot,
    pub head_tree: Snapshot,
}

pub fn required_checks(
    native: &[NativeCheck],
    enabled: &NativeConfig,
    tenant_checks: &[NodeView],
) -> Vec<(SemanticId, SnapshotKind)> {
    let projects: Vec<&NodeView> = tenant_checks
        .iter()
        .filter(|node| is_required_project_check(node))
        .collect();
    let natives = native
        .iter()
        .filter(|check| enabled.checks.contains(check.id))
        .filter(|check| check.blocking && REQUIRED_EVENTS.contains(&check.event))
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

fn is_required_project_check(node: &NodeView) -> bool {
    let covered_event = node.data["when"]["event"]
        .as_str()
        .is_some_and(|event| REQUIRED_EVENTS.contains(&event));

    !node.shadowed && node.data["blocking"] == true && covered_event
}

pub fn spec_digests(tenant_checks: &[NodeView]) -> Vec<(SemanticId, String)> {
    tenant_checks
        .iter()
        .filter(|node| !node.shadowed && is_mechanical(&node.data))
        .map(|node| (node.id.clone(), spec_digest(&node.data)))
        .collect()
}

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
                .map(|(_, digest)| {
                    GateQuestion::new(
                        format!("{SPEC_DIGEST_LABEL}:{}", check.as_str()),
                        occurred(check, work_item, snapshot, Some(digest)),
                        QuestionClass::Refuse,
                    )
                });
            let plain = GateQuestion::new(
                check.as_str(),
                occurred(check, work_item, snapshot, None),
                plain_check_class(check, sealed.is_some()),
            );

            once(plain).chain(sealed)
        })
        .chain(once(contradiction_question(work_item)))
        .collect()
}

// Um check mecânico ou o CI verde do merge recusa sem `--force`; o demais é aviso (CMP-006).
fn plain_check_class(check: &SemanticId, mechanical: bool) -> QuestionClass {
    if mechanical || check.as_str() == CI_GREEN_ID {
        QuestionClass::Refuse
    } else {
        QuestionClass::Warn
    }
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
    GateQuestion::new(
        NO_OPEN_CONTRADICTION_LABEL,
        Question::NoOpenContradiction {
            of: Some(decisions_filter(work_item)),
        },
        QuestionClass::Warn,
    )
}

fn decisions_filter(work_item: &SemanticId) -> ListFilter {
    ListFilter {
        type_name: Some(type_name("decision")),
        id_prefix: None,
        snapshot: None,
        where_eq: vec![pair(
            "work_item",
            Scalar::Str(work_item.as_str().to_owned()),
        )],
    }
}

/// Um `requires:<átomo>` por átomo do tipo; o Engine já leu a satisfação de cada um.
pub fn requirement_questions(atoms: &[(RequireAtom, bool)]) -> Vec<GateQuestion> {
    atoms
        .iter()
        .map(|(atom, holds)| GateQuestion {
            next_step: Some(requirement_next_step(*atom)),
            ..GateQuestion::new(
                format!("requires:{}", atom.as_str()),
                Question::Requirement { holds: *holds },
                QuestionClass::Refuse,
            )
        })
        .collect()
}

fn requirement_next_step(atom: RequireAtom) -> &'static str {
    match atom {
        RequireAtom::BusinessPremise => {
            "record a business premise and ask the human to confirm it (premise: status confirmed with confirmed_by), before the work item closes"
        }
        RequireAtom::TechnicalPremiseConfirmed => {
            "record the technical premise of the choice and have it confirmed"
        }
        RequireAtom::AgentPremise => "record the agent premise of the technical choice",
        RequireAtom::Spec => "make the spec valid with omhl spec validate",
    }
}

pub fn anchor_questions(work_item: &SemanticId) -> Vec<GateQuestion> {
    let anchored = |target| Question::Anchored {
        select: decisions_filter(work_item),
        by: RelationTypeName::parse(RESTS_ON)
            .expect("the gate relation type names are valid constants"),
        target,
    };
    let confirmed = TargetCheck {
        scope: ["premise", "guideline"]
            .map(|name| ListFilter {
                type_name: Some(type_name(name)),
                id_prefix: None,
                snapshot: None,
                where_eq: Vec::new(),
            })
            .to_vec(),
        confirmed: Predicate(is_anchor_confirmed),
    };

    vec![
        GateQuestion {
            next_step: Some(
                "record a rests-on from the decision to a confirmed guideline or premise",
            ),
            ..GateQuestion::new(
                DECISIONS_ANCHORED_LABEL,
                anchored(None),
                QuestionClass::Warn,
            )
        },
        GateQuestion {
            next_step: Some(
                "ask the human to confirm the target (premise: set status confirmed with confirmed_by) or rest the decision on a confirmed one",
            ),
            ..GateQuestion::new(
                ANCHORS_CONFIRMED_LABEL,
                anchored(Some(confirmed)),
                QuestionClass::Warn,
            )
        },
    ]
}

fn is_anchor_confirmed(target: &TargetFact<'_>) -> bool {
    if target.native {
        return is_confirmed_guideline(true, false, target.data);
    }

    match target.type_name {
        Some("premise") => is_confirmed_premise(target.data),
        Some("guideline") => {
            is_confirmed_guideline(false, target.author_client == LOADER_CLIENT, target.data)
        }
        _ => false,
    }
}

pub(crate) fn project_checks_filter() -> ListFilter {
    ListFilter {
        type_name: Some(type_name("check")),
        id_prefix: Some("checks".to_owned()),
        snapshot: None,
        where_eq: Vec::new(),
    }
}

/// Por que o `close` recusa, com todas as perguntas avaliadas: o erro lista só as reprovadas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseRefusal {
    /// Uma pergunta `refuse` reprovada: nem `--force` contorna.
    Refused(Vec<QuestionReport>),
    /// Só perguntas `warn` reprovadas, sem `--force --note`.
    Warnings(Vec<QuestionReport>),
}

/// O motivo e a autorização literal do dono para fechar sem entregar.
pub struct Abandonment<'a> {
    pub reason: &'a str,
    pub note: &'a str,
}

/// O que o fechamento grava além do gate: a nota do dono, ou só o aviso de que ela foi descartada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forcing {
    Discarded,
    Forced(GateOverride),
}

/// Um `--force` só contorna perguntas `warn`; uma pergunta sem classe conta como `refuse`.
pub fn close_verdict(
    report: &GateReport,
    note: Option<&str>,
) -> Result<Option<Forcing>, CloseRefusal> {
    let failing: Vec<QuestionReport> = report
        .questions
        .iter()
        .filter(|question| !question.passed)
        .cloned()
        .collect();
    if failing.is_empty() {
        return Ok(note.map(|_| Forcing::Discarded));
    }
    if failing
        .iter()
        .any(|question| question.class != Some(QuestionClass::Warn))
    {
        return Err(CloseRefusal::Refused(report.questions.clone()));
    }
    let Some(note) = note else {
        return Err(CloseRefusal::Warnings(report.questions.clone()));
    };

    Ok(Some(Forcing::Forced(GateOverride {
        note: note.to_owned(),
        overridden: failing.into_iter().map(|question| question.label).collect(),
    })))
}

pub(crate) fn closing_request(
    tenant: &TenantId,
    author: &Author,
    current: NodeView,
    report: &GateReport,
    snapshot: &Snapshot,
    forcing: Option<&Forcing>,
) -> AppendRequest {
    let mut gate = json!({
        "name": GATE_NAME,
        "passed": report.passed,
        "chain_head_seq": report.chain_head.seq,
        "snapshot": snapshot,
    });
    if let Some(Forcing::Forced(gate_override)) = forcing {
        gate["forced"] = json!(true);
        gate["overridden"] = json!(gate_override.overridden);
        gate["note"] = json!(gate_override.note);
    }

    closed_request(tenant, author, current, gate, None)
}

/// O abandono não avalia o gate: o `gate` tem nome próprio e nunca passa.
pub(crate) fn abandonment_request(
    tenant: &TenantId,
    author: &Author,
    current: NodeView,
    chain_head_seq: u64,
    snapshot: &Snapshot,
    abandonment: &Abandonment<'_>,
) -> AppendRequest {
    let gate = json!({
        "name": ABANDONED_GATE_NAME,
        "passed": false,
        "chain_head_seq": chain_head_seq,
        "snapshot": snapshot,
    });
    let recorded = json!({ "reason": abandonment.reason, "note": abandonment.note });

    closed_request(tenant, author, current, gate, Some(recorded))
}

fn closed_request(
    tenant: &TenantId,
    author: &Author,
    current: NodeView,
    gate: Value,
    abandonment: Option<Value>,
) -> AppendRequest {
    let mut data = current.data;
    data["status"] = json!("closed");
    data["gate"] = gate;
    if let Some(abandonment) = abandonment {
        data["abandonment"] = abandonment;
    }
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

#[cfg(test)]
mod tests;
