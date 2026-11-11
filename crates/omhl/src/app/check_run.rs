use serde_json::Value;

use super::action::CheckTarget;
use super::engine::Engine;
use super::error::AppError;

impl Engine<'_> {
    pub(super) fn check_run(
        &self,
        _target: &CheckTarget,
        _pre_push: Option<&str>,
    ) -> Result<Value, AppError> {
        unimplemented!()
    }
}
