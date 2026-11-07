use futures_executor::block_on;
use serde_json::{Value, json};
use sqlx::query::Query;
use sqlx::sqlite::{Sqlite, SqliteArguments, SqliteConnection, SqliteRow};
use sqlx::{Connection, Decode, Row, Transaction, Type};

use crate::engine::model::{
    ChainHead, FieldName, Lineage, ListFilter, Origin, PageRequest, RelationQuery, Scalar,
    lineage_columns,
};
use crate::engine::ports::{
    AppendPlan, BatchRow, LatestRow, LineageKind, NewRecord, QueryReads, RecordColumns, RecordRow,
    StoredBatch, StoredRow, StoredTenant, TenantRow, VerifyReads, WriteReads,
};
use crate::engine::snapshot::{Snapshot, SnapshotKind};
use crate::error::{ErrorCode, HexlogError};
use crate::ids::{
    ExternalId, Hash, IdempotencyKey, Name, RecordId, RelationTypeName, SemanticId, TenantId,
};

// Fragmentos de SQL fixos: `concat!` os junta em tempo de compilação, então toda consulta é `&'static str`.
macro_rules! record_columns {
    () => {
        "r.tenant, r.seq, r.id, r.prev_hash, r.hash, r.body, r.at, r.type, r.lineage_kind, \
         r.lineage_key, r.retired, r.revises, r.relation_type, r.from_id, r.to_id, \
         r.from_origin, r.to_origin, r.snapshot_kind, r.snapshot_id, r.external_id, r.def_hash"
    };
}

/// Vigência sem coluna mutável: o latest record é o que nenhum outro revisa.
macro_rules! no_successor {
    ($alias:literal) => {
        concat!(
            "NOT EXISTS (SELECT 1 FROM records AS s WHERE s.tenant = ",
            $alias,
            ".tenant AND s.revises = ",
            $alias,
            ".id)"
        )
    };
}

/// Extremo da relation `r` vigente: de origem nativa, ou um node do tenant cujo latest record não é retirement.
macro_rules! live_endpoint {
    ($id:literal, $origin:literal) => {
        concat!(
            "(r.",
            $origin,
            " = 'native' OR EXISTS (SELECT 1 FROM records AS n WHERE n.tenant = r.tenant \
             AND n.lineage_kind = 'node' AND n.lineage_key = r.",
            $id,
            " AND n.retired = 0 AND ",
            no_successor!("n"),
            "))"
        )
    };
}

macro_rules! select_records {
    ($($tail:tt)*) => {
        concat!("SELECT ", record_columns!(), " FROM records AS r ", $($tail)*)
    };
}

macro_rules! select_batches {
    ($($tail:tt)*) => {
        concat!(
            "SELECT key, fingerprint, first_seq, item_count FROM batches ",
            $($tail)*
        )
    };
}

const SELECT_TENANT: &str =
    "SELECT tenant, anchor, anchor_doc, created_at FROM tenants WHERE tenant = ?1";
const INSERT_TENANT: &str = "INSERT INTO tenants (tenant, anchor, anchor_doc, created_at) \
    VALUES (?1, ?2, ?3, ?4) ON CONFLICT (tenant) DO NOTHING";
const SELECT_LAST_RECORD: &str =
    "SELECT seq, hash FROM records WHERE tenant = ?1 ORDER BY seq DESC LIMIT 1";
const SELECT_LATEST_RECORD: &str = concat!(
    "SELECT r.id, r.seq, r.retired, r.type, r.def_hash FROM records AS r \
     WHERE r.tenant = ?1 AND r.lineage_kind = ?2 AND r.lineage_key = ?3 AND ",
    no_successor!("r")
);
const SELECT_RECORD_BY_ID: &str = select_records!("WHERE r.id = ?1");
const SELECT_BATCH_BY_KEY: &str = select_batches!("WHERE tenant = ?1 AND key = ?2");
const SELECT_LIVE_RELATION_PAIRS: &str = concat!(
    "SELECT r.from_id, r.to_id FROM records AS r \
     WHERE r.tenant = ?1 AND r.lineage_kind = 'relation' \
     AND r.relation_type = ?2 AND r.retired = 0 AND ",
    no_successor!("r"),
    " AND ",
    live_endpoint!("from_id", "from_origin"),
    " AND ",
    live_endpoint!("to_id", "to_origin")
);
// Cada condição de `where_eq` chega em `?8` como `[caminho, tipo, valor]`; `IS NOT` trata o caminho ausente.
const SELECT_CURRENT_NODES: &str = select_records!(
    "WHERE r.tenant = ?1 AND r.lineage_kind = 'node' AND ",
    no_successor!("r"),
    " AND (?2 IS NULL OR r.type = ?2) AND (?3 = 1 OR r.retired = 0) \
     AND (?4 IS NULL OR (r.snapshot_kind = ?4 AND r.snapshot_id = ?5)) \
     AND (?6 IS NULL OR r.lineage_key = ?6 \
          OR substr(r.lineage_key, 1, length(?6) + 1) = ?6 || '.') \
     AND r.seq > ?7 \
     AND NOT EXISTS (SELECT 1 FROM json_each(?8) AS cond \
          WHERE json_type(r.body, json_extract(cond.value, '$[0]')) \
                IS NOT json_extract(cond.value, '$[1]') \
             OR json_extract(r.body, json_extract(cond.value, '$[0]')) \
                IS NOT json_extract(cond.value, '$[2]')) \
     ORDER BY r.seq LIMIT ?9"
);
const SELECT_REVISIONS: &str = select_records!(
    "WHERE r.tenant = ?1 AND r.lineage_kind = ?2 AND r.lineage_key = ?3 ORDER BY r.seq"
);
const SELECT_LATEST_RELATIONS: &str = select_records!(
    "WHERE r.tenant = ?1 AND r.lineage_kind = 'relation' AND ",
    no_successor!("r"),
    " AND (?2 IS NULL OR r.relation_type = ?2) AND (?3 IS NULL OR r.from_id = ?3) \
     AND (?4 IS NULL OR r.to_id = ?4) AND (?5 = 1 OR r.retired = 0) ORDER BY r.seq"
);
const SELECT_RECORDS_FROM: &str =
    select_records!("WHERE r.tenant = ?1 AND r.seq >= ?2 ORDER BY r.seq LIMIT ?3");
const SELECT_STORED_RECORDS: &str =
    select_records!("WHERE r.tenant = ?1 AND (?2 IS NULL OR r.seq > ?2) ORDER BY r.seq LIMIT ?3");
const SELECT_BATCHES: &str = select_batches!("WHERE tenant = ?1 ORDER BY first_seq");
const INSERT_RECORD: &str = "INSERT INTO records (tenant, seq, id, prev_hash, hash, body, at, \
    type, lineage_kind, lineage_key, retired, revises, relation_type, from_id, to_id, \
    from_origin, to_origin, snapshot_kind, snapshot_id, external_id, def_hash) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
    ?19, ?20, ?21)";
const INSERT_BATCH: &str = "INSERT INTO batches (tenant, key, fingerprint, first_seq, item_count) \
    VALUES (?1, ?2, ?3, ?4, ?5)";

pub(super) const BEGIN_IMMEDIATE: &str = "BEGIN IMMEDIATE";

const SQLITE_BUSY: i32 = 5;
const SQLITE_CONSTRAINT: i32 = 19;
const SQLITE_CONSTRAINT_UNIQUE: i32 = 2067;
const PRIMARY_CODE_MASK: i32 = 0xFF;
const DATABASE_ERROR: &str = "database error";

type SqliteQuery = Query<'static, Sqlite, SqliteArguments>;

// ---------------------------------------------------------------- erros

fn extended_code(error: &sqlx::Error) -> Option<i32> {
    error.as_database_error()?.code()?.parse().ok()
}

pub(super) fn is_busy(error: &sqlx::Error) -> bool {
    extended_code(error).is_some_and(|code| code & PRIMARY_CODE_MASK == SQLITE_BUSY)
}

fn is_constraint(error: &sqlx::Error) -> bool {
    extended_code(error).is_some_and(|code| code & PRIMARY_CODE_MASK == SQLITE_CONSTRAINT)
}

/// Fork é o índice de raiz ou de sucessor; qualquer outra violação única é falha do banco.
fn is_fork(error: &sqlx::Error) -> bool {
    let Some(database) = error.as_database_error() else {
        return false;
    };
    extended_code(error) == Some(SQLITE_CONSTRAINT_UNIQUE)
        && (database.message().contains("records.revises")
            || database.message().contains("records.lineage_key"))
}

fn store_error(detail_code: &str) -> HexlogError {
    HexlogError::refused(ErrorCode::StoreError, detail_code, DATABASE_ERROR)
}

/// Nunca ecoa a mensagem do SQLite: ela pode trazer o caminho do banco.
pub(super) fn translate(error: &sqlx::Error) -> HexlogError {
    if is_busy(error) {
        return HexlogError::refused(ErrorCode::Busy, "busy", "database is busy");
    }
    if is_fork(error) {
        return HexlogError::refused(
            ErrorCode::ForkRejected,
            "fork-rejected",
            "lineage already has a record at this position",
        );
    }
    match (extended_code(error), error) {
        (Some(code), _) => store_error(&code.to_string()),
        (None, sqlx::Error::Io(_)) => store_error("io"),
        (None, sqlx::Error::Configuration(_)) => store_error("configuration"),
        (None, sqlx::Error::Protocol(_)) => store_error("protocol"),
        (None, _) => store_error("other"),
    }
}

/// Linha do banco que os tipos do motor não aceitam.
fn corrupt() -> HexlogError {
    HexlogError::refused(
        ErrorCode::StoreError,
        "corrupt-row",
        "stored row is not readable",
    )
}

fn signed(value: u64) -> Result<i64, HexlogError> {
    i64::try_from(value).map_err(|_| HexlogError::internal("integer out of database range"))
}

// ---------------------------------------------------------------- texto das colunas

fn parse_origin(text: &str) -> Result<Origin, HexlogError> {
    Origin::parse(text).ok_or_else(corrupt)
}

fn kind_text(kind: &LineageKind) -> &'static str {
    match kind {
        LineageKind::Node => "node",
        LineageKind::Relation => "relation",
    }
}

fn parse_kind(text: &str) -> Result<LineageKind, HexlogError> {
    match text {
        "node" => Ok(LineageKind::Node),
        "relation" => Ok(LineageKind::Relation),
        _ => Err(corrupt()),
    }
}

/// Condições de `where_eq` como `[caminho, tipo, valor]`; o tipo é um literal fixo escolhido aqui.
fn conditions_json(where_eq: &[(FieldName, Scalar)]) -> String {
    let conditions = where_eq.iter().map(|(field, scalar)| {
        let (kind, value) = match scalar {
            Scalar::Str(text) => ("text", json!(text)),
            Scalar::Int(number) => ("integer", json!(number)),
            Scalar::Bool(true) => ("true", json!(true)),
            Scalar::Bool(false) => ("false", json!(false)),
        };
        json!([format!("$.data.\"{}\"", field.as_str()), kind, value])
    });
    Value::Array(conditions.collect()).to_string()
}

// ---------------------------------------------------------------- leitura de linhas

fn column<'r, T: Decode<'r, Sqlite> + Type<Sqlite>>(
    row: &'r SqliteRow,
    name: &str,
) -> Result<T, HexlogError> {
    row.try_get(name).map_err(|error| translate(&error))
}

fn text<'r>(row: &'r SqliteRow, name: &str) -> Result<&'r str, HexlogError> {
    column(row, name)
}

fn parsed<T>(
    row: &SqliteRow,
    name: &str,
    parse: impl FnOnce(&str) -> Result<T, HexlogError>,
) -> Result<T, HexlogError> {
    parse(text(row, name)?).map_err(|_| corrupt())
}

fn parsed_optional<T>(
    row: &SqliteRow,
    name: &str,
    parse: impl FnOnce(&str) -> Result<T, HexlogError>,
) -> Result<Option<T>, HexlogError> {
    column::<Option<&str>>(row, name)?
        .map(|found| parse(found).map_err(|_| corrupt()))
        .transpose()
}

fn unsigned(row: &SqliteRow, name: &str) -> Result<u64, HexlogError> {
    column(row, name)
}

fn retired_of(row: &SqliteRow) -> Result<bool, HexlogError> {
    match column::<i64>(row, "retired")? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt()),
    }
}

fn snapshot_of(row: &SqliteRow) -> Result<Option<Snapshot>, HexlogError> {
    match (column(row, "snapshot_kind")?, column(row, "snapshot_id")?) {
        (None, None) => Ok(None),
        (Some(kind), Some(id)) => {
            let kind = SnapshotKind::parse(kind).ok_or_else(corrupt)?;
            Snapshot::new(kind, id).map(Some).map_err(|_| corrupt())
        }
        _ => Err(corrupt()),
    }
}

fn tenant_from(row: &SqliteRow) -> Result<TenantRow, HexlogError> {
    Ok(TenantRow {
        tenant: parsed(row, "tenant", TenantId::parse)?,
        anchor: parsed(row, "anchor", Hash::parse)?,
        anchor_doc: text(row, "anchor_doc")?.to_owned(),
        created_at: text(row, "created_at")?.to_owned(),
    })
}

fn latest_from(row: &SqliteRow) -> Result<LatestRow, HexlogError> {
    Ok(LatestRow {
        record_id: parsed(row, "id", RecordId::parse)?,
        seq: unsigned(row, "seq")?,
        retired: retired_of(row)?,
        type_name: parsed(row, "type", Name::parse)?,
        def_hash: parsed(row, "def_hash", Hash::parse)?,
    })
}

fn batch_from(row: &SqliteRow) -> Result<BatchRow, HexlogError> {
    Ok(BatchRow {
        key: parsed(row, "key", IdempotencyKey::parse)?,
        fingerprint: parsed(row, "fingerprint", Hash::parse)?,
        first_seq: unsigned(row, "first_seq")?,
        item_count: column(row, "item_count")?,
    })
}

fn columns_from(row: &SqliteRow) -> Result<RecordColumns, HexlogError> {
    Ok(RecordColumns {
        at: text(row, "at")?.to_owned(),
        type_name: parsed(row, "type", Name::parse)?,
        lineage_kind: parsed(row, "lineage_kind", parse_kind)?,
        lineage_key: text(row, "lineage_key")?.to_owned(),
        retired: retired_of(row)?,
        revises: parsed_optional(row, "revises", RecordId::parse)?,
        relation_type: parsed_optional(row, "relation_type", RelationTypeName::parse)?,
        from_id: parsed_optional(row, "from_id", SemanticId::parse)?,
        to_id: parsed_optional(row, "to_id", SemanticId::parse)?,
        from_origin: parsed_optional(row, "from_origin", parse_origin)?,
        to_origin: parsed_optional(row, "to_origin", parse_origin)?,
        snapshot: snapshot_of(row)?,
        external_id: parsed_optional(row, "external_id", ExternalId::parse)?,
        def_hash: parsed(row, "def_hash", Hash::parse)?,
    })
}

fn stored_tenant_from(row: &SqliteRow) -> Result<StoredTenant, HexlogError> {
    Ok(StoredTenant {
        tenant: column(row, "tenant")?,
        anchor: column(row, "anchor")?,
        anchor_doc: column(row, "anchor_doc")?,
        created_at: column(row, "created_at")?,
    })
}

fn stored_row_from(row: &SqliteRow) -> Result<StoredRow, HexlogError> {
    Ok(StoredRow {
        tenant: column(row, "tenant")?,
        seq: column(row, "seq")?,
        id: column(row, "id")?,
        prev_hash: column(row, "prev_hash")?,
        hash: column(row, "hash")?,
        body: column(row, "body")?,
        at: column(row, "at")?,
        type_name: column(row, "type")?,
        lineage_kind: column(row, "lineage_kind")?,
        lineage_key: column(row, "lineage_key")?,
        retired: column(row, "retired")?,
        revises: column(row, "revises")?,
        relation_type: column(row, "relation_type")?,
        from_id: column(row, "from_id")?,
        to_id: column(row, "to_id")?,
        from_origin: column(row, "from_origin")?,
        to_origin: column(row, "to_origin")?,
        snapshot_kind: column(row, "snapshot_kind")?,
        snapshot_id: column(row, "snapshot_id")?,
        external_id: column(row, "external_id")?,
        def_hash: column(row, "def_hash")?,
    })
}

fn stored_batch_from(row: &SqliteRow) -> Result<StoredBatch, HexlogError> {
    Ok(StoredBatch {
        key: column(row, "key")?,
        fingerprint: column(row, "fingerprint")?,
        first_seq: column(row, "first_seq")?,
        item_count: column(row, "item_count")?,
    })
}

fn record_from(row: &SqliteRow) -> Result<RecordRow, HexlogError> {
    Ok(RecordRow {
        tenant: parsed(row, "tenant", TenantId::parse)?,
        seq: unsigned(row, "seq")?,
        id: parsed(row, "id", RecordId::parse)?,
        prev_hash: parsed(row, "prev_hash", Hash::parse)?,
        hash: parsed(row, "hash", Hash::parse)?,
        body: text(row, "body")?.to_owned(),
        columns: columns_from(row)?,
    })
}

// ---------------------------------------------------------------- transação

/// Uma transação aberta: as portas de leitura e a gravação do plano. Cada chamada faz o seu `block_on`.
pub(super) struct Tx<'c> {
    tx: Transaction<'c, Sqlite>,
    /// Erro que não é violação de restrição: a conexão não é mais confiável (issue 4434 do `sqlx`).
    broken: bool,
}

impl<'c> Tx<'c> {
    fn new(tx: Transaction<'c, Sqlite>) -> Tx<'c> {
        Tx { tx, broken: false }
    }

    /// Fecha a transação (`COMMIT` no sucesso, `ROLLBACK` no erro) e diz se a conexão deve ser trocada.
    fn finish<T>(self, result: Result<T, HexlogError>) -> (Result<T, HexlogError>, bool) {
        match result {
            Ok(value) => match block_on(self.tx.commit()) {
                Ok(()) => (Ok(value), self.broken),
                Err(error) => (Err(translate(&error)), true),
            },
            Err(error) => {
                let rolled_back = block_on(self.tx.rollback());
                (Err(error), self.broken || rolled_back.is_err())
            }
        }
    }

    fn guard<T>(&mut self, result: Result<T, sqlx::Error>) -> Result<T, HexlogError> {
        result.map_err(|error| {
            self.broken |= !is_constraint(&error);
            translate(&error)
        })
    }

    fn fetch_all(&mut self, query: SqliteQuery) -> Result<Vec<SqliteRow>, HexlogError> {
        let result = block_on(query.fetch_all(&mut *self.tx));
        self.guard(result)
    }

    fn fetch_optional(&mut self, query: SqliteQuery) -> Result<Option<SqliteRow>, HexlogError> {
        let result = block_on(query.fetch_optional(&mut *self.tx));
        self.guard(result)
    }

    fn execute(&mut self, query: SqliteQuery) -> Result<u64, HexlogError> {
        let result = block_on(query.execute(&mut *self.tx));
        self.guard(result.map(|done| done.rows_affected()))
    }

    fn fetch_mapped<T>(
        &mut self,
        query: SqliteQuery,
        from: fn(&SqliteRow) -> Result<T, HexlogError>,
    ) -> Result<Vec<T>, HexlogError> {
        self.fetch_all(query)?.iter().map(from).collect()
    }

    fn fetch_mapped_optional<T>(
        &mut self,
        query: SqliteQuery,
        from: fn(&SqliteRow) -> Result<T, HexlogError>,
    ) -> Result<Option<T>, HexlogError> {
        self.fetch_optional(query)?.as_ref().map(from).transpose()
    }

    fn records(&mut self, query: SqliteQuery) -> Result<Vec<RecordRow>, HexlogError> {
        self.fetch_mapped(query, record_from)
    }

    fn tenant_row(&mut self, tenant: &TenantId) -> Result<Option<TenantRow>, HexlogError> {
        let query = sqlx::query(SELECT_TENANT).bind(tenant.as_str());
        self.fetch_mapped_optional(query, tenant_from)
    }

    /// Cria o tenant se ainda não existe; devolve a linha gravada e se ela nasceu agora.
    pub(super) fn ensure_tenant(
        &mut self,
        tenant: &TenantId,
        anchor_doc: &str,
        anchor: &Hash,
        created_at: &str,
    ) -> Result<(TenantRow, bool), HexlogError> {
        let insert = sqlx::query(INSERT_TENANT)
            .bind(tenant.as_str())
            .bind(anchor.as_str())
            .bind(anchor_doc)
            .bind(created_at);
        let created = self.execute(insert)? == 1;
        let row = self
            .tenant_row(tenant)?
            .ok_or_else(|| HexlogError::internal("tenant row missing after insert"))?;
        Ok((row, created))
    }

    /// Grava os records e depois o lote, que referencia o primeiro deles.
    pub(super) fn insert_plan(
        &mut self,
        tenant: &TenantId,
        plan: &AppendPlan,
    ) -> Result<(), HexlogError> {
        for record in &plan.records {
            self.insert_record(tenant, record)?;
        }
        if let Some(batch) = &plan.batch {
            let insert = sqlx::query(INSERT_BATCH)
                .bind(tenant.as_str())
                .bind(batch.key.as_str())
                .bind(batch.fingerprint.as_str())
                .bind(signed(batch.first_seq)?)
                .bind(i64::from(batch.item_count));
            self.execute(insert)?;
        }
        Ok(())
    }

    fn insert_record(&mut self, tenant: &TenantId, record: &NewRecord) -> Result<(), HexlogError> {
        let columns = &record.columns;
        let insert = sqlx::query(INSERT_RECORD)
            .bind(tenant.as_str())
            .bind(signed(record.seq)?)
            .bind(record.id.as_str())
            .bind(record.prev_hash.as_str())
            .bind(record.hash.as_str())
            .bind(record.body.as_str())
            .bind(columns.at.as_str())
            .bind(columns.type_name.as_str())
            .bind(kind_text(&columns.lineage_kind))
            .bind(columns.lineage_key.as_str())
            .bind(i64::from(columns.retired))
            .bind(columns.revises.as_ref().map(RecordId::as_str))
            .bind(columns.relation_type.as_ref().map(RelationTypeName::as_str))
            .bind(columns.from_id.as_ref().map(SemanticId::as_str))
            .bind(columns.to_id.as_ref().map(SemanticId::as_str))
            .bind(columns.from_origin.map(Origin::as_str))
            .bind(columns.to_origin.map(Origin::as_str))
            .bind(columns.snapshot.as_ref().map(|found| found.kind().as_str()))
            .bind(columns.snapshot.as_ref().map(Snapshot::id))
            .bind(columns.external_id.as_ref().map(ExternalId::as_str))
            .bind(columns.def_hash.as_str());
        self.execute(insert)?;
        Ok(())
    }
}

/// Devolve o resultado e se a conexão ficou suspeita (falha ao abrir que não seja `BUSY`, ou erro dentro da transação).
pub(super) fn run_in<T>(
    conn: &mut SqliteConnection,
    immediate: bool,
    body: impl FnOnce(&mut Tx<'_>) -> Result<T, HexlogError>,
) -> (Result<T, HexlogError>, bool) {
    let begun = if immediate {
        block_on(conn.begin_with(BEGIN_IMMEDIATE))
    } else {
        block_on(conn.begin())
    };
    match begun {
        Ok(begun) => {
            let mut tx = Tx::new(begun);
            let result = body(&mut tx);
            tx.finish(result)
        }
        Err(error) => (Err(translate(&error)), !is_busy(&error)),
    }
}

impl WriteReads for Tx<'_> {
    fn tenant(&mut self, tenant: &TenantId) -> Result<Option<TenantRow>, HexlogError> {
        self.tenant_row(tenant)
    }

    fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, HexlogError> {
        let last = self.fetch_optional(sqlx::query(SELECT_LAST_RECORD).bind(tenant.as_str()))?;
        if let Some(row) = last {
            return Ok(ChainHead {
                seq: unsigned(&row, "seq")?,
                hash: parsed(&row, "hash", Hash::parse)?,
            });
        }
        let Some(known) = self.tenant_row(tenant)? else {
            return Err(HexlogError::refused(
                ErrorCode::TenantNotFound,
                "tenant-not-found",
                "unknown tenant",
            ));
        };
        Ok(ChainHead {
            seq: 0,
            hash: known.anchor,
        })
    }

    fn latest_record(
        &mut self,
        tenant: &TenantId,
        lineage: &Lineage,
    ) -> Result<Option<LatestRow>, HexlogError> {
        let (kind, key) = lineage_columns(lineage);
        let query = sqlx::query(SELECT_LATEST_RECORD)
            .bind(tenant.as_str())
            .bind(kind)
            .bind(key.as_ref());
        self.fetch_mapped_optional(query, latest_from)
    }

    fn record_by_id(&mut self, id: &RecordId) -> Result<Option<RecordRow>, HexlogError> {
        let query = sqlx::query(SELECT_RECORD_BY_ID).bind(id.as_str());
        self.fetch_mapped_optional(query, record_from)
    }

    fn batch_by_key(
        &mut self,
        tenant: &TenantId,
        key: &IdempotencyKey,
    ) -> Result<Option<BatchRow>, HexlogError> {
        let query = sqlx::query(SELECT_BATCH_BY_KEY)
            .bind(tenant.as_str())
            .bind(key.as_str());
        self.fetch_mapped_optional(query, batch_from)
    }

    fn batch_records(
        &mut self,
        tenant: &TenantId,
        first_seq: u64,
        count: u32,
    ) -> Result<Vec<RecordRow>, HexlogError> {
        let query = sqlx::query(SELECT_RECORDS_FROM)
            .bind(tenant.as_str())
            .bind(signed(first_seq)?)
            .bind(i64::from(count));
        self.records(query)
    }

    fn live_relation_pairs(
        &mut self,
        tenant: &TenantId,
        relation_type: &RelationTypeName,
    ) -> Result<Vec<(SemanticId, SemanticId)>, HexlogError> {
        let query = sqlx::query(SELECT_LIVE_RELATION_PAIRS)
            .bind(tenant.as_str())
            .bind(relation_type.as_str());
        self.fetch_mapped(query, |row| {
            Ok((
                parsed(row, "from_id", SemanticId::parse)?,
                parsed(row, "to_id", SemanticId::parse)?,
            ))
        })
    }
}

impl QueryReads for Tx<'_> {
    fn current_nodes(
        &mut self,
        tenant: &TenantId,
        filter: &ListFilter,
        page: &PageRequest,
    ) -> Result<Vec<RecordRow>, HexlogError> {
        let snapshot = filter.snapshot.as_ref();
        let query = sqlx::query(SELECT_CURRENT_NODES)
            .bind(tenant.as_str())
            .bind(filter.type_name.as_ref().map(|name| name.as_str()))
            .bind(i64::from(filter.include_retired))
            .bind(snapshot.map(|found| found.kind().as_str()))
            .bind(snapshot.map(Snapshot::id))
            .bind(filter.id_prefix.as_deref())
            .bind(signed(page.after_seq.unwrap_or(0))?)
            .bind(conditions_json(&filter.where_eq))
            .bind(i64::from(page.limit));
        self.records(query)
    }

    fn revisions(
        &mut self,
        tenant: &TenantId,
        lineage: &Lineage,
    ) -> Result<Vec<RecordRow>, HexlogError> {
        let (kind, key) = lineage_columns(lineage);
        let query = sqlx::query(SELECT_REVISIONS)
            .bind(tenant.as_str())
            .bind(kind)
            .bind(key.as_ref());
        self.records(query)
    }

    fn latest_relations(
        &mut self,
        tenant: &TenantId,
        query: &RelationQuery,
    ) -> Result<Vec<RecordRow>, HexlogError> {
        let statement = sqlx::query(SELECT_LATEST_RELATIONS)
            .bind(tenant.as_str())
            .bind(query.relation_type.as_ref().map(RelationTypeName::as_str))
            .bind(query.from.as_ref().map(SemanticId::as_str))
            .bind(query.to.as_ref().map(SemanticId::as_str))
            .bind(i64::from(query.include_retired));
        self.records(statement)
    }
}

impl VerifyReads for Tx<'_> {
    fn stored_tenant(&mut self, tenant: &TenantId) -> Result<Option<StoredTenant>, HexlogError> {
        let query = sqlx::query(SELECT_TENANT).bind(tenant.as_str());
        self.fetch_mapped_optional(query, stored_tenant_from)
    }

    fn stored_records(
        &mut self,
        tenant: &TenantId,
        after_seq: Option<i64>,
        limit: u32,
    ) -> Result<Vec<StoredRow>, HexlogError> {
        let query = sqlx::query(SELECT_STORED_RECORDS)
            .bind(tenant.as_str())
            .bind(after_seq)
            .bind(i64::from(limit));
        self.fetch_mapped(query, stored_row_from)
    }

    fn stored_batches(&mut self, tenant: &TenantId) -> Result<Vec<StoredBatch>, HexlogError> {
        let query = sqlx::query(SELECT_BATCHES).bind(tenant.as_str());
        self.fetch_mapped(query, stored_batch_from)
    }
}
