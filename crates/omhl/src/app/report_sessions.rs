use std::collections::{BTreeMap, BTreeSet};

use hexlog::{Lineage, ScannedRecord, SemanticId};
use serde_json::{Value, json};

use super::engine::{Engine, parse_at};
use super::error::AppError;
use crate::facade::OUTSIDE_CLAUDE_CODE;
use crate::graphs::WORK_ITEM_TYPES;

struct Tally {
    first_seq: u64,
    first_at: String,
    last_at: String,
    nodes: usize,
    edges: usize,
    work_items: BTreeSet<String>,
}

/// Lê o banco numa só passada em ordem de `seq`: o work item de uma ponta vale o que ela tinha no `seq` da aresta.
#[derive(Default)]
struct Ledger {
    node_work_items: BTreeMap<String, String>,
    edge_work_items: BTreeMap<[String; 3], Vec<String>>,
    sessions: BTreeMap<String, Tally>,
}

impl Ledger {
    fn record(&mut self, record: ScannedRecord) {
        let work_items = self.track_work_items(&record);
        let Some(session) = record.session.as_ref().map(|label| label.as_str()) else {
            return;
        };
        if session == OUTSIDE_CLAUDE_CODE {
            return;
        }
        let tally = self
            .sessions
            .entry(session.to_owned())
            .or_insert_with(|| Tally {
                first_seq: record.seq,
                first_at: record.at.clone(),
                last_at: record.at.clone(),
                nodes: 0,
                edges: 0,
                work_items: BTreeSet::new(),
            });
        tally.last_at = record.at;
        match record.lineage {
            Lineage::Node(_) => tally.nodes += 1,
            Lineage::Relation { .. } => tally.edges += 1,
        }
        tally.work_items.extend(work_items);
    }

    /// Todo registro passa por aqui, de qualquer sessão: o mapa das linhagens não depende de quem gravou.
    fn track_work_items(&mut self, record: &ScannedRecord) -> Vec<String> {
        match &record.lineage {
            Lineage::Node(id) => {
                // A retirada não tem dado: herda o work item do registro anterior da linhagem.
                if !record.retired {
                    match node_work_item(id, record) {
                        Some(work_item) => self
                            .node_work_items
                            .insert(id.as_str().to_owned(), work_item),
                        None => self.node_work_items.remove(id.as_str()),
                    };
                }
                self.node_work_items
                    .get(id.as_str())
                    .cloned()
                    .into_iter()
                    .collect()
            }
            Lineage::Relation {
                relation_type,
                from,
                to,
            } => {
                let edge = [relation_type.as_str(), from.as_str(), to.as_str()].map(str::to_owned);
                // A retirada não relê as pontas: elas podem ter mudado de work item desde a aresta.
                if !record.retired {
                    let of_ends = [from, to]
                        .into_iter()
                        .filter_map(|end| self.node_work_items.get(end.as_str()).cloned())
                        .collect();
                    self.edge_work_items.insert(edge.clone(), of_ends);
                }
                self.edge_work_items.get(&edge).cloned().unwrap_or_default()
            }
        }
    }

    fn into_json(self, only: &BTreeSet<String>) -> Vec<Value> {
        let mut sessions: Vec<(String, Tally)> = self.sessions.into_iter().collect();
        sessions.sort_by_key(|(_, tally)| tally.first_seq);

        sessions
            .into_iter()
            .filter(|(_, tally)| only.is_empty() || !tally.work_items.is_disjoint(only))
            .map(|(id, tally)| {
                json!({
                    "id": id,
                    "first": tally.first_at,
                    "last": tally.last_at,
                    "nodes": tally.nodes,
                    "edges": tally.edges,
                    "work_items": tally.work_items,
                })
            })
            .collect()
    }
}

fn node_work_item(id: &SemanticId, record: &ScannedRecord) -> Option<String> {
    if WORK_ITEM_TYPES.contains(&record.type_name.as_str()) {
        return Some(id.as_str().to_owned());
    }

    record
        .data
        .as_ref()?
        .get("work_item")?
        .as_str()
        .map(str::to_owned)
}

impl Engine<'_> {
    pub(super) fn report_sessions(&mut self, work_items: &[String]) -> Result<Value, AppError> {
        let mut only = BTreeSet::new();
        for work_item in work_items {
            let id = parse_at(SemanticId::parse, work_item, "/work_item")?;
            self.omhl.find_work_item(&self.tenant, &id)?;
            only.insert(id.as_str().to_owned());
        }
        let mut ledger = Ledger::default();
        self.omhl
            .scan_records(&self.tenant, |record| ledger.record(record))?;

        Ok(json!({ "sessions": ledger.into_json(&only) }))
    }
}
