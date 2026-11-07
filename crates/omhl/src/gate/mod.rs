mod source;

use std::collections::HashSet;

use hexlog::{
    ChainHead, EndpointStatus, ListFilter, NodeView, RelationTypeName, RelationView, SemanticId,
    TenantId,
};

pub use self::source::HexlogSource;
use crate::error::{OmhlError, RuleCode};

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateQuestion {
    pub label: String,
    pub question: Question,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionKind {
    Approved,
    Occurred,
    NoPending,
    NoOpenContradiction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionReport {
    pub label: String,
    pub kind: QuestionKind,
    pub passed: bool,
    pub matched: usize,
    pub failing: Vec<SemanticId>,
    pub omitted: usize,
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
    match &entry.question {
        Question::Approved { of } => approved(source, tenant, label, of),
        Question::Occurred { select, min } => occurred(source, tenant, label, select, *min),
        Question::NoPending {
            pending,
            resolved_by,
        } => no_pending(source, tenant, label, pending, resolved_by),
        Question::NoOpenContradiction { of } => {
            no_open_contradiction(source, tenant, label, of.as_ref())
        }
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
    }
}
