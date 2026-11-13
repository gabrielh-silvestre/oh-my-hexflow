use hexlog::{
    FieldName, NodeView, Origin, RelationQuery, RelationTypeName, RelationView, Scalar, SemanticId,
};
use serde_json::{Value, json};

use super::engine::{Engine, parse_at};
use super::error::AppError;
use super::output;
use super::read::{all_nodes, nodes_matching};
use crate::workflow::gap::{Answerer, Edge, Fact, GapFacts, GapState, answered_by, gap_state};

const GAP_RELATIONS: [&str; 3] = ["answers", "supports", "contradicts"];

impl Engine<'_> {
    /// Só gaps vivos, na ordem de criação; o filtro vale para o campo `work_item` do gap.
    pub(super) fn report_answers(&mut self, work_item: Option<&str>) -> Result<Value, AppError> {
        let where_eq = match work_item {
            None => Vec::new(),
            Some(id) => {
                parse_at(SemanticId::parse, id, "/work_item")?;
                vec![(FieldName::parse("work_item")?, Scalar::Str(id.to_owned()))]
            }
        };
        let gaps = nodes_matching(&mut self.omhl, &self.tenant, "gap", where_eq)?;
        let (premises, facts) = self.load_gap_facts()?;
        let entries: Vec<Value> = gaps
            .iter()
            .map(|gap| gap_entry(&facts, &premises, gap))
            .collect();

        Ok(json!({ "gaps": entries }))
    }

    /// As premissas vigentes e os fatos de gap do tenant (premissas, diretivas e relations).
    pub(super) fn load_gap_facts(&mut self) -> Result<(Vec<NodeView>, GapFacts), AppError> {
        let premises = all_nodes(&mut self.omhl, &self.tenant, "premise")?;
        let guidelines = all_nodes(&mut self.omhl, &self.tenant, "guideline")?;
        let mut relations = Vec::new();
        for name in GAP_RELATIONS {
            let query = RelationQuery {
                relation_type: Some(RelationTypeName::parse(name)?),
                from: None,
                to: None,
                include_retired: false,
            };
            relations.extend(self.omhl.relations(&self.tenant, &query)?);
        }
        let facts = gap_facts(&premises, &guidelines, &relations);

        Ok((premises, facts))
    }
}

/// Diretiva nativa não é node do tenant: entra pelas pontas das relations.
fn gap_facts(
    premises: &[NodeView],
    guidelines: &[NodeView],
    relations: &[RelationView],
) -> GapFacts {
    let mut nodes: Vec<Fact> = premises.iter().chain(guidelines).map(tenant_fact).collect();
    for relation in relations {
        for (id, origin) in [
            (&relation.from, relation.from_origin),
            (&relation.to, relation.to_origin),
        ] {
            if origin == Origin::Native {
                nodes.push(Fact {
                    id: id.as_str().to_owned(),
                    type_name: "guideline".to_owned(),
                    native: true,
                    loader: false,
                    data: Value::Null,
                });
            }
        }
    }
    let edges = relations
        .iter()
        .map(|relation| Edge {
            relation: relation.relation_type.as_str().to_owned(),
            from: relation.from.as_str().to_owned(),
            to: relation.to.as_str().to_owned(),
        })
        .collect();

    GapFacts { nodes, edges }
}

fn tenant_fact(view: &NodeView) -> Fact {
    Fact {
        id: view.id.as_str().to_owned(),
        type_name: view.type_name.as_str().to_owned(),
        native: false,
        loader: view.author.client == "loader",
        data: view.data.clone(),
    }
}

fn gap_entry(facts: &GapFacts, premises: &[NodeView], gap: &NodeView) -> Value {
    let state = gap_state(facts, gap.id.as_str());
    let mut entry = json!({ "id": gap.id, "state": state_name(state) });
    if let Some(work_item) = gap.data.get("work_item") {
        entry["work_item"] = work_item.clone();
    }
    let answering = answered_by(facts, gap.id.as_str())
        .and_then(|id| premises.iter().find(|premise| premise.id.as_str() == id));
    if let Some(premise) = answering {
        entry["answered_by"] = json!(premise.id);
        entry["author"] = output::author(&premise.author);
    }
    if let GapState::Answered { by, criterion } = state {
        entry["answerer"] = json!(match by {
            Answerer::Human => "human",
            Answerer::Autonomous => "autonomous",
        });
        entry["no_criterion"] = json!(!criterion);
    }

    entry
}

fn state_name(state: GapState) -> &'static str {
    match state {
        GapState::Open => "open",
        GapState::Answered { .. } => "answered",
        GapState::NeedsHuman => "needs-human",
    }
}
