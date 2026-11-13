use hexlog::{
    FieldName, NodeView, Origin, RelationQuery, RelationTypeName, RelationView, Scalar, SemanticId,
};
use serde_json::{Value, json};

use super::engine::{Engine, parse_at};
use super::error::AppError;
use super::output;
use super::read::{all_nodes, nodes_matching};
use crate::ports::workflow::read_workflow;
use crate::workflow::config::WorkflowConfig;
use crate::workflow::gap::{Answerer, Edge, Fact, GapFacts, GapState, answered_by, gap_state};
use crate::workflow::recurrence::{Premise, recurrences};
use crate::workflow::retro::{DivergenceFact, GapFact, RetroFacts, retro};

const GAP_RELATIONS: [&str; 3] = ["answers", "supports", "contradicts"];

impl Engine<'_> {
    /// Só gaps vivos, na ordem de criação; o filtro vale para o campo `work_item` do gap.
    pub(super) fn report_answers(&mut self, work_item: Option<&str>) -> Result<Value, AppError> {
        let where_eq = match work_item {
            None => Vec::new(),
            Some(id) => {
                parse_at(SemanticId::parse, id, "/work_item")?;
                work_item_filter(id)?
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

    /// Só lê: vale para work item aberto ou fechado.
    pub(super) fn report_retro(&mut self, work_item: &str) -> Result<Value, AppError> {
        let id = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let facts = self.retro_facts(&id)?;

        Ok(json!({ "retro": output::retro(retro(&facts).as_ref()) }))
    }

    /// Os fatos da retro não dependem do fechamento: o `close` os lê antes de gravar.
    pub(super) fn retro_facts(&mut self, work_item: &SemanticId) -> Result<RetroFacts, AppError> {
        self.omhl.find_work_item(&self.tenant, work_item)?;
        let filter = work_item_filter(work_item.as_str())?;
        let divergences =
            nodes_matching(&mut self.omhl, &self.tenant, "divergence", filter.clone())?
                .iter()
                .filter_map(divergence_fact)
                .collect();
        let gap_nodes = nodes_matching(&mut self.omhl, &self.tenant, "gap", filter)?;
        let (_, facts) = self.load_gap_facts()?;
        let gaps = gap_nodes
            .iter()
            .map(|gap| GapFact {
                id: gap.id.as_str().to_owned(),
                state: gap_state(&facts, gap.id.as_str()),
                answered_by: answered_by(&facts, gap.id.as_str()),
            })
            .collect();

        Ok(RetroFacts {
            divergences,
            gaps,
            amendment_files: self.amendment_files(work_item.as_str())?,
        })
    }

    /// Só lê: sugerir a promoção é com o agente, e a `guideline` só entra com `confirmed_by`.
    pub(super) fn report_recurrences(&mut self, min: Option<u32>) -> Result<Value, AppError> {
        let min = match min {
            Some(min) => min,
            None => self.default_promotion_min()?,
        };
        let premises: Vec<Premise> = all_nodes(&mut self.omhl, &self.tenant, "premise")?
            .iter()
            .filter_map(recurrence_premise)
            .collect();
        let directives: Vec<String> = all_nodes(&mut self.omhl, &self.tenant, "guideline")?
            .iter()
            .filter_map(|view| text_field(view, "text"))
            .collect();
        let groups: Vec<Value> = recurrences(&premises, &directives, min)
            .into_iter()
            .map(|group| {
                json!({
                    "kind": group.kind,
                    "text": group.text,
                    "work_items": group.work_items,
                    "premises": group.premises,
                })
            })
            .collect();

        Ok(json!({ "min": min, "recurrences": groups }))
    }

    /// Sem raiz de projeto (`--db`) não há `workflow.toml`: vale o padrão.
    fn default_promotion_min(&self) -> Result<u32, AppError> {
        let config = match self.project_root.as_deref() {
            Some(root) => read_workflow(root)?,
            None => WorkflowConfig::default(),
        };

        Ok(config.promotion.min_work_items)
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

fn work_item_filter(work_item: &str) -> Result<Vec<(FieldName, Scalar)>, AppError> {
    Ok(vec![(
        FieldName::parse("work_item")?,
        Scalar::Str(work_item.to_owned()),
    )])
}

fn divergence_fact(view: &NodeView) -> Option<DivergenceFact> {
    Some(DivergenceFact {
        id: view.id.as_str().to_owned(),
        kind: text_field(view, "kind")?,
        resolution: text_field(view, "resolution")?,
        amendment: text_field(view, "amendment"),
    })
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

pub(super) fn state_name(state: GapState) -> &'static str {
    match state {
        GapState::Open => "open",
        GapState::Answered { .. } => "answered",
        GapState::NeedsHuman => "needs-human",
    }
}

/// Premissa legada, sem `kind` ou `work_item`, não entra no agrupamento.
fn recurrence_premise(view: &NodeView) -> Option<Premise> {
    Some(Premise {
        id: view.id.as_str().to_owned(),
        kind: text_field(view, "kind")?,
        text: text_field(view, "text")?,
        work_item: text_field(view, "work_item")?,
    })
}

fn text_field(view: &NodeView, field: &str) -> Option<String> {
    view.data.get(field)?.as_str().map(str::to_owned)
}
