use hexlog::SemanticId;
use serde_json::Value;

use super::spec::{CheckSpec, Outcome, SpecError};

#[derive(Debug, Clone, PartialEq)]
pub struct CheckNode {
    pub id: SemanticId,
    pub data: Value,
    pub shadowed: bool,
}

/// Spec gravado que o `from_data` recusa segue no plano como `Err`.
#[derive(Debug)]
pub struct PlannedCheck {
    pub id: SemanticId,
    pub spec: Result<CheckSpec, SpecError>,
    pub blocking: bool,
    pub snapshot: hexlog::SnapshotKind,
}

pub fn plan_run(
    _event: &str,
    _nodes: &[CheckNode],
    _after: &[(SemanticId, SemanticId)],
) -> Vec<PlannedCheck> {
    unimplemented!()
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
    pub not_passed: Vec<SemanticId>,
}

pub fn summarize(_results: &[CheckResult]) -> RunSummary {
    unimplemented!()
}

pub fn exit_blocked(_summary: &RunSummary) -> bool {
    unimplemented!()
}
