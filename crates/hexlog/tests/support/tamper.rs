use std::path::Path;

use futures_executor::block_on;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::{AssertSqlSafe, Connection, Row};

// Os sete triggers da seção 4.3 do plano, por literal fixo: `raw_sql` só aceita `&'static str`.
const DROP_TRIGGERS: &str = "DROP TRIGGER tenants_no_update; \
    DROP TRIGGER tenants_no_delete; \
    DROP TRIGGER records_no_update; \
    DROP TRIGGER records_no_delete; \
    DROP TRIGGER batches_no_update; \
    DROP TRIGGER batches_no_delete; \
    DROP TRIGGER records_chain_guard;";

/// Conexão crua ao arquivo do banco, sem triggers, sem chaves estrangeiras e sem restrições
/// de domínio, para adulterar o que o store grava. Uso só em teste.
pub struct Tamper {
    conn: SqliteConnection,
}

impl Tamper {
    /// Não cria o arquivo: o store já o criou.
    pub fn open(path: &Path) -> Tamper {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(false)
            .pragma("ignore_check_constraints", "ON");
        let mut conn =
            block_on(SqliteConnection::connect_with(&options)).expect("tamper connection opens");
        block_on(sqlx::raw_sql(DROP_TRIGGERS).execute(&mut conn)).expect("triggers are dropped");
        Tamper { conn }
    }

    /// Roda `sql` com `binds` ligados como texto e devolve as linhas alteradas.
    /// Texto ligado a coluna `INTEGER` de tabela `STRICT` é coagido pelo SQLite.
    pub fn execute(&mut self, sql: &str, binds: &[&str]) -> u64 {
        let mut query = sqlx::query(AssertSqlSafe(sql));
        for bind in binds {
            query = query.bind(*bind);
        }
        block_on(query.execute(&mut self.conn))
            .expect("tamper statement runs")
            .rows_affected()
    }

    /// Primeira coluna de cada linha; o `sql` faz `CAST(.. AS TEXT)` do que não for texto.
    pub fn read_text(&mut self, sql: &str, binds: &[&str]) -> Vec<String> {
        let mut query = sqlx::query(AssertSqlSafe(sql));
        for bind in binds {
            query = query.bind(*bind);
        }
        block_on(query.fetch_all(&mut self.conn))
            .expect("tamper query runs")
            .iter()
            .map(|row| row.try_get::<String, _>(0).expect("first column is text"))
            .collect()
    }

    pub fn read_body(&mut self, tenant: &str, seq: u64) -> String {
        let seq = seq.to_string();
        self.read_text(
            "SELECT CAST(body AS TEXT) FROM records WHERE tenant = ? AND seq = ?",
            &[tenant, &seq],
        )
        .into_iter()
        .next()
        .expect("record exists")
    }
}
