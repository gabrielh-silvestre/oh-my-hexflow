//! Helpers dos testes unitários (`tests.rs` ao lado da fonte).
//!
//! Cópias dos helpers de `tests/integration/support/` (uma mudança aqui vale também lá, e vice-versa):
//! `SeqIds`, `assert_refusal` e `catalog::reference_catalog`.
//!
//! Helpers que o `catalog/types/tests.rs` copia de `tests/integration/catalog.rs` (uma mudança num
//! vale também no outro): `type_def`, `pairs`, `refused`, `accepted` e `violations`.

pub mod catalog;

use std::sync::atomic::{AtomicU64, Ordering};

use uuid::Uuid;

use crate::{ErrorCode, HexlogError, IdGenerator};

const UUID_V7_BASE: u128 = 0x0192_a000_0000_7000_8000_0000_0000_0000;

pub struct SeqIds(AtomicU64);

impl SeqIds {
    pub fn new() -> SeqIds {
        SeqIds(AtomicU64::new(1))
    }
}

impl Default for SeqIds {
    fn default() -> SeqIds {
        SeqIds::new()
    }
}

impl IdGenerator for SeqIds {
    fn next(&self) -> Uuid {
        let n = self.0.fetch_add(1, Ordering::Relaxed);
        Uuid::from_u128(UUID_V7_BASE | u128::from(n))
    }
}

pub fn assert_refusal(error: &HexlogError, code: ErrorCode, detail_code: &str, path: &str) {
    assert_eq!(error.code, code, "error code of {error:?}");
    assert_eq!(error.details[0].code, detail_code, "detail code");
    assert_eq!(error.details[0].path, path, "detail path");
}
