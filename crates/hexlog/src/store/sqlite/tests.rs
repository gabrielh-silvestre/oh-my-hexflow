use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_executor::block_on;
use serde_json::{Value, json};
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tempfile::TempDir;

use super::SqliteStore;
use crate::chain;
use crate::engine::model::{
    AppendOutcome, AppendedRecord, ChainHead, FieldName, Lineage, LineageRef, ListFilter, Origin,
    PageRequest, RelationQuery, Scalar,
};
use crate::engine::ports::{
    AppendPlan, BatchRow, LineageKind, NewRecord, RecordColumns, RecordRow, Store, StoreReader,
    StoredRow, TenantRow, WriteReads,
};
use crate::engine::snapshot::{Snapshot, SnapshotKind};
use crate::error::{ErrorCode, HexlogError};
use crate::ids::{
    ExternalId, Hash, IdempotencyKey, Name, RecordId, RelationTypeName, SemanticId, TenantId,
    TypeName,
};

const BODIES: &str = include_str!("../../../tests/fixtures/golden_bodies.json");
const CREATED_AT: &str = "2026-11-07T12:00:00.000Z";
const RECORD_AT: &str = "2026-11-07T12:30:00.000Z";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const SHORT_TIMEOUT: Duration = Duration::from_millis(50);
const REOPEN_ROUNDS: u64 = 100;

const INSERT_RECORD: &str = "INSERT INTO records (tenant, seq, id, prev_hash, hash, body, at, \
    type, lineage_kind, lineage_key, retired, revises, relation_type, from_id, to_id, \
    from_origin, to_origin, snapshot_kind, snapshot_id, external_id, def_hash) \
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

// ---------------------------------------------------------------- cadeia de teste

/// Um record por montar: o que o `decide` entregaria ao store, escrito à mão.
struct Draft {
    key: String,
    type_name: String,
    retired: bool,
    revises: Option<RecordId>,
    data: Value,
    snapshot: Option<Snapshot>,
    external_id: Option<String>,
    def_hash: char,
    relation: Option<Edge>,
}

struct Edge {
    relation_type: String,
    from: String,
    to: String,
    to_origin: Origin,
}

impl Draft {
    fn node(id: &str, type_name: &str) -> Draft {
        Draft {
            key: id.to_owned(),
            type_name: type_name.to_owned(),
            retired: false,
            revises: None,
            data: json!({}),
            snapshot: None,
            external_id: None,
            def_hash: '0',
            relation: None,
        }
    }

    fn relation(relation_type: &str, from: &str, to: &str) -> Draft {
        Draft {
            key: format!("{relation_type}|{from}|{to}"),
            type_name: relation_type.to_owned(),
            retired: false,
            revises: None,
            data: json!({}),
            snapshot: None,
            external_id: None,
            def_hash: '0',
            relation: Some(Edge {
                relation_type: relation_type.to_owned(),
                from: from.to_owned(),
                to: to.to_owned(),
                to_origin: Origin::Tenant,
            }),
        }
    }

    fn with_data(mut self, data: Value) -> Draft {
        self.data = data;
        self
    }

    fn with_snapshot(mut self, snapshot: Snapshot) -> Draft {
        self.snapshot = Some(snapshot);
        self
    }

    fn with_external_id(mut self, external_id: &str) -> Draft {
        self.external_id = Some(external_id.to_owned());
        self
    }

    fn with_def_hash(mut self, digit: char) -> Draft {
        self.def_hash = digit;
        self
    }

    fn with_native_target(mut self) -> Draft {
        if let Some(edge) = &mut self.relation {
            edge.to_origin = Origin::Native;
        }
        self
    }

    fn revising(mut self, record_id: &RecordId) -> Draft {
        self.revises = Some(record_id.clone());
        self
    }

    fn retired(mut self) -> Draft {
        self.retired = true;
        self
    }
}

fn hash_of(digit: char) -> Hash {
    Hash::parse(&digit.to_string().repeat(64)).unwrap()
}

fn snapshot(digit: char) -> Snapshot {
    // O nome do tipo de snapshot vem da fixture dourada: o varredor veta a palavra em literal de `src/`.
    let fixture: Value = serde_json::from_str(BODIES).unwrap();
    let kind = fixture["shapes"]["node_full"]["fields"]["snapshot"]["kind"]
        .as_str()
        .unwrap();
    let kind = SnapshotKind::parse(kind).unwrap();
    Snapshot::new(kind, &digit.to_string().repeat(40)).unwrap()
}

fn origin_text(origin: Origin) -> &'static str {
    match origin {
        Origin::Tenant => "tenant",
        Origin::Native => "native",
    }
}

fn is_relation(kind: &LineageKind) -> bool {
    matches!(kind, LineageKind::Relation)
}

fn columns_of(draft: &Draft) -> RecordColumns {
    let edge = draft.relation.as_ref();
    RecordColumns {
        at: RECORD_AT.to_owned(),
        type_name: Name::parse(&draft.type_name).unwrap(),
        lineage_kind: if edge.is_some() {
            LineageKind::Relation
        } else {
            LineageKind::Node
        },
        lineage_key: draft.key.clone(),
        retired: draft.retired,
        revises: draft.revises.clone(),
        relation_type: edge.map(|edge| RelationTypeName::parse(&edge.relation_type).unwrap()),
        from_id: edge.map(|edge| SemanticId::parse(&edge.from).unwrap()),
        to_id: edge.map(|edge| SemanticId::parse(&edge.to).unwrap()),
        from_origin: edge.map(|_| Origin::Tenant),
        to_origin: edge.map(|edge| edge.to_origin),
        snapshot: draft.snapshot.clone(),
        external_id: draft
            .external_id
            .as_deref()
            .map(|text| ExternalId::parse(text).unwrap()),
        def_hash: hash_of(draft.def_hash),
    }
}

fn body_of(tenant: &TenantId, seq: u64, id: &RecordId, draft: &Draft) -> String {
    let lineage = match &draft.relation {
        None => json!({"kind": "node", "id": draft.key}),
        Some(edge) => json!({
            "kind": "relation",
            "relation_type": edge.relation_type,
            "from": edge.from,
            "to": edge.to,
            "from_origin": "tenant",
            "to_origin": origin_text(edge.to_origin),
        }),
    };
    let mut body = json!({
        "v": 1,
        "tenant": tenant.as_str(),
        "seq": seq,
        "id": id.as_str(),
        "at": RECORD_AT,
        "type": draft.type_name,
        "lineage": lineage,
        "retirement": draft.retired,
        "author": {"agent": "tester", "client": "cli"},
        "def_hash": hash_of(draft.def_hash).as_str(),
    });
    if let Some(revises) = &draft.revises {
        body["revises"] = json!(revises.as_str());
    }
    if !draft.retired {
        body["data"] = draft.data.clone();
    }
    chain::canonical(&body).unwrap()
}

/// O estado da cadeia de um tenant na visão do teste: próximo `seq`, último hash e próximo id.
struct Ledger {
    tenant: TenantId,
    anchor_doc: String,
    anchor: Hash,
    seq: u64,
    head: Hash,
    next_id: u64,
}

impl Ledger {
    fn new(name: &str) -> Ledger {
        let tenant = TenantId::parse(name).unwrap();
        let anchor_doc = chain::anchor_doc(&tenant, CREATED_AT);
        let anchor = chain::anchor(&anchor_doc);
        Ledger {
            tenant,
            head: anchor.clone(),
            anchor,
            anchor_doc,
            seq: 0,
            next_id: 1,
        }
    }

    fn record_id(&self, number: u64) -> RecordId {
        RecordId::parse(&format!(
            "{}:0192a000-0000-7000-8000-{number:012x}",
            self.tenant.as_str()
        ))
        .unwrap()
    }

    /// Records encadeados a partir do estado atual, sem avançá-lo.
    fn records(&self, drafts: &[Draft]) -> Vec<NewRecord> {
        let mut prev_hash = self.head.clone();
        let mut records = Vec::new();
        for (offset, draft) in (0u64..).zip(drafts) {
            let seq = self.seq + 1 + offset;
            let id = self.record_id(self.next_id + offset);
            let body = body_of(&self.tenant, seq, &id, draft);
            let hash = chain::hash_record(prev_hash.as_str(), &body);
            let next_prev_hash = hash.clone();
            records.push(NewRecord {
                seq,
                id,
                prev_hash,
                hash,
                body,
                columns: columns_of(draft),
            });
            prev_hash = next_prev_hash;
        }
        records
    }

    fn plan(&self, drafts: &[Draft], batch_key: Option<&str>) -> AppendPlan {
        let records = self.records(drafts);
        let last = records.last().unwrap();
        let outcome = AppendOutcome {
            records: records
                .iter()
                .map(|record| AppendedRecord {
                    id: record.id.clone(),
                    seq: record.seq,
                })
                .collect(),
            replayed: false,
            chain_head: ChainHead {
                seq: last.seq,
                hash: last.hash.clone(),
            },
        };
        let batch = batch_key.map(|key| BatchRow {
            key: IdempotencyKey::parse(key).unwrap(),
            fingerprint: chain::sha256hex(key),
            first_seq: records[0].seq,
            item_count: u32::try_from(records.len()).unwrap(),
        });
        AppendPlan {
            records,
            batch,
            outcome,
        }
    }

    fn advance(&mut self, plan: &AppendPlan) {
        self.seq = plan.outcome.chain_head.seq;
        self.head = plan.outcome.chain_head.hash.clone();
        self.next_id += u64::try_from(plan.records.len()).unwrap();
    }
}

// ---------------------------------------------------------------- ambiente

struct Env {
    store: SqliteStore,
    path: PathBuf,
    _dir: TempDir,
}

fn env() -> Env {
    env_with(DEFAULT_TIMEOUT)
}

fn env_with(busy_timeout: Duration) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hexlog.db");
    let store = SqliteStore::open(&path, busy_timeout).unwrap();
    Env {
        store,
        path,
        _dir: dir,
    }
}

fn failure<T>(result: Result<T, HexlogError>) -> HexlogError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected an error"),
    }
}

fn ensure(store: &mut SqliteStore, ledger: &Ledger) -> (TenantRow, bool) {
    store
        .ensure_tenant(
            &ledger.tenant,
            &ledger.anchor_doc,
            &ledger.anchor,
            CREATED_AT,
        )
        .unwrap()
}

fn start(store: &mut SqliteStore, name: &str) -> Ledger {
    let ledger = Ledger::new(name);
    ensure(store, &ledger);
    ledger
}

/// Entrega o plano ao store e devolve o que ele devolveu, sem avançar o `Ledger`.
fn attempt(
    store: &mut SqliteStore,
    tenant: &TenantId,
    plan: AppendPlan,
) -> Result<AppendPlan, HexlogError> {
    let mut slot = Some(plan);
    store.append_with(tenant, &mut |_| Ok(slot.take().expect("decide ran once")))
}

fn commit(store: &mut SqliteStore, ledger: &mut Ledger, plan: AppendPlan) -> AppendPlan {
    let stored = attempt(store, &ledger.tenant, plan).unwrap();
    ledger.advance(&stored);
    stored
}

fn append(store: &mut SqliteStore, ledger: &mut Ledger, drafts: &[Draft]) -> AppendPlan {
    let plan = ledger.plan(drafts, None);
    commit(store, ledger, plan)
}

fn append_keyed(
    store: &mut SqliteStore,
    ledger: &mut Ledger,
    drafts: &[Draft],
    key: &str,
) -> AppendPlan {
    let plan = ledger.plan(drafts, Some(key));
    commit(store, ledger, plan)
}

fn ids_of(plan: &AppendPlan) -> Vec<RecordId> {
    plan.outcome
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect()
}

fn read_with<T>(
    store: &mut SqliteStore,
    mut f: impl FnMut(&mut dyn StoreReader) -> Result<T, HexlogError>,
) -> Result<T, HexlogError> {
    let mut out = None;
    store.read(&mut |reader| {
        out = Some(f(reader)?);
        Ok(())
    })?;
    Ok(out.expect("read ran the closure"))
}

fn head_of(store: &mut SqliteStore, tenant: &TenantId) -> ChainHead {
    read_with(store, |reader| WriteReads::chain_head(reader, tenant)).unwrap()
}

fn rows_from(
    store: &mut SqliteStore,
    tenant: &TenantId,
    from_seq: u64,
    limit: u32,
) -> Vec<RecordRow> {
    read_with(store, |reader| {
        reader.batch_records(tenant, from_seq, limit)
    })
    .unwrap()
}

fn stored_rows(
    store: &mut SqliteStore,
    tenant: &TenantId,
    after_seq: Option<i64>,
    limit: u32,
) -> Vec<StoredRow> {
    read_with(store, |reader| {
        reader.stored_records(tenant, after_seq, limit)
    })
    .unwrap()
}

fn stored_seqs(rows: &[StoredRow]) -> Vec<i64> {
    rows.iter().map(|row| row.seq).collect()
}

/// Cada campo do `StoredRow`, em texto, igual ao do record que o teste mandou gravar.
fn assert_stored_matches(row: &StoredRow, expected: &NewRecord) {
    let columns = &expected.columns;
    let kind = if is_relation(&columns.lineage_kind) {
        "relation"
    } else {
        "node"
    };
    assert_eq!(row.seq, i64::try_from(expected.seq).unwrap());
    assert_eq!(row.id, expected.id.as_str());
    assert_eq!(row.prev_hash, expected.prev_hash.as_str());
    assert_eq!(row.hash, expected.hash.as_str());
    assert_eq!(row.body, expected.body);
    assert_eq!(row.at, columns.at);
    assert_eq!(row.type_name, columns.type_name.as_str());
    assert_eq!(row.lineage_kind, kind);
    assert_eq!(row.lineage_key, columns.lineage_key);
    assert_eq!(row.retired, i64::from(columns.retired));
    assert_eq!(
        row.revises.as_deref(),
        columns.revises.as_ref().map(RecordId::as_str)
    );
    assert_eq!(
        row.relation_type.as_deref(),
        columns.relation_type.as_ref().map(RelationTypeName::as_str)
    );
    assert_eq!(
        row.from_id.as_deref(),
        columns.from_id.as_ref().map(SemanticId::as_str)
    );
    assert_eq!(
        row.to_id.as_deref(),
        columns.to_id.as_ref().map(SemanticId::as_str)
    );
    assert_eq!(
        row.from_origin.as_deref(),
        columns.from_origin.map(origin_text)
    );
    assert_eq!(row.to_origin.as_deref(), columns.to_origin.map(origin_text));
    assert_eq!(
        row.snapshot_kind.as_deref(),
        columns.snapshot.as_ref().map(|found| found.kind().as_str())
    );
    assert_eq!(
        row.snapshot_id.as_deref(),
        columns.snapshot.as_ref().map(Snapshot::id)
    );
    assert_eq!(
        row.external_id.as_deref(),
        columns.external_id.as_ref().map(ExternalId::as_str)
    );
    assert_eq!(row.def_hash, columns.def_hash.as_str());
}

fn keys_of(rows: &[RecordRow]) -> Vec<String> {
    rows.iter()
        .map(|row| row.columns.lineage_key.clone())
        .collect()
}

fn seqs_of(rows: &[RecordRow]) -> Vec<u64> {
    rows.iter().map(|row| row.seq).collect()
}

fn assert_row_matches(row: &RecordRow, expected: &NewRecord) {
    assert_eq!(row.seq, expected.seq);
    assert_eq!(row.id, expected.id);
    assert_eq!(row.prev_hash, expected.prev_hash);
    assert_eq!(row.hash, expected.hash);
    assert_eq!(row.body, expected.body);
    let (actual, wanted) = (&row.columns, &expected.columns);
    assert_eq!(actual.at, wanted.at);
    assert_eq!(actual.type_name, wanted.type_name);
    assert_eq!(
        is_relation(&actual.lineage_kind),
        is_relation(&wanted.lineage_kind)
    );
    assert_eq!(actual.lineage_key, wanted.lineage_key);
    assert_eq!(actual.retired, wanted.retired);
    assert_eq!(actual.revises, wanted.revises);
    assert_eq!(actual.relation_type, wanted.relation_type);
    assert_eq!(actual.from_id, wanted.from_id);
    assert_eq!(actual.to_id, wanted.to_id);
    assert_eq!(actual.from_origin, wanted.from_origin);
    assert_eq!(actual.to_origin, wanted.to_origin);
    assert_eq!(actual.snapshot, wanted.snapshot);
    assert_eq!(actual.external_id, wanted.external_id);
    assert_eq!(actual.def_hash, wanted.def_hash);
}

// ---------------------------------------------------------------- conexão crua (sem desligar nada)

/// Conexão `sqlx` ao mesmo arquivo, com os triggers e as chaves estrangeiras ligados.
struct Raw {
    conn: SqliteConnection,
}

impl Raw {
    fn open(path: &Path) -> Raw {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        Raw {
            conn: block_on(SqliteConnection::connect_with(&options)).unwrap(),
        }
    }

    fn create(path: &Path) -> Raw {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        Raw {
            conn: block_on(SqliteConnection::connect_with(&options)).unwrap(),
        }
    }

    fn close(self) {
        block_on(self.conn.close()).unwrap();
    }

    fn run(&mut self, sql: &'static str) -> Result<(), sqlx::Error> {
        block_on(sqlx::raw_sql(sql).execute(&mut self.conn)).map(|_| ())
    }

    fn journal_mode(&mut self) -> String {
        block_on(sqlx::query_scalar::<_, String>("PRAGMA journal_mode").fetch_one(&mut self.conn))
            .unwrap()
    }

    fn version(&mut self) -> i64 {
        block_on(sqlx::query_scalar::<_, i64>("PRAGMA user_version").fetch_one(&mut self.conn))
            .unwrap()
    }

    fn insert(&mut self, tenant: &TenantId, record: &NewRecord) -> Result<(), sqlx::Error> {
        self.insert_as(
            tenant,
            record,
            record.hash.as_str(),
            record.columns.type_name.as_str(),
        )
    }

    /// Como `insert`, mas com `hash` e `type` em texto livre, para valores que os tipos recusam.
    fn insert_as(
        &mut self,
        tenant: &TenantId,
        record: &NewRecord,
        hash: &str,
        type_name: &str,
    ) -> Result<(), sqlx::Error> {
        let columns = &record.columns;
        let kind = if is_relation(&columns.lineage_kind) {
            "relation"
        } else {
            "node"
        };
        let query = sqlx::query(INSERT_RECORD)
            .bind(tenant.as_str())
            .bind(i64::try_from(record.seq).unwrap())
            .bind(record.id.as_str())
            .bind(record.prev_hash.as_str())
            .bind(hash)
            .bind(record.body.as_str())
            .bind(columns.at.as_str())
            .bind(type_name)
            .bind(kind)
            .bind(columns.lineage_key.as_str())
            .bind(i64::from(columns.retired))
            .bind(columns.revises.as_ref().map(RecordId::as_str))
            .bind(columns.relation_type.as_ref().map(RelationTypeName::as_str))
            .bind(columns.from_id.as_ref().map(SemanticId::as_str))
            .bind(columns.to_id.as_ref().map(SemanticId::as_str))
            .bind(columns.from_origin.map(origin_text))
            .bind(columns.to_origin.map(origin_text))
            .bind(columns.snapshot.as_ref().map(|found| found.kind().as_str()))
            .bind(columns.snapshot.as_ref().map(Snapshot::id))
            .bind(columns.external_id.as_ref().map(ExternalId::as_str))
            .bind(columns.def_hash.as_str());
        block_on(query.execute(&mut self.conn)).map(|_| ())
    }
}

fn db_failure<T>(result: Result<T, sqlx::Error>) -> sqlx::Error {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected a database error"),
    }
}

fn db_code(error: &sqlx::Error) -> String {
    error
        .as_database_error()
        .and_then(|database| database.code())
        .map(|code| code.into_owned())
        .expect("a database error with a code")
}

fn assert_store_error(error: &HexlogError, detail_code: &str) {
    assert_eq!(error.code, ErrorCode::StoreError);
    assert_eq!(error.details[0].code, detail_code);
    assert_eq!(error.details[0].path, "");
}

// ---------------------------------------------------------------- esquema e triggers

#[test]
fn it_should_refuse_updates_and_deletes() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    append_keyed(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note")],
        "k1",
    );
    let mut raw = Raw::open(&env.path);
    let statements = [
        "UPDATE tenants SET created_at = 'x'",
        "DELETE FROM tenants",
        "UPDATE records SET at = 'x'",
        "DELETE FROM records",
        "UPDATE batches SET fingerprint = 'x'",
        "DELETE FROM batches",
    ];

    let codes: Vec<String> = statements
        .into_iter()
        .map(|sql| db_code(&db_failure(raw.run(sql))))
        .collect();

    assert_eq!(codes, ["1811"; 6]);
}

#[test]
fn it_should_refuse_out_of_order_seq_and_a_wrong_prev_hash() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    append(&mut env.store, &mut ledger, &[Draft::node("a.one", "note")]);
    let mut raw = Raw::open(&env.path);
    let skipped = ledger.records(&[Draft::node("a.two", "note"), Draft::node("a.three", "note")]);
    let mut forked = ledger.records(&[Draft::node("a.two", "note")]);
    forked[0].prev_hash = hash_of('0');

    let out_of_order = db_failure(raw.insert(&ledger.tenant, &skipped[1]));
    let wrong_prev_hash = db_failure(raw.insert(&ledger.tenant, &forked[0]));

    assert_eq!(db_code(&out_of_order), "1811");
    assert_eq!(db_code(&wrong_prev_hash), "1811");
}

#[test]
fn it_should_refuse_a_second_root_for_the_same_lineage_at_the_index_level() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    append(&mut env.store, &mut ledger, &[Draft::node("a.one", "note")]);
    let mut raw = Raw::open(&env.path);
    let second_root = ledger.records(&[Draft::node("a.one", "note")]);

    let error = db_failure(raw.insert(&ledger.tenant, &second_root[0]));

    assert_eq!(db_code(&error), "2067");
}

#[test]
fn it_should_refuse_a_second_successor_of_the_same_record_at_the_index_level() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let first = append(&mut env.store, &mut ledger, &[Draft::node("a.one", "note")]);
    let root_id = ids_of(&first).remove(0);
    append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note").revising(&root_id)],
    );
    let mut raw = Raw::open(&env.path);
    let second_successor = ledger.records(&[Draft::node("a.one", "note").revising(&root_id)]);

    let error = db_failure(raw.insert(&ledger.tenant, &second_successor[0]));

    assert_eq!(db_code(&error), "2067");
}

// ---------------------------------------------------------------- abertura e versão

#[test]
fn it_should_initialize_an_empty_database_with_user_version_1() {
    let env = env();
    let path = env.path.clone();
    drop(env.store);

    let mut raw = Raw::open(&path);

    assert_eq!(raw.version(), 1);
}

#[test]
fn it_should_refuse_a_database_with_a_newer_user_version() {
    let env = env();
    let path = env.path.clone();
    drop(env.store);
    let mut raw = Raw::open(&path);
    raw.run("PRAGMA user_version = 2").unwrap();
    raw.close();

    let error = failure(SqliteStore::open(&path, DEFAULT_TIMEOUT));

    assert_store_error(&error, "unsupported-schema-version");
}

#[test]
fn it_should_refuse_a_non_empty_database_with_user_version_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stray.db");
    let mut raw = Raw::create(&path);
    raw.run("CREATE TABLE stray (id INTEGER)").unwrap();
    raw.close();

    let error = failure(SqliteStore::open(&path, DEFAULT_TIMEOUT));

    assert_store_error(&error, "unsupported-schema-version");
}

#[test]
fn it_should_keep_all_data_after_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hexlog.db");
    let (tenant, before) = {
        let mut store = SqliteStore::open(&path, DEFAULT_TIMEOUT).unwrap();
        let mut ledger = start(&mut store, "acme");
        append(
            &mut store,
            &mut ledger,
            &[Draft::node("a.one", "note"), Draft::node("a.two", "note")],
        );
        let head = head_of(&mut store, &ledger.tenant);
        (ledger.tenant, head)
    };

    let mut reopened = SqliteStore::open(&path, DEFAULT_TIMEOUT).unwrap();
    let after = head_of(&mut reopened, &tenant);

    assert_eq!(after, before);
    assert_eq!(after.seq, 2);
}

#[test]
fn it_should_create_tenants_idempotently() {
    let mut env = env();
    let ledger = Ledger::new("acme");

    let (first_row, first_created) = ensure(&mut env.store, &ledger);
    let (second_row, second_created) = ensure(&mut env.store, &ledger);

    assert!(first_created);
    assert!(!second_created);
    assert_eq!(first_row.anchor, ledger.anchor);
    assert_eq!(second_row.anchor, first_row.anchor);
}

#[test]
fn it_should_reopen_the_same_database_repeatedly_without_a_lock_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hexlog.db");
    let mut ledger = Ledger::new("acme");

    for round in 0..REOPEN_ROUNDS {
        let mut store = SqliteStore::open(&path, SHORT_TIMEOUT)
            .unwrap_or_else(|error| panic!("round {round}: {error}"));
        if round == 0 {
            ensure(&mut store, &ledger);
        }
        append(
            &mut store,
            &mut ledger,
            &[Draft::node(&format!("item.n{round}"), "note")],
        );
    }

    let mut last = SqliteStore::open(&path, SHORT_TIMEOUT).unwrap();
    assert_eq!(head_of(&mut last, &ledger.tenant).seq, REOPEN_ROUNDS);
}

// ---------------------------------------------------------------- leituras das portas

#[test]
fn it_should_read_a_tenant_row_and_none_for_an_unknown_tenant() {
    let mut env = env();
    let ledger = start(&mut env.store, "acme");
    let unknown = TenantId::parse("ghost").unwrap();

    let (write_side, verify_side, missing, verify_missing) = read_with(&mut env.store, |reader| {
        Ok((
            WriteReads::tenant(reader, &ledger.tenant)?,
            reader.stored_tenant(&ledger.tenant)?,
            WriteReads::tenant(reader, &unknown)?,
            reader.stored_tenant(&unknown)?,
        ))
    })
    .unwrap();

    let write_side = write_side.unwrap();
    assert_eq!(write_side.anchor, ledger.anchor);
    assert_eq!(write_side.created_at, CREATED_AT);
    let verify_side = verify_side.unwrap();
    assert_eq!(verify_side.anchor, ledger.anchor.as_str());
    assert_eq!(verify_side.anchor_doc, ledger.anchor_doc);
    assert_eq!(verify_side.created_at, CREATED_AT);
    assert!(missing.is_none());
    assert!(verify_missing.is_none());
}

#[test]
fn it_should_report_the_anchor_as_chain_head_of_an_empty_tenant_and_the_last_record_afterwards() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let unknown = TenantId::parse("ghost").unwrap();

    let empty = head_of(&mut env.store, &ledger.tenant);
    let plan = append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note"), Draft::node("a.two", "note")],
    );
    let filled = head_of(&mut env.store, &ledger.tenant);
    let missing = failure(read_with(&mut env.store, |reader| {
        WriteReads::chain_head(reader, &unknown)
    }));

    assert_eq!(
        empty,
        ChainHead {
            seq: 0,
            hash: ledger.anchor.clone()
        }
    );
    assert_eq!(filled, plan.outcome.chain_head);
    assert_eq!(missing.code, ErrorCode::TenantNotFound);
    assert_eq!(missing.details[0].path, "");
    assert!(!format!("{missing:?}").contains("ghost"));
}

#[test]
fn it_should_find_the_latest_record_of_node_and_relation_lineages() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let nodes = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::node("a.one", "note").with_def_hash('1'),
            Draft::node("a.two", "note"),
        ],
    );
    let ids = ids_of(&nodes);
    let revision = append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note")
            .revising(&ids[0])
            .with_def_hash('2')],
    );
    let edge = append(
        &mut env.store,
        &mut ledger,
        &[Draft::relation("depends-on", "a.one", "a.two")],
    );
    let retirement = append(
        &mut env.store,
        &mut ledger,
        &[Draft::relation("depends-on", "a.one", "a.two")
            .revising(&ids_of(&edge)[0])
            .retired()],
    );
    let node_lineage = Lineage::Node(SemanticId::parse("a.one").unwrap());
    let relation_lineage = Lineage::Relation {
        relation_type: RelationTypeName::parse("depends-on").unwrap(),
        from: SemanticId::parse("a.one").unwrap(),
        to: SemanticId::parse("a.two").unwrap(),
    };
    let unwritten = Lineage::Node(SemanticId::parse("a.none").unwrap());

    let (node, relation, none) = read_with(&mut env.store, |reader| {
        Ok((
            reader.latest_record(&ledger.tenant, LineageRef::from(&node_lineage))?,
            reader.latest_record(&ledger.tenant, LineageRef::from(&relation_lineage))?,
            reader.latest_record(&ledger.tenant, LineageRef::from(&unwritten))?,
        ))
    })
    .unwrap();

    let node = node.unwrap();
    assert_eq!(node.record_id, ids_of(&revision)[0]);
    assert!(!node.retired);
    assert_eq!(node.type_name.as_str(), "note");
    assert_eq!(node.def_hash, hash_of('2'));
    let relation = relation.unwrap();
    assert_eq!(relation.record_id, ids_of(&retirement)[0]);
    assert!(relation.retired);
    assert_eq!(relation.type_name.as_str(), "depends-on");
    assert!(none.is_none());
}

#[test]
fn it_should_find_a_record_by_id() {
    let mut env = env();
    let mut acme = start(&mut env.store, "acme");
    let first = append(
        &mut env.store,
        &mut acme,
        &[
            Draft::node("a.one", "note")
                .with_data(json!({"title": "first"}))
                .with_snapshot(snapshot('a'))
                .with_external_id("ext-1"),
            Draft::relation("depends-on", "a.one", "native.x").with_native_target(),
        ],
    );
    let relation_id = ids_of(&first)[1].clone();
    let node_id = ids_of(&first)[0].clone();
    let never_written = acme.record_id(999);

    let (node, relation, none) = read_with(&mut env.store, |reader| {
        Ok((
            reader.record_by_id(&acme.tenant, &node_id)?,
            reader.record_by_id(&acme.tenant, &relation_id)?,
            reader.record_by_id(&acme.tenant, &never_written)?,
        ))
    })
    .unwrap();

    assert_row_matches(&node.unwrap(), &first.records[0]);
    assert_row_matches(&relation.unwrap(), &first.records[1]);
    assert!(none.is_none());
}

#[test]
fn it_should_not_find_a_record_by_id_under_another_tenant() {
    let mut env = env();
    let acme = start(&mut env.store, "acme");
    let mut beta = start(&mut env.store, "beta");
    let other_tenant = append(&mut env.store, &mut beta, &[Draft::node("b.one", "note")]);
    let beta_id = ids_of(&other_tenant)[0].clone();

    let (foreign, own) = read_with(&mut env.store, |reader| {
        Ok((
            reader.record_by_id(&acme.tenant, &beta_id)?,
            reader.record_by_id(&beta.tenant, &beta_id)?,
        ))
    })
    .unwrap();

    assert!(foreign.is_none());
    assert_row_matches(&own.unwrap(), &other_tenant.records[0]);
}

#[test]
fn it_should_find_a_batch_by_key_and_read_its_records_in_order() {
    let mut env = env();
    let mut acme = start(&mut env.store, "acme");
    let mut beta = start(&mut env.store, "beta");
    append(&mut env.store, &mut acme, &[Draft::node("a.zero", "note")]);
    let batch = append_keyed(
        &mut env.store,
        &mut acme,
        &[
            Draft::node("a.one", "note"),
            Draft::node("a.two", "note"),
            Draft::node("a.three", "note"),
        ],
        "k1",
    );
    append(&mut env.store, &mut acme, &[Draft::node("a.four", "note")]);
    append_keyed(
        &mut env.store,
        &mut beta,
        &[Draft::node("b.one", "note")],
        "k1",
    );
    let key = IdempotencyKey::parse("k1").unwrap();
    let missing_key = IdempotencyKey::parse("k2").unwrap();

    let (acme_row, beta_row, missing, records) = read_with(&mut env.store, |reader| {
        Ok((
            reader.batch_by_key(&acme.tenant, &key)?,
            reader.batch_by_key(&beta.tenant, &key)?,
            reader.batch_by_key(&acme.tenant, &missing_key)?,
            reader.batch_records(&acme.tenant, 2, 3)?,
        ))
    })
    .unwrap();

    let acme_row = acme_row.unwrap();
    let stored = batch.batch.as_ref().unwrap();
    assert_eq!(acme_row.key, stored.key);
    assert_eq!(acme_row.fingerprint, stored.fingerprint);
    assert_eq!(acme_row.first_seq, 2);
    assert_eq!(acme_row.item_count, 3);
    assert_eq!(beta_row.unwrap().first_seq, 1);
    assert!(missing.is_none());
    assert_eq!(seqs_of(&records), [2, 3, 4]);
    for (row, expected) in records.iter().zip(&batch.records) {
        assert_row_matches(row, expected);
    }
}

#[test]
fn it_should_list_live_relation_pairs_without_retired_relations_or_retired_endpoints() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let nodes = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::node("n.a", "note"),
            Draft::node("n.b", "note"),
            Draft::node("n.c", "note"),
            Draft::node("n.d", "note"),
        ],
    );
    let edges = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::relation("depends-on", "n.a", "n.b"),
            Draft::relation("depends-on", "n.b", "n.c"),
            Draft::relation("depends-on", "n.a", "n.d"),
            Draft::relation("depends-on", "n.a", "native.x").with_native_target(),
        ],
    );
    append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::relation("depends-on", "n.b", "n.c")
                .revising(&ids_of(&edges)[1])
                .retired(),
            Draft::node("n.d", "note")
                .revising(&ids_of(&nodes)[3])
                .retired(),
            Draft::relation("other-type", "n.a", "n.c"),
        ],
    );
    let depends_on = RelationTypeName::parse("depends-on").unwrap();
    let other_type = RelationTypeName::parse("other-type").unwrap();

    let (depends, other) = read_with(&mut env.store, |reader| {
        Ok((
            reader.live_relation_pairs(&ledger.tenant, &depends_on)?,
            reader.live_relation_pairs(&ledger.tenant, &other_type)?,
        ))
    })
    .unwrap();

    let texts = |pairs: Vec<(SemanticId, SemanticId)>| -> Vec<(String, String)> {
        let mut texts: Vec<(String, String)> = pairs
            .iter()
            .map(|(from, to)| (from.as_str().to_owned(), to.as_str().to_owned()))
            .collect();
        texts.sort();
        texts
    };
    assert_eq!(
        texts(depends),
        [
            ("n.a".to_owned(), "n.b".to_owned()),
            ("n.a".to_owned(), "native.x".to_owned())
        ]
    );
    assert_eq!(texts(other), [("n.a".to_owned(), "n.c".to_owned())]);
}

fn no_filter() -> ListFilter {
    ListFilter {
        type_name: None,
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    }
}

fn page(limit: u32, after_seq: Option<u64>) -> PageRequest {
    PageRequest { limit, after_seq }
}

fn equals(field: &str, value: Scalar) -> Vec<(FieldName, Scalar)> {
    vec![(FieldName::parse(field).unwrap(), value)]
}

fn listed(
    store: &mut SqliteStore,
    tenant: &TenantId,
    filter: &ListFilter,
    request: &PageRequest,
) -> Vec<String> {
    let rows = read_with(store, |reader| {
        reader.current_nodes(tenant, filter, request)
    })
    .unwrap();
    keys_of(&rows)
}

#[test]
fn it_should_list_current_nodes_by_each_filter() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let first = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::node("area.alpha", "note")
                .with_data(json!({"x": 1, "y": "a"}))
                .with_snapshot(snapshot('a')),
            Draft::node("area.beta", "task").with_data(json!({"x": true})),
            Draft::node("areax.gamma", "note").with_data(json!({"x": "1"})),
            Draft::node("area.alpha.child", "note").with_data(json!({"x": null})),
            Draft::node("solo", "task").with_data(json!({"x": 0})),
            Draft::node("old", "note").with_data(json!({"x": false})),
        ],
    );
    let ids = ids_of(&first);
    append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::node("old", "note").revising(&ids[5]).retired(),
            Draft::node("area.beta", "task")
                .revising(&ids[1])
                .with_data(json!({"x": true, "y": "a"})),
            Draft::node("flagoff", "note").with_data(json!({"x": false})),
            Draft::relation("depends-on", "area.alpha", "solo"),
        ],
    );
    let tenant = &ledger.tenant;
    let everything = page(50, None);
    let mut list = |filter: ListFilter| listed(&mut env.store, tenant, &filter, &everything);

    let all = list(no_filter());
    let tasks = list(ListFilter {
        type_name: Some(TypeName::parse("task").unwrap()),
        ..no_filter()
    });
    let by_snapshot = list(ListFilter {
        snapshot: Some(snapshot('a')),
        ..no_filter()
    });
    let by_other_snapshot = list(ListFilter {
        snapshot: Some(snapshot('b')),
        ..no_filter()
    });
    let by_prefix = list(ListFilter {
        id_prefix: Some("area".to_owned()),
        ..no_filter()
    });
    let by_exact_prefix = list(ListFilter {
        id_prefix: Some("area.alpha".to_owned()),
        ..no_filter()
    });
    let by_partial_prefix = list(ListFilter {
        id_prefix: Some("are".to_owned()),
        ..no_filter()
    });
    let by_integer = list(ListFilter {
        where_eq: equals("x", Scalar::Int(1)),
        ..no_filter()
    });
    let by_true = list(ListFilter {
        where_eq: equals("x", Scalar::Bool(true)),
        ..no_filter()
    });
    let by_text = list(ListFilter {
        where_eq: equals("x", Scalar::Str("1".to_owned())),
        ..no_filter()
    });
    let by_zero = list(ListFilter {
        where_eq: equals("x", Scalar::Int(0)),
        ..no_filter()
    });
    let by_false = list(ListFilter {
        where_eq: equals("x", Scalar::Bool(false)),
        ..no_filter()
    });
    let by_text_field = list(ListFilter {
        where_eq: equals("y", Scalar::Str("a".to_owned())),
        ..no_filter()
    });
    let by_two_fields = list(ListFilter {
        where_eq: vec![
            (FieldName::parse("x").unwrap(), Scalar::Bool(true)),
            (FieldName::parse("y").unwrap(), Scalar::Str("a".to_owned())),
        ],
        ..no_filter()
    });
    let first_page = listed(&mut env.store, tenant, &no_filter(), &page(2, None));
    let second_page = listed(&mut env.store, tenant, &no_filter(), &page(2, Some(3)));
    let past_the_end = listed(&mut env.store, tenant, &no_filter(), &page(2, Some(9)));

    assert_eq!(
        all,
        [
            "area.alpha",
            "areax.gamma",
            "area.alpha.child",
            "solo",
            "area.beta",
            "flagoff"
        ]
    );
    assert_eq!(tasks, ["solo", "area.beta"]);
    assert_eq!(by_snapshot, ["area.alpha"]);
    assert!(by_other_snapshot.is_empty());
    assert_eq!(by_prefix, ["area.alpha", "area.alpha.child", "area.beta"]);
    assert_eq!(by_exact_prefix, ["area.alpha", "area.alpha.child"]);
    assert!(by_partial_prefix.is_empty());
    assert_eq!(by_integer, ["area.alpha"]);
    assert_eq!(by_true, ["area.beta"]);
    assert_eq!(by_text, ["areax.gamma"]);
    assert_eq!(by_zero, ["solo"]);
    assert_eq!(by_false, ["flagoff"]);
    assert_eq!(by_text_field, ["area.alpha", "area.beta"]);
    assert_eq!(by_two_fields, ["area.beta"]);
    assert_eq!(first_page, ["area.alpha", "areax.gamma"]);
    assert_eq!(second_page, ["area.alpha.child", "solo"]);
    assert!(past_the_end.is_empty());
}

#[test]
fn it_should_list_the_revisions_of_a_lineage_in_order() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let first = append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note"), Draft::node("a.two", "note")],
    );
    let ids = ids_of(&first);
    let revision = append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note").revising(&ids[0])],
    );
    let retirement = append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note")
            .revising(&ids_of(&revision)[0])
            .retired()],
    );
    let edge = append(
        &mut env.store,
        &mut ledger,
        &[Draft::relation("depends-on", "a.one", "a.two")],
    );
    append(
        &mut env.store,
        &mut ledger,
        &[Draft::relation("depends-on", "a.one", "a.two")
            .revising(&ids_of(&edge)[0])
            .retired()],
    );
    let node_lineage = Lineage::Node(SemanticId::parse("a.one").unwrap());
    let relation_lineage = Lineage::Relation {
        relation_type: RelationTypeName::parse("depends-on").unwrap(),
        from: SemanticId::parse("a.one").unwrap(),
        to: SemanticId::parse("a.two").unwrap(),
    };
    let unwritten = Lineage::Node(SemanticId::parse("a.none").unwrap());

    let (node, relation, none) = read_with(&mut env.store, |reader| {
        Ok((
            reader.revisions(&ledger.tenant, LineageRef::from(&node_lineage))?,
            reader.revisions(&ledger.tenant, LineageRef::from(&relation_lineage))?,
            reader.revisions(&ledger.tenant, LineageRef::from(&unwritten))?,
        ))
    })
    .unwrap();

    assert_eq!(seqs_of(&node), [1, 3, 4]);
    assert_eq!(node[2].id, ids_of(&retirement)[0]);
    assert_eq!(
        node.iter()
            .map(|row| row.columns.retired)
            .collect::<Vec<_>>(),
        [false, false, true]
    );
    assert_eq!(seqs_of(&relation), [5, 6]);
    assert!(none.is_empty());
}

#[test]
fn it_should_list_latest_relations_by_each_filter() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let edges = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::relation("depends-on", "a", "b"),
            Draft::relation("depends-on", "a", "c"),
            Draft::relation("blocks", "b", "c"),
        ],
    );
    append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::relation("depends-on", "a", "c")
                .revising(&ids_of(&edges)[1])
                .retired(),
            Draft::relation("depends-on", "c", "a"),
        ],
    );
    let query = |relation_type: Option<&str>,
                 from: Option<&str>,
                 to: Option<&str>,
                 include_retired: bool| RelationQuery {
        relation_type: relation_type.map(|text| RelationTypeName::parse(text).unwrap()),
        from: from.map(|text| SemanticId::parse(text).unwrap()),
        to: to.map(|text| SemanticId::parse(text).unwrap()),
        include_retired,
    };
    let mut run = |query: &RelationQuery| {
        let rows = read_with(&mut env.store, |reader| {
            reader.latest_relations(&ledger.tenant, query)
        })
        .unwrap();
        let mut keys = keys_of(&rows);
        keys.sort();
        (keys, rows)
    };

    let (live, _) = run(&query(None, None, None, false));
    let (by_type, _) = run(&query(Some("depends-on"), None, None, false));
    let (by_from, _) = run(&query(None, Some("a"), None, false));
    let (by_to, _) = run(&query(None, None, Some("c"), false));
    let (with_retired, _) = run(&query(Some("depends-on"), Some("a"), None, true));
    let (retired_pair, rows) = run(&query(None, Some("a"), Some("c"), true));

    assert_eq!(live, ["blocks|b|c", "depends-on|a|b", "depends-on|c|a"]);
    assert_eq!(by_type, ["depends-on|a|b", "depends-on|c|a"]);
    assert_eq!(by_from, ["depends-on|a|b"]);
    assert_eq!(by_to, ["blocks|b|c"]);
    assert_eq!(with_retired, ["depends-on|a|b", "depends-on|a|c"]);
    assert_eq!(retired_pair, ["depends-on|a|c"]);
    assert!(rows[0].columns.retired);
}

#[test]
fn it_should_read_records_from_a_seq_up_to_a_limit() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let plan = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::node("a.one", "note"),
            Draft::node("a.two", "note"),
            Draft::node("a.three", "note"),
            Draft::node("a.four", "note"),
            Draft::node("a.five", "note"),
        ],
    );
    let tenant = &ledger.tenant;

    let middle = rows_from(&mut env.store, tenant, 2, 3);
    let tail = rows_from(&mut env.store, tenant, 4, 10);
    let beyond = rows_from(&mut env.store, tenant, 6, 10);
    let whole = rows_from(&mut env.store, tenant, 1, 10);

    assert_eq!(seqs_of(&middle), [2, 3, 4]);
    assert_eq!(seqs_of(&tail), [4, 5]);
    assert!(beyond.is_empty());
    for (row, expected) in whole.iter().zip(&plan.records) {
        assert_row_matches(row, expected);
    }
}

#[test]
fn it_should_list_the_batches_of_a_tenant() {
    let mut env = env();
    let mut acme = start(&mut env.store, "acme");
    let mut beta = start(&mut env.store, "beta");
    append_keyed(
        &mut env.store,
        &mut acme,
        &[Draft::node("a.one", "note"), Draft::node("a.two", "note")],
        "k1",
    );
    append(&mut env.store, &mut acme, &[Draft::node("a.three", "note")]);
    append_keyed(
        &mut env.store,
        &mut acme,
        &[Draft::node("a.four", "note")],
        "k2",
    );
    append_keyed(
        &mut env.store,
        &mut beta,
        &[Draft::node("b.one", "note")],
        "k3",
    );

    let mut acme_batches =
        read_with(&mut env.store, |reader| reader.stored_batches(&acme.tenant)).unwrap();
    let beta_batches =
        read_with(&mut env.store, |reader| reader.stored_batches(&beta.tenant)).unwrap();

    acme_batches.sort_by(|a, b| a.key.cmp(&b.key));
    let shape: Vec<(&str, i64, i64)> = acme_batches
        .iter()
        .map(|row| (row.key.as_str(), row.first_seq, row.item_count))
        .collect();
    assert_eq!(shape, [("k1", 1, 2), ("k2", 4, 1)]);
    assert_eq!(beta_batches.len(), 1);
    assert_eq!(beta_batches[0].key, "k3");
}

#[test]
fn it_should_read_stored_records_after_a_seq_as_text() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let first = ledger.record_id(1);
    let second = ledger.record_id(2);
    let plan = append(
        &mut env.store,
        &mut ledger,
        &[
            Draft::node("a.one", "note"),
            Draft::node("a.two", "note")
                .with_snapshot(snapshot('c'))
                .with_external_id("x")
                .with_def_hash('2'),
            Draft::relation("after", "a.one", "n.other").with_native_target(),
            Draft::node("a.one", "note")
                .with_data(json!({"text": "two"}))
                .revising(&first),
            Draft::node("a.two", "note").revising(&second).retired(),
        ],
    );
    let tenant = &ledger.tenant;

    let whole = stored_rows(&mut env.store, tenant, None, 10);
    let after_two = stored_rows(&mut env.store, tenant, Some(2), 10);
    let cut = stored_rows(&mut env.store, tenant, None, 2);
    let beyond = stored_rows(&mut env.store, tenant, Some(5), 10);

    assert_eq!(stored_seqs(&whole), [1, 2, 3, 4, 5]);
    for (row, expected) in whole.iter().zip(&plan.records) {
        assert_stored_matches(row, expected);
    }
    assert_eq!(stored_seqs(&after_two), [3, 4, 5]);
    assert_eq!(stored_seqs(&cut), [1, 2]);
    assert!(beyond.is_empty());
}

#[test]
fn it_should_read_stored_rows_even_when_a_value_is_not_valid_for_its_type() {
    let mut env = env();
    let ledger = start(&mut env.store, "acme");
    let record = ledger.records(&[Draft::node("a.one", "note")]).remove(0);
    let mut raw = Raw::open(&env.path);
    raw.insert_as(&ledger.tenant, &record, "ZZ", "Bad Type")
        .unwrap();
    raw.close();

    let rows = stored_rows(&mut env.store, &ledger.tenant, None, 10);
    let typed = failure(read_with(&mut env.store, |reader| {
        reader.batch_records(&ledger.tenant, 1, 1)
    }));

    assert_eq!(stored_seqs(&rows), [1]);
    assert_eq!(rows[0].hash, "ZZ");
    assert_eq!(rows[0].type_name, "Bad Type");
    assert_store_error(&typed, "corrupt-row");
}

// ---------------------------------------------------------------- escrita

#[test]
fn it_should_persist_nothing_when_decide_fails() {
    let mut env = env();
    let ledger = start(&mut env.store, "acme");
    let refusal = HexlogError::refused(ErrorCode::ForkRejected, "rejected", "decide refused");

    let result = env.store.append_with(&ledger.tenant, &mut |reader| {
        WriteReads::chain_head(reader, &ledger.tenant)?;
        Err(refusal.clone())
    });

    assert_eq!(failure(result), refusal);
    assert_eq!(head_of(&mut env.store, &ledger.tenant).seq, 0);
    assert!(rows_from(&mut env.store, &ledger.tenant, 1, 10).is_empty());
}

#[test]
fn it_should_append_records_and_batch_atomically() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let stored = append_keyed(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note"), Draft::node("a.two", "note")],
        "k1",
    );
    let mut broken = ledger.plan(
        &[
            Draft::node("a.three", "note"),
            Draft::node("a.four", "note"),
        ],
        Some("k2"),
    );
    broken.records[1].seq += 3;
    let key = IdempotencyKey::parse("k1").unwrap();
    let failed_key = IdempotencyKey::parse("k2").unwrap();

    let rejected = attempt(&mut env.store, &ledger.tenant, broken);

    assert_eq!(failure(rejected).code, ErrorCode::StoreError);
    assert_eq!(stored.outcome.records.len(), 2);
    let (rows, kept, dropped) = read_with(&mut env.store, |reader| {
        Ok((
            reader.batch_records(&ledger.tenant, 1, 10)?,
            reader.batch_by_key(&ledger.tenant, &key)?,
            reader.batch_by_key(&ledger.tenant, &failed_key)?,
        ))
    })
    .unwrap();
    assert_eq!(seqs_of(&rows), [1, 2]);
    assert_eq!(kept.unwrap().item_count, 2);
    assert!(dropped.is_none());
}

#[test]
fn it_should_translate_a_unique_violation_into_fork_rejected() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let root = append(&mut env.store, &mut ledger, &[Draft::node("a.one", "note")]);
    let root_id = ids_of(&root).remove(0);
    append(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note").revising(&root_id)],
    );
    let second_root = ledger.plan(&[Draft::node("a.one", "note")], None);
    let second_successor = ledger.plan(&[Draft::node("a.one", "note").revising(&root_id)], None);
    let mut repeated_id = ledger.plan(&[Draft::node("a.two", "note")], None);
    repeated_id.records[0].id = root_id;

    let root_error = failure(attempt(&mut env.store, &ledger.tenant, second_root));
    let successor_error = failure(attempt(&mut env.store, &ledger.tenant, second_successor));
    let id_error = failure(attempt(&mut env.store, &ledger.tenant, repeated_id));

    assert_eq!(root_error.code, ErrorCode::ForkRejected);
    assert_eq!(successor_error.code, ErrorCode::ForkRejected);
    assert_store_error(&id_error, "2067");
}

#[test]
fn it_should_report_busy_when_another_connection_holds_the_write_lock() {
    let mut env = env_with(SHORT_TIMEOUT);
    let mut ledger = start(&mut env.store, "acme");
    append(&mut env.store, &mut ledger, &[Draft::node("a.one", "note")]);
    let mut holder = Raw::open(&env.path);
    holder.run("BEGIN IMMEDIATE").unwrap();

    let result = env.store.append_with(&ledger.tenant, &mut |_| {
        Err(HexlogError::internal("decide should not run while busy"))
    });
    holder.run("ROLLBACK").unwrap();
    append(&mut env.store, &mut ledger, &[Draft::node("a.two", "note")]);

    assert_eq!(failure(result).code, ErrorCode::Busy);
    assert_eq!(head_of(&mut env.store, &ledger.tenant).seq, 2);
}

#[test]
fn it_should_translate_a_trigger_abort_into_store_error_with_its_code() {
    let mut env = env();
    let ledger = start(&mut env.store, "acme");
    let mut skipped = ledger.plan(&[Draft::node("a.one", "note")], None);
    skipped.records[0].seq = 5;

    let error = failure(attempt(&mut env.store, &ledger.tenant, skipped));

    assert_store_error(&error, "1811");
    assert_eq!(error.details[0].message, "database error");
    assert!(!format!("{error:?}").contains(env.path.to_str().unwrap()));
}

#[test]
fn it_should_open_in_wal_mode_and_enforce_foreign_keys() {
    let mut env = env();
    let ledger = start(&mut env.store, "acme");
    let mut orphan = ledger.plan(&[Draft::node("a.one", "note")], Some("k1"));
    orphan.batch.as_mut().unwrap().first_seq = 99;
    let mut raw = Raw::open(&env.path);

    let mode = raw.journal_mode();
    let error = failure(attempt(&mut env.store, &ledger.tenant, orphan));

    assert_eq!(mode, "wal");
    assert_store_error(&error, "787");
}

#[test]
fn it_should_translate_a_duplicate_batch_key_into_store_error() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    append_keyed(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note")],
        "k1",
    );
    let repeated = ledger.plan(&[Draft::node("a.two", "note")], Some("k1"));

    let error = failure(attempt(&mut env.store, &ledger.tenant, repeated));

    assert_store_error(&error, "1555");
}

#[test]
fn it_should_write_nothing_for_an_empty_plan_from_a_replay() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let stored = append_keyed(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note")],
        "k1",
    );
    let replay = AppendPlan {
        records: Vec::new(),
        batch: None,
        outcome: AppendOutcome {
            records: stored.outcome.records.clone(),
            replayed: true,
            chain_head: stored.outcome.chain_head.clone(),
        },
    };

    let returned = attempt(&mut env.store, &ledger.tenant, replay).unwrap();

    assert!(returned.outcome.replayed);
    assert_eq!(
        head_of(&mut env.store, &ledger.tenant),
        stored.outcome.chain_head
    );
    assert_eq!(rows_from(&mut env.store, &ledger.tenant, 1, 10).len(), 1);
}

#[test]
fn it_should_accept_a_new_write_after_a_non_constraint_error_inside_a_transaction() {
    let mut env = env();
    let mut ledger = start(&mut env.store, "acme");
    let mut raw = Raw::open(&env.path);
    raw.run("ALTER TABLE batches RENAME TO batches_gone")
        .unwrap();
    let doomed = ledger.plan(&[Draft::node("a.one", "note")], Some("k1"));

    let error = failure(attempt(&mut env.store, &ledger.tenant, doomed));
    raw.run("ALTER TABLE batches_gone RENAME TO batches")
        .unwrap();
    let stored = append_keyed(
        &mut env.store,
        &mut ledger,
        &[Draft::node("a.one", "note")],
        "k1",
    );

    assert_eq!(error.code, ErrorCode::StoreError);
    assert_eq!(stored.outcome.chain_head.seq, 1);
    assert_eq!(head_of(&mut env.store, &ledger.tenant).seq, 1);
}

#[test]
fn it_should_open_an_initialized_database_while_another_connection_holds_the_write_lock() {
    let env = env();
    let path = env.path.clone();
    drop(env.store);
    let mut holder = Raw::open(&path);
    holder.run("BEGIN IMMEDIATE").unwrap();

    let reopened = SqliteStore::open(&path, SHORT_TIMEOUT);

    assert!(reopened.is_ok());
    holder.run("ROLLBACK").unwrap();
}

#[test]
fn it_should_refuse_a_database_with_user_version_1_and_no_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hollow.db");
    let mut raw = Raw::create(&path);
    raw.run("PRAGMA user_version = 1").unwrap();
    raw.close();

    let error = failure(SqliteStore::open(&path, DEFAULT_TIMEOUT));

    assert_store_error(&error, "schema-missing");
}
