use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;

impl Engine<'_> {
    pub(super) fn hooks_install(&self, _force: bool) -> Result<Value, AppError> {
        unimplemented!()
    }
}
