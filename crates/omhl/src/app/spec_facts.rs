use hexlog::NodeView;
use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;
use super::read::all_nodes;
use crate::native::NativeBase;
use crate::workflow::gap::{GapFacts, GapState, gap_state};
use crate::workflow::spec::{DivergenceFact, SPEC_STALE, SpecFacts};

const PREMISE: &str = "premise";

/// Os fatos do grafo que o `validate_graph` consulta, lidos uma vez por validação.
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
}

/// Diretiva nativa não é node do tenant: vale por existir no catálogo nativo.
fn is_native(id: &str) -> bool {
    NativeBase
        .guidelines()
        .iter()
        .any(|guideline| guideline.id == id)
}

fn text<'a>(data: &'a Value, field: &str) -> Option<&'a str> {
    data.get(field).and_then(Value::as_str)
}

impl SpecFacts for GraphFacts {
    fn exists(&self, id: &str) -> bool {
        is_native(id) || self.gap.nodes.iter().any(|fact| fact.id == id)
    }

    fn confirmed(&self, id: &str) -> bool {
        if is_native(id) {
            return true;
        }
        let Some(fact) = self.gap.nodes.iter().find(|fact| fact.id == id) else {
            return false;
        };
        let has_confirmed_by = fact.data.get("confirmed_by").is_some();
        if fact.type_name == PREMISE {
            return text(&fact.data, "status") == Some("confirmed") && has_confirmed_by;
        }

        fact.native || fact.loader || has_confirmed_by
    }

    fn kind(&self, id: &str) -> Option<String> {
        let fact = self.gap.nodes.iter().find(|fact| fact.id == id)?;

        text(&fact.data, "kind").map(str::to_owned)
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
