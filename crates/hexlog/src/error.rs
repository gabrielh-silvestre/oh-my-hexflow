use thiserror::Error;

use crate::ids::RecordId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidInput,
    InvalidRecord,
    InvalidSchema,
    TypeNotFound,
    RelationTypeNotFound,
    TenantNotFound,
    ReservedId,
    ForkRejected,
    CycleRejected,
    IdempotencyConflict,
    ChainBroken,
    Busy,
    StoreError,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidInput => "INVALID_INPUT",
            ErrorCode::InvalidRecord => "INVALID_RECORD",
            ErrorCode::InvalidSchema => "INVALID_SCHEMA",
            ErrorCode::TypeNotFound => "TYPE_NOT_FOUND",
            ErrorCode::RelationTypeNotFound => "RELATION_TYPE_NOT_FOUND",
            ErrorCode::TenantNotFound => "TENANT_NOT_FOUND",
            ErrorCode::ReservedId => "RESERVED_ID",
            ErrorCode::ForkRejected => "FORK_REJECTED",
            ErrorCode::CycleRejected => "CYCLE_REJECTED",
            ErrorCode::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            ErrorCode::ChainBroken => "CHAIN_BROKEN",
            ErrorCode::Busy => "BUSY",
            ErrorCode::StoreError => "STORE_ERROR",
            ErrorCode::Internal => "INTERNAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub path: String,
    pub code: String,
    pub message: String,
    pub current: Option<RecordId>,
    pub retired: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{}: {}", .code.as_str(), .message)]
pub struct HexlogError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Vec<Detail>,
}

impl HexlogError {
    /// Recusa com um único detalhe de `path` vazio; `message` descreve a regra, nunca o valor.
    pub(crate) fn refused(code: ErrorCode, detail_code: &str, message: &str) -> HexlogError {
        HexlogError {
            code,
            message: message.to_owned(),
            details: vec![Detail {
                path: String::new(),
                code: detail_code.to_owned(),
                message: message.to_owned(),
                current: None,
                retired: None,
            }],
        }
    }

    pub(crate) fn internal(message: &str) -> HexlogError {
        HexlogError {
            code: ErrorCode::Internal,
            message: message.to_owned(),
            details: Vec::new(),
        }
    }

    /// Tenant desconhecido, sem ecoar o nome.
    pub(crate) fn tenant_not_found() -> HexlogError {
        HexlogError::refused(
            ErrorCode::TenantNotFound,
            "tenant-not-found",
            "unknown tenant",
        )
        .at("/tenant")
    }

    /// Antepõe `prefix` ao `path` de cada detalhe (um `path` vazio vira o próprio `prefix`).
    pub fn at(mut self, prefix: &str) -> HexlogError {
        for detail in &mut self.details {
            detail.path.insert_str(0, prefix);
        }
        self
    }

    /// Como `at`, para o campo `field` do item `index` do lote.
    pub(crate) fn at_item(self, index: usize, field: &str) -> HexlogError {
        self.at(&format!("/items/{index}/{field}"))
    }
}

/// Escapa um segmento de JSON Pointer (RFC 6901).
pub fn pointer_segment(segment: &str) -> String {
    let mut escaped = String::with_capacity(segment.len());
    push_pointer_segment(&mut escaped, segment);
    escaped
}

/// Como `pointer_segment`, mas acrescenta ao `out` em vez de alocar.
pub(crate) fn push_pointer_segment(out: &mut String, segment: &str) {
    for c in segment.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            c => out.push(c),
        }
    }
}
