# Pesquisa de reuso: sqlx como motor do store SQLite do hexlog

Consulta feita em 2026-11-07 por `esopo-sonnet_omc-document-specialist_pesquisa-store-sqlite-e`, depois da decisão do dono de trocar `rusqlite` por `sqlx` (versão 0.9.0). Este relatório existe para o `test-engineer`, o `executor` da T4 e o crítico não refazerem a pesquisa. Cada afirmação tem fonte: arquivo do registry (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`), issue do repositório ou saída de um programa que rodei no ambiente descartável `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/research-store/` (`sqlx-spike`, `sqlx-macro`, `conflict`, `full`). Complementa `store-sqlite.md`, que comparou as bibliotecas antes da decisão.

## Tabela de Conteúdos

- [sqlx: aptidão pelo critério do dono](#sqlx-aptidão-pelo-critério-do-dono)
- [Features mínimas e versão do SQLite](#features-mínimas-e-versão-do-sqlite)
- [Rodar sem runtime](#rodar-sem-runtime)
- [Macros query!](#macros-query)
- [API de conexão, transação e erros](#api-de-conexão-transação-e-erros)
- [Send, Sync e empréstimo mutável](#send-sync-e-empréstimo-mutável)
- [Thread de trabalho por conexão](#thread-de-trabalho-por-conexão)
- [Conflito de links com rusqlite](#conflito-de-links-com-rusqlite)
- [Spike que o plano exigia](#spike-que-o-plano-exigia)
- [Medição contra o rusqlite](#medição-contra-o-rusqlite)
- [Armadilhas e gaps](#armadilhas-e-gaps)
- [Links Úteis](#links-úteis)

---

## [sqlx: aptidão pelo critério do dono](https://github.com/transact-rs/sqlx)

### O que é?

Kit SQL assíncrono em Rust (PostgreSQL, MySQL, SQLite) com consultas verificadas em tempo de compilação. O repositório mudou de `launchbadge/sqlx` para `transact-rs/sqlx`: a busca do GitHub pelo nome antigo falha, e o `crates.io` ainda aponta para o nome antigo.

### Avaliação

| critério | valor (conferido em 2026-11-07) |
|---|---|
| versão e data | 0.9.0, 2026-05-21; anterior 0.8.6, 2025-05-19; nenhum patch 0.9.x até hoje (5,5 meses) |
| último commit | 2026-10-31 (`7908fe7`, correção de MySQL); `pushed_at` 2026-10-31 |
| downloads recentes | 55.706.867 (total 183.868.169) |
| mantenedores | 2 no crates.io (`abonander`, `mehcode`) |
| estrelas | **17632** |
| licença | MIT OR Apache-2.0 no crates.io; Apache-2.0 na API do GitHub |
| saúde | CI `sqlx.yml`, `sqlx-cli.yml`, `examples.yml`; README cita docs.rs, crates.io, shields.io e workflows; docs.rs ok na 0.9.0; **786 issues abertas** |
| MSRV | 1.94.0 (compila em 1.97.0, edition 2024: todos os programas abaixo compilaram) |
| veredito | **apto** |

Issues abertas que tocam SQLite (busca por "sqlite busy locked transaction bug", 2026-11-07): [#4434](https://github.com/transact-rs/sqlx/issues/4434) (relevante, ver armadilhas), [#2249](https://github.com/transact-rs/sqlx/issues/2249) (WAL não removido ao fechar; reportada na 0.6.2, **não reproduzida** na 0.9.0: o `-wal` sumiu após `close()` no spike) e [#2085](https://github.com/transact-rs/sqlx/issues/2085) (migração com `foreign_keys` desligado, fora do escopo do hexlog).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Features mínimas e versão do SQLite](https://docs.rs/crate/sqlx/0.9.0/features)

### O que é?

O `sqlx` 0.9.0 não escolhe SQLite embutido por padrão: os defaults são `any`, `macros`, `migrate`, `json` (`sqlx-0.9.0/Cargo.toml`, `[features] default`). O SQLite embutido vem de `sqlite-bundled`; a feature `sqlite` é um atalho para `sqlite-bundled` mais `sqlite-deserialize`, `sqlite-load-extension` e `sqlite-unlock-notify`.

### Como usar?

```toml
sqlx = { version = "=0.9.0", default-features = false, features = ["sqlite-bundled", "derive"] }
futures-executor = "=0.3.34"   # block_on; o próprio sqlx-sqlite já depende dele
```

- `derive` habilita `#[derive(FromRow)]` (`sqlx-0.9.0/Cargo.toml`: `derive = ["sqlx-macros/derive"]`). `macros` (que inclui `derive`) só é preciso para `query!`.
- `json`, `uuid`, `time` entram só se o código mapear `serde_json::Value`, `Uuid` ou `OffsetDateTime` direto nas colunas.
- Nenhuma dessas features liga runtime: `_rt-tokio`, `_rt-async-std`, `_rt-smol` e `_rt-async-global-executor` só nascem de `runtime-*`.
- Versão do SQLite: `libsqlite3-sys 0.37.0` embutida, que o programa confirmou com `select sqlite_version()` = **3.51.3**. O `rusqlite 0.41.1` traz 3.54.1.
- Fecho de dependências normais (`cargo tree -e normal`): 138 pacotes com `sqlite-bundled` + `derive` (inclui `futures-executor`); 147 com `macros`, `json`, `uuid`, `time`. O `rusqlite` com `bundled`, `serde_json`, `uuid`, `time` tem 22. Nos dois casos, sem `tokio`, `async-std`, `smol`, `async-io`, `hyper`, `reqwest`, `ureq`, `git2` e `gix`.
- Entram junto: `flume`, `futures-intrusive`, `event-listener`, `crossbeam-queue`, `parking_lot`, `tracing`, `log`, `sha2 0.10.9` (o `hexlog` pina `sha2 =0.11.1`, então haverá duas versões), `indexmap`, `hashlink`, `percent-encoding`, `url`, `serde`, `serde_json`, `thiserror 2.0.21`.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Rodar sem runtime](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html)

### O que é?

`SqliteConnection` roda com `futures_executor::block_on`, sem `runtime-tokio` nem `runtime-async-std`. Provado: o programa `sqlx-spike` (embutido mais abaixo) conectou, aplicou PRAGMAs, criou tabela `STRICT`, rodou transações, leu e fechou, tudo dentro de `block_on`. O motivo está em `sqlx-sqlite-0.9.0/src/connection/worker.rs`: cada conexão sobe a própria thread com `std::thread::Builder` e conversa por canais `flume`, sem pedir nada ao runtime.

### O que quebra sem runtime?

`sqlx-core-0.9.0/src/rt/mod.rs` define `timeout`, `sleep`, `spawn` e `spawn_blocking`; sem feature de runtime, as quatro caem em `missing_rt`, que dá `panic!("one of the `runtime` features of SQLx must be enabled")`. Quem chama essas funções (grep em `sqlx-core-0.9.0/src`):

| uso | arquivo | efeito sem runtime |
|---|---|---|
| `Pool` (aquisição com timeout, manutenção, devolução ao pool) | `pool/inner.rs`, `pool/connection.rs` | **panic em tempo de execução**; reproduzi: `SqlitePoolOptions::connect_with` panicou em `pool/inner.rs:57` |
| leitura de diretório de migrações | `fs.rs`, `migrate/source.rs` | panic (a feature `migrate` fica desligada no conjunto mínimo) |
| TLS e DNS | `net/` | não se aplica ao SQLite |

Compila sem aviso e panica ao rodar: não existe erro de compilação que proteja do `Pool`. O caminho `SqliteConnection` direto não chama nenhuma das quatro. `Connection::close()` e o `Drop` não precisam de runtime (verificado).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Macros query!](https://docs.rs/sqlx/0.9.0/sqlx/macro.query.html)

### O que é?

`query!` e `query_as!` checam o SQL em tempo de compilação. Dois modos: online, com `DATABASE_URL` apontando para um banco real, e offline, lendo `.sqlx/query-<hash>.json`.

### O que exigem?

Resultado de `sqlx-macro` (feature `macros`, sem runtime, `rustc 1.97.0`):

| cenário | resultado |
|---|---|
| online, `DATABASE_URL=sqlite:/caminho/ct.db`, sem runtime | **compila** e valida o SQL (`SELECT seq, nope` falhou com `no such column: nope`) |
| sem `DATABASE_URL` e sem `.sqlx` | erro de compilação: `set DATABASE_URL to use query macros online, or run cargo sqlx prepare` |
| `DATABASE_URL` com arquivo inexistente | erro de compilação: `unable to open database file` |
| gerar o cache: `DATABASE_URL=... SQLX_OFFLINE_DIR=$PWD/.sqlx cargo build` (o diretório precisa existir) | grava `.sqlx/query-<hash>.json` com `db_name`, `query`, `describe`, `hash` |
| offline: `SQLX_OFFLINE=true`, sem `DATABASE_URL` | compila e roda |
| offline sem o JSON da consulta | erro: `SQLX_OFFLINE=true but there is no cached data for this query` |

Por que não precisa de runtime: `sqlx-macros-core-0.9.0/src/database/impls.rs:72` liga o SQLite a `sqlx_sqlite::describe_blocking`, síncrono; o `block_on` assíncrono (`sqlx-macros-core/src/lib.rs:60`) só vale para PostgreSQL e MySQL. O fecho com `macros` ganha `sqlx-macros`, `sqlx-macros-core`, `syn` e `proc-macro2`, sem `tokio`.

Não verificado: `cargo sqlx prepare` (a CLI `sqlx-cli` não está instalada aqui e não consta no fecho do projeto); usei `SQLX_OFFLINE_DIR`, que gera o mesmo formato. O `.sqlx` precisa ser versionado, e o hash depende do texto exato da consulta.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [API de conexão, transação e erros](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/index.html)

### O que é?

O conjunto de chamadas que o `store/sqlite` usa. Tudo testado no `sqlx-spike`.

### Como aplicar?

| item | o que acontece | fonte |
|---|---|---|
| ordem de abertura | `sqlite3_busy_timeout` é aplicado na abertura, **antes** dos PRAGMAs; depois vêm, nesta ordem fixa, `locking_mode`, `auto_vacuum`, `journal_mode`, `foreign_keys`, `synchronous`, `analysis_limit` | `sqlx-sqlite-0.9.0/src/connection/establish.rs:181-184`, `options/mod.rs` (mapa `pragmas`), `options/connect.rs` |
| padrões de `SqliteConnectOptions::new()` | `busy_timeout` 5 s, `foreign_keys` ON, `journal_mode` não definido, `create_if_missing` **false** (abrir arquivo inexistente deu `unable to open database file`), cache de 100 consultas | `options/mod.rs:110-170`; spike |
| opções usadas | `.filename(p).create_if_missing(true).journal_mode(SqliteJournalMode::Wal).synchronous(SqliteSynchronous::Full).busy_timeout(d).foreign_keys(true)`; PRAGMAs lidos de volta: `wal`, `2`, `1`, `200` | spike |
| `schema.sql` | `sqlx::raw_sql("...;...;").execute(&mut c)` roda várias instruções de uma vez | spike |
| `BEGIN IMMEDIATE` | `c.begin_with("BEGIN IMMEDIATE").await` (devolve `Transaction`); o argumento é `impl SqlSafeStr`, então só literal `&'static str` ou `AssertSqlSafe(String)` | `sqlx-core-0.9.0/src/connection.rs:61` |
| `Drop` de `Transaction` aberta | enfileira um `ROLLBACK` (`start_rollback`), que a thread da conexão executa em seguida; nada é aguardado. Em 200 repetições de drop seguido de `BEGIN IMMEDIATE` em outra conexão (`busy_timeout` 50 ms) deu 0 erros | `sqlx-core-0.9.0/src/transaction.rs:265-280`; spike `extra` |
| `PRAGMA user_version` | ler: `query_scalar::<_, i64>("PRAGMA user_version")`; gravar: `raw_sql("PRAGMA user_version = 1")`; deu 0 e depois 1 | spike |
| `json_extract` com caminho ligado | `query_scalar("SELECT json_extract(data, ?) FROM r WHERE rid = ?").bind("$.a.b[1]")` devolveu 20 | spike |
| `FromRow` | `#[derive(FromRow)]` com `i64`, `String`, `Option<String>`, `Option<i64>` leu a linha; NULL virou `None` | spike (feature `derive`) |
| `Decode` de inteiros | `u64` **decodifica** (5 virou `Ok(5)`; -1 deu `TryFromIntError(NegOverflow)`), mas `bind(u64)` **não compila** (`u64: Encode<Sqlite>` não implementado; existem `u8`/`u16`/`u32`/`i8`/`i16`/`i32`/`i64`) | `sqlx-sqlite-0.9.0/src/types/uint.rs:85-95`; compilação |
| índice único | `Error::Database(d)`: `d.code()` = `Some("2067")` (código **estendido**, texto), `d.is_unique_violation()` = true, `d.kind()` = `UniqueViolation` | spike |
| `BUSY` | `d.code()` = `Some("5")` ao tomar `BEGIN IMMEDIATE` com o lock ocupado (esperou `busy_timeout`: 201 ms com 200 ms), mensagem `database is locked`, `kind()` = **`Other`**; ao promover leitura para escrita depois que outro escritor confirmou, veio `Some("517")` (`SQLITE_BUSY_SNAPSHOT`) com a mesma mensagem | spike e `extra` |
| conexão após `BUSY` no `begin_with` | continua usável: `in_transaction` false e a nova tentativa deu certo | spike `busy_retry` |
| `STRICT` | `INSERT` de texto em coluna `INTEGER` deu `code: 3091` | spike |

Trecho mínimo (executa dentro de `block_on`):

```rust
let mut tx = c.begin_with("BEGIN IMMEDIATE").await?;
sqlx::query("INSERT INTO r(kind, rid) VALUES (?, ?)").bind("a").bind("id-1").execute(&mut *tx).await?;
tx.commit().await?;
```

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Send, Sync e empréstimo mutável](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnection.html)

### O que é?

Resultado de compilar asserções de traço em `rustc 1.97.0` (`sqlx-spike/src/bin/traits.rs`, `sync_conn.rs`, `sync_tx.rs`, `shared_ref.rs`).

### O que muda para o hexlog?

| tipo | resultado |
|---|---|
| `SqliteConnection: Send` | compila |
| `SqliteConnection: Sync` | **compila** (diferente de `rusqlite::Connection`, que é `Send` e não `Sync`) |
| `Transaction<'static, Sqlite>: Send` e `Sync` | compilam |
| leitura com `&SqliteConnection` | **não compila**: `E0277 the trait Executor<'_> is not implemented for &SqliteConnection` |

`Executor` existe só para `&mut SqliteConnection`, então toda consulta, inclusive leitura, empresta a conexão por `&mut`. `Hexlog` pode ser `Send + Sync` sem `Mutex` e sem `unsafe`, mas continua precisando de `&mut self` em todos os métodos que tocam o banco. Duas leituras simultâneas por `&self` seguem impossíveis sem tipo de interior mutável. A decisão `&mut self` do relatório anterior vale igual com `sqlx`.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Thread de trabalho por conexão](https://github.com/transact-rs/sqlx/blob/main/sqlx-sqlite/src/connection/worker.rs)

### O que é?

Cada `SqliteConnection` tem uma thread do sistema, nomeada `sqlx-sqlite-worker-N`, que dona o handle do SQLite e processa comandos de um canal `flume` limitado a 50 (`command_buffer_size`) com buffer de linhas também de 50 (`sqlx-sqlite-0.9.0/src/connection/worker.rs:108`; `options/mod.rs`).

### Custo e encerramento

- Contagem de threads em `/proc/self/task`: 1 antes de conectar, 2 com uma conexão, 3 com duas, 1 depois de `close()` das duas.
- `connect().await` de arquivo existente: mediana de 554 a 627 µs com outra conexão aberta e 977 a 1668 µs sem nenhuma, incluindo `close()` (casos E e E2 abaixo, sem escrita). A primeira conexão do processo em arquivo novo levou 107 ms (cria arquivo, WAL e PRAGMAs).
- `close().await` envia `Shutdown` e espera a thread terminar (`connection/mod.rs:220-235`); o checkpoint do WAL acontece na thread. Depois de `close()` o `-wal` não existe mais.
- `drop` sem `close`: o canal fecha, a thread sai sozinha e fecha o SQLite (a contagem voltou a 1 em menos de 100 ms), mas **depois** que o `drop` retorna. O checkpoint final pode estar em andamento quando o processo termina ou quando outro código abre o arquivo. Para o `Hexlog` usar sempre `close().await`.
- `Drop` de `Transaction` usa `send` bloqueante num canal limitado a 50 (`worker.rs:406`): em teoria bloqueia a thread chamadora se a fila estiver cheia (não reproduzido).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Conflito de links com rusqlite](https://doc.rust-lang.org/cargo/reference/resolver.html#links)

### O que é?

O `libsqlite3-sys` declara `links = "sqlite3"`, e o cargo aceita um só por grafo.

### O que muda?

- `sqlx 0.9.0` usa `libsqlite3-sys 0.37.0`; `rusqlite 0.41.1` usa `0.39.0`. Projeto `conflict` com os dois: o cargo recusou (`package libsqlite3-sys links to the native library sqlite3, but it conflicts with a previous package`).
- Qualquer outra dependência que puxe `rusqlite` (ferramentas, testes, `rusqlite_migration`, `r2d2_sqlite`) fica proibida no mesmo grafo.
- O `Cargo.toml` atual de `crates/hexlog` ainda lista `rusqlite = { version = "=0.41.1", features = ["bundled"] }` (lido em 2026-11-07): sair dele é parte da troca.
- Dependências já pinadas com `=` convivem: `serde =1.0.229`, `serde_json =1.0.154`, `thiserror =2.0.21` e `url =2.5.8` coincidem com as versões que o `sqlx` resolveu. `sha2` ficará duplicado (0.10.9 do sqlx, 0.11.1 do hexlog).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Spike que o plano exigia](https://www.sqlite.org/stricttables.html)

### O que é?

Os quatro itens do spike do plano para o `rusqlite`, repetidos com `sqlx` (programa `sqlx-spike`, SQLite 3.51.3).

### Resultado

| item | resultado |
|---|---|
| tabela `STRICT` | cria; recusa texto em coluna inteira (`code: 3091`) |
| `json_extract` com caminho ligado | `bind("$.a.b[1]")` devolveu 20 |
| `PRAGMA journal_mode=WAL` devolve `wal` | sim (`PRAGMA journal_mode` = `wal` lido depois do `connect`) |
| FTS5 | `CREATE VIRTUAL TABLE fts USING fts5(body)` e `MATCH 'hello'` devolveu 1 linha |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Medição contra o rusqlite

### O que é?

Mesmo ambiente da medição anterior: WSL2 (kernel 6.18.33.2), ext4 em `/dev/sdd`, i9-7900X, 23 GB, `rustc 1.97.0` em release, WAL, `synchronous=FULL`, `busy_timeout` 5000, tabela `records` com 5000 linhas e índice, 500 repetições (200 em E e E2), `futures_executor::block_on` em cada chamada, arquivo em disco fora de `/mnt`. Carga do sistema de 4 a 5 durante estas rodadas (outros agentes ativos).

### Resultado

Mediana / p95 em microssegundos, cinco rodadas do `sqlx`:

| caso | rodada 1 | rodada 2 | rodada 3 | rodada 4 | rodada 5 |
|---|---|---|---|---|---|
| A) append reutilizando a conexão | 6.469 / 10.142 | 10.341 / 22.517 | 5.214 / 6.516 | 6.248 / 7.588 | 4.816 / 6.388 |
| C) leitura reutilizando | 157 / 233 | 80 / 130 | 135 / 215 | 131 / 229 | 117 / 212 |
| E) conectar e fechar, sem outra conexão | 977 / 1.892 | 981 / 1.355 | 1.668 / 2.046 | n/m | n/m |
| E2) o mesmo, com outra conexão aberta | 604 / 1.251 | 554 / 962 | 627 / 1.392 | n/m | n/m |

Comparação lado a lado nas rodadas 4 e 5 (os dois binários rodados em sequência, mesma carga), mediana / p95:

| caso | sqlx 0.9.0 | rusqlite 0.41.1 |
|---|---|---|
| A) append | 6.248 / 7.588 e 4.816 / 6.388 | 5.458 / 7.211 e 4.767 / 6.836 |
| C) leitura | 131 / 229 e 117 / 212 | 7 / 11 e 16 / 21 |

Medição anterior do `rusqlite` (carga 3): A 4.678 e 4.802; C 16 e 15.

Leitura: o append praticamente empata (o fsync de 4 a 5 ms domina; a diferença ficou entre 0 e 1,5 ms, dentro do ruído da rodada 2, que teve carga maior). A leitura simples custa de 80 a 157 µs contra 7 a 16 µs, entre cinco e dez vezes mais, porque cada consulta atravessa dois canais e uma troca de thread. Rodadas com picos de até 600 ms no p100 de A são o fsync do WSL2; não descartei dados do `sqlx`, mas a rodada 2 (10,3 ms) deve ser lida como ruído.

### Como reproduzir?

```bash
mkdir -p sqlx-spike/src/bin && cd sqlx-spike
# salvar o Cargo.toml, src/main.rs (spike) e src/bin/bench.rs abaixo
cargo build --release
mkdir -p ~/run && ./target/release/sqlx-spike ~/run     # spike funcional
./target/release/bench ~/run                            # medição; diretório em ext4, fora de /mnt
```

`Cargo.toml`:

```toml
[package]
name = "sqlx-spike"
version = "0.1.0"
edition = "2024"

[dependencies]
sqlx = { version = "=0.9.0", default-features = false, features = ["sqlite-bundled", "derive"] }
futures-executor = "0.3"
```

`src/main.rs` (spike funcional):

```rust
use futures_executor::block_on;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{AssertSqlSafe, Connection, FromRow, Row, SqliteConnection};
use std::time::Duration;

#[derive(Debug, FromRow)]
struct Rec { seq: i64, kind: String, data: Option<String>, n: Option<i64> }

fn opts(path: &str) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_millis(200))
        .foreign_keys(true)
}

fn threads() -> usize { std::fs::read_dir("/proc/self/task").unwrap().count() }

fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let path = format!("{dir}/spike.db");
    for s in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{path}{s}")); }

    block_on(async {
        use sqlx::ConnectOptions;
        let t0 = threads();
        let started = std::time::Instant::now();
        let mut c: SqliteConnection = opts(&path).connect().await.unwrap();
        println!("connect: {:?}, threads {} -> {}", started.elapsed(), t0, threads());
        // PRAGMAs applied
        for p in ["journal_mode", "synchronous", "foreign_keys", "busy_timeout", "user_version"] {
            let r = sqlx::query(AssertSqlSafe(format!("PRAGMA {p}"))).fetch_one(&mut c).await.unwrap();
            let v: sqlx::sqlite::SqliteValueRef = sqlx::Row::try_get_raw(&r, 0).unwrap();
            let _ = v;
            let s = r.try_get::<i64, _>(0).map(|x| x.to_string()).or_else(|_| r.try_get::<String, _>(0)).unwrap();
            println!("PRAGMA {p} = {s}");
        }
        let v: String = sqlx::query_scalar("select sqlite_version()").fetch_one(&mut c).await.unwrap();
        println!("sqlite_version = {v}");
        // raw_sql with multiple statements + STRICT + FTS5
        sqlx::raw_sql("CREATE TABLE r(seq INTEGER PRIMARY KEY, kind TEXT NOT NULL, data TEXT, n INTEGER, rid TEXT UNIQUE) STRICT;
                       CREATE VIRTUAL TABLE fts USING fts5(body);
                       INSERT INTO fts(body) VALUES ('hello world');").execute(&mut c).await.unwrap();
        let hit: i64 = sqlx::query_scalar("SELECT count(*) FROM fts WHERE fts MATCH 'hello'").fetch_one(&mut c).await.unwrap();
        println!("FTS5 hits = {hit}; STRICT ok");
        // user_version set
        sqlx::raw_sql("PRAGMA user_version = 1").execute(&mut c).await.unwrap();
        let uv: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(&mut c).await.unwrap();
        println!("user_version = {uv}");
        // STRICT rejects wrong type
        let e = sqlx::query("INSERT INTO r(kind, n) VALUES ('a', 'texto')").execute(&mut c).await.unwrap_err();
        println!("STRICT mismatch -> {e}");
        // BEGIN IMMEDIATE via begin_with
        {
            let mut tx = c.begin_with("BEGIN IMMEDIATE").await.unwrap();
            sqlx::query("INSERT INTO r(kind, data, n, rid) VALUES (?, ?, ?, ?)")
                .bind("decision").bind(r#"{"a":{"b":[10,20]}}"#).bind(5i64).bind("id-1")
                .execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        }
        // json_extract with bound path
        let got: i64 = sqlx::query_scalar("SELECT json_extract(data, ?) FROM r WHERE rid = ?")
            .bind("$.a.b[1]").bind("id-1").fetch_one(&mut c).await.unwrap();
        println!("json_extract(bound path) = {got}");
        // unique violation
        let e = sqlx::query("INSERT INTO r(kind, rid) VALUES ('x', 'id-1')").execute(&mut c).await.unwrap_err();
        if let sqlx::Error::Database(d) = &e {
            println!("unique: code={:?} is_unique={} kind={:?} msg={}", d.code(), d.is_unique_violation(), d.kind(), d.message());
        }
        // FromRow
        let rec: Rec = sqlx::query_as("SELECT seq, kind, data, n FROM r").fetch_one(&mut c).await.unwrap();
        println!("FromRow: {rec:?}");
        // u64 / Option decode
        let r = sqlx::query("SELECT 5 AS a, NULL AS b").fetch_one(&mut c).await.unwrap();
        println!("u64={:?} i64={:?} opt_none={:?}", r.try_get::<u64, _>("a"), r.try_get::<i64, _>("a"), r.try_get::<Option<i64>, _>("b"));
        // Drop of open Transaction -> rollback
        {
            let mut tx = c.begin_with("BEGIN IMMEDIATE").await.unwrap();
            sqlx::query("INSERT INTO r(kind, rid) VALUES ('dropme','id-2')").execute(&mut *tx).await.unwrap();
            drop(tx);
        }
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM r WHERE rid='id-2'").fetch_one(&mut c).await.unwrap();
        println!("after Drop(Transaction): rows with id-2 = {n}; in_transaction={}", c.is_in_transaction());
        // BUSY: second connection while first holds IMMEDIATE
        let mut c2 = opts(&path).connect().await.unwrap();
        let mut tx = c.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let t = std::time::Instant::now();
        {
        let e = c2.begin_with("BEGIN IMMEDIATE").await;
        match e {
            Err(sqlx::Error::Database(d)) => println!("busy: code={:?} kind={:?} msg={} after {:?}", d.code(), d.kind(), d.message(), t.elapsed()),
            other => println!("busy: unexpected {:?}", other.map(|_| ())),
        }
        }
        tx.rollback().await.unwrap();
        // close
        let t1 = threads();
        c2.close().await.unwrap();
        c.close().await.unwrap();
        println!("threads before close {} after close {}", t1, threads());
        // WAL after last close
        println!("wal exists after close: {}", std::path::Path::new(&format!("{path}-wal")).exists());
    });
    // drop without close
    block_on(async {
        use sqlx::ConnectOptions;
        let t0 = threads();
        let c = opts(&path).connect().await.unwrap();
        let t1 = threads();
        drop(c);
        std::thread::sleep(Duration::from_millis(100));
        println!("drop-without-close threads {t0} -> {t1} -> {}", threads());
    });
}
```

`src/bin/bench.rs`:

```rust
use futures_executor::block_on;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{ConnectOptions, Connection, SqliteConnection};
use std::time::{Duration, Instant};

const N: usize = 500;

fn opts(path: &str) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_millis(5000))
        .foreign_keys(true)
}

async fn append(c: &mut SqliteConnection, i: i64) {
    let mut tx = c.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT INTO records(project, process, target, type, data, ts) VALUES (?,?,?,?,?,?)")
        .bind("p").bind("proc").bind("a.b.c").bind("decision")
        .bind(format!("{{\"n\":{i},\"pad\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}}"))
        .bind(i)
        .execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

async fn read(c: &mut SqliteConnection, i: i64) {
    let id = (i % 5000) + 1;
    let v: i64 = sqlx::query_scalar("SELECT seq FROM records WHERE seq = ?").bind(id).fetch_one(c).await.unwrap();
    assert_eq!(v, id);
}

fn fresh(path: &str) {
    for s in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{path}{s}")); }
    block_on(async {
        let mut c = opts(path).connect().await.unwrap();
        sqlx::raw_sql("CREATE TABLE records(seq INTEGER PRIMARY KEY, project TEXT, process TEXT, target TEXT, type TEXT, data TEXT, ts INTEGER);
                       CREATE INDEX i1 ON records(project, process, target);").execute(&mut c).await.unwrap();
        let mut tx = c.begin().await.unwrap();
        for i in 0..5000i64 {
            sqlx::query("INSERT INTO records(project, process, target, type, data, ts) VALUES ('p','proc','a.b.c','decision','{}',?)").bind(i).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        c.close().await.unwrap();
    });
}

fn stats(label: &str, mut v: Vec<f64>) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = v[v.len() / 2];
    let p95 = v[(v.len() as f64 * 0.95) as usize - 1];
    println!("{label}: median={med:.0}us p95={p95:.0}us min={:.0}us max={:.0}us", v[0], v[v.len()-1]);
}

fn timed<F: FnMut(i64)>(mut f: F, n: usize) -> Vec<f64> {
    (0..n as i64).map(|i| { let t = Instant::now(); f(i); t.elapsed().as_secs_f64() * 1e6 }).collect()
}

fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let path = format!("{dir}/bench.db");
    println!("sqlx 0.9.0 (sqlite-bundled) sqlite_version via libsqlite3-sys 0.37.0");

    fresh(&path);
    let mut c = block_on(opts(&path).connect()).unwrap();
    for i in 0..20 { block_on(append(&mut c, i)); }
    stats("A append reuse (block_on)", timed(|i| block_on(append(&mut c, i)), N));
    block_on(c.close()).unwrap();

    fresh(&path);
    let mut c = block_on(opts(&path).connect()).unwrap();
    for i in 0..20 { block_on(read(&mut c, i)); }
    stats("C read reuse (block_on)", timed(|i| block_on(read(&mut c, i)), N));
    block_on(c.close()).unwrap();

    // E: connect + close, no other connection (checkpoint on close)
    fresh(&path);
    stats("E connect+close, no other conn", timed(|_| { let c = block_on(opts(&path).connect()).unwrap(); block_on(c.close()).unwrap(); }, 200));
    let keeper = block_on(opts(&path).connect()).unwrap();
    stats("E2 connect+close, keeper open", timed(|_| { let c = block_on(opts(&path).connect()).unwrap(); block_on(c.close()).unwrap(); }, 200));
    block_on(keeper.close()).unwrap();
}
```

Programas menores no mesmo diretório: `src/bin/traits.rs`, `sync_conn.rs`, `sync_tx.rs`, `shared_ref.rs` (traços), `norun_pool.rs` (panic do `Pool`), `extra.rs` (`create_if_missing`, `Drop` de `Transaction`, decode de `u64`, `517`) e `busy_retry.rs`.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Armadilhas e gaps

### O que é?

O que o `executor` da T4 e o crítico precisam saber antes de escrever ou revisar o `store/sqlite` em `sqlx`, em ordem de perigo.

### Como evitar?

1. **`Pool` e timeouts panicam em tempo de execução, sem erro de compilação.** Sem `runtime-*`, `SqlitePool`, `acquire_timeout` e qualquer `rt::spawn` chamam `missing_rt`. Usar só `SqliteConnection`, e proibir `Pool` por lint ou teste de arquitetura.
2. **Transação e rollback automático.** O `Drop` de `Transaction` só enfileira o `ROLLBACK`; e quando o SQLite desfaz a transação sozinho (por exemplo `SQLITE_FULL`, código 13), o `sqlx` mantém a profundidade antiga e o próximo `begin_with("BEGIN IMMEDIATE")` falha com `InvalidSavePointStatement` ([#4434](https://github.com/transact-rs/sqlx/issues/4434), aberta; correção em [#4473](https://github.com/transact-rs/sqlx/pull/4473), aberta em 2026-11-07). Fechar a conexão e reabrir depois de qualquer erro dentro de transação que não seja violação de restrição, e sempre chamar `commit()` ou `rollback()` explicitamente.
3. **Classificação de erros.** `BUSY` chega como `"5"` ou `"517"` com a mesma mensagem e `kind()` = `Other`; `UNIQUE` chega como `"2067"` (estendido, texto). Comparar o código primário (`código_numérico & 0xFF`) e tratar 5 como "ocupado", 2067 como `FORK_REJECTED`; não confiar em `kind()` para `BUSY`.
4. **`SqlSafeStr`.** `query`, `begin_with` e `raw_sql` aceitam só `&'static str` ou `AssertSqlSafe(String)`; SQL montado em tempo de execução exige `AssertSqlSafe`, o que merece revisão a cada uso.
5. **Fechar sempre com `close().await`.** `drop` termina a thread depois de retornar, e o checkpoint do WAL pode correr em paralelo ao próximo código.
6. **Tipos.** `u64` lê mas não grava; guardar como `i64` e converter com `try_from`. As 19 colunas se mapeiam com `#[derive(FromRow)]`, mas `bind` é posicional: manter a lista de colunas num único lugar.
7. **Fecho de dependências e versão do SQLite** maiores que no `rusqlite` (138 contra 22 pacotes; SQLite 3.51.3 contra 3.54.1); `sha2` duplicado.

Gaps que continuam internos com `sqlx` (nenhuma função pronta): abertura concorrente de banco novo (`BEGIN IMMEDIATE` e releitura de `user_version` dentro da transação, como em `store-sqlite.md`), recusa de `user_version` maior que o do código, tradução de código estendido para `FORK_REJECTED`, paginação por `seq` e o laço de reabertura do item 2. O `sqlx::migrate` não serve: precisa de `migrate`, de diretório de arquivos e de runtime para ler o disco.

Não verificado: `cargo sqlx prepare`; o comportamento sob o `Drop` com fila de comandos cheia; a correção #4473 (aberta); `sqlx` com `Pool` e runtime, fora do escopo.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Todos consultados em 2026-11-07.

- [transact-rs/sqlx](https://github.com/transact-rs/sqlx) - repositório atual (antes `launchbadge/sqlx`): estrelas, commits, workflows, issues
- [crates.io: sqlx](https://crates.io/crates/sqlx), [API](https://crates.io/api/v1/crates/sqlx) - versão, datas, downloads, donos
- [docs.rs: sqlx 0.9.0](https://docs.rs/sqlx/0.9.0/sqlx/), [features](https://docs.rs/crate/sqlx/0.9.0/features), [status do build](https://docs.rs/crate/sqlx/latest/status.json) - páginas do docs.rs não reabertas nesta sessão; os fatos de API vêm do código-fonte no registry
- Registry local: `sqlx-0.9.0/Cargo.toml`, `sqlx-sqlite-0.9.0/Cargo.toml`, `sqlx-sqlite-0.9.0/src/connection/{establish,worker,mod}.rs`, `sqlx-sqlite-0.9.0/src/options/{mod,connect}.rs`, `sqlx-core-0.9.0/src/{rt/mod,connection,transaction,fs}.rs`, `sqlx-core-0.9.0/src/pool/inner.rs`, `sqlx-macros-core-0.9.0/src/{lib.rs,database/impls.rs,query/mod.rs}`
- [Issue #4434](https://github.com/transact-rs/sqlx/issues/4434), [PR #4473](https://github.com/transact-rs/sqlx/pull/4473), [issue #2249](https://github.com/transact-rs/sqlx/issues/2249), [issue #2085](https://github.com/transact-rs/sqlx/issues/2085)
- [Cargo: links](https://doc.rust-lang.org/cargo/reference/resolver.html#links) - regra de um só pacote por `links`
- [SQLite: tabelas STRICT](https://www.sqlite.org/stricttables.html), [WAL](https://www.sqlite.org/wal.html), [códigos de resultado](https://www.sqlite.org/rescode.html), [json1](https://www.sqlite.org/json1.html), [FTS5](https://www.sqlite.org/fts5.html), [transações](https://www.sqlite.org/lang_transaction.html)
- Relatório anterior: [store-sqlite.md](store-sqlite.md) - comparação de bibliotecas e medição do `rusqlite`
