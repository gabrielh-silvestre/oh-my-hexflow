use std::future::Future;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use futures_executor::block_on;
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqliteSynchronous};

use super::tx::{BEGIN_IMMEDIATE, is_busy, translate};
use crate::error::{ErrorCode, HexlogError};

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i64 = 1;
const SET_SCHEMA_VERSION: &str = "PRAGMA user_version = 1";
const PROBE_SCHEMA: &str = "SELECT (SELECT user_version FROM pragma_user_version), \
    (SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'), \
    (SELECT count(*) FROM sqlite_schema \
      WHERE type = 'table' AND name IN ('tenants', 'records', 'batches'))";
const SCHEMA_TABLES: i64 = 3;
const ENABLE_WAL: &str = "PRAGMA journal_mode = WAL";
// O PRAGMA do WAL não honra o `busy_timeout`, por isso a repetição própria.
const WAL_RETRY_PAUSE: Duration = Duration::from_millis(20);

fn run<T>(future: impl Future<Output = Result<T, sqlx::Error>>) -> Result<T, HexlogError> {
    block_on(future).map_err(|error| translate(&error))
}

/// Opções de conexão sem `journal_mode`: o WAL só entra depois da criação do esquema.
pub(super) fn connect(
    db_path: &Path,
    busy_timeout: Duration,
) -> Result<SqliteConnection, HexlogError> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .busy_timeout(busy_timeout)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true);
    run(SqliteConnection::connect_with(&options))
}

pub(super) fn open(
    db_path: &Path,
    busy_timeout: Duration,
) -> Result<SqliteConnection, HexlogError> {
    let mut conn = connect(db_path, busy_timeout)?;
    let prepared = prepare_schema(&mut conn).and_then(|()| enable_wal(&mut conn, busy_timeout));
    match prepared {
        Ok(()) => Ok(conn),
        Err(error) => {
            // Solta o arquivo antes de devolver o erro; o erro original é o que importa.
            let _ = block_on(conn.close());
            Err(error)
        }
    }
}

enum Schema {
    Install,
    Ready,
}

fn classify(version: i64, objects: i64, tables: i64) -> Result<Schema, HexlogError> {
    match (version, objects, tables) {
        (0, 0, _) => Ok(Schema::Install),
        (SCHEMA_VERSION, _, SCHEMA_TABLES) => Ok(Schema::Ready),
        (SCHEMA_VERSION, ..) => Err(HexlogError::refused(
            ErrorCode::StoreError,
            "schema-missing",
            "database has the schema version but not its tables",
        )),
        _ => Err(HexlogError::refused(
            ErrorCode::StoreError,
            "unsupported-schema-version",
            "unsupported database schema version",
        )),
    }
}

async fn probe(conn: &mut SqliteConnection) -> Result<(i64, i64, i64), sqlx::Error> {
    sqlx::query_as(PROBE_SCHEMA).fetch_one(conn).await
}

fn prepare_schema(conn: &mut SqliteConnection) -> Result<(), HexlogError> {
    let (version, objects, tables) = run(probe(conn))?;
    match classify(version, objects, tables)? {
        Schema::Ready => Ok(()),
        Schema::Install => install_schema(conn),
    }
}

fn install_schema(conn: &mut SqliteConnection) -> Result<(), HexlogError> {
    let mut tx = run(conn.begin_with(BEGIN_IMMEDIATE))?;
    let (version, objects, tables) = run(probe(&mut tx))?;
    match classify(version, objects, tables) {
        Ok(Schema::Install) => {
            run(sqlx::raw_sql(SCHEMA).execute(&mut *tx))?;
            run(sqlx::raw_sql(SET_SCHEMA_VERSION).execute(&mut *tx))?;
        }
        Ok(Schema::Ready) => {}
        Err(error) => {
            let _ = block_on(tx.rollback());
            return Err(error);
        }
    }
    run(tx.commit())
}

fn enable_wal(conn: &mut SqliteConnection, busy_timeout: Duration) -> Result<(), HexlogError> {
    let deadline = Instant::now() + busy_timeout;
    loop {
        let attempt = block_on(sqlx::query_scalar::<_, String>(ENABLE_WAL).fetch_one(&mut *conn));
        match attempt {
            Ok(_) => return Ok(()),
            Err(error) if is_busy(&error) && Instant::now() < deadline => sleep(WAL_RETRY_PAUSE),
            Err(error) => return Err(translate(&error)),
        }
    }
}
