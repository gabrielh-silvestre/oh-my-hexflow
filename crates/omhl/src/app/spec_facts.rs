use hexlog::NodeView;
use hexlog::SemanticId;
use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;
use super::read::{all_nodes, nodes_matching};
use super::report::work_item_filter;
use crate::workflow::gap::{
    GapFacts, GapState, gap_state, is_confirmed_guideline, is_confirmed_premise,
};
use crate::workflow::matrix::{PremiseFact, PremiseKind};
use crate::workflow::spec::{DivergenceFact, SPEC_STALE, SpecFacts};

const PREMISE: &str = "premise";

pub(super) struct GraphFacts {
    gap: GapFacts,
    divergences: Vec<NodeView>,
}

impl Engine<'_> {
    pub(super) fn load_spec_facts(&mut self) -> Result<GraphFacts, AppError> {
        let (_, gap) = self.load_gap_facts()?;
        let divergences = all_nodes(&mut self.omhl, &self.tenant, "divergence")?;

        Ok(GraphFacts { gap, divergences })
    }

    /// As premissas vigentes do work item, filtradas no banco.
    pub(super) fn premise_facts(
        &mut self,
        work_item: &SemanticId,
    ) -> Result<Vec<PremiseFact>, AppError> {
        let filter = work_item_filter(work_item.as_str())?;
        let premises = nodes_matching(&mut self.omhl, &self.tenant, PREMISE, filter)?;

        Ok(premises
            .iter()
            .map(|premise| premise_fact(&premise.data))
            .collect())
    }
}

fn premise_fact(data: &Value) -> PremiseFact {
    let kind = match text(data, "kind") {
        Some("business") => Some(PremiseKind::Business),
        Some("technical") => Some(PremiseKind::Technical),
        _ => None,
    };

    PremiseFact {
        kind,
        confirmed: is_confirmed_premise(data),
        late: is_late(data),
    }
}

fn is_late(data: &Value) -> bool {
    text(data, "origin") == Some("late")
}

fn text<'a>(data: &'a Value, field: &str) -> Option<&'a str> {
    data.get(field).and_then(Value::as_str)
}

impl SpecFacts for GraphFacts {
    fn exists(&self, id: &str) -> bool {
        self.gap.nodes.iter().any(|fact| fact.id == id)
    }

    fn confirmed(&self, id: &str) -> bool {
        let Some(fact) = self.gap.nodes.iter().find(|fact| fact.id == id) else {
            return false;
        };
        if fact.type_name == PREMISE {
            return is_confirmed_premise(&fact.data);
        }

        is_confirmed_guideline(fact.native, fact.loader, &fact.data)
    }

    fn kind(&self, id: &str) -> Option<String> {
        let fact = self.gap.nodes.iter().find(|fact| fact.id == id)?;

        text(&fact.data, "kind").map(str::to_owned)
    }

    fn late(&self, id: &str) -> bool {
        self.gap
            .nodes
            .iter()
            .find(|fact| fact.id == id)
            .is_some_and(|fact| is_late(&fact.data))
    }

    fn work_item(&self, id: &str) -> Option<String> {
        let fact = self.gap.nodes.iter().find(|fact| fact.id == id)?;

        text(&fact.data, "work_item").map(str::to_owned)
    }

    fn gap_state(&self, gap: &str) -> GapState {
        gap_state(&self.gap, gap)
    }

    fn divergence(&self, id: &str) -> Option<DivergenceFact> {
        let node = self
            .divergences
            .iter()
            .find(|node| node.id.as_str() == id)?;
        let data = &node.data;

        Some(DivergenceFact {
            work_item: text(data, "work_item")?.to_owned(),
            resolution: text(data, "resolution")?.to_owned(),
            source_ref: text(data, "source_ref")?.to_owned(),
        })
    }

    fn amendment_approved(&self, work_item: &str, amendment: &str) -> bool {
        self.divergences.iter().any(|node| {
            text(&node.data, "work_item") == Some(work_item)
                && text(&node.data, "resolution") == Some(SPEC_STALE)
                && text(&node.data, "amendment") == Some(amendment)
        })
    }
}
