use hexlog::SemanticId;
use serde_json::Value;

use super::order::order;
use super::spec::{CheckSpec, Outcome, SpecError, snapshot_for, spec_digest};

#[derive(Debug, Clone, PartialEq)]
pub struct CheckNode {
    pub id: SemanticId,
    pub data: Value,
    pub shadowed: bool,
}

#[derive(Debug)]
pub struct PlannedCheck {
    pub id: SemanticId,
    pub spec: Result<CheckSpec, SpecError>,
    pub blocking: bool,
    pub snapshot: hexlog::SnapshotKind,
    pub spec_digest: String,
    pub concern: Option<String>,
    pub variant: Option<String>,
}

pub fn plan_run(
    event: &str,
    nodes: &[CheckNode],
    after: &[(SemanticId, SemanticId)],
) -> Vec<PlannedCheck> {
    let mut planned: Vec<PlannedCheck> = nodes
        .iter()
        .filter(|node| !node.shadowed && node.data["when"]["event"] == event)
        .filter_map(|node| {
            // `Ok(None)` é check só de texto e sai; `Err` fica no plano.
            let spec = CheckSpec::from_data(&node.data).transpose()?;
            Some(PlannedCheck {
                id: node.id.clone(),
                spec,
                blocking: node.data["blocking"] == true,
                snapshot: snapshot_for(&node.data),
                spec_digest: spec_digest(&node.data),
                concern: node.data["concern"].as_str().map(str::to_owned),
                variant: node.data["variant"].as_str().map(str::to_owned),
            })
        })
        .collect();
    let ids: Vec<SemanticId> = planned.iter().map(|check| check.id.clone()).collect();
    let sequence = order(&ids, after);
    planned.sort_by_key(|check| sequence.iter().position(|id| *id == check.id));
    planned
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub id: SemanticId,
    pub outcome: Outcome,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub blocked: Vec<SemanticId>,
}

pub fn summarize(results: &[CheckResult]) -> RunSummary {
    RunSummary {
        blocked: results
            .iter()
            .filter(|result| result.outcome == Outcome::Failed && result.blocking)
            .map(|result| result.id.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests;
