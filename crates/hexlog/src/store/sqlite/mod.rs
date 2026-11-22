use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_executor::block_on;
use sqlx::Connection;
use sqlx::sqlite::SqliteConnection;

use crate::engine::ports::{AppendPlan, Store, StoreReader, TenantRow, WriteReads};
use crate::error::HexlogError;
use crate::ids::{Hash, TenantId};

mod ddl;
#[cfg(test)]
mod tests;
mod tx;

use tx::{Tx, run_in};

/// Dono da conexão: escritas e leituras tomam `&mut self`, então o compilador prova um usuário por instância.
pub(crate) struct SqliteStore {
    /// `None` depois de uma falha que deixou a conexão suspeita; a próxima chamada reconecta.
    conn: Option<SqliteConnection>,
    db_path: PathBuf,
    busy_timeout: Duration,
}

impl SqliteStore {
    pub(crate) fn open(db_path: &Path, busy_timeout: Duration) -> Result<SqliteStore, HexlogError> {
        Ok(SqliteStore {
            conn: Some(ddl::open(db_path, busy_timeout)?),
            db_path: db_path.to_owned(),
            busy_timeout,
        })
    }

    /// `close` espera a thread da conexão terminar; o `drop` sozinho deixa o arquivo preso por um instante.
    fn discard_connection(&mut self) {
        if let Some(conn) = self.conn.take() {
            let _ = block_on(conn.close());
        }
    }

    fn in_transaction<T>(
        &mut self,
        immediate: bool,
        body: impl FnOnce(&mut Tx<'_>) -> Result<T, HexlogError>,
    ) -> Result<T, HexlogError> {
        let conn = match self.conn.take() {
            Some(conn) => conn,
            None => ddl::connect(&self.db_path, self.busy_timeout)?,
        };
        let (outcome, broken) = run_in(self.conn.insert(conn), immediate, body);
        if broken {
            self.discard_connection();
        }
        outcome
    }
}

impl Drop for SqliteStore {
    fn drop(&mut self) {
        self.discard_connection();
    }
}

impl Store for SqliteStore {
    fn ensure_tenant(
        &mut self,
        tenant: &TenantId,
        anchor_doc: &str,
        anchor: &Hash,
        created_at: &str,
    ) -> Result<(TenantRow, bool), HexlogError> {
        self.in_transaction(true, |tx| {
            tx.ensure_tenant(tenant, anchor_doc, anchor, created_at)
        })
    }

    fn append_with(
        &mut self,
        tenant: &TenantId,
        decide: &mut dyn FnMut(&mut dyn WriteReads) -> Result<AppendPlan, HexlogError>,
    ) -> Result<AppendPlan, HexlogError> {
        self.in_transaction(true, |tx| {
            let plan = decide(tx)?;
            tx.insert_plan(tenant, &plan)?;
            Ok(plan)
        })
    }

    fn read(
        &mut self,
        f: &mut dyn FnMut(&mut dyn StoreReader) -> Result<(), HexlogError>,
    ) -> Result<(), HexlogError> {
        self.in_transaction(false, |tx| f(tx))
    }
}
