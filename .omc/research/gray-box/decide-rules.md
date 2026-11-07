# Pesquisa de reuso: caixas cinza engine/decide e engine/rules

## Tabela de Conteúdos

- [Critérios de aptidão](#critérios-de-aptidão)
- [petgraph](#petgraph)
- [pathfinding](#pathfinding)
- [daggy, incremental-topo e topological-sort](#daggy-incremental-topo-e-topological-sort)
- [cqrs-es e sqlite-es](#cqrs-es-e-sqlite-es)
- [esrs](#esrs)
- [disintegrate](#disintegrate)
- [happenstance-core](#happenstance-core)
- [fmodel-rust](#fmodel-rust)
- [eventually, thalo, mire e es-entity](#eventually-thalo-mire-e-es-entity)
- [Crates de idempotency key](#crates-de-idempotency-key)
- [Gap interno: pipeline de decide e regras](#gap-interno-pipeline-de-decide-e-regras)
- [Links Úteis](#links-úteis)

---

## Critérios de aptidão

### O que é?

Os critérios que um crate precisa cumprir para ser adotado no `engine`. Consulta feita em **2026-11-07** (hoje, na sessão). Rust 1.97.0, edition 2024.

### O que faz?

Separa candidato apto de não apto. Os critérios do dono, em ordem de aplicação:

- No mínimo **1000 estrelas** no GitHub.
- Atualização relevante recente (mais de 18 meses sem release conta como manutenção ruim, salvo crate estável e terminado).
- Saúde: documentação, testes e badges.
- Licença compatível com MIT ou Apache-2.0.
- Compila em Rust 1.97 edition 2024 e tem API síncrona.
- Fecho de dependências normais sem `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std`.
- API que não obrigue o chamador a usar `Rc`, `Arc`, `RefCell` ou `'static`.

### Como foi medido?

- Downloads: campo `recent_downloads` da API do crates.io (janela de 90 dias).
- Fecho: um crate descartável por candidato em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/research-decide/p_<nome>/`, com `cargo tree -e normal --prefix none | sort -u` e `cargo check`. O número do fecho conta crates únicos, features padrão.
- Estrelas, commits e arquivos de CI: tools MCP do GitHub (`search_repositories`, `list_commits`, `get_file_contents`). A API REST sem autenticação respondeu 403, então **número de pessoas que mantêm e issues críticas abertas não foram verificados**; só há o total de issues+PRs abertos.
- docs.rs: `status.json` de cada crate, build ok na última versão para todos.
- Testes: contagem de `#[test]` no código baixado, só para os dois crates que passaram em estrelas.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [petgraph](https://crates.io/crates/petgraph)

### O que é?

Biblioteca de grafos do ecossistema Rust. Versão 0.8.3, repositório [petgraph/petgraph](https://github.com/petgraph/petgraph).

### O que faz?

Oferece `DiGraphMap`, `algo::has_path_connecting` e `algo::is_cyclic_directed`. Para o ciclo do `decide`: adicionar a aresta `from -> to` fecha ciclo se já existe caminho `to -> from` (ou se `from == to`).

### Avaliação

| critério | valor |
|---|---|
| estrelas | 4055 |
| última release | 2025-09-30 |
| último commit | 2026-10-31 (Raoul Luqué, testes e CI) |
| downloads recentes | 140,8 mi |
| licença | MIT OR Apache-2.0 |
| MSRV declarado | 1.64 |
| fecho (features padrão) | 8 crates: `indexmap`, `hashbrown` (duas versões), `fixedbitset`, `foldhash`, `equivalent` |
| bloqueios | nenhum; compila em 1.97 / edition 2024 |
| saúde | docs.rs ok; README, CHANGELOG e CONTRIBUTING; CI com 5 workflows (`ci`, `merge-check`, `semver-checks`, `release-plz`, `pr-title`); 378 `#[test]` e 25 arquivos em `tests/`; 5 badges (CI, crates.io, docs.rs, rustc 1.64+, discord); 307 itens abertos (issues+PRs, sem triagem de severidade) |

**Veredito: apto, adotar em parte (só o ciclo).** Montei a mesma checagem com `DiGraphMap<&str, ()>::from_edges` + `has_path_connecting(&g, to, from, None)` e com um DFS escrito à mão (cerca de 14 linhas, `HashMap` + `HashSet` + pilha). Um teste com os três casos (caminho existe, caminho não existe, auto-aresta) passou nas duas. A petgraph não evita montar o grafo a cada chamada: relations vivas do tenant, nativas e itens do lote entram como lista de arestas nos dois casos, o custo é O(arestas). O que a petgraph dá a mais é `is_cyclic_directed`, que o `decide` não usa porque o grafo gravado já é acíclico por invariante. Recomendação: DFS próprio; petgraph é a alternativa aceitável, decisão do dono.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [pathfinding](https://crates.io/crates/pathfinding)

### O que é?

Algoritmos de caminho mínimo, fluxo e componentes fortemente conexas. Versão 4.16.0, repositório [evenfurther/pathfinding](https://github.com/evenfurther/pathfinding).

### O que faz?

Resolve caminho mínimo, SCC e ordenação topológica. Para ciclo em grafo aciclíclico, é mais do que o necessário.

### Avaliação

| critério | valor |
|---|---|
| estrelas | 1085 |
| última release | 2026-09-07 |
| último commit | 2026-10-23 (renovate) |
| downloads recentes | 630 mil |
| licença | Apache-2.0 / MIT |
| MSRV declarado | 1.88.0 |
| fecho | 19 crates; nenhum proibido |
| saúde | docs.rs ok; README e CHANGELOG, sem CONTRIBUTING; CI `tests.yml`, `pre-commit`, `zizmor`, `iai-callgrind`; 238 `#[test]` e 39 arquivos em `tests/`; 3 badges (crates.io versão, licença, docs.rs), sem badge de CI; 28 itens abertos |

**Veredito: apto pelo critério, descartar.** Fecho de 19 crates sem ganho sobre o DFS de 14 linhas.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## daggy, incremental-topo e topological-sort

### O que é?

Três crates menores de grafo acíclico. [daggy 0.9.0](https://crates.io/crates/daggy) (wrapper de petgraph que recusa aresta que fecha ciclo), [incremental-topo 0.3.1](https://crates.io/crates/incremental-topo) (ordem topológica incremental) e [topological-sort 0.3.1](https://crates.io/crates/topological-sort) (ordenação por contagem de dependências).

### O que faz?

Detectam ciclo no momento de inserir a aresta (daggy, incremental-topo) ou ao esvaziar o grafo (topological-sort). Os dois primeiros exigem manter um grafo persistente na memória; o `engine` reconstrói o grafo a cada chamada, então a vantagem incremental não existe.

### Avaliação

| crate | estrelas | última release | último commit | downloads | licença | fecho | veredito |
|---|---|---|---|---|---|---|---|
| daggy | 249 | 2025-04-17 (~19 meses) | 2025-04-17 | 2,2 mi | MIT/Apache-2.0 | 9 crates | não apto |
| incremental-topo | 19 | 2025-03-29 (~19 meses) | 2025-05-20 (dependabot) | 4,5 mil | MIT/Apache-2.0 | 6 crates | não apto |
| topological-sort | 19 | 2026-08-07 | 2026-11-06 (renovate) | 9,2 mi | MIT/Apache-2.0 | 2 crates | não apto |

Saúde não conferida: já reprovam pelas estrelas. Se o dono relaxar o critério, `daggy` e `incremental-topo` ainda têm mais de 18 meses sem release e não consegui dizer se são estáveis e terminados; `topological-sort` não tem consulta de caminho.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [cqrs-es e sqlite-es](https://crates.io/crates/cqrs-es)

### O que é?

Framework de CQRS e event sourcing ([serverlesstechnology/cqrs](https://github.com/serverlesstechnology/cqrs)) e o store SQLite dele ([johnbcodes/sqlite-es](https://github.com/johnbcodes/sqlite-es)). cqrs-es 0.5.0, sqlite-es 0.5.0.

### O que faz?

`cqrs-es` define a trait `Aggregate: Default + Serialize + DeserializeOwned + Sync + Send` com `const TYPE: &'static str`, `type Services: Send + Sync` e um `handle` async. `sqlite-es` persiste em SQLite via `sqlx`.

### Avaliação

| crate | estrelas | última release | downloads | licença | fecho | bloqueios |
|---|---|---|---|---|---|---|
| cqrs-es | 502 | 2025-12-30 | 25 mil | Apache-2.0 | 22 crates | `async-trait`, `tokio`; 84 `async fn`; 21 usos de `Arc`/`Rc`/`RefCell`/`'static` no código |
| sqlite-es | 18 | 2026-04-23 | 689 | Apache-2.0 | 161 crates | `sqlx`, `tokio`, `futures`, `async-trait` |

**Veredito: não apto** (estrelas) e impõe modelo de agregado, async e tokio.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [esrs](https://crates.io/crates/esrs)

### O que é?

Framework de event sourcing da Prima ([primait/event_sourcing.rs](https://github.com/primait/event_sourcing.rs)). Versão 0.18.0.

### Avaliação

| critério | valor |
|---|---|
| estrelas | 88 |
| última release | 2024-11-25 (~23 meses) |
| último commit | 2026-10-27 (imagens Docker) |
| downloads recentes | 9 mil |
| licença | MIT OR Apache-2.0 |
| fecho | 62 crates, com `tokio`, `futures`, `async-trait` (igual com `default-features = false`) |

**Veredito: não apto** (estrelas, mais de 18 meses sem release, async).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [disintegrate](https://crates.io/crates/disintegrate)

### O que é?

Biblioteca de event sourcing com Dynamic Consistency Boundary (DCB), [disintegrate-es/disintegrate](https://github.com/disintegrate-es/disintegrate). Versão 4.0.0.

### O que faz?

A condição de append é uma query sobre eventos, não um agregado. Conceitualmente parecida com o `base` do hexlog (citar o último record; divergência dá `FORK_REJECTED`), mas o hexlog usa linhagem por record, não query.

### Avaliação

| critério | valor |
|---|---|
| estrelas | 124 |
| última release | 2026-02-02 |
| último commit | 2026-08-03 (dependabot) |
| downloads recentes | 3,2 mil |
| licença | MIT |
| fecho | 37 crates, com `futures`, `async-trait`; backend Postgres (`tonic` aparece nos commits recentes do repo, não no fecho medido) |

**Veredito: não apto** (estrelas, async).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [happenstance-core](https://crates.io/crates/happenstance-core)

### O que é?

Contrato de event store com DCB: portas `EventStore` e `ProjectionStore`, repositório [Wet-Ink-Corporation/happenstance](https://github.com/Wet-Ink-Corporation/happenstance). Versão 0.4.6.

### O que faz?

Reporta violação de condição de append por `AppendError::ConditionViolated`, separada do erro do adaptador. É o desenho mais próximo das portas (traits) que o `engine` precisa, mas é async (`trait-variant`) e jovem (repo criado em 2026-08-05).

### Avaliação

| critério | valor |
|---|---|
| estrelas | 1 |
| última release | 2026-11-02 |
| downloads recentes | 3,7 mil |
| licença | não verificada |
| fecho | 14 crates, com `futures-core`; 66 `async fn`; 35 usos de `Arc`/`Rc`/`'static` no código |

**Veredito: não apto.** Serve só como leitura de desenho do `base`.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [fmodel-rust](https://crates.io/crates/fmodel-rust)

### O que é?

Biblioteca de modelagem funcional de domínio ([fraktalio/fmodel-rust](https://github.com/fraktalio/fmodel-rust)). Versão 0.9.1.

### O que faz?

A struct `Decider<'a, C, S, E, Error>` guarda três funções (`decide`, `evolve`, `initial_state`). O núcleo é síncrono e sem `'static` (0 ocorrências de `Arc`/`Rc`/`RefCell`/`'static` no código), mas não tem idempotência, `base` nem lote atômico; os 72 `async fn` ficam nos módulos de agregado.

### Avaliação

| critério | valor |
|---|---|
| estrelas | 64 |
| última release | 2025-09-14 |
| downloads recentes | 1,1 mil |
| licença | Apache-2.0 |
| fecho | 14 crates, sem os proibidos |

**Veredito: não apto.** É só o formato `(estado, comando) -> eventos`, que o `decide` já tem.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## eventually, thalo, mire e es-entity

### O que é?

Quatro frameworks de event sourcing com store embutido ou Postgres.

### Avaliação

| crate | repositório | estrelas | última release | downloads | licença | fecho | veredito |
|---|---|---|---|---|---|---|---|
| [eventually 0.4.0](https://crates.io/crates/eventually) | [get-eventually/eventually-rs](https://github.com/get-eventually/eventually-rs) | 600 | 2020-10-04 | 83 | MIT | 44 crates, com `futures`, `tokio` | não apto |
| [thalo 0.8.0](https://crates.io/crates/thalo) | [thalo-rs/thalo](https://github.com/thalo-rs/thalo) | 677 | 2023-11-21 | 316 | Apache-2.0 OR MIT | 53 crates, sem os proibidos; API não inspecionada | não apto |
| [mire 0.4.0](https://crates.io/crates/mire) | [git.kjuulh.io/kjuulh/mire](https://git.kjuulh.io/kjuulh/mire) (fora do GitHub) | não verificada | 2026-09-26 | 5,8 mil | não verificada | 127 crates, com `sqlx`, `tokio` | não apto |
| [es-entity 0.20.4](https://crates.io/crates/es-entity) | [GaloyMoney/es-entity](https://github.com/GaloyMoney/es-entity) | 24 | 2026-11-05 | 21,7 mil | não verificada | 225 crates, com `sqlx`, `tokio` | não apto |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Crates de idempotency key

### O que é?

Biblioteca síncrona que calcule a impressão (fingerprint) de um lote e decida replay ou conflito pela chave.

### O que faz?

Nada encontrado. A busca por "idempotency key" no crates.io retornou ruído (ORMs, SDKs de terceiros, bancos). Não esgotei o catálogo; busca por outros nomes (`idempotent`, `dedup`) não foi feita. Tratar como gap interno.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap interno: pipeline de decide e regras

### O que é?

O que sobra sem biblioteca apta. Fica dentro de `engine/decide` e `engine/rules`, falando com o `store` e o `catalog` só por portas (traits), conforme o plano ([omhl-v2-core-step3.md](../../plans/omhl-v2-core-step3.md), linhas 526-557):

- Forma do lote (1 a 50 itens) e `key` válida.
- Replay ou conflito por `key`, com impressão `sha256hex(JCS(itens de entrada))`, antes de qualquer regra e de novo dentro de `BEGIN IMMEDIATE`.
- Regras estáticas (tipos no catálogo, schema, tamanho de `data`, `snapshot`, `external_id`).
- Regras dependentes do histórico, dentro da transação exclusiva: lineage e `base` (`FORK_REJECTED`), extremos de relation, tipos permitidos, ao menos um extremo no tenant, relation entre nativos recusada, `acyclic`.
- Ciclo: DFS sobre relations vivas do tenant, nativas e itens do lote ainda não gravados (a petgraph é opção aceitável, ver seção própria).

### Em que se apoiar?

Fundamentação para a implementação interna (nenhuma é biblioteca):

- **Impressão do lote.** Canonização JSON: [RFC 8785, JCS](https://www.rfc-editor.org/rfc/rfc8785). Hash: [FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final) (SHA-256). Semântica de chave de idempotência (mesma chave e mesma carga devolve o resultado guardado, carga diferente é conflito): [draft-ietf-httpapi-idempotency-key-header](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/) (o estado atual do draft não foi conferido) e a [documentação de idempotência da Stripe](https://docs.stripe.com/api/idempotent_requests).
- **Concorrência otimista por `base`.** H. T. Kung e J. T. Robinson, "On Optimistic Methods for Concurrency Control", ACM TODS 6(2), 1981, [doi:10.1145/319566.319567](https://doi.org/10.1145/319566.319567) (o DOI respondeu 403 a robôs; o conteúdo não foi lido). Variante por consulta de eventos: [Dynamic Consistency Boundary](https://dcb.events/) e [o texto de Sara Pellegrini](https://sara.event-thinking.io/2023/04/kill-aggregate-chapter-1-I-am-here-to-kill-the-aggregate.html).
- **Transação exclusiva e releitura sob lock.** [SQLite, BEGIN IMMEDIATE](https://www.sqlite.org/lang_transaction.html).
- **Forma da decisão.** [Event Sourcing, Martin Fowler](https://martinfowler.com/eaaDev/EventSourcing.html) e [Functional Event Sourcing Decider, Jérémie Chassaing](https://thinkbeforecoding.com/post/2021/12/17/functional-event-sourcing-decider): `decide` como função pura de comando e estado para eventos.
- **Ciclo.** Um grafo dirigido é acíclico se e somente se a busca em profundidade não encontra aresta de retorno; fonte clássica: R. Tarjan, "Depth-First Search and Linear Graph Algorithms", SIAM J. Comput. 1(2), 1972, [doi:10.1137/0201010](https://doi.org/10.1137/0201010) (DOI respondeu 403 a robôs; conteúdo não lido), e o capítulo de busca em profundidade de Cormen et al., *Introduction to Algorithms* (cap. 22 na 3ª edição, cap. 20 na 4ª; numeração não conferida). Para auto-aresta e aresta que fecha ciclo, a checagem é `from == to` ou existência de caminho `to -> from`, validada contra [`has_path_connecting`](https://docs.rs/petgraph/0.8.3/petgraph/algo/fn.has_path_connecting.html).

### Recomendação

Implementar `decide` e `rules` internamente. Para o ciclo, DFS próprio de cerca de 14 linhas (recomendado) ou petgraph (aceitável, 8 crates). Os crates DCB servem de leitura de desenho do `base`, não de dependência.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Consultados em 2026-11-07 (os dois DOIs responderam 403 a robôs; os demais, 200).

- [petgraph no crates.io](https://crates.io/crates/petgraph) e [no GitHub](https://github.com/petgraph/petgraph)
- [pathfinding no crates.io](https://crates.io/crates/pathfinding) e [no GitHub](https://github.com/evenfurther/pathfinding)
- [daggy](https://crates.io/crates/daggy), [incremental-topo](https://crates.io/crates/incremental-topo), [topological-sort](https://crates.io/crates/topological-sort)
- [cqrs-es](https://crates.io/crates/cqrs-es), [sqlite-es](https://crates.io/crates/sqlite-es), [esrs](https://crates.io/crates/esrs), [disintegrate](https://crates.io/crates/disintegrate)
- [happenstance-core](https://crates.io/crates/happenstance-core), [fmodel-rust](https://crates.io/crates/fmodel-rust), [eventually](https://crates.io/crates/eventually), [thalo](https://crates.io/crates/thalo), [mire](https://crates.io/crates/mire), [es-entity](https://crates.io/crates/es-entity)
- [RFC 8785, JCS](https://www.rfc-editor.org/rfc/rfc8785)
- [FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final)
- [draft-ietf-httpapi-idempotency-key-header](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/)
- [Stripe, idempotent requests](https://docs.stripe.com/api/idempotent_requests)
- [SQLite, BEGIN IMMEDIATE](https://www.sqlite.org/lang_transaction.html)
- [Dynamic Consistency Boundary](https://dcb.events/)
- [Sara Pellegrini, Kill Aggregate](https://sara.event-thinking.io/2023/04/kill-aggregate-chapter-1-I-am-here-to-kill-the-aggregate.html)
- [Martin Fowler, Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html)
- [Jérémie Chassaing, Functional Event Sourcing Decider](https://thinkbeforecoding.com/post/2021/12/17/functional-event-sourcing-decider)
- [Kung e Robinson, 1981](https://doi.org/10.1145/319566.319567)
- [Tarjan, 1972](https://doi.org/10.1137/0201010)
- [docs.rs, has_path_connecting](https://docs.rs/petgraph/0.8.3/petgraph/algo/fn.has_path_connecting.html)
