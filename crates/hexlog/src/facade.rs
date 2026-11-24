use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::catalog::Catalog;
use crate::engine::chain::{anchor, anchor_doc};
use crate::engine::limits::Limits;
use crate::engine::model::{
    AppendOutcome, AppendRequest, Author, BodyAuthor, BodyLineage, BodySnapshot, ChainHead,
    ChainReport, Lineage, LineageRef, ListFilter, Origin, PageRequest, RecordBody, RelationQuery,
    invalid_body, read_body,
};
use crate::engine::ports::{
    Clock, IdGenerator, NativeNodes, RecordRow, Store, StoreReader, WriteReads,
};
use crate::engine::scan::{ScannedRecord, scan_records};
use crate::engine::snapshot::{Snapshot, SnapshotKind};
use crate::engine::{DecideCtx, decide, instant_is_well_formed, verify_chain};
use crate::error::{ErrorCode, HexlogError};
use crate::ids::{
    ExternalId, Hash, RecordId, RelationTypeName, SemanticId, SessionLabel, TenantId, TypeName,
};
use crate::store::SqliteStore;

/// Tetos que o esquema do banco impõe (`CHECK` de `item_count`) ou que a página aceita.
const BATCH_ITEMS_CEILING: usize = 50;
const DETAILS_CEILING: usize = 50;
const PAGE_CEILING: u32 = 200;
const BUSY_TIMEOUT_DEFAULT: Duration = Duration::from_secs(5);
/// Maior prazo que o SQLite aceita (um `int` de milissegundos); acima disso o `sqlx` dá panic na própria thread.
const BUSY_TIMEOUT_CEILING: Duration = Duration::from_millis(2_147_483_647);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyPolicy {
    Tail,
    Full,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub db_path: PathBuf,
    pub limits: Limits,
    /// Um `fsync` parado pode
    /// estender o `COMMIT` de quem segura a trava por dezenas de segundos, então nenhum prazo é
    /// garantido sob `fsync` sem teto.
    pub busy_timeout: Duration,
    pub verify_policy: VerifyPolicy,
}

impl Config {
    pub fn new(db_path: PathBuf) -> Config {
        Config {
            db_path,
            limits: Limits::default(),
            busy_timeout: BUSY_TIMEOUT_DEFAULT,
            verify_policy: VerifyPolicy::Tail,
        }
    }
}

pub struct Deps<'d> {
    pub clock: &'d dyn Clock,
    pub ids: &'d dyn IdGenerator,
    pub catalog: Catalog,
    pub native: &'d dyn NativeNodes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TenantInfo {
    pub tenant: TenantId,
    pub anchor: Hash,
    pub created_at: String,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_after_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeView {
    pub id: SemanticId,
    pub type_name: TypeName,
    pub record_id: RecordId,
    pub seq: u64,
    pub at: String,
    pub data: Value,
    pub snapshot: Option<Snapshot>,
    pub external_id: Option<ExternalId>,
    pub author: Author,
    pub shadowed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RevisionView {
    pub record_id: RecordId,
    pub seq: u64,
    pub at: String,
    pub revises: Option<RecordId>,
    pub retired: bool,
    pub data: Option<Value>,
    pub snapshot: Option<Snapshot>,
    pub external_id: Option<ExternalId>,
    pub author: Author,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointStatus {
    Current,
    Retired,
    MissingNative,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationView {
    pub record_id: RecordId,
    pub seq: u64,
    pub at: String,
    pub relation_type: RelationTypeName,
    pub from: SemanticId,
    pub to: SemanticId,
    pub from_origin: Origin,
    pub to_origin: Origin,
    pub from_status: EndpointStatus,
    pub to_status: EndpointStatus,
    pub native_catalog: Option<String>,
    pub external_id: Option<ExternalId>,
    pub author: Author,
    pub retired: bool,
}

pub struct Hexlog<'d> {
    store: SqliteStore,
    deps: Deps<'d>,
    limits: Limits,
    verify_policy: VerifyPolicy,
}

impl<'d> Hexlog<'d> {
    pub fn open(config: &Config, deps: Deps<'d>) -> Result<Hexlog<'d>, HexlogError> {
        validate_limits(&config.limits)?;
        validate_busy_timeout(config.busy_timeout)?;
        validate_db_path(&config.db_path)?;
        Ok(Hexlog {
            store: SqliteStore::open(&config.db_path, config.busy_timeout)?,
            deps,
            limits: config.limits,
            verify_policy: config.verify_policy,
        })
    }

    pub fn ensure_tenant(&mut self, tenant: &TenantId) -> Result<TenantInfo, HexlogError> {
        let now = self.deps.clock.now();
        if !instant_is_well_formed(&now) {
            return Err(HexlogError::refused(
                ErrorCode::Internal,
                "clock-format",
                "clock returned a malformed instant",
            ));
        }
        let doc = anchor_doc(tenant, &now);
        let (row, created) = self
            .store
            .ensure_tenant(tenant, &doc, &anchor(&doc), &now)?;
        Ok(TenantInfo {
            tenant: tenant.clone(),
            anchor: row.anchor,
            created_at: row.created_at,
            created,
        })
    }

    /// Entre escritores concorrentes `at` pode sair fora da ordem de `seq`.
    /// O `acyclic` relê os pares vivos do tipo a cada `append`, então o custo cresce com o grafo gravado, e só é conferido
    /// na escrita da relation: reativar nó retirado ou nativo de catálogo novo não revalida.
    pub fn append(&mut self, req: &AppendRequest) -> Result<AppendOutcome, HexlogError> {
        self.append_labeled(req, None)
    }

    /// O rótulo não entra na impressão digital do lote: o retry com outra sessão é replay.
    pub fn append_with_session(
        &mut self,
        req: &AppendRequest,
        session: &SessionLabel,
    ) -> Result<AppendOutcome, HexlogError> {
        self.append_labeled(req, Some(session))
    }

    fn append_labeled(
        &mut self,
        req: &AppendRequest,
        session: Option<&SessionLabel>,
    ) -> Result<AppendOutcome, HexlogError> {
        if self.verify_policy == VerifyPolicy::Full {
            // Fora da transação de escrita: a verificação não disputa corrida, um concorrente só acrescenta.
            // Tenant desconhecido segue para o `decide`, que recusa a forma antes do tenant, como em `Tail`.
            match self
                .store
                .read(&mut |reader| refuse_when_broken(reader, &req.tenant))
            {
                Err(error) if error.code == ErrorCode::TenantNotFound => {}
                verified => verified?,
            }
        }
        let now = self.deps.clock.now();
        // Campos disjuntos de `self`: o `ctx` empresta `deps` e `limits` enquanto o store é emprestado como `&mut`.
        let ctx = DecideCtx {
            defs: &self.deps.catalog,
            native: self.deps.native,
            ids: self.deps.ids,
            limits: &self.limits,
            now: &now,
            session,
        };
        let plan = self
            .store
            .append_with(&req.tenant, &mut |reads| decide(&ctx, req, reads))?;
        Ok(plan.outcome)
    }

    pub fn get(
        &mut self,
        tenant: &TenantId,
        id: &SemanticId,
    ) -> Result<Option<NodeView>, HexlogError> {
        let native = self.deps.native;
        self.read(tenant, |reader| {
            let Some(latest) = reader.latest_record(tenant, LineageRef::Node(id))? else {
                return Ok(None);
            };
            if latest.retired {
                return Ok(None);
            }
            let row = reader
                .record_by_id(tenant, &latest.record_id)?
                .ok_or_else(|| HexlogError::internal("latest record is missing"))?;
            node_view(row, native).map(Some)
        })
    }

    pub fn lineage_records(
        &mut self,
        tenant: &TenantId,
        lineage: &Lineage,
    ) -> Result<Vec<RevisionView>, HexlogError> {
        self.read(tenant, |reader| {
            reader
                .revisions(tenant, LineageRef::from(lineage))?
                .into_iter()
                .map(|row| revision_from(read_body(&row.body)?, row.id, row.seq))
                .collect()
        })
    }

    pub fn list(
        &mut self,
        tenant: &TenantId,
        filter: &ListFilter,
        page: &PageRequest,
    ) -> Result<Page<NodeView>, HexlogError> {
        if !(1..=self.limits.page_max).contains(&page.limit) {
            return Err(HexlogError::refused(
                ErrorCode::InvalidInput,
                "page-limit",
                "page limit is outside the supported range",
            )
            .at("/page/limit"));
        }
        if page
            .after_seq
            .is_some_and(|after| i64::try_from(after).is_err())
        {
            return Err(HexlogError::refused(
                ErrorCode::InvalidInput,
                "page-after-seq",
                "page cursor is above the database range",
            )
            .at("/page/after_seq"));
        }
        let limit = usize::try_from(page.limit)
            .map_err(|_| HexlogError::internal("page limit out of range"))?;
        // Uma linha a mais diz se há próxima página sem uma segunda consulta.
        let probe = PageRequest {
            limit: page.limit + 1,
            after_seq: page.after_seq,
        };
        let native = self.deps.native;
        self.read(tenant, |reader| {
            let mut rows = reader.current_nodes(tenant, filter, &probe)?;
            let has_more = rows.len() > limit;
            rows.truncate(limit);
            let next_after_seq = rows.last().filter(|_| has_more).map(|row| row.seq);
            let items = rows
                .into_iter()
                .map(|row| node_view(row, native))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page {
                items,
                next_after_seq,
            })
        })
    }

    pub fn relations(
        &mut self,
        tenant: &TenantId,
        query: &RelationQuery,
    ) -> Result<Vec<RelationView>, HexlogError> {
        let native = self.deps.native;
        self.read(tenant, |reader| {
            reader
                .latest_relations(tenant, query)?
                .into_iter()
                .map(|row| relation_view(row, native, tenant, reader))
                .collect()
        })
    }

    /// Todo record do tenant, em ordem de `seq`, numa só transação de leitura.
    pub fn scan_records(
        &mut self,
        tenant: &TenantId,
        mut visit: impl FnMut(ScannedRecord),
    ) -> Result<(), HexlogError> {
        self.read(tenant, |reader| scan_records(reader, tenant, &mut visit))
    }

    pub fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, HexlogError> {
        self.read(tenant, |reader| WriteReads::chain_head(reader, tenant))
    }

    /// `expected_chain_head` revela uma cauda apagada ou reescrita.
    pub fn verify_chain(
        &mut self,
        tenant: &TenantId,
        expected_chain_head: Option<&Hash>,
    ) -> Result<ChainReport, HexlogError> {
        let mut found = None;
        self.store.read(&mut |reader| {
            found = Some(verify_chain(reader, tenant, expected_chain_head)?);
            Ok(())
        })?;
        found.ok_or_else(|| HexlogError::internal("verification produced no report"))
    }

    pub fn catalog(&self) -> &Catalog {
        &self.deps.catalog
    }

    fn read<T>(
        &mut self,
        tenant: &TenantId,
        mut query: impl FnMut(&mut dyn StoreReader) -> Result<T, HexlogError>,
    ) -> Result<T, HexlogError> {
        let policy = self.verify_policy;
        let mut found = None;
        self.store.read(&mut |reader| {
            if policy == VerifyPolicy::Full {
                refuse_when_broken(reader, tenant)?;
            }
            if WriteReads::tenant(reader, tenant)?.is_none() {
                return Err(HexlogError::tenant_not_found());
            }
            found = Some(query(reader)?);
            Ok(())
        })?;
        found.ok_or_else(|| HexlogError::internal("read produced no result"))
    }
}

fn refuse_when_broken(reader: &mut dyn StoreReader, tenant: &TenantId) -> Result<(), HexlogError> {
    let report = verify_chain(reader, tenant, None)?;
    match report.breaks.first() {
        None => Ok(()),
        Some(first) => Err(HexlogError::refused(
            ErrorCode::ChainBroken,
            first.reason.as_str(),
            "chain verification found a break",
        )),
    }
}

fn validate_limits(limits: &Limits) -> Result<(), HexlogError> {
    let fields = [
        (
            "batch_items",
            (1..=BATCH_ITEMS_CEILING).contains(&limits.batch_items),
        ),
        (
            "details_max",
            (1..=DETAILS_CEILING).contains(&limits.details_max),
        ),
        ("page_max", (1..=PAGE_CEILING).contains(&limits.page_max)),
        (
            "page_default",
            (1..=limits.page_max).contains(&limits.page_default),
        ),
    ];
    match fields.iter().find(|(_, in_range)| !in_range) {
        Some((field, _)) => Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "limits-out-of-range",
            "limit is outside the supported range",
        )
        .at(&format!("/limits/{field}"))),
        None => Ok(()),
    }
}

fn validate_busy_timeout(busy_timeout: Duration) -> Result<(), HexlogError> {
    if busy_timeout > BUSY_TIMEOUT_CEILING {
        return Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "busy-timeout-out-of-range",
            "busy timeout is above what the database accepts",
        )
        .at("/busy_timeout"));
    }
    Ok(())
}

/// O SQLite lê `file:` como URI e `:memory:` como banco sem arquivo; os dois fugiriam do WAL e do arquivo.
fn validate_db_path(db_path: &Path) -> Result<(), HexlogError> {
    let bytes = db_path.as_os_str().as_encoded_bytes();
    if bytes.starts_with(b"file:") || bytes == b":memory:" {
        return Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "db-path-invalid",
            "database path is read as a URI or an in-memory database",
        )
        .at("/db_path"));
    }
    Ok(())
}

fn author_from(author: BodyAuthor<'_>) -> Author {
    Author {
        agent: author.agent.into_owned(),
        model: author.model.map(Cow::into_owned),
        client: author.client.into_owned(),
    }
}

fn snapshot_from(snapshot: Option<BodySnapshot<'_>>) -> Result<Option<Snapshot>, HexlogError> {
    snapshot
        .map(|found| {
            let kind = SnapshotKind::parse(&found.kind).ok_or_else(invalid_body)?;
            Snapshot::new(kind, &found.id).map_err(|_| invalid_body())
        })
        .transpose()
}

fn external_id_from(external_id: Option<&str>) -> Result<Option<ExternalId>, HexlogError> {
    external_id
        .map(ExternalId::parse)
        .transpose()
        .map_err(|_| invalid_body())
}

fn revision_from(
    body: RecordBody<'_>,
    record_id: RecordId,
    seq: u64,
) -> Result<RevisionView, HexlogError> {
    Ok(RevisionView {
        record_id,
        seq,
        at: body.at.into_owned(),
        revises: body
            .revises
            .as_deref()
            .map(RecordId::parse)
            .transpose()
            .map_err(|_| invalid_body())?,
        retired: body.retirement,
        data: body.data.map(Cow::into_owned),
        snapshot: snapshot_from(body.snapshot)?,
        external_id: external_id_from(body.external_id.as_deref())?,
        author: author_from(body.author),
    })
}

fn node_view(row: RecordRow, native: &dyn NativeNodes) -> Result<NodeView, HexlogError> {
    let body = read_body(&row.body)?;
    let BodyLineage::Node { id } = &body.lineage else {
        return Err(invalid_body());
    };
    let id = SemanticId::parse(id).map_err(|_| invalid_body())?;
    let type_name = TypeName::parse(&body.type_name).map_err(|_| invalid_body())?;
    let shadowed = native.lookup(&id).is_some();
    Ok(NodeView {
        id,
        type_name,
        record_id: row.id,
        seq: row.seq,
        at: body.at.into_owned(),
        data: body.data.map(Cow::into_owned).ok_or_else(invalid_body)?,
        snapshot: snapshot_from(body.snapshot)?,
        external_id: external_id_from(body.external_id.as_deref())?,
        author: author_from(body.author),
        shadowed,
    })
}

fn relation_view(
    row: RecordRow,
    native: &dyn NativeNodes,
    tenant: &TenantId,
    reader: &mut dyn StoreReader,
) -> Result<RelationView, HexlogError> {
    let body = read_body(&row.body)?;
    let BodyLineage::Relation {
        relation_type,
        from,
        to,
        from_origin,
        to_origin,
    } = &body.lineage
    else {
        return Err(invalid_body());
    };
    let relation_type = RelationTypeName::parse(relation_type).map_err(|_| invalid_body())?;
    let from = SemanticId::parse(from).map_err(|_| invalid_body())?;
    let to = SemanticId::parse(to).map_err(|_| invalid_body())?;
    let from_origin = Origin::parse(from_origin).ok_or_else(invalid_body)?;
    let to_origin = Origin::parse(to_origin).ok_or_else(invalid_body)?;
    let from_status = endpoint_status(reader, native, tenant, &from, from_origin)?;
    let to_status = endpoint_status(reader, native, tenant, &to, to_origin)?;
    Ok(RelationView {
        record_id: row.id,
        seq: row.seq,
        at: body.at.into_owned(),
        relation_type,
        from,
        to,
        from_origin,
        to_origin,
        from_status,
        to_status,
        native_catalog: body.native_catalog.map(Cow::into_owned),
        external_id: external_id_from(body.external_id.as_deref())?,
        author: author_from(body.author),
        retired: body.retirement,
    })
}

fn endpoint_status(
    reader: &mut dyn StoreReader,
    native: &dyn NativeNodes,
    tenant: &TenantId,
    id: &SemanticId,
    origin: Origin,
) -> Result<EndpointStatus, HexlogError> {
    match origin {
        Origin::Native if native.lookup(id).is_some() => Ok(EndpointStatus::Current),
        Origin::Native => Ok(EndpointStatus::MissingNative),
        Origin::Tenant => {
            let latest = reader
                .latest_record(tenant, LineageRef::Node(id))?
                .ok_or_else(invalid_body)?;
            Ok(if latest.retired {
                EndpointStatus::Retired
            } else {
                EndpointStatus::Current
            })
        }
    }
}
