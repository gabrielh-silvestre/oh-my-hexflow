//! Retro do fechamento de um work item. Pura.

use std::collections::BTreeMap;

use super::gap::GapState;
use super::names::FILE_SUFFIX;
use super::spec::SPEC_STALE;
use crate::gate::QuestionClass;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetroFacts {
    pub divergences: Vec<DivergenceFact>,
    pub gaps: Vec<GapFact>,
    pub amendment_files: Vec<String>,
    pub gate_override: Option<GateOverride>,
    pub delivery_failure: Option<DeliveryFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateOverride {
    pub note: String,
    pub overridden: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryFailure {
    pub note: String,
    pub failed: Vec<FailedQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedQuestion {
    pub label: String,
    pub class: QuestionClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivergenceFact {
    pub id: String,
    pub kind: String,
    pub resolution: String,
    pub amendment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapFact {
    pub id: String,
    pub state: GapState,
    pub answered_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Retro {
    pub divergences: Vec<DivergenceGroup>,
    pub approved: Vec<ApprovedAmendment>,
    pub pending: Vec<String>,
    pub no_criterion: Vec<AnsweredGap>,
    pub open_gaps: Vec<OpenGap>,
    pub gate_override: Option<GateOverride>,
    pub delivery_failure: Option<DeliveryFailure>,
    pub abandonment: Option<RetroAbandonment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetroAbandonment {
    pub reason: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivergenceGroup {
    pub kind: String,
    pub resolution: String,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedAmendment {
    pub name: String,
    pub divergence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsweredGap {
    pub gap: String,
    pub answered_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGap {
    pub gap: String,
    pub state: GapState,
}

pub fn retro(facts: &RetroFacts) -> Option<Retro> {
    let no_criterion = facts
        .gaps
        .iter()
        .filter(|gap| {
            matches!(
                gap.state,
                GapState::Answered {
                    criterion: false,
                    ..
                }
            )
        })
        .filter_map(|gap| {
            Some(AnsweredGap {
                gap: gap.id.clone(),
                answered_by: gap.answered_by.clone()?,
            })
        })
        .collect::<Vec<_>>();
    if facts.divergences.is_empty()
        && no_criterion.is_empty()
        && facts.gate_override.is_none()
        && facts.delivery_failure.is_none()
    {
        return None;
    }
    let approved = approved_amendments(&facts.divergences);
    let pending = facts
        .amendment_files
        .iter()
        .filter(|file| !approved.iter().any(|amendment| &amendment.name == *file))
        .cloned()
        .collect();
    let open_gaps = facts
        .gaps
        .iter()
        .filter(|gap| matches!(gap.state, GapState::Open | GapState::NeedsHuman))
        .map(|gap| OpenGap {
            gap: gap.id.clone(),
            state: gap.state,
        })
        .collect();

    Some(Retro {
        divergences: group_divergences(&facts.divergences),
        approved,
        pending,
        no_criterion,
        open_gaps,
        gate_override: facts.gate_override.clone(),
        delivery_failure: facts.delivery_failure.clone(),
        abandonment: None,
    })
}

/// A retro de um nó abandonado sempre existe: ela mostra o motivo e a nota do dono.
pub fn retro_with_abandonment(
    facts: &RetroFacts,
    abandonment: Option<RetroAbandonment>,
) -> Option<Retro> {
    let Some(abandonment) = abandonment else {
        return retro(facts);
    };

    Some(Retro {
        abandonment: Some(abandonment),
        ..retro(facts).unwrap_or_default()
    })
}

fn approved_amendments(divergences: &[DivergenceFact]) -> Vec<ApprovedAmendment> {
    divergences
        .iter()
        .filter(|divergence| divergence.resolution == SPEC_STALE)
        .filter_map(|divergence| {
            Some(ApprovedAmendment {
                name: format!("{}{FILE_SUFFIX}", divergence.amendment.as_ref()?),
                divergence: divergence.id.clone(),
            })
        })
        .collect()
}

fn group_divergences(divergences: &[DivergenceFact]) -> Vec<DivergenceGroup> {
    let mut groups: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    for divergence in divergences {
        groups
            .entry((&divergence.kind, &divergence.resolution))
            .or_default()
            .push(divergence.id.clone());
    }

    groups
        .into_iter()
        .map(|((kind, resolution), ids)| DivergenceGroup {
            kind: kind.to_owned(),
            resolution: resolution.to_owned(),
            ids,
        })
        .collect()
}

#[cfg(test)]
mod tests;
