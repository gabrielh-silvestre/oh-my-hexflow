use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;

impl Engine<'_> {
    pub(super) fn byproduct(
        &self,
        _check: &str,
        _work_item: &str,
        _tail: u32,
    ) -> Result<Value, AppError> {
        unimplemented!()
    }
}
