# Reuso para o grafo versionado do hexlog

Pesquisa feita em 2026-11-07 (rustc 1.97.0) sobre bancos e motores embutíveis que poderiam substituir o miolo do `hexlog`: nodes com id semântico e revisões imutáveis, relations tipadas versionadas, catálogo de tipos com JSON Schema, consulta da revisão vigente, retirada sem apagar histórico e tudo por tenant. Resultado: nenhum candidato cobre o miolo, e a lógica vira gap interno. Os dados de crates.io (downloads dos últimos cerca de 90 dias) e GitHub foram conferidos em 2026-11-07; o que não foi conferido está marcado.

## Tabela de Conteúdos

- [Critérios de avaliação](#critérios-de-avaliação)
- [CozoDB](#cozodb)
- [Oxigraph](#oxigraph)
- [IndraDB](#indradb)
- [Kuzu e LadybugDB](#kuzu-e-ladybugdb)
- [SurrealDB](#surrealdb)
- [Grafeo](#grafeo)
- [GlueSQL](#gluesql)
- [redb e fjall](#redb-e-fjall)
- [sled](#sled)
- [Event stores em Rust](#event-stores-em-rust)
- [Servidores com versionamento](#servidores-com-versionamento)
- [Gap: revisão imutável e concorrência otimista](#gap-revisão-imutável-e-concorrência-otimista)
- [Gap: cadeia de hash JCS](#gap-cadeia-de-hash-jcs)
- [Gap: idempotência por chave](#gap-idempotência-por-chave)
- [Gap: catálogo de tipos com JSON Schema](#gap-catálogo-de-tipos-com-json-schema)
- [Gap: consulta, relation inversa e tenant](#gap-consulta-relation-inversa-e-tenant)
- [Links Úteis](#links-úteis)

---

## Critérios de avaliação

### O que é?
O filtro que decide se uma biblioteca é **apta**: no mínimo 1000 estrelas no GitHub, atualizações relevantes recentes e boa saúde (documentação, testes, badges). Somam-se os critérios técnicos do projeto: licença compatível com MIT ou Apache-2.0, compila em Rust 1.97 edition 2024, API síncrona, fecho de dependências normais sem `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std` e API sem `Rc`, `Arc`, `RefCell` ou `'static` obrigatórios. Mais de 18 meses sem release conta como manutenção ruim, salvo crate estável e terminado.

### Como foi medido?
- Release, licença, `rust-version` e downloads: API do crates.io (`/api/v1/crates/<nome>`), campo `recent_downloads`.
- Fecho de dependências: `cargo tree -e normal` com features padrão, em projeto descartável, e `cargo check` para confirmar que compila no 1.97.
- Estrelas, arquivamento e último push: busca de repositórios do GitHub. Workflows: diretório `.github/workflows`. Badges: `![...](...)` no README. Docs: status do build no docs.rs.
- **Não verificado:** issues críticas abertas e número de contribuidores (a API do GitHub devolveu limite de taxa). "Pessoas" abaixo é só o dono no crates.io, o que subestima a equipe. Badges em HTML puro não foram contados.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [CozoDB](https://github.com/cozodb/cozo)

### O que é?
Banco transacional relacional-grafo com Datalog, embutível, com backends em memória e SQLite. Licença MPL-2.0.

### O que faz?
A coluna `Validity` como última chave de uma relation habilita time travel: `ASSERT` e `RETRACT` gravam o fato com timestamp e `@ "NOW"` lê só o que está vigente. É a ideia mais próxima de "retirada sem apagar histórico". Falta o ponteiro para a revisão anterior, a concorrência otimista por record, o JSON Schema e a cadeia de hash.

### Veredito: não apto
- Estrelas: 4142. Saúde: docs.rs ok, site cozodb.org, 14 badges, um único workflow (`build.yml`).
- Última release 0.7.6 em 2023-12-11 (23 meses), último push em 2024-12-04, 48 issues abertas, dono único (zh217). Downloads recentes: 19.084.
- Fecho sem crates proibidos e compila no 1.97, mas o crate está parado. O fork `cozo-ce` (0.7.12, 2024-12-08, 3.282 downloads) também está parado, e o repositório `cozo-community/cozo` não apareceu na busca do GitHub (não verificado).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Oxigraph](https://github.com/oxigraph/oxigraph)

### O que é?
Banco de grafos RDF com SPARQL, em Rust, API síncrona. Licença MIT OR Apache-2.0.

### O que faz?
Guarda quads (sujeito, predicado, objeto, grafo) e consulta por SPARQL. O `data` JSON de um record viraria literal RDF, sem JSON Schema nativo. Não tem revisão encadeada, concorrência otimista por record nem retirada com histórico.

### Veredito: apto pelo critério, descartado por não servir
- Estrelas: 2025. Saúde: docs.rs ok, 14 badges, `tests.yml` e `nightly.yml`, push em 2026-11-05.
- Release 0.5.13 em 2026-10-22, 288.125 downloads, `rust-version` 1.87. Fecho sem proibidos, `cargo check` passou.
- O padrão puxa `oxrocksdb-sys` (C++). Um build só em memória com `default-features = false` não foi testado.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [IndraDB](https://github.com/indradb/indradb)

### O que é?
Banco de grafos de propriedades em Rust, com datastore em memória ou persistente. Licença MPL-2.0.

### O que faz?
Vértices e arestas tipados com propriedades JSON, consulta por prefixo e aresta inversa. Não guarda revisões imutáveis nem retirada: o versionamento seria todo do `hexlog`.

### Veredito: não apto
- Estrelas: 2470. Saúde: docs.rs ok, 3 badges, `test.yml`.
- Release 5.0.0 e último push em 2025-08-16 (15 meses), dono único (ysimonson), 17 issues abertas, 4.697 downloads de `indradb-lib`. Fecho limpo e compila.
- Sem atualização recente e sem o modelo de revisão.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Kuzu e LadybugDB](https://github.com/kuzudb/kuzu)

### O que é?
Kuzu é um banco de grafos embutido com Cypher, em C++ (MIT). LadybugDB é o fork ativo, e o crate Rust `lbug` é o binding.

### O que faz?
Modela property graph com esquema tipado, parecido com tabelas SQL. Não tem revisão imutável nem histórico. O build exige C++ (`cxx`).

### Veredito: não apto
- Kuzu: 4023 estrelas, mas o repositório está **arquivado** (último push 2025-10-10, release 0.11.3 do mesmo dia).
- `lbug` 0.22.3 (2026-11-06, 960.195 downloads): o repositório do crate (`LadybugDB/ladybug-rust`) tem 4 estrelas, README de 14 bytes, e o build no docs.rs falhou. O repositório C++ `LadybugDB/ladybug` tem 1902 estrelas e push em 2026-11-07, mas o que se adotaria é o binding.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [SurrealDB](https://github.com/surrealdb/surrealdb)

### O que é?
Banco multimodelo (documento e grafo) com namespaces, embutível.

### O que faz?
Cobre grafo, documentos e isolamento por namespace, mas sem revisão encadeada nem concorrência otimista por record no sentido do `hexlog`.

### Veredito: descartado, apto mas bloqueado
- Estrelas: 33289. Saúde: docs.rs ok, cerca de 30 workflows (ci, fuzzing, coverage, codeql, scorecard).
- Release 4.0.0 em 2026-11-03, 587.412 downloads.
- Licença `non-standard` no crates.io e `NOASSERTION` no GitHub, fora de MIT/Apache. `tokio` no fecho e API assíncrona.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Grafeo](https://github.com/GrafeoDB/grafeo)

### O que é?
Banco de grafos em Rust (LPG e RDF), embutível. Apache-2.0.

### O que faz?
Várias linguagens de consulta sobre grafo em memória ou persistente. Sem modelo de revisão.

### Veredito: não apto
- 832 estrelas (abaixo de 1000), repositório criado em 2026-01. Saúde boa: 17 badges, CI, codecov.
- Release 0.6.1 em 2026-11-01, 13.730 downloads, mas `rust-version` 1.99.0 não compila no 1.97.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [GlueSQL](https://github.com/gluesql/gluesql)

### O que é?
Motor SQL em Rust com storages plugáveis. Apache-2.0.

### O que faz?
Dá SQL sobre vários backends. Não traz grafo nem revisão.

### Veredito: descartado, apto mas bloqueado
- Estrelas: 3134. Saúde: `rust.yml` e coverage, 6 badges, mas o build no docs.rs falhou na 0.20.0.
- Release 0.20.0 em 2026-08-30, 60.228 downloads. `tokio` e `async-std` no fecho, API assíncrona.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [redb e fjall](https://github.com/cberner/redb)

### O que é?
Dois KV embutidos em Rust: redb (B-tree, ACID) e fjall (LSM). Ambos MIT OR Apache-2.0.

### O que faz?
Só armazenam chave e valor com transação. Índices, esquema, revisões e cadeia ficam a cargo do `hexlog`. Servem de camada de storage, não de miolo.

### Veredito: redb adotar em parte, fjall ficar de olho
- redb: 4902 estrelas, release 4.5.0 em 2026-11-05, 6.700.798 downloads, `ci.yml`, 5 badges, docs.rs ok, 3 issues abertas, fecho de 8 linhas, compila. É o plano B puramente Rust ao store SQLite.
- fjall ([repositório](https://github.com/fjall-rs/fjall)): 2425 estrelas, release 3.1.13 em 2026-10-25, 759.169 downloads, `test.yml`, docs.rs ok, README sem badges de imagem, 48 issues abertas.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [sled](https://github.com/spacejam/sled)

### O que é?
KV embutido em Rust, MIT OR Apache-2.0.

### O que faz?
Armazena chave e valor, mesmo papel do redb.

### Veredito: não apto
- 9105 estrelas, `test.yml`, docs.rs ok, sem badges. Última release estável 0.34.7 em 2024-10-11 (25 meses), 172 issues abertas. Os downloads (3,69 milhões) vêm de inércia, não de atividade.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Event stores em Rust](https://github.com/serverlesstechnology/cqrs)

### O que é?
Frameworks de event sourcing: `cqrs-es` (Apache-2.0) e `disintegrate` (MIT).

### O que faz?
Guardam eventos por agregado com concorrência otimista por versão. Não modelam grafo, catálogo de tipos nem JSON Schema.

### Veredito: não apto
- cqrs-es: 502 estrelas, release 0.5.0 em 2025-12-30, 25.265 downloads, 3 badges, `tokio` no fecho.
- [disintegrate](https://github.com/disintegrate-es/disintegrate): 124 estrelas, release 4.0.0 em 2026-02-02, 3.244 downloads, 3 badges. Se a API é assíncrona não foi verificado.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Servidores com versionamento](https://github.com/terminusdb/terminusdb)

### O que é?
TerminusDB (Prolog, 3472 estrelas), XTDB (Clojure/JVM, 3095 estrelas, bitemporal), Dolt (Go) e Fossil (C).

### O que faz?
Versionam dados ou código com histórico completo, mas rodam como servidor ou ferramenta externa.

### Veredito: descartados
Não são crates embutíveis em Rust, e dois deles exigem processo à parte. As estrelas de Dolt e Fossil não foram verificadas. Servem só como referência de modelo (git para dados, bitemporalidade).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap: revisão imutável e concorrência otimista

### O que é?
Cada node acumula revisões que apontam a anterior; revisar exige citar o último record. Nenhum candidato oferece isso por record com ponteiro explícito.

### Como aplicar?
A fundamentação para a implementação interna: o controle de concorrência otimista (validar no commit que ninguém gravou depois do que você leu), o padrão event sourcing (estado como sequência imutável de eventos) e a ideia de banco como valor imutável do Datomic. O time travel do Cozo é a alternativa por timestamp, descartada porque o `hexlog` exige o ponteiro para a revisão anterior.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap: cadeia de hash JCS

### O que é?
Cada record entra numa cadeia de hash sobre a serialização canônica (JCS, RFC 8785), para detectar adulteração. Nenhum candidato oferece isso.

### Como aplicar?
O RFC 8785 define a serialização JSON canônica. A cadeia segue a linha de Haber e Stornetta (carimbo de tempo por encadeamento de hashes), das árvores de Merkle do Certificate Transparency (RFC 6962 e RFC 9162) e do modelo de objetos do Git.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap: idempotência por chave

### O que é?
Repetir o mesmo registro com a mesma chave devolve o resultado guardado em vez de gravar de novo. Nenhum candidato cobre isso no nível do `hexlog`.

### Como aplicar?
O rascunho do IETF `Idempotency-Key` descreve o contrato (mesma chave e mesmo corpo repetem a resposta, corpo diferente com a mesma chave é conflito). A documentação da Stripe mostra o mesmo desenho em produção.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap: catálogo de tipos com JSON Schema

### O que é?
Tipos de node e de relation são dado em um catálogo, validados por JSON Schema. Nenhum motor de grafo avaliado valida o `data` por schema versionado.

### Como aplicar?
Usar a especificação JSON Schema 2020-12 como contrato do catálogo. A escolha do crate validador não foi pesquisada nesta frente.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap: consulta, relation inversa e tenant

### O que é?
Consulta da revisão vigente por id, tipo, prefixo e campo de `data`, leitura inversa de relation e isolamento por tenant. IndraDB, Kuzu e SurrealDB cobrem parte (prefixo, aresta inversa, namespace), mas sem o modelo de revisão.

### Como aplicar?
Fica sobre o storage escolhido, com índices próprios. Não há fonte externa específica; a decisão depende do store (SQLite ou redb) definido em outra frente.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Consultados em 2026-11-07. URLs de papers e RFCs marcadas como abertas responderam 200; o DOI da ACM respondeu 403 a robô e não foi aberto.

- [CozoDB (GitHub)](https://github.com/cozodb/cozo) e [time travel](https://docs.cozodb.org/en/latest/timetravel.html), [crate](https://crates.io/crates/cozo), [cozo-ce](https://crates.io/crates/cozo-ce)
- [Oxigraph (GitHub)](https://github.com/oxigraph/oxigraph), [crate](https://crates.io/crates/oxigraph)
- [IndraDB (GitHub)](https://github.com/indradb/indradb), [crate indradb-lib](https://crates.io/crates/indradb-lib)
- [Kuzu (GitHub, arquivado)](https://github.com/kuzudb/kuzu), [crate kuzu](https://crates.io/crates/kuzu), [LadybugDB](https://github.com/LadybugDB/ladybug), [ladybug-rust](https://github.com/LadybugDB/ladybug-rust), [crate lbug](https://crates.io/crates/lbug)
- [SurrealDB (GitHub)](https://github.com/surrealdb/surrealdb), [crate](https://crates.io/crates/surrealdb)
- [Grafeo (GitHub)](https://github.com/GrafeoDB/grafeo), [crate](https://crates.io/crates/grafeo)
- [GlueSQL (GitHub)](https://github.com/gluesql/gluesql), [crate](https://crates.io/crates/gluesql)
- [redb (GitHub)](https://github.com/cberner/redb), [crate](https://crates.io/crates/redb); [fjall (GitHub)](https://github.com/fjall-rs/fjall), [crate](https://crates.io/crates/fjall)
- [sled (GitHub)](https://github.com/spacejam/sled), [crate](https://crates.io/crates/sled)
- [cqrs (GitHub)](https://github.com/serverlesstechnology/cqrs), [crate cqrs-es](https://crates.io/crates/cqrs-es); [disintegrate (GitHub)](https://github.com/disintegrate-es/disintegrate), [crate](https://crates.io/crates/disintegrate)
- [TerminusDB](https://github.com/terminusdb/terminusdb), [XTDB](https://github.com/xtdb/xtdb)
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
- [RFC 6962: Certificate Transparency](https://www.rfc-editor.org/rfc/rfc6962) e [RFC 9162](https://www.rfc-editor.org/rfc/rfc9162)
- [Haber e Stornetta, How to time-stamp a digital document (1991)](https://doi.org/10.1007/BF00196791)
- [Kung e Robinson, On Optimistic Methods for Concurrency Control (1981)](https://doi.org/10.1145/319566.319567), não aberto (403)
- [Martin Fowler, Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html)
- [Rich Hickey, The Database as a Value](https://www.infoq.com/presentations/Datomic-Database-Value/)
- [Git Internals: Git Objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)
- [IETF: The Idempotency-Key HTTP Header Field](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/)
- [Stripe: Idempotent requests](https://docs.stripe.com/api/idempotent_requests)
- [JSON Schema 2020-12 Core](https://json-schema.org/draft/2020-12/json-schema-core)
