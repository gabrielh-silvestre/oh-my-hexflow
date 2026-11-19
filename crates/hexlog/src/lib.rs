mod catalog;
mod engine;
mod error;
mod facade;
mod ids;
mod store;
#[cfg(test)]
mod test_support;

pub use crate::catalog::{Catalog, RelationTypeDef, TypeDef};
pub use crate::engine::CHECKED_FIELDS;
pub use crate::engine::limits::Limits;
pub use crate::engine::model::{
    AppendItem, AppendOutcome, AppendRequest, AppendedRecord, Author, BreakReason, ChainBreak,
    ChainHead, ChainReport, FieldName, ItemBody, Lineage, ListFilter, Origin, PageRequest,
    RelationQuery, Scalar,
};
pub use crate::engine::ports::{Clock, DataViolation, IdGenerator, NativeNode, NativeNodes};
pub use crate::engine::snapshot::{Snapshot, SnapshotKind};
pub use crate::error::{Detail, ErrorCode, HexlogError, pointer_segment};
pub use crate::facade::{
    Config, Deps, EndpointStatus, Hexlog, NodeView, Page, RelationView, RevisionView, TenantInfo,
    VerifyPolicy,
};
pub use crate::ids::{
    ExternalId, Hash, IdempotencyKey, Name, RecordId, RelationTypeName, SemanticId, TenantId,
    TypeName,
};

pub mod chain {
    pub use crate::engine::chain::{
        anchor, anchor_doc, canonical, fingerprint, hash_record, sha256hex,
    };
}
