use serde_json::Value;

use super::action::CheckTarget;
use super::engine::Engine;
use super::error::AppError;

impl Engine<'_> {
    #[allow(clippy::needless_pass_by_ref_mut)] // S7 muta o engine
    pub(super) fn check_run(
        &mut self,
        _target: &CheckTarget,
        _pre_push: Option<&str>,
    ) -> Result<Value, AppError> {
        unimplemented!()
    }
}
