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
const READ_SCHEMA_VERSION: &str = "PRAGMA user_version";
const COUNT_TABLES: &str = "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'";
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

/// Abertura em cinco passos: conectar, `BEGIN IMMEDIATE`, esquema se vazio, `COMMIT`, WAL.
pub(super) fn open(
    db_path: &Path,
    busy_timeout: Duration,
) -> Result<SqliteConnection, HexlogError> {
    let mut conn = connect(db_path, busy_timeout)?;
    let prepared = install_schema(&mut conn).and_then(|()| enable_wal(&mut conn, busy_timeout));
    match prepared {
        Ok(()) => Ok(conn),
        Err(error) => {
            // Solta o arquivo antes de devolver o erro; o erro original é o que importa.
            let _ = block_on(conn.close());
            Err(error)
        }
    }
}

fn install_schema(conn: &mut SqliteConnection) -> Result<(), HexlogError> {
    let mut tx = run(conn.begin_with(BEGIN_IMMEDIATE))?;
    let version = run(sqlx::query_scalar::<_, i64>(READ_SCHEMA_VERSION).fetch_one(&mut *tx))?;
    let tables = run(sqlx::query_scalar::<_, i64>(COUNT_TABLES).fetch_one(&mut *tx))?;
    if version == 0 && tables == 0 {
        run(sqlx::raw_sql(SCHEMA).execute(&mut *tx))?;
        run(sqlx::raw_sql(SET_SCHEMA_VERSION).execute(&mut *tx))?;
    } else if version != SCHEMA_VERSION {
        let _ = block_on(tx.rollback());
        return Err(HexlogError::refused(
            ErrorCode::StoreError,
            "unsupported-schema-version",
            "unsupported database schema version",
        ));
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
