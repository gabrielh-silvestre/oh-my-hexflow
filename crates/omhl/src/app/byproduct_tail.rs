use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;

impl Engine<'_> {
    #[allow(clippy::needless_pass_by_ref_mut)] // S14 muta o engine
    pub(super) fn byproduct(
        &mut self,
        _check: &str,
        _work_item: &str,
        _tail: u32,
    ) -> Result<Value, AppError> {
        unimplemented!()
    }
}
