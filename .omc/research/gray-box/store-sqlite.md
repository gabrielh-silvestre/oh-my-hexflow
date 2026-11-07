# Pesquisa de reuso: store e conexão SQLite do hexlog

Consulta feita em 2026-11-07 por `esopo-sonnet_omc-document-specialist_pesquisa-store-sqlite-e`. Pergunta: alguma biblioteca já cobre a lógica das caixas `store` e `store/sqlite` (esquema com `user_version`, mapeamento de `records`, transação `BEGIN IMMEDIATE`, erro de índice único, `json_extract`, paginação)? E o que usar no lugar de `Mutex<rusqlite::Connection>`? Critério de aptidão do dono: no mínimo 1000 estrelas, atualizações recentes e boa saúde (docs, testes, badges). Quem não cumpre é `não apto` e a lógica vira gap interno.

**Atualização de 2026-11-07: o dono escolheu `sqlx` (0.9.0) no lugar de `rusqlite` para o `hexlog`.** O veredito sobre o `rusqlite` abaixo vale como histórico e como base de comparação. As features, o comportamento sem runtime, as armadilhas e a medição do `sqlx` estão em [store-sqlx.md](store-sqlx.md). Duas consequências já conferidas lá: o `sqlx` usa `libsqlite3-sys 0.37.0` e o `rusqlite 0.41.1` usa `0.39.0`, então os dois não convivem no mesmo grafo; e `SqliteConnection` é `Send + Sync`, mas toda consulta exige `&mut`, o que mantém a recomendação de API com `&mut self`.

## Tabela de Conteúdos

- [rusqlite](#rusqlite)
- [rusqlite_migration](#rusqlite_migration)
- [refinery](#refinery)
- [serde_rusqlite](#serde_rusqlite)
- [sea-query e sea-query-rusqlite](#sea-query-e-sea-query-rusqlite)
- [r2d2_sqlite e r2d2](#r2d2_sqlite-e-r2d2)
- [deadpool-sqlite](#deadpool-sqlite)
- [libsql](#libsql)
- [turso](#turso)
- [Medição: alternativas ao Mutex](#medição-alternativas-ao-mutex)
- [Gap interno e fundamentação](#gap-interno-e-fundamentação)
- [Links Úteis](#links-úteis)

Legenda das colunas de saúde, valores conferidos em 2026-11-07: estrelas e licença pela API do GitHub; "CI" é o arquivo de workflow encontrado em `.github/workflows` (existência, não resultado de execução); "badges" é uma contagem por expressão regular no README da raiz (heurística); "docs.rs" é o `status.json` da versão mais recente. Fecho de dependências medido com `cargo tree -e normal` em Rust 1.97.0, edition 2024.

---

## [rusqlite](https://github.com/rusqlite/rusqlite)

### O que é?

Binding ergonômico de SQLite para Rust, síncrono. É a base do hexlog e o único candidato que cobre a abertura, a transação e o mapeamento sem ajuda.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 0.41.1, 2026-10-28 (0.41.0 em 2026-10-24, 0.40.2 em 2026-08-08) |
| último commit | 2026-11-06 |
| downloads recentes | 57.047.884 |
| mantenedores | 2 no crates.io (`thomcc`, `gwenn`) |
| estrelas | 4467 |
| licença | MIT |
| saúde | CI `main.yml`; 7 badges (codecov, docs.rs, crates.io, msrv); docs.rs ok; 177 issues abertas |
| MSRV e edition | 1.89, edition 2024 |
| fecho com `bundled`, `serde_json`, `uuid`, `time` | 22 pacotes, nenhum proibido |
| veredito | **apto, adotar** (superado em 2026-11-07: o dono escolheu `sqlx`, ver [store-sqlx.md](store-sqlx.md)) |

Fatos de código lidos na fonte 0.41.1: `Connection` tem `db: RefCell<InnerConnection>` e `unsafe impl Send for Connection {}` (`src/lib.rs:364-371`), logo é `Send` e não `Sync`. `Transaction::new(conn: &mut Connection, behavior)` empresta a conexão (`src/transaction.rs:107`). `DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` existe (`src/config.rs:28`).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [rusqlite_migration](https://github.com/cljoly/rusqlite_migration)

### O que é?

Biblioteca de migração de esquema que guarda a versão em `PRAGMA user_version`.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 2.6.0, 2026-05-28 |
| último commit | 2026-10-31 (estilo de TOML e bump de dependências) |
| downloads recentes | 1.236.142 |
| mantenedores | 1 (`cljoly`) |
| estrelas | **118** |
| licença | Apache-2.0 |
| saúde | CI `ci.yml`; 7 badges (coveralls, docs.rs, msrv); docs.rs ok; 10 issues abertas, nenhuma de defeito crítico |
| bloqueios técnicos | exige `rusqlite ^0.40`; com `rusqlite 0.41.1` o cargo recusa por `links = "sqlite3"` |
| veredito | **não apto** (118 estrelas, abaixo de 1000) |

Teste que fiz: 16 threads chamando `to_latest` sobre um arquivo novo (`rusqlite 0.40.2`, `busy_timeout` de 5 s, journal padrão, WAL não testado). 4 das 16 falharam com `table already exists`. `DatabaseTooFarAhead` funcionou quando `user_version` era 5 e o código conhecia 1. Ou seja, a recusa de versão maior existe, mas a abertura concorrente de banco novo não é segura, e é justamente o requisito do plano.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [refinery](https://github.com/rust-db/refinery)

### O que é?

Kit de migração SQL para vários bancos, com migrações em arquivos ou embutidas por macro.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 0.10.0, 2026-10-02; `refinery-core` 0.10.1, 2026-10-26 (suporte a rusqlite 0.41) |
| último commit | 2026-11-01 |
| downloads recentes | 3.496.608 |
| mantenedores | 2 (`jxs`, `spacekookie`) |
| estrelas | 1712 |
| licença | MIT (core: MIT OR Apache-2.0) |
| saúde | CI `ci.yml`, `releases.yml`, `stale.yml`; README com 1 badge de CircleCI, provavelmente obsoleto porque o repo usa GitHub Actions; docs.rs ok; 4 issues abertas |
| fecho | 90 pacotes, nenhum proibido (`tokio` só como feature opcional) |
| veredito | **apto pelo critério, descartar por encaixe** |

Motivo do descarte: guarda o histórico na tabela `refinery_schema_history`, não em `user_version`, e o plano da v1 não tem migração alguma. Traria 90 pacotes para resolver uma tabela e uma verificação de versão.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [serde_rusqlite](https://github.com/twistedfall/serde_rusqlite)

### O que é?

Converte linhas do `rusqlite` em structs com `serde`.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 0.44.0, 2026-10-25 |
| último commit | 2026-10-25 |
| downloads recentes | 857.822 |
| mantenedores | 1 (`twistedfall`) |
| estrelas | **102** |
| licença | MIT OR Apache-2.0 no crates.io; Apache-2.0 na API do GitHub |
| saúde | CI `serde_rusqlite.yml`; 4 badges; docs.rs ok; 4 issues abertas (nenhuma crítica) |
| fecho | 13 pacotes, nenhum proibido |
| veredito | **não apto** (102 estrelas) |

Teste: `from_rows` com `rusqlite 0.41.1` e as features `serde_json`, `uuid`, `time` compilou e leu `NULL` e `Some("{}")` corretamente. Limites: o erro é `serde_rusqlite::Error`, distinto de `rusqlite::Error` (issue #34), e UUID em BLOB não é suportado (#17).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [sea-query e sea-query-rusqlite](https://github.com/SeaQL/sea-query)

### O que é?

Construtor de SQL dinâmico. O adaptador `sea-query-rusqlite` vive no mesmo repositório.

### Avaliação

| critério | valor |
|---|---|
| versão e data | `sea-query` 1.0.2, 2026-08-12; `sea-query-rusqlite` 0.8.0, 2026-05-28 |
| último commit | 2026-11-04 |
| downloads recentes | 7.110.535 / 186.918 |
| mantenedores | 2 (`tyt2y3`, `billy1624`) |
| estrelas | 1778 |
| licença | MIT OR Apache-2.0 no crates.io; `NOASSERTION` na API do GitHub (divergência a conferir antes de adotar) |
| saúde | CI `rust.yml`, `diesel.yml`, `release-bot.yml`; 6 badges; docs.rs ok; 112 issues abertas |
| bloqueios técnicos | o adaptador exige `rusqlite ^0.38`, em conflito com 0.41.1; é pesado para SQL fixo |
| fecho | 32 pacotes, nenhum proibido |
| veredito | **apto pelo critério, ficar de olho** (só se o filtro virar dinâmico) |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [r2d2_sqlite e r2d2](https://github.com/ivanceras/r2d2-sqlite)

### O que é?

Pool de conexões SQLite sobre o `r2d2`.

### Avaliação

| critério | valor |
|---|---|
| versão e data | `r2d2_sqlite` 0.35.0, 2026-07-06; `r2d2` 0.8.10, 2022-06-21 (crate estável, mais de 18 meses sem release) |
| último commit | `r2d2-sqlite` 2026-09-05; `r2d2` 2026-10-26 |
| downloads recentes | 2.181.087 / 9.370.051 |
| mantenedores | `r2d2_sqlite`: 3 (`ivanceras`, `showengineer`, `jerebtw`) |
| estrelas | `r2d2-sqlite` **135**; `r2d2` 1655 |
| licença | MIT; `r2d2` Apache-2.0 / MIT |
| saúde | `r2d2-sqlite`: CI `rust.yml`, 3 badges, README de 502 bytes, docs.rs ok, 0 issues abertas. `r2d2`: CircleCI (`.circleci/config.yml` existe), 1 badge, 26 issues abertas |
| bloqueios técnicos | exige `rusqlite ^0.40`; `scheduled-thread-pool` cria threads de fundo |
| fecho | 30 pacotes, nenhum proibido |
| veredito | **não apto** (o adaptador tem 135 estrelas) |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [deadpool-sqlite](https://github.com/deadpool-rs/deadpool)

### O que é?

Pool assíncrono; o crate `deadpool-sqlite` fica no monorepo `deadpool`.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 0.14.0, 2026-08-26 |
| último commit | 2026-09-04 |
| downloads recentes | 200.018 |
| mantenedores | 2 (`Darksonn`, `bikeshedder`) e o time `github:tokio-rs:core` |
| estrelas | 1336 (repositório) |
| licença | MIT OR Apache-2.0 |
| saúde | CI por crate (`deadpool-sqlite.yml` e outros 10); sem README na raiz (README por crate, não verificado); 66 issues abertas |
| bloqueios técnicos | `tokio` no fecho de dependências normais (`deadpool-runtime`); `rusqlite ^0.40` |
| veredito | **apto pelo critério, descartar** (tokio é proibido) |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [libsql](https://github.com/tursodatabase/libsql)

### O que é?

Fork do SQLite (em C) com binding Rust, mantido pela Turso.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 0.10.0, 2026-10-13 |
| último commit | pushed em 2026-10-17 |
| downloads recentes | 845.634 |
| mantenedores | 2 (`glommer`, `penberg`) e o time `github:tursodatabase:publish` |
| estrelas | 17309 |
| licença | MIT |
| saúde | CI `rust.yml` e outros 12; 3 badges; docs.rs ok; 477 issues abertas |
| bloqueios técnicos | `async-trait` e `futures` no fecho (API provavelmente assíncrona, inferido da árvore); double-free no `Drop` corrigido só em 2026-10-13 (#2251) |
| fecho | 50 pacotes com `default-features = false, features = ["core"]` |
| veredito | **apto pelo critério, descartar** (async e fork do motor) |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [turso](https://github.com/tursodatabase/turso)

### O que é?

Reescrita do SQLite em Rust, compatível em arquivo, ainda em 0.x.

### Avaliação

| critério | valor |
|---|---|
| versão e data | 0.8.3, 2026-10-12 (0.9.0-pre.2 existe) |
| último commit | 2026-11-06 |
| downloads recentes | 797.272 |
| mantenedores | 1 no crates.io (`penberg`) |
| estrelas | 25182 |
| licença | MIT |
| saúde | 35 workflows de CI; 14 badges; **docs.rs com `doc_status: false` para 0.8.3**; 1603 issues abertas |
| fecho | 313 pacotes; `async-trait` e `futures` presentes (API provavelmente assíncrona, inferido) |
| veredito | **apto em estrelas, ficar de olho** (0.x, docs.rs sem build, escopo muito maior que o necessário) |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Medição: alternativas ao Mutex

### O que é?

Comparação do custo de reutilizar uma conexão contra abrir e fechar a cada chamada, para decidir entre (i) conexão por chamada, (ii) API com `&mut self`, (iii) pool e (iv) `Mutex<Connection>`.

### Resultado

Ambiente: `rusqlite 0.41.1` com `bundled` (SQLite 3.54.1, `libsqlite3-sys 0.39.1`), WAL, `synchronous=FULL`, `busy_timeout=5000`; i9-7900X (20 threads), 23 GB, WSL2 (kernel 6.18.33.2), ext4 em `/dev/sdd`, arquivo fora de `/mnt`; `rustc 1.97.0`, perfil release. Tabela `records` com 5000 linhas e índice. 500 repetições por caso, tempo em microssegundos. Execuções 4 e 5 (carga do sistema 3), mediana / p95:

| caso | execução 4 | execução 5 |
|---|---|---|
| A) append reutilizando a conexão | 4.678 / 6.230 | 4.802 / 5.956 |
| B1) append abrindo e fechando, sem outra conexão | 16.234 / 19.216 | 16.241 / 22.031 |
| B2) o mesmo, com uma conexão ociosa aberta | 5.707 / 7.181 | 5.594 / 6.837 |
| B3) o mesmo, com `NO_CKPT_ON_CLOSE` | 8.517 / 16.041 | 8.518 / 16.627 |
| C) leitura reutilizando | 16 / 17 | 15 / 15 |
| D1) leitura abrindo e fechando | 354 / 557 | 345 / 911 |
| D2) o mesmo, com conexão ociosa aberta | 119 / 196 | 133 / 420 |
| D3) leitura abrindo e fechando, sem o PRAGMA `journal_mode` | 267 / 839 | 262 / 797 |

As execuções 1 a 3 rodaram com carga de 7 a 13 (outros agentes na máquina): A teve mediana de 5,3 a 9,8 ms, e houve picos de até 51 s por parada de fsync no WSL2. Descartei-as por ruído; só a mediana de A fica no intervalo de 5 a 10 ms.

Leitura dos números:

- O fechamento da última conexão faz o checkpoint do WAL e apaga o arquivo; a documentação do SQLite diz isso (ver Links). Esse é o custo de B1 sobre A: cerca de 11,5 ms por append.
- Com outra conexão aberta (B2), o checkpoint no fechamento não acontece e o custo some.
- `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` (B3) corta para cerca de 3,8 ms acima de A, mas o WAL fica em disco (4,5 MB no fim) e precisa ser relido na abertura seguinte. Não muda o quadro.
- O custo de abrir para leitura é de 0,35 ms (D1) contra 0,015 ms reutilizando (C).

Consequências por alternativa, sem recomendação: (i) deixa o `SqliteStore{caminho, busy_timeout}` `Send + Sync` sem `Mutex` e sem `unsafe`, e custa 3,4 vezes no append. (ii) `&mut self` mantém `Hexlog: Send`, custo igual a A, sem tipo de interior mutável. (iii) pool traz `Arc`, threads de fundo e o adaptador sem versão compatível. (iv) `Mutex` dá `Sync`, mas é um tipo de memória vetado pela regra do dono.

### Como reproduzir?

O código está embutido abaixo. Em qualquer diretório novo, com Rust 1.97:

```bash
mkdir -p bench/src && cd bench
# salvar o Cargo.toml e o src/main.rs abaixo
cargo build --release
mkdir -p ~/bench-run && ./target/release/bench ~/bench-run   # o diretório deve ficar em ext4, fora de /mnt
```

Rodar três vezes e comparar `uptime` antes de cada rodada; descartar rodadas com carga alta. Código usado em 2026-11-07 (também em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/research-store/bench/`):

```toml
[package]
name = "bench"
version = "0.1.0"
edition = "2024"

[dependencies]
rusqlite = { version = "=0.41.1", features = ["bundled"] }
```

```rust
use rusqlite::{Connection, TransactionBehavior, params, config::DbConfig};
use std::time::Instant;

const N: usize = 500;

fn open(path: &str, no_ckpt: bool) -> Connection {
    let c = Connection::open(path).unwrap();
    if no_ckpt {
        c.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true).unwrap();
    }
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.pragma_update(None, "synchronous", "FULL").unwrap();
    c.pragma_update(None, "busy_timeout", 5000).unwrap();
    c
}

fn append(c: &mut Connection, i: i64) {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
    tx.execute(
        "INSERT INTO records(project, process, target, type, data, ts) VALUES (?1,?2,?3,?4,?5,?6)",
        params!["p", "proc", "a.b.c", "decision", format!("{{\"n\":{i},\"pad\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}}"), i],
    ).unwrap();
    tx.commit().unwrap();
}

fn read(c: &Connection, i: i64) {
    let id = (i % 5000) + 1;
    let v: i64 = c.query_row("SELECT seq FROM records WHERE seq = ?1", params![id], |r| r.get(0)).unwrap();
    assert_eq!(v, id);
}

fn stats(label: &str, mut v: Vec<f64>) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = v[v.len() / 2];
    let p95 = v[(v.len() as f64 * 0.95) as usize - 1];
    println!("{label}: median={med:.0}us p95={p95:.0}us min={:.0}us max={:.0}us", v[0], v[v.len()-1]);
}

fn fresh(path: &str) {
    for s in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{path}{s}")); }
    let mut c = open(path, false);
    c.execute_batch("CREATE TABLE records(seq INTEGER PRIMARY KEY, project TEXT, process TEXT, target TEXT, type TEXT, data TEXT, ts INTEGER); CREATE INDEX i1 ON records(project, process, target);").unwrap();
    let tx = c.transaction().unwrap();
    for i in 0..5000 {
        tx.execute("INSERT INTO records(project, process, target, type, data, ts) VALUES ('p','proc','a.b.c','decision','{}',?1)", params![i]).unwrap();
    }
    tx.commit().unwrap();
}

fn wal_size(path: &str) -> u64 { std::fs::metadata(format!("{path}-wal")).map(|m| m.len()).unwrap_or(0) }

fn timed<F: FnMut(i64)>(mut f: F) -> Vec<f64> {
    (0..N as i64).map(|i| { let t = Instant::now(); f(i); t.elapsed().as_secs_f64() * 1e6 }).collect()
}

fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let path = format!("{dir}/bench.db");
    println!("rusqlite 0.41.1 sqlite_version={}", rusqlite::version());

    // A: append, reuse connection
    fresh(&path);
    let mut c = open(&path, false);
    for i in 0..20 { append(&mut c, i); } // warmup
    stats("A append reuse", timed(|i| append(&mut c, i)));
    drop(c);

    // B1: append, open/close each time, no other connection (checkpoint on last close)
    fresh(&path);
    stats("B1 append open/close (ckpt on close)", timed(|i| { let mut c = open(&path, false); append(&mut c, i); }));
    println!("   wal after B1: {} bytes", wal_size(&path));

    // B2: with an idle keeper connection open (no checkpoint at close)
    fresh(&path);
    let keeper = open(&path, false);
    stats("B2 append open/close (keeper open)", timed(|i| { let mut c = open(&path, false); append(&mut c, i); }));
    println!("   wal with keeper: {} bytes", wal_size(&path));
    drop(keeper);

    // B3: NO_CKPT_ON_CLOSE, no keeper
    fresh(&path);
    stats("B3 append open/close (NO_CKPT_ON_CLOSE)", timed(|i| { let mut c = open(&path, true); append(&mut c, i); }));
    println!("   wal after B3: {} bytes", wal_size(&path));

    // C: read reuse
    fresh(&path);
    let c = open(&path, false);
    for i in 0..20 { read(&c, i); }
    stats("C read reuse", timed(|i| read(&c, i)));
    drop(c);

    // D1: read open/close, no keeper
    fresh(&path);
    stats("D1 read open/close (no keeper)", timed(|i| { let c = open(&path, false); read(&c, i); }));
    // D2: with keeper
    let keeper = open(&path, false);
    stats("D2 read open/close (keeper open)", timed(|i| { let c = open(&path, false); read(&c, i); }));
    drop(keeper);
    // D3: open/close without journal_mode pragma (already persistent) -> only 2 pragmas
    stats("D3 read open/close (no journal_mode pragma)", timed(|i| {
        let c = Connection::open(&path).unwrap();
        c.pragma_update(None, "synchronous", "FULL").unwrap();
        c.pragma_update(None, "busy_timeout", 5000).unwrap();
        read(&c, i);
    }));
}
```

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap interno e fundamentação

### O que é?

O que nenhum candidato apto cobre e o hexlog escreve por conta própria, todo com a API do `rusqlite`.

### Como aplicar?

| gap | como implementar | fundamento |
|---|---|---|
| esquema com `user_version` e abertura concorrente de banco novo | `BEGIN IMMEDIATE`, reler `PRAGMA user_version` dentro da transação, criar o esquema e gravar a versão antes do `COMMIT`; quem perde a disputa espera pelo `busy_timeout` e vê a versão já gravada | [lang_transaction](https://www.sqlite.org/lang_transaction.html) (comportamento `IMMEDIATE`), [pragma user_version](https://www.sqlite.org/pragma.html#pragma_user_version), [busy_timeout](https://www.sqlite.org/c3ref/busy_timeout.html); a falha do `rusqlite_migration` acima mostra o que acontece com transação adiada |
| recusa de versão maior | comparar `user_version` com a versão do código e devolver erro tipado se for maior | mesmo `PRAGMA`; o `DatabaseTooFarAhead` do `rusqlite_migration` é o modelo do contrato |
| mapeamento das cerca de 19 colunas | função `row → struct` com `row.get(indice)`; `serde_json::Value`, `Uuid` e `OffsetDateTime` pelas features do `rusqlite` | docs.rs do `rusqlite` (Links) |
| erro de índice único para `FORK_REJECTED` | casar `rusqlite::Error::SqliteFailure` com código estendido `SQLITE_CONSTRAINT_UNIQUE` (2067) | [rescode](https://www.sqlite.org/rescode.html) |
| consulta com `json_extract` e parâmetro ligado | `json_extract(coluna, ?)` com o caminho como parâmetro, nunca concatenado | [json1](https://www.sqlite.org/json1.html) |
| paginação por `seq` | `WHERE seq > ?1 ORDER BY seq LIMIT ?2` (paginação por chave, sem `OFFSET`) | [use-the-index-luke: no-offset](https://use-the-index-luke.com/no-offset) |
| conexão sem `Mutex` | ver a seção de medição | [wal](https://www.sqlite.org/wal.html), código do `rusqlite` 0.41.1 |

Não encontrei paper nem RFC específico para estes itens; a fundamentação é a documentação do SQLite. Não verificado: comportamento do `rusqlite_migration` em modo WAL e a licença efetiva de `sea-query` (crates.io diz MIT OR Apache-2.0, a API do GitHub diz `NOASSERTION`).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Todos consultados em 2026-11-07.

- [crates.io: rusqlite](https://crates.io/crates/rusqlite), [rusqlite_migration](https://crates.io/crates/rusqlite_migration), [refinery](https://crates.io/crates/refinery), [serde_rusqlite](https://crates.io/crates/serde_rusqlite), [sea-query](https://crates.io/crates/sea-query), [sea-query-rusqlite](https://crates.io/crates/sea-query-rusqlite), [r2d2_sqlite](https://crates.io/crates/r2d2_sqlite), [r2d2](https://crates.io/crates/r2d2), [deadpool-sqlite](https://crates.io/crates/deadpool-sqlite), [libsql](https://crates.io/crates/libsql), [turso](https://crates.io/crates/turso) - versões, datas, downloads, donos (API `https://crates.io/api/v1/crates/<nome>`)
- [rusqlite/rusqlite](https://github.com/rusqlite/rusqlite), [cljoly/rusqlite_migration](https://github.com/cljoly/rusqlite_migration), [rust-db/refinery](https://github.com/rust-db/refinery), [twistedfall/serde_rusqlite](https://github.com/twistedfall/serde_rusqlite), [SeaQL/sea-query](https://github.com/SeaQL/sea-query), [ivanceras/r2d2-sqlite](https://github.com/ivanceras/r2d2-sqlite), [sfackler/r2d2](https://github.com/sfackler/r2d2), [deadpool-rs/deadpool](https://github.com/deadpool-rs/deadpool), [tursodatabase/libsql](https://github.com/tursodatabase/libsql), [tursodatabase/turso](https://github.com/tursodatabase/turso) - estrelas, commits, issues, workflows
- [docs.rs: rusqlite 0.41.1 Connection](https://docs.rs/rusqlite/0.41.1/rusqlite/struct.Connection.html) - API de `Connection`; status de build em `https://docs.rs/crate/<nome>/latest/status.json`
- [SQLite: WAL](https://www.sqlite.org/wal.html) - checkpoint e remoção do WAL ao fechar a última conexão
- [SQLite: sqlite3_db_config](https://www.sqlite.org/c3ref/c_dbconfig_defensive.html) - lista `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE`
- [SQLite: transações](https://www.sqlite.org/lang_transaction.html), [PRAGMA](https://www.sqlite.org/pragma.html), [busy_timeout](https://www.sqlite.org/c3ref/busy_timeout.html), [códigos de resultado](https://www.sqlite.org/rescode.html), [json1](https://www.sqlite.org/json1.html)
- [Use The Index, Luke: paginação sem OFFSET](https://use-the-index-luke.com/no-offset)
- Plano do projeto: `/home/gabriel/personal/oh-my-hexflow/.omc/plans/omhl-v2-core-step3.md`
