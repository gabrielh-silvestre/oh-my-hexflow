use hexlog::SemanticId;
use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;

/// `DryRun` é a branch `develop`: nada é gravado.
#[allow(dead_code)] // construído pela S7 (`resolve_work_item`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemRoute {
    Recorded(SemanticId),
    DryRun,
}

/// `&mut` porque `Omhl::get` pede `&mut self`; o erro sai antes de rodar qualquer comando.
#[allow(dead_code)] // consumido pela S7 (`app/check_run.rs`)
pub(super) fn resolve_work_item(_engine: &mut Engine<'_>) -> Result<WorkItemRoute, AppError> {
    unimplemented!()
}

pub(super) fn consume_develop_mark(_engine: &mut Engine<'_>) -> Result<Value, AppError> {
    unimplemented!()
}
