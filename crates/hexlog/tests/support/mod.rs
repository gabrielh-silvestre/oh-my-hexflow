#![allow(dead_code)]

pub mod catalog;
pub mod mutations;
pub mod tamper;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hexlog::{
    AppendItem, AppendRequest, Author, Catalog, Clock, Config, Deps, ErrorCode, ExternalId,
    FieldName, Hexlog, HexlogError, IdGenerator, IdempotencyKey, ItemBody, Lineage, ListFilter,
    NativeNode, NativeNodes, PageRequest, RecordId, RelationQuery, RelationTypeName, SemanticId,
    Snapshot, SnapshotKind, TenantId, TypeName,
};
use serde_json::Value;
use tempfile::TempDir;
use uuid::Uuid;

use self::catalog::reference_catalog;

pub const DEFAULT_INSTANT: &str = "2026-11-07T12:00:00.000Z";
const SMALL_STACK: usize = 2 * 1024 * 1024;
const UUID_V7_BASE: u128 = 0x0192_a000_0000_7000_8000_0000_0000_0000;

pub struct FixedClock {
    instant: String,
}

impl FixedClock {
    pub fn at(instant: &str) -> FixedClock {
        FixedClock {
            instant: instant.to_owned(),
        }
    }
}

impl Default for FixedClock {
    fn default() -> FixedClock {
        FixedClock::at(DEFAULT_INSTANT)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> String {
        self.instant.clone()
    }
}

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

pub struct FakeNative {
    version: String,
    nodes: Vec<(SemanticId, TypeName)>,
    relations: Vec<(RelationTypeName, SemanticId, SemanticId)>,
}

impl FakeNative {
    pub fn new(version: &str) -> FakeNative {
        FakeNative {
            version: version.to_owned(),
            nodes: Vec::new(),
            relations: Vec::new(),
        }
    }

    pub fn reserve(&mut self, id: SemanticId, type_name: TypeName) {
        self.nodes.push((id, type_name));
    }

    pub fn reserve_relation(
        &mut self,
        relation_type: RelationTypeName,
        from: SemanticId,
        to: SemanticId,
    ) {
        self.relations.push((relation_type, from, to));
    }

    /// Esquece um id reservado, para simular um nativo que sumiu do catálogo nativo.
    pub fn forget(&mut self, id: &SemanticId) {
        self.nodes.retain(|(reserved, _)| reserved != id);
    }
}

impl NativeNodes for FakeNative {
    fn lookup(&self, id: &SemanticId) -> Option<NativeNode> {
        self.nodes
            .iter()
            .find(|(reserved, _)| reserved == id)
            .map(|(_, type_name)| NativeNode {
                type_name: type_name.clone(),
            })
    }

    fn native_relations(&self, relation_type: &RelationTypeName) -> Vec<(SemanticId, SemanticId)> {
        self.relations
            .iter()
            .filter(|(reserved, _, _)| reserved == relation_type)
            .map(|(_, from, to)| (from.clone(), to.clone()))
            .collect()
    }

    fn catalog_version(&self) -> &str {
        &self.version
    }
}

/// Donos do que um `Hexlog` empresta. Para alterar `clock` ou `native`, solte o `Hexlog` antes.
pub struct Harness {
    pub dir: TempDir,
    pub clock: FixedClock,
    pub ids: SeqIds,
    pub native: FakeNative,
}

impl Harness {
    pub fn new() -> Harness {
        Harness {
            dir: TempDir::new().expect("temporary directory is created"),
            clock: FixedClock::default(),
            ids: SeqIds::new(),
            native: FakeNative::new("v1"),
        }
    }

    /// Catálogo nativo `v1` com `n.other`, do tipo `note`, já reservado.
    pub fn with_native_other() -> Harness {
        let mut harness = Harness::new();
        harness.native.reserve(sid("n.other"), ty("note"));

        harness
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir.path().join("hexlog.db")
    }

    pub fn config(&self) -> Config {
        Config::new(self.db_path())
    }

    /// Catálogo de referência, sem tenant.
    pub fn open(&self) -> Hexlog<'_> {
        self.open_with(&self.config(), reference_catalog())
    }

    pub fn open_with(&self, config: &Config, catalog: Catalog) -> Hexlog<'_> {
        self.try_open_with(config, catalog).expect("hexlog opens")
    }

    /// Como `open_with`, mas devolve a recusa em vez de falhar o teste.
    pub fn try_open_with(
        &self,
        config: &Config,
        catalog: Catalog,
    ) -> Result<Hexlog<'_>, HexlogError> {
        let deps = Deps {
            clock: &self.clock,
            ids: &self.ids,
            catalog,
            native: &self.native,
        };

        Hexlog::open(config, deps)
    }

    /// Catálogo de referência, com o tenant `acme` já criado.
    pub fn open_acme(&self) -> Hexlog<'_> {
        self.open_acme_with(&self.config(), reference_catalog())
    }

    pub fn open_acme_with(&self, config: &Config, catalog: Catalog) -> Hexlog<'_> {
        let mut hexlog = self.open_with(config, catalog);
        hexlog
            .ensure_tenant(&tenant("acme"))
            .expect("tenant acme is created");

        hexlog
    }
}

pub fn author() -> Author {
    Author {
        agent: "tester".to_owned(),
        model: None,
        client: "cli".to_owned(),
    }
}

pub fn tenant(text: &str) -> TenantId {
    TenantId::parse(text).expect("valid tenant")
}

pub fn sid(text: &str) -> SemanticId {
    SemanticId::parse(text).expect("valid semantic id")
}

pub fn ty(text: &str) -> TypeName {
    TypeName::parse(text).expect("valid type name")
}

pub fn rty(text: &str) -> RelationTypeName {
    RelationTypeName::parse(text).expect("valid relation type name")
}

pub fn field(text: &str) -> FieldName {
    FieldName::parse(text).expect("valid field name")
}

pub fn node_lineage(id: &str) -> Lineage {
    Lineage::Node(sid(id))
}

/// Id técnico bem formado que nenhum teste grava.
pub fn unused_record_id() -> RecordId {
    RecordId::parse("acme:0192a000-0000-7000-8000-0000000000ff").expect("valid record id")
}

/// Commit de 40 caracteres iguais a `fill`.
pub fn commit(fill: char) -> Snapshot {
    let id = fill.to_string().repeat(40);

    Snapshot::new(SnapshotKind::GitCommit, &id).expect("valid snapshot")
}

/// Item `Node` sem `base`, `snapshot` nem `external_id`.
pub fn node(id: &str, type_name: &str, data: Value) -> AppendItem {
    AppendItem {
        body: ItemBody::Node {
            id: sid(id),
            type_name: ty(type_name),
            data,
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

pub fn revise(mut item: AppendItem, base: &RecordId) -> AppendItem {
    item.base = Some(base.clone());
    item
}

pub fn retire(id: &str, base: &RecordId) -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: node_lineage(id),
        },
        base: Some(base.clone()),
        snapshot: None,
        external_id: None,
    }
}

/// Item `Relation` sem `base`, `snapshot` nem `external_id`.
pub fn relation(relation_type: &str, from: &str, to: &str) -> AppendItem {
    AppendItem {
        body: ItemBody::Relation {
            relation_type: rty(relation_type),
            from: sid(from),
            to: sid(to),
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

pub fn revise_relation(item: AppendItem, base: &RecordId) -> AppendItem {
    revise(item, base)
}

pub fn relation_lineage(relation_type: &str, from: &str, to: &str) -> Lineage {
    Lineage::Relation {
        relation_type: rty(relation_type),
        from: sid(from),
        to: sid(to),
    }
}

pub fn retire_relation(relation_type: &str, from: &str, to: &str, base: &RecordId) -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: relation_lineage(relation_type, from, to),
        },
        base: Some(base.clone()),
        snapshot: None,
        external_id: None,
    }
}

pub fn with_snapshot(mut item: AppendItem, snapshot: Snapshot) -> AppendItem {
    item.snapshot = Some(snapshot);
    item
}

pub fn with_external(mut item: AppendItem, external_id: &str) -> AppendItem {
    item.external_id = Some(ExternalId::parse(external_id).expect("valid external id"));
    item
}

pub fn request(tenant_name: &str, key: Option<&str>, items: Vec<AppendItem>) -> AppendRequest {
    AppendRequest {
        tenant: tenant(tenant_name),
        author: author(),
        key: key.map(|text| IdempotencyKey::parse(text).expect("valid key")),
        items,
    }
}

/// Grava `item` sozinho no tenant `acme`, sem `key`, e devolve o id técnico do record.
pub fn append_one(hexlog: &mut Hexlog<'_>, item: AppendItem) -> RecordId {
    hexlog
        .append(&request("acme", None, vec![item]))
        .expect("the item is appended")
        .records
        .into_iter()
        .next()
        .expect("one record")
        .id
}

/// Filtro que não restringe nada.
pub fn all() -> ListFilter {
    ListFilter {
        type_name: None,
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    }
}

/// Consulta que não restringe nada e não inclui relations retiradas.
pub fn relation_query() -> RelationQuery {
    RelationQuery {
        relation_type: None,
        from: None,
        to: None,
        include_retired: false,
    }
}

pub fn page(limit: u32) -> PageRequest {
    PageRequest {
        limit,
        after_seq: None,
    }
}

/// Afirma `code`, o `code` do primeiro detalhe e o `path` dele.
pub fn assert_refusal(error: &HexlogError, code: ErrorCode, detail_code: &str, path: &str) {
    assert_eq!(error.code, code, "error code of {error:?}");
    assert_eq!(error.details[0].code, detail_code, "detail code");
    assert_eq!(error.details[0].path, path, "detail path");
}

/// Recusa por base velha: `ForkRejected`, `not-current`, o `current` e se ele é uma retirement.
pub fn assert_fork_rejected(error: &HexlogError, path: &str, current: &RecordId, retired: bool) {
    assert_refusal(error, ErrorCode::ForkRejected, "not-current", path);
    assert_eq!(error.details[0].current.as_ref(), Some(current));
    assert_eq!(error.details[0].retired, Some(retired));
}

/// Pares `(path, code)` de todos os detalhes, na ordem em que saíram.
pub fn detail_pairs(error: &HexlogError) -> Vec<(&str, &str)> {
    error
        .details
        .iter()
        .map(|detail| (detail.path.as_str(), detail.code.as_str()))
        .collect()
}

/// Roda `work` numa thread de 2 MiB. O valor fundo nasce e morre dentro dela, porque o `Drop`
/// recursivo do `Value` também usa a pilha.
pub fn on_small_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn_scoped(scope, work)
            .expect("thread spawns")
            .join()
            .expect("thread does not panic")
    })
}

/// `data` com `containers` níveis de contêiner: o objeto da raiz e arrays aninhados na chave `deep`.
pub fn nested_data(containers: usize) -> Value {
    let mut inner = Value::Null;
    for _ in 1..containers {
        inner = Value::Array(vec![inner]);
    }

    serde_json::json!({ "deep": inner })
}
