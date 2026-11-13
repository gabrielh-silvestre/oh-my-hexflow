//! Retro do fechamento de um work item. Pura: recebe os fatos já lidos e devolve o relatório.

use std::collections::BTreeMap;

use super::gap::GapState;
use super::names::FILE_SUFFIX;
use super::spec::SPEC_STALE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetroFacts {
    /// Revisões vigentes das divergências do work item, em ordem de criação.
    pub divergences: Vec<DivergenceFact>,
    pub gaps: Vec<GapFact>,
    /// `emenda-NN.md` da pasta da spec; vazio sem pasta.
    pub amendment_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivergenceFact {
    pub id: String,
    pub kind: String,
    pub resolution: String,
    /// Sem `.md`: `emenda-01`.
    pub amendment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapFact {
    pub id: String,
    pub state: GapState,
    pub answered_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retro {
    pub divergences: Vec<DivergenceGroup>,
    pub approved: Vec<ApprovedAmendment>,
    pub pending: Vec<String>,
    pub no_criterion: Vec<AnsweredGap>,
    pub open_gaps: Vec<OpenGap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivergenceGroup {
    pub kind: String,
    pub resolution: String,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedAmendment {
    /// `emenda-01.md`.
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

/// `None` sem divergência e sem resposta sem critério: gap aberto ou arquivo de emenda sozinhos não
/// geram retro. A emenda aprovada vem do grafo (divergência `spec-stale` que a cita), não do disco;
/// pendente é o arquivo que nenhuma delas cita, válido ou não.
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
    if facts.divergences.is_empty() && no_criterion.is_empty() {
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
