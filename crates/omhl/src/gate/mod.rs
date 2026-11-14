mod source;

use std::collections::{HashMap, HashSet};

use hexlog::{
    ChainHead, EndpointStatus, ListFilter, NodeView, Origin, RelationTypeName, RelationView,
    SemanticId, TenantId,
};
use serde_json::Value;

pub use self::source::HexlogSource;
use crate::error::{OmhlError, RuleCode};
use crate::guidance::Guidance;

pub const SUPPORTS: &str = "supports";
pub const CONTRADICTS: &str = "contradicts";
pub const EVIDENCE_CAP: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    Approved {
        of: ListFilter,
    },
    Occurred {
        select: ListFilter,
        min: u32,
    },
    NoPending {
        pending: ListFilter,
        resolved_by: RelationTypeName,
    },
    NoOpenContradiction {
        of: Option<ListFilter>,
    },
    /// Sem `target`: cada node de `select` precisa de uma relation `by` vigente como origem.
    /// Com `target`: nenhuma relation `by` vigente do node pode apontar para um destino que o predicado recuse.
    Anchored {
        select: ListFilter,
        by: RelationTypeName,
        target: Option<TargetCheck>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetCheck {
    /// Onde buscar os destinos tenant; um destino nativo dispensa a busca.
    pub scope: Vec<ListFilter>,
    pub confirmed: Predicate,
}

/// O destino de uma relation como o predicado o vê; o gate não sabe o tipo de um destino nativo.
#[derive(Debug, Clone, Copy)]
pub struct TargetFact<'a> {
    pub type_name: Option<&'a str>,
    pub native: bool,
    pub author_client: &'a str,
    pub data: &'a Value,
}

#[derive(Debug, Clone, Copy)]
pub struct Predicate(pub fn(&TargetFact<'_>) -> bool);

// A derivação por ponteiro de função dispara `unpredictable_function_pointer_comparisons`.
impl PartialEq for Predicate {
    fn eq(&self, other: &Predicate) -> bool {
        std::ptr::fn_addr_eq(self.0, other.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateQuestion {
    pub label: String,
    pub question: Question,
    /// Só aparece no relatório quando a pergunta reprova.
    pub next_step: Option<Guidance>,
}

impl GateQuestion {
    pub fn new(label: impl Into<String>, question: Question) -> GateQuestion {
        GateQuestion {
            label: label.into(),
            question,
            next_step: None,
        }
    }

    pub fn with_next_step(self, next_step: Guidance) -> GateQuestion {
        GateQuestion {
            next_step: Some(next_step),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionKind {
    Approved,
    Occurred,
    NoPending,
    NoOpenContradiction,
    Anchored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionReport {
    pub label: String,
    pub kind: QuestionKind,
    pub passed: bool,
    pub matched: usize,
    pub failing: Vec<SemanticId>,
    pub omitted: usize,
    pub next_step: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    pub gate: String,
    pub passed: bool,
    pub questions: Vec<QuestionReport>,
    pub chain_head: ChainHead,
}

impl GateReport {
    /// `passed` = há ao menos uma pergunta e todas passaram.
    pub fn assemble(
        gate: &str,
        chain_head: ChainHead,
        questions: Vec<QuestionReport>,
    ) -> GateReport {
        GateReport {
            gate: gate.to_owned(),
            passed: !questions.is_empty() && questions.iter().all(|report| report.passed),
            questions,
            chain_head,
        }
    }
}

pub trait GateSource {
    fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, OmhlError>;
    /// Todos os nodes vigentes que casam o filtro, todas as páginas, em ordem de `seq`.
    fn nodes(&mut self, tenant: &TenantId, filter: &ListFilter)
    -> Result<Vec<NodeView>, OmhlError>;
    /// Relations do tipo cujo lineage não está retirado, em ordem de `seq`.
    fn relations(
        &mut self,
        tenant: &TenantId,
        relation_type: &RelationTypeName,
    ) -> Result<Vec<RelationView>, OmhlError>;
}

pub fn evaluate<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    questions: &[GateQuestion],
) -> Result<Vec<QuestionReport>, OmhlError> {
    questions
        .iter()
        .map(|entry| evaluate_question(source, tenant, entry))
        .collect()
}

/// Lê o chain head, roda `body`, lê o chain head de novo. Cabeças diferentes: `GateInconsistent`.
pub fn read_consistent<S: GateSource, T>(
    source: &mut S,
    tenant: &TenantId,
    body: impl FnOnce(&mut S) -> Result<T, OmhlError>,
) -> Result<(ChainHead, T), OmhlError> {
    let opening = source.chain_head(tenant)?;
    let value = body(source)?;
    let closing = source.chain_head(tenant)?;
    if opening != closing {
        return Err(OmhlError::rule(RuleCode::GateInconsistent, ""));
    }

    Ok((opening, value))
}

fn evaluate_question<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    entry: &GateQuestion,
) -> Result<QuestionReport, OmhlError> {
    let label = entry.label.as_str();
    let report = match &entry.question {
        Question::Approved { of } => approved(source, tenant, label, of),
        Question::Occurred { select, min } => occurred(source, tenant, label, select, *min),
        Question::NoPending {
            pending,
            resolved_by,
        } => no_pending(source, tenant, label, pending, resolved_by),
        Question::NoOpenContradiction { of } => {
            no_open_contradiction(source, tenant, label, of.as_ref())
        }
        Question::Anchored { select, by, target } => {
            anchored(source, tenant, label, select, by, target.as_ref())
        }
    }?;

    Ok(with_next_step(report, entry.next_step))
}

fn with_next_step(report: QuestionReport, guidance: Option<Guidance>) -> QuestionReport {
    let next_step = guidance
        .filter(|_| !report.passed)
        .map(|guidance| guidance.next_step.to_owned());

    QuestionReport {
        next_step,
        ..report
    }
}

fn approved<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    label: &str,
    of: &ListFilter,
) -> Result<QuestionReport, OmhlError> {
    let matches = source.nodes(tenant, of)?;
    let supports = named_relations(source, tenant, SUPPORTS)?;
    let contradicts = named_relations(source, tenant, CONTRADICTS)?;
    let supported = current_targets(&supports);
    let contradicted = current_targets(&contradicts);
    let matched = matches.len();
    let failing = ids_where(matches, |id| {
        !supported.contains(id) || contradicted.contains(id)
    });
    let passed = matched > 0 && failing.is_empty();

    Ok(report(
        label,
        QuestionKind::Approved,
        passed,
        matched,
        failing,
    ))
}

fn occurred<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    label: &str,
    select: &ListFilter,
    min: u32,
) -> Result<QuestionReport, OmhlError> {
    let matched = source.nodes(tenant, select)?.len();
    let passed = matched >= usize::try_from(min).unwrap_or(usize::MAX);

    Ok(report(
        label,
        QuestionKind::Occurred,
        passed,
        matched,
        Vec::new(),
    ))
}

fn no_pending<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    label: &str,
    pending: &ListFilter,
    resolved_by: &RelationTypeName,
) -> Result<QuestionReport, OmhlError> {
    let pending = source.nodes(tenant, pending)?;
    let resolutions = source.relations(tenant, resolved_by)?;
    let resolved = current_targets(&resolutions);
    let matched = pending.len();
    let failing = ids_where(pending, |id| !resolved.contains(id));

    Ok(report(
        label,
        QuestionKind::NoPending,
        failing.is_empty(),
        matched,
        failing,
    ))
}

fn no_open_contradiction<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    label: &str,
    of: Option<&ListFilter>,
) -> Result<QuestionReport, OmhlError> {
    let everything;
    let filter = match of {
        Some(filter) => filter,
        None => {
            everything = ListFilter {
                type_name: None,
                id_prefix: None,
                snapshot: None,
                where_eq: Vec::new(),
            };
            &everything
        }
    };
    let scope = source.nodes(tenant, filter)?;
    let contradicts = named_relations(source, tenant, CONTRADICTS)?;
    let contradicted = current_targets(&contradicts);
    let matched = scope.len();
    let failing = ids_where(scope, |id| contradicted.contains(id));

    Ok(report(
        label,
        QuestionKind::NoOpenContradiction,
        failing.is_empty(),
        matched,
        failing,
    ))
}

fn anchored<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    label: &str,
    select: &ListFilter,
    by: &RelationTypeName,
    target: Option<&TargetCheck>,
) -> Result<QuestionReport, OmhlError> {
    let nodes = source.nodes(tenant, select)?;
    let relations = source.relations(tenant, by)?;
    let current: Vec<&RelationView> = relations.iter().filter(|r| is_current(r)).collect();
    let matched = nodes.len();
    let failing = match target {
        None => ids_where(nodes, |id| current.iter().all(|r| r.from != *id)),
        Some(check) => {
            let scope = scope_nodes(source, tenant, &check.scope)?;
            ids_where(nodes, |id| {
                current
                    .iter()
                    .filter(|r| r.from == *id)
                    .any(|r| !target_confirmed(r, &scope, &check.confirmed))
            })
        }
    };

    Ok(report(
        label,
        QuestionKind::Anchored,
        failing.is_empty(),
        matched,
        failing,
    ))
}

fn is_current(relation: &RelationView) -> bool {
    !relation.retired
        && relation.from_status == EndpointStatus::Current
        && relation.to_status == EndpointStatus::Current
}

fn scope_nodes<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    scope: &[ListFilter],
) -> Result<HashMap<SemanticId, NodeView>, OmhlError> {
    let mut nodes = HashMap::new();
    for filter in scope {
        for node in source.nodes(tenant, filter)? {
            nodes.insert(node.id.clone(), node);
        }
    }

    Ok(nodes)
}

/// Um destino tenant fora do `scope` não está confirmado.
fn target_confirmed(
    relation: &RelationView,
    scope: &HashMap<SemanticId, NodeView>,
    confirmed: &Predicate,
) -> bool {
    if relation.to_origin == Origin::Native {
        return (confirmed.0)(&TargetFact {
            type_name: None,
            native: true,
            author_client: "",
            data: &Value::Null,
        });
    }

    scope.get(&relation.to).is_some_and(|node| {
        (confirmed.0)(&TargetFact {
            type_name: Some(node.type_name.as_str()),
            native: false,
            author_client: &node.author.client,
            data: &node.data,
        })
    })
}

fn named_relations<S: GateSource>(
    source: &mut S,
    tenant: &TenantId,
    name: &str,
) -> Result<Vec<RelationView>, OmhlError> {
    let relation_type =
        RelationTypeName::parse(name).expect("the gate relation type names are valid constants");

    source.relations(tenant, &relation_type)
}

/// Alvos (`to`) que têm ao menos uma origem vigente: retirada e `MissingNative` não contam.
fn current_targets(relations: &[RelationView]) -> HashSet<&SemanticId> {
    relations
        .iter()
        .filter(|relation| !relation.retired && relation.from_status == EndpointStatus::Current)
        .map(|relation| &relation.to)
        .collect()
}

/// Move os ids dos nodes que satisfazem `keep` para a lista de evidência, na ordem recebida.
fn ids_where(nodes: Vec<NodeView>, keep: impl Fn(&SemanticId) -> bool) -> Vec<SemanticId> {
    nodes
        .into_iter()
        .map(|node| node.id)
        .filter(|id| keep(id))
        .collect()
}

fn report(
    label: &str,
    kind: QuestionKind,
    passed: bool,
    matched: usize,
    mut failing: Vec<SemanticId>,
) -> QuestionReport {
    let omitted = failing.len().saturating_sub(EVIDENCE_CAP);
    failing.truncate(EVIDENCE_CAP);

    QuestionReport {
        label: label.to_owned(),
        kind,
        passed,
        matched,
        failing,
        omitted,
        next_step: None,
    }
}
