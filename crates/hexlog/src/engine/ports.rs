use serde_json::Value;
use uuid::Uuid;

use crate::engine::model::{
    AppendOutcome, ChainHead, Lineage, ListFilter, Origin, PageRequest, RelationQuery,
};
use crate::engine::snapshot::Snapshot;
use crate::error::HexlogError;
use crate::ids::{
    ExternalId, Hash, IdempotencyKey, Name, RecordId, RelationTypeName, SemanticId, TenantId,
    TypeName,
};

// Portas públicas: aparecem em `Deps` e o `omhl` as implementa.

pub trait Clock: Send + Sync {
    /// UTC com milissegundos, `YYYY-MM-DDTHH:MM:SS.mmmZ`.
    fn now(&self) -> String;
}

pub trait IdGenerator: Send + Sync {
    /// uuid v7 em produção.
    fn next(&self) -> Uuid;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNode {
    pub type_name: TypeName,
}

pub trait NativeNodes: Send + Sync {
    /// Id reservado é `lookup(id).is_some()`.
    fn lookup(&self, id: &SemanticId) -> Option<NativeNode>;
    fn native_relations(&self, relation_type: &RelationTypeName) -> Vec<(SemanticId, SemanticId)>;
    fn catalog_version(&self) -> &str;
}

// Portas internas

pub(crate) struct NodeTypeInfo {
    pub(crate) requires_snapshot: bool,
    pub(crate) def_hash: Hash,
}

pub(crate) struct RelationTypeInfo {
    pub(crate) from: Option<Vec<TypeName>>,
    pub(crate) to: Option<Vec<TypeName>>,
    pub(crate) acyclic: bool,
    pub(crate) def_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataViolation {
    pub pointer: String,
    pub code: String,
}

/// Implementada por `Catalog` (em `catalog`); o `engine` só conhece este trait.
pub(crate) trait Definitions: Send + Sync {
    fn node_type(&self, name: &TypeName) -> Option<&NodeTypeInfo>;
    fn relation_type(&self, name: &RelationTypeName) -> Option<&RelationTypeInfo>;
    /// No máximo 50 violações, com ponteiro JSON, e `too-many-errors` quando passa disso: o corte é só aqui.
    fn validate_data(&self, name: &TypeName, data: &Value) -> Result<(), Vec<DataViolation>>;
}

pub(crate) enum LineageKind {
    Node,
    Relation,
}

pub(crate) struct TenantRow {
    pub(crate) tenant: TenantId,
    pub(crate) anchor: Hash,
    pub(crate) anchor_doc: String,
    pub(crate) created_at: String,
}

/// `type_name` guarda o tipo do node ou o relation type, por isso `Name`.
pub(crate) struct LatestRow {
    pub(crate) record_id: RecordId,
    pub(crate) seq: u64,
    pub(crate) retired: bool,
    pub(crate) type_name: Name,
    pub(crate) def_hash: Hash,
}

pub(crate) struct BatchRow {
    pub(crate) key: IdempotencyKey,
    pub(crate) fingerprint: Hash,
    pub(crate) first_seq: u64,
    pub(crate) item_count: u32,
}

pub(crate) struct RecordColumns {
    pub(crate) at: String,
    pub(crate) type_name: Name,
    pub(crate) lineage_kind: LineageKind,
    pub(crate) lineage_key: String,
    pub(crate) retired: bool,
    pub(crate) revises: Option<RecordId>,
    pub(crate) relation_type: Option<RelationTypeName>,
    pub(crate) from_id: Option<SemanticId>,
    pub(crate) to_id: Option<SemanticId>,
    pub(crate) from_origin: Option<Origin>,
    pub(crate) to_origin: Option<Origin>,
    pub(crate) snapshot: Option<Snapshot>,
    pub(crate) external_id: Option<ExternalId>,
    pub(crate) def_hash: Hash,
}

pub(crate) struct RecordRow {
    pub(crate) tenant: TenantId,
    pub(crate) seq: u64,
    pub(crate) id: RecordId,
    pub(crate) prev_hash: Hash,
    pub(crate) hash: Hash,
    pub(crate) body: String,
    pub(crate) columns: RecordColumns,
}

pub(crate) struct NewRecord {
    pub(crate) seq: u64,
    pub(crate) id: RecordId,
    pub(crate) prev_hash: Hash,
    pub(crate) hash: Hash,
    pub(crate) body: String,
    pub(crate) columns: RecordColumns,
}

/// Resultado de `decide`: o que gravar (vazio no replay) e o que devolver ao chamador.
pub(crate) struct AppendPlan {
    pub(crate) records: Vec<NewRecord>,
    pub(crate) batch: Option<BatchRow>,
    pub(crate) outcome: AppendOutcome,
}

/// O que `decide` lê para gravar. Portas síncronas: o `block_on` fica dentro de `store/sqlite`.
pub(crate) trait WriteReads {
    fn tenant(&mut self, tenant: &TenantId) -> Result<Option<TenantRow>, HexlogError>;
    fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    fn latest_record(
        &mut self,
        tenant: &TenantId,
        lineage: &Lineage,
    ) -> Result<Option<LatestRow>, HexlogError>;
    fn record_by_id(&mut self, id: &RecordId) -> Result<Option<RecordRow>, HexlogError>;
    fn batch_by_key(
        &mut self,
        tenant: &TenantId,
        key: &IdempotencyKey,
    ) -> Result<Option<BatchRow>, HexlogError>;
    fn batch_records(
        &mut self,
        tenant: &TenantId,
        first_seq: u64,
        count: u32,
    ) -> Result<Vec<RecordRow>, HexlogError>;
    /// Pares das relations vigentes de `relation_type` cujos extremos do tenant também estão vigentes.
    fn live_relation_pairs(
        &mut self,
        tenant: &TenantId,
        relation_type: &RelationTypeName,
    ) -> Result<Vec<(SemanticId, SemanticId)>, HexlogError>;
}

/// O que a Facade lê para responder consultas.
pub(crate) trait QueryReads {
    fn current_nodes(
        &mut self,
        tenant: &TenantId,
        filter: &ListFilter,
        page: &PageRequest,
    ) -> Result<Vec<RecordRow>, HexlogError>;
    fn revisions(
        &mut self,
        tenant: &TenantId,
        lineage: &Lineage,
    ) -> Result<Vec<RecordRow>, HexlogError>;
    fn latest_relations(
        &mut self,
        tenant: &TenantId,
        query: &RelationQuery,
    ) -> Result<Vec<RecordRow>, HexlogError>;
}

/// O que `verify_chain` lê.
pub(crate) trait VerifyReads {
    fn tenant(&mut self, tenant: &TenantId) -> Result<Option<TenantRow>, HexlogError>;
    fn records_from(
        &mut self,
        tenant: &TenantId,
        from_seq: u64,
        limit: u32,
    ) -> Result<Vec<RecordRow>, HexlogError>;
    fn batches(&mut self, tenant: &TenantId) -> Result<Vec<BatchRow>, HexlogError>;
}

/// A soma das três, implementada uma vez pela transação do SQLite.
pub(crate) trait StoreReader: WriteReads + QueryReads + VerifyReads {}
impl<T: WriteReads + QueryReads + VerifyReads + ?Sized> StoreReader for T {}

pub(crate) trait Store: Send {
    fn ensure_tenant(
        &mut self,
        tenant: &TenantId,
        anchor_doc: &str,
        anchor: &Hash,
        created_at: &str,
    ) -> Result<(TenantRow, bool), HexlogError>;
    /// Abre transação exclusiva, entrega `WriteReads` a `decide` e anexa o plano devolvido de forma atômica.
    fn append_with(
        &mut self,
        tenant: &TenantId,
        decide: &mut dyn FnMut(&mut dyn WriteReads) -> Result<AppendPlan, HexlogError>,
    ) -> Result<AppendPlan, HexlogError>;
    /// Leitura consistente; o resultado sai por variável capturada pelo fechamento.
    fn read(
        &mut self,
        f: &mut dyn FnMut(&mut dyn StoreReader) -> Result<(), HexlogError>,
    ) -> Result<(), HexlogError>;
}
