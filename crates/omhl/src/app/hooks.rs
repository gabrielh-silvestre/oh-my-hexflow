use serde_json::Value;

use super::engine::Engine;
use super::error::AppError;

impl Engine<'_> {
    #[allow(clippy::needless_pass_by_ref_mut)] // a S15 remove ao preencher o corpo
    pub(super) fn hooks_install(&mut self, _force: bool) -> Result<Value, AppError> {
        unimplemented!()
    }
}
