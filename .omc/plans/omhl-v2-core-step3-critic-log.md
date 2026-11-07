# Registro do crítico: decisões técnicas do passo 3

Cadeira do crítico pedida pelo dono durante a execução do plano [omhl-v2-core-step3.md](omhl-v2-core-step3.md); regras em [step3-core-autopilot.md](../handoffs/step3-core-autopilot.md). O crítico faz as perguntas puramente técnicas e registra aqui cada decisão tomada. Pergunta que não é técnica (produto, prioridade, escopo) vai ao dono.

Formato de cada registro:

- **Pergunta:** o que estava em aberto.
- **Alternativas:** as opções reais.
- **Interferência no comportamento final:** o que muda no resultado com cada alternativa.
- **Escolhida e por quê:** a opção e o motivo.
- **Método:** como se concluiu que era a melhor (medição, experimento de compilação, teste, leitura da fonte), com a evidência.

## Registros

### D-0: guarda mecânica da regra de memória (clippy deny + `clippy.toml` + `#[expect]`) — mantida, com três acréscimos

- **Pergunta:** a guarda decidida pela sessão principal fecha a regra do dono, ou deixa passar algo que o dono acredita estar vetado?
- **Alternativas:**
  1. Só revisão humana e `grep` (descartada pela sessão principal; mantenho o descarte).
  2. Guarda do D-0 como está (`[workspace.lints.clippy]` em `deny` + `clippy.toml` + `#[expect(... reason = "memory-exception: ...")]`).
  3. D-0 mais `clippy::allow_attributes = "deny"` e `allow_attributes_without_reason = "deny"`.
  4. D-0 com `disallowed_types = "forbid"`.
  5. D-0 mais um varredor de atributos no `tests/architecture.rs` (token a token, como os outros varredores do plano).
- **Interferência no comportamento final:**
  - (2) pega o tipo vetado escrito no código, inclusive por alias (`use Rc as R`) e por `core::mem::forget`; **não pega** quem escreve `#[allow(clippy::disallowed_types)]` (o `allow` local vence o `deny` do workspace) nem um `#[expect]` com motivo qualquer.
  - (3) pega o `#[allow]` externo (mesmo com `reason`) e o interno sem `reason`, mas **o `#![allow(clippy::disallowed_types, reason = "x")]` interno passa**.
  - (4) fecha o `allow`, mas também recusa o `#[expect]` (erro E0453): a exceção registrada deixa de ser possível por código, contrariando a regra do dono (exceção aprovada entra no local).
  - (5) é o único que fecha os três buracos (`allow` externo, `allow` interno com `reason`, `expect` sem o prefixo `memory-exception:`).
- **Escolhida e por quê:** (2) + (5). Cada buraco abaixo foi reproduzido; o (5) é o único que os fecha sem matar o `#[expect]`. Sem o (5), a regra "exceção só com aprovação do dono" depende de quem lê o diff.
- **Método (experimento em `.../scratchpad/critic/d0`, workspace espelho do real: `clippy.toml` e `Cargo.toml` copiados da raiz, três membros `a`, `b`, `c`):**
  - `cargo clippy --offline -p a -- -D warnings`: `Rc` em membro com `[lints] workspace = true` dá 2 erros `disallowed_types`. Confirma que o `clippy.toml` da raiz do workspace vale para os membros em `crates/*`.
  - `-p b` (membro **sem** `[lints] workspace = true`): o `Rc` ainda dá erro, porque `disallowed_types`/`disallowed_methods` são lints `style` (warn por padrão) e o `-D warnings` os promove. Mas `fn f(s: String) -> usize { s.len() }` em `b` passa sem erro e em `a` falha com `needless_pass_by_value`: os seis lints pedantic/nursery/restriction do D-0 só valem com o opt-in. **Buraco 1:** um membro novo sem `[lints] workspace = true` perde seis dos oito lints.
  - `-p c`: `#[allow(clippy::disallowed_types)]` em `Rc` compila limpo (**buraco 2**); `#[expect(clippy::disallowed_types, reason = "porque sim")]` em `Arc` compila limpo (**buraco 3**).
  - Com `allow_attributes`/`allow_attributes_without_reason` em `deny`: `#[allow(...)]` externo dá erro `#[allow] attribute found`; `#![allow(clippy::disallowed_types)]` interno sem motivo dá erro; **`#![allow(clippy::disallowed_types, reason = "inner com motivo")]` compila limpo** (confirmado em módulo `n`).
  - Com `disallowed_types = "forbid"`: `error[E0453]: expect(clippy::disallowed_types) incompatible with previous forbid`, além do mesmo erro para `allow`.
  - Passam sem erro, por não estarem na lista: `AtomicU64`/`static`, `std::sync::mpsc`, `Once`, `Condvar`, `Box::into_raw` (vaza sem `unsafe`). Pegos: alias `use std::rc::Rc as R`, `core::mem::forget`, `core::mem::ManuallyDrop`, `Cell` dentro de `thread_local!`.
  - `.clone()` usado para calar o compilador (`let t = s.clone(); t.len() + s.len()`) **não** é pego por `redundant_clone`, que só vê clone cujo original não é mais usado. `to_owned` desnecessário e `&String` são pegos (lints `perf`/`style` padrão).
- **O que muda no plano (para a sessão principal aplicar):**
  1. `architecture.rs` ganha uma checagem de que todo `crates/*/Cargo.toml` tem `[lints]` com `workspace = true`.
  2. O varredor de atributos: qualquer `allow`/`warn`/`forbid` que cite `clippy::disallowed_types`, `disallowed_methods`, `mem_forget`, `redundant_clone`, `implicit_clone`, `needless_pass_by_value`, `needless_pass_by_ref_mut` ou `large_types_passed_by_value` é acusado; `expect` desses lints só passa com `reason` começando por `memory-exception:`. Cabe como caso do varredor que já existe (`it_should_flag_forbidden_words_...`) ou como 9º teste de `architecture`; a escolha muda a contagem (8 testes), então é decisão da sessão principal/dono, não técnica.
  3. Limite que fica registrado, sem guarda mecânica: clone que cala o compilador e átomos/`static`/`mpsc` ficam por conta da revisão (`code-simplifier` com `verifier-standards`). Tipos de crates de terceiros (por exemplo `parking_lot::Mutex`) não estão na lista; o que os barra é a lista fechada de dependências com versão `=` (inferência, não testei com crate de terceiros).

### D-1: como `Hexlog` recebe suas dependências — `Deps<'d>` com referências (`&'d dyn Clock/IdGenerator/NativeNodes`), `Catalog` dono, `Hexlog<'d>` sem `Mutex` e sem `Arc`

- **Pergunta:** `Arc<dyn ..>` (plano), referências com lifetime, `Box<dyn ..>` ou genéricos para `Clock`, `IdGenerator`, `NativeNodes` (e `Catalog`) em `Deps`; mais `open(config: Config | &Config)`, `append(req: AppendRequest | &AppendRequest)` e o estado do fake de `IdGenerator`.
- **Descoberta que reordena a pergunta:** `Send + Sync` de `Hexlog` **não depende de como as dependências entram**. O que o impede é a `rusqlite::Connection` (contém `RefCell`): em B, C e D o erro é o mesmo, `E0277 RefCell<InnerConnection> cannot be shared between threads safely`. Só `Mutex<Connection>` dá `Sync`, e `Mutex` está vetado pela regra do dono e por `clippy.toml`. Então o plano atual (A) só fecha com uma exceção de memória aprovada. A decisão real tem dois eixos: (1) `Hexlog` é `Sync`? (2) como entram as dependências?
- **Alternativas (eixo 1):**
  - 1a. `Mutex<Connection>`, `Hexlog: Send + Sync`, `Arc<Hexlog>` entre threads (plano). Exige exceção registrada na seção 16 e aprovação do dono.
  - 1b. `Connection` dona direta, `Hexlog: Send`, não `Sync`; escrita por `&mut self`, leitura por `&self`; uma instância por thread e por processo.
- **Alternativas (eixo 2):** A `Arc<dyn>`; B `Deps<'d>` com `&'d dyn` (e `catalog` dono ou `&'d Catalog`); C `Box<dyn>`; D genéricos `Hexlog<C, I, N>`.
- **Interferência no comportamento final (medida):**

| | A Arc+Mutex | B `&'d dyn` | C `Box<dyn>` | D genéricos |
|---|---|---|---|---|
| `clippy -D warnings` (regras do D-0) | **7 erros** (`Arc` x5, `Mutex` x2), só no código de produção; o fake `Arc<Mutex<Vec>>` do teste dá mais 6 | limpo (lib, testes, exemplos) | limpo | limpo |
| `Hexlog: Send` | sim | sim | sim | sim |
| `Hexlog: Sync` | sim (pelo `Mutex`) | não, `E0277` | não, `E0277` | não, `E0277` |
| N threads, uma instância cada | passa | passa (8 threads, `thread::scope`, fixture compartilhada por `&`) | passa, **mas** ver linha abaixo | passa, idem |
| um `IdGenerator` fake serve várias instâncias abertas ao mesmo tempo | sim (`Arc`) | **sim** (`&SeqIds`, teste `it_should_serve_two_instances_at_once_with_one_ids_fake` verde) | **não**: cada instância é dona do seu gerador; com contador igual nas duas, `UNIQUE constraint failed: r.rid` (reproduzido em `it_should_run_n_threads_each_with_its_own_instance` e `..._swap_by_close_and_reopen`); só passa dando início distinto por instância (`from_start(i * 1000)`) | idem C |
| teste troca o `NativeNodes` fake no meio | precisa de `Arc<Mutex<Vec>>` no fake: proibido pelo próprio D-0 | `fx.native.ids.push(..)` com `&mut`, **depois de largar o `Hexlog`**; com o `Hexlog` aberto o compilador recusa, `E0502` | o fake foi movido para dentro; só reabrindo com fake novo, copiando a lista | idem C |
| `thread::spawn` (exige `'static`) | sim | `E0597` se os donos são locais; **sim** com `b_own` e referências a unit struct sem estado (`&SystemClock`, promoção de constante, sem `Box::leak`): `fn make() -> Hexlog<'static>` compila e roda | sim | sim |
| tipos no `main` do `omhl` | `Arc::new(..)` x3 | `&clock, &ids, &native` | `Box::new(..)` x3 | `Hexlog<SystemClock, UuidV7, EmbeddedNative>` em toda assinatura que guarda a instância |

- **Escolhida e por quê:** eixo 1: **1b**. Eixo 2: **B com `catalog` dono** (`Deps<'d> { clock: &'d dyn Clock, ids: &'d dyn IdGenerator, catalog: Catalog, native: &'d dyn NativeNodes }`, `Hexlog<'d>`). Razões:
  1. 1b elimina `Arc` e `Mutex` sem exceção. Os consumidores previstos (CLI, hook, MCP) são processos distintos, cada um com instância e conexão próprias; nenhum precisa compartilhar uma instância entre threads. O compilador passa a impedir o que o `Mutex` só serializava.
  2. B é a única opção em que um fake de `IdGenerator` serve a várias instâncias simultâneas (testes de concorrência e idempotência do plano abrem duas instâncias no mesmo banco). Em C e D isso quebra por colisão de id técnico, e o contorno é um parâmetro de partida por instância no fake.
  3. B é a única em que o teste mantém a posse do fake e o altera por `&mut` entre duas aberturas, com o compilador provando que a instância antiga já foi largada. Isso reproduz a atualização real do plugin (processo novo com catálogo/nativos novos).
  4. `catalog` dono e não `&'d Catalog`: com dono, `Hexlog<'static>` é possível (donos sem estado viram `&'static` por promoção de constante), então um `omhl` com tokio/MCP pode mover o `Hexlog` para uma thread dona (ator com `std::sync::mpsc`, `examples/actor.rs`, 4 requisições concorrentes, sem `Arc` nem `Mutex`, clippy limpo). Com `&'d Catalog` isso não compila sem `Box::leak`. O custo do dono é reconstruir o `Catalog` por instância nos testes de concorrência; `jsonschema::Validator` é `Send + Sync` (a própria crate afirma em `validator.rs:1579`), então nada impede o compartilhamento se o dono preferir `&'d Catalog` no futuro, ao custo de perder o `'static`.
  5. D foi derrotado por ergonomia (três parâmetros de tipo vazam para o `omhl`) e por perder os dois ganhos acima; C por perder os dois ganhos; A pelo D-0 e pelo fake com `Mutex`.
- **Ataques tentados antes de registrar:** (i) "`Hexlog<'d>` não vai a runtime assíncrona": refutado por `b_own_static_ok` e pelo ator; resta a premissa de que o `NativeNodes` real do passo 4 seja um `static`/const sem estado (um `static` com dados const não é vetado; `OnceLock`/`LazyLock` são), que fica como verificação do passo 4. (ii) "sem `Sync` o MCP concorrente não funciona": o ator resolve sem exceção; só um framework que exija handler `Sync` com estado mutável compartilhado voltaria a pedir exceção, e aí é decisão futura do dono no `omhl`. (iii) "o lifetime contamina": `McpServer<'d> { hx: Hexlog<'d> }` compila e o `main` fica com os donos no mesmo escopo (`examples/omhl_main.rs` roda). (iv) "struct autorreferente em `tests/support`": só existe se a fixture guardar o `Hexlog`; guardando só os donos e devolvendo `Hexlog` por `fn open(&self, ..) -> Hexlog<'_>` não há autorreferência.
- **Corolários:**
  - **`open(config: &Config, ..)`**. Com o corpo do plano (lê `db_path`, copia `limits` e `busy_timeout`), `open(config: Config)` dá `needless_pass_by_value` (reproduzido); só passa se o `Hexlog` guardar o `PathBuf` por move, o que ele não precisa. Além do lint, os testes reabrem N vezes e em várias threads com o mesmo `Config`; por valor exigiria `Config: Clone` e um clone por abertura.
  - **`append(&mut self, req: &AppendRequest)`**. Por valor com corpo que só lê (fingerprint, validação, serialização) dá `needless_pass_by_value` (reproduzido); por valor só passa consumindo os itens (movendo `String`s para o record), e então o chamador não repete a mesma requisição depois de um erro (`E0382 use of moved value`, reproduzido), justamente o caso do replay por `key`, que é recurso central. Custo da referência: clonar duas `String` por item ao montar o record. Medido: 6,1 µs para 50 itens contra 6,6 µs consumindo (já contando reconstruir a requisição); uma transação `BEGIN IMMEDIATE`+insert+commit mediu 59 ms nesta máquina (WSL). A diferença é irrelevante. `ensure_tenant` também passa a `&mut self`; leituras (`get`, `list`, `relations`, `lineage_records`, `chain_head`, `verify_chain`, `catalog`) ficam `&self`.
  - **Fake de `IdGenerator`**: `next(&self)` com `AtomicU64` no fake (átomos não estão em `clippy.toml`). Alternativas: (a) `next(&mut self)` com contador `u64` simples: `Deps` guardaria `&'d mut dyn IdGenerator` e a segunda instância simultânea dá `E0499 cannot borrow ids as mutable more than once` (reproduzido), inviabilizando os testes de duas instâncias; (b) função pura `next(&self, tenant, seq)`: fake sem estado, mas o `UuidV7` de produção ignoraria os parâmetros (a porta mentiria sobre o que recebe) e nenhum teste do plano afirma id exato sob concorrência. Fica o `AtomicU64`.
- **Método:** crate descartável `.../scratchpad/critic/d1` (rusqlite 0.41.1 `bundled` real, toolchain 1.97.0, `clippy.toml` e lints copiados da raiz). Comandos: `cargo test --offline --test t_b --test t_c --test t_d` (verde após o ajuste de partida do gerador em C e D; vermelho antes, com `UNIQUE constraint failed: r.rid`), `cargo test --offline --features plan_a --test t_a` (A verde, só serve de controle), `cargo clippy --offline --all-targets -- -D warnings` (B, C, D limpos), `cargo clippy --offline --features plan_a --lib -- -D warnings` (7 erros em A), cinco exemplos de falha de compilação em `cf/` (`b_share` e `c_share` e `d_share`: E0277; `b_mutate_while_open`: E0502; `b_spawn_static`: E0597; `ids_mut`: E0499), `examples/omhl_main.rs`, `examples/actor.rs`, `examples/b_own_static_ok.rs`. Corolários em `.../scratchpad/critic/d1c` (clippy: `needless_pass_by_value` nas duas versões "por valor que só lê", limpo nas versões que consomem ou emprestam; benchmark release de 10 000 chamadas).
- **O que muda no plano (para a sessão principal aplicar):**
  1. Seção 4.4 (linha 287 e bloco 291-318): `Hexlog<'d>` é `Send` e não `Sync`, guarda o `SqliteStore` concreto com a `Connection` dona; sem `Mutex` e sem `Arc`. `Deps<'d> { clock: &'d dyn Clock, ids: &'d dyn IdGenerator, catalog: Catalog, native: &'d dyn NativeNodes }`. Assinaturas: `impl<'d> Hexlog<'d> { pub fn open(config: &Config, deps: Deps<'d>) -> Result<Hexlog<'d>, HexlogError>; pub fn ensure_tenant(&mut self, tenant: &TenantId) -> ..; pub fn append(&mut self, req: &AppendRequest) -> ..; get/lineage_records/list/relations/chain_head/verify_chain/catalog com &self }`.
  2. Linha 33 (E1, "a Facade serializa com `Mutex`"), 168 e 619 (`SqliteStore (Mutex<Connection>)`), 890 (ADR) e R11: trocar `Mutex<Connection>` por `Connection` dona; a coordenação entre instâncias continua por WAL, `busy_timeout` e `BEGIN IMMEDIATE`. As portas `Clock`, `IdGenerator`, `NativeNodes` e `Definitions` mantêm `Send + Sync` (é o que torna `&'d dyn Clock` `Send`); `Store: Send + Sync` (linha 421) vira `Store: Send`, e `append_with` passa a `&mut self` (usa `Connection::transaction_with_behavior(Immediate)`); `read` fica `&self` (`unchecked_transaction`).
  3. Linha 605 (`tests/support`): `Env { clock: FixedClock, ids: SeqIds(AtomicU64), native: FakeNative { entries: Vec<..> } }` com `fn deps(&self, catalog: Catalog) -> Deps<'_>`; o fake de `NativeNodes` tem `reserve(&mut self, ..)` sem `Mutex`. Os testes que "trocam o fake no meio" (linhas 696-704 e 746) passam a largar o `Hexlog` (bloco ou `drop(hx)`), chamar `env.native.reserve(..)` e reabrir.
  4. T5d (linhas 829-837): `it_should_be_send_and_sync` vira `it_should_be_send` (asserção `Hexlog<'_>: Send`); `it_should_serialize_calls_from_threads_sharing_one_instance` **sai** (o compilador proíbe compartilhar uma instância entre threads; é a garantia, não perda). Isso muda as contagens do handoff: `concurrency` de 9 para 8 e total de 117 para 116, a menos que o dono queira um substituto (candidato: `it_should_keep_seq_contiguous_when_two_instances_alternate_in_one_thread`). Contagem é decisão do dono: `[DONO]`.
  5. A frase do T5d "e com uma instância compartilhada" (linha 646) sai junto com o teste removido.
  6. Observação lateral para o `store`: `u64` não implementa `FromSql` em `rusqlite 0.41.1` (erro E0277 ao fazer `row.get::<_, u64>`); `seq` e `count` como `u64` no plano precisam de leitura como `i64` e conversão com `u64::try_from`.

### D-1 (revisão): `Mutex` tem alternativa sem exceção além da (1b): conexão por chamada (1c). Mantida a (1b), `Hexlog<'d>: Send`, com `&mut self` nas escritas

Complementa o D-1 acima, que continua valendo no que não é reaberto aqui. O D-1 afirmou "só `Mutex<Connection>` dá `Sync`". Isso vale para uma `Connection` guardada na instância; não vale para uma instância que não guarda conexão. A sessão principal apontou a omissão e a reabertura abaixo corrige a afirmação.

- **Pergunta:** (1b) `Connection` dona, `Hexlog: Send`, escritas por `&mut self`, ou (1c) `SqliteStore` com caminho e `busy_timeout`, uma conexão aberta e fechada a cada chamada, `Hexlog: Send + Sync`, tudo por `&self`?
- **Alternativas:**
  - 1b: como registrado no D-1.
  - 1c: `Hexlog<'d> { path, busy_timeout, deps }`; `connect()` abre, aplica `busy_timeout` primeiro e `foreign_keys`, a escrita usa `transaction_with_behavior(Immediate)` sobre `&mut Connection` local. `journal_mode=WAL` é persistente no arquivo e só precisa do `open`.
  - 1a (`Mutex`) e (iii) pool seguem descartadas (exceção de memória; `Arc` e threads de fundo).
- **Interferência no comportamento final (medida, release, `rusqlite 0.41.1` bundled, WAL, `synchronous` padrão, ext4 do WSL2, carga da máquina entre 0,8 e 4 por causa de outros agentes):**
  - **(a) Disputa em 1c, N threads na mesma instância com `busy_timeout`.** `BUSY` aparece, mas só sob disputa extrema, e aparece também em 1b (uma conexão por thread). Tabela de `ok / BUSY` por 5 rodadas:

    | caso | 1b (conexão por thread) | 1c (instância compartilhada) |
    |---|---|---|
    | 2 threads x 20 appends, 5 s | 200 / 0 | 200 / 0 |
    | 8 threads x 20, 5 s | 800 / 0 | 800 / 0 |
    | 32 threads x 10, 5 s | 1600 / 0 | 1600 / 0 |
    | 32 threads x 10, 500 ms | 1561 / 39 (2,4 %) | 1330 / **270 (17 %)** |
    | 64 threads x 20, 5 s, execução 1 | 6308 / 92 (1,4 %) | 6281 / 119 (1,9 %) |
    | 64 threads x 20, 5 s, execução 2 | 6085 / 315 (4,9 %) | 6283 / 117 (1,8 %) |
    | 64 threads x 20, 5 s, execução 3 | 6303 / 97 (1,5 %) | 5999 / 401 (6,3 %) |

    Leitura: até 32 threads com o `busy_timeout` padrão de 5 s não houve `BUSY` em nenhuma das duas, em 5 rodadas por caso, então os testes do plano (2 a 8 threads) não são afetados por nenhuma. O `busy_timeout` do SQLite não é fila justa: com 64 escritores uma thread pode esperar mais que 5 s e leva `BUSY`, nas duas. A diferença entre 1b e 1c nos casos extremos está dentro do ruído entre execuções (a mesma configuração de 1b variou de 92 a 315); o único sinal fora do ruído é o caso de 500 ms com 32 threads (270 contra 39, uma amostra de cada), que não representa os testes nem os consumidores. Não classifiquei em que instrução cada `BUSY` nasce.
  - **(c) Custo para o consumidor real.** CLI e hook (um processo por chamada): 1b e 1c são equivalentes, porque as duas abrem uma conexão, fazem uma chamada e fecham ao sair do processo; é o caso B1/D1 da medição da pesquisa (16 ms no append, 0,35 ms na leitura). A diferença nasce só em chamadas em sequência no mesmo processo (MCP) e em chamadas que fazem duas operações (por exemplo `ensure_tenant` e depois `append`: duas aberturas em 1c). Medi sequencial, mesma máquina, mediana de 300 chamadas, três execuções (a carga variou, daí a dispersão): append 1b 4,7 / 18,7 / 10,0 ms contra 1c 14,9 / 87,0 / 15,7 ms (razão 1,6 a 4,6, concorda com os 3,4 da pesquisa); leitura (`count`) 1b 3,7 a 12,6 µs contra 1c 286 a 373 µs (cerca de 70 vezes). Em valor absoluto ambas as leituras são irrelevantes para um humano, e o append de 15 ms em 1c também; o que muda é que o custo do checkpoint no fechamento da última conexão passa a ser pago a cada append.
  - **(b) O que o compilador prova.** 1b: uma instância tem no máximo um escritor por vez, em tempo de compilação (duas escritas simultâneas na mesma instância são irrepresentáveis, `E0499`/`E0502`); a disputa entre instâncias e processos continua resolvida em tempo de execução por `BEGIN IMMEDIATE` e `busy_timeout`, igual em 1c. 1c: nada é provado sobre escritores; a ausência de estado compartilhado dispensa a prova, e a disputa dentro do processo passa de "o compilador impede" para "o SQLite arbitra". As duas respeitam a regra 2 (sem `Mutex`, `Arc`, `Cell`); a regra 3 ("borrow checker ao máximo") só é exercida de fato em 1b. Em 1c o `&mut Connection` aparece apenas local, dentro da chamada.
  - **(d) Custo de mudança.** 1c deixa seção 4.4, `Store: Send + Sync`, `&self` em todas as assinaturas, `tests/support` do plano e as contagens (117, `concurrency` 9) como estão; só troca `Mutex<Connection>` por caminho e `busy_timeout` e exige `PRAGMA` por chamada (`busy_timeout` antes de qualquer outro, `foreign_keys`). 1b exige as mudanças do D-1 (assinaturas `&mut self`, `Store: Send`, remoção de `it_should_serialize_calls_from_threads_sharing_one_instance`, `it_should_be_send_and_sync` renomeado, 117 para 116). Todos os testes das linhas 825-837 são cumpridos por 1c: `it_should_serialize_calls_from_threads_sharing_one_instance` e `it_should_be_send_and_sync` passam (reproduzidos em `tests/t_c1.rs`, e o clippy do crate segue limpo); os de duas instâncias, os de adulteração (conexão crua separada) e o de abertura concorrente não dependem da escolha. O risco de 1c nesses testes é estabilidade: a regra de 20 repetições seguidas fica mais exposta a `BUSY` em máquina lenta quanto mais threads o teste usar; com até 8 threads não vi nenhum.
- **Escolhida e por quê:** mantida a **1b** (a decisão do D-1 não muda). Razões, em ordem de peso:
  1. A regra do dono manda usar o borrow checker ao máximo; só 1b faz o compilador provar algo (escritor único por instância). 1c é compatível com a regra 2, mas não exercita a 3.
  2. Para CLI e hook as duas custam o mesmo; para o MCP em sequência 1b é de 1,6 a 4,6 vezes mais rápida no append e cerca de 70 vezes na leitura. Não é decisivo em milissegundos absolutos, mas é custo pago sem ganho para o consumidor, já que nenhum consumidor previsto precisa compartilhar uma instância entre threads (e o ator por `mpsc` do D-1 cobre o MCP concorrente).
  3. Disputa: não há vantagem de 1c; os dois falham do mesmo jeito sob disputa extrema.
  4. O argumento a favor de 1c é de estabilidade do plano (contagens, assinaturas e `Sync` intactos), não de comportamento. É um argumento legítimo, mas de prioridade do dono, não técnico.
- **Ataques tentados:** (i) "1c elimina a necessidade do ator": sim, mas só por pagar a reabertura em toda chamada; o ator de 1b continua sem exceção. (ii) "1b perde `Sync` e isso é regressão": nenhum teste do plano nem consumidor previsto exige `Sync` além dos dois testes já tratados. (iii) "o `BUSY` de 1c é pior": medido, só em um caso com amostra única; não sustentado. (iv) "a deriva de `user_version` entre chamadas": em 1c a versão do esquema só é conferida se cada `connect()` repetir a checagem (uma `PRAGMA user_version` por chamada, barata); em 1b é conferida uma vez no `open`. Equivalente em risco, 1c paga mais.
- **Método:** `.../scratchpad/critic/d1`: módulo `src/c1_percall.rs`, testes `tests/t_c1.rs` (3 verdes), `cargo clippy --offline --all-targets -- -D warnings` limpo; `examples/contend.rs` (release; `contend persistent|percall <busy_ms> [heavy]`, 5 rodadas por caso, banco novo a cada rodada, contagem de `DatabaseBusy`/`DatabaseLocked`; saída em `cont.out`, orquestrada por `run_cont.sh`); `examples/seq.rs` (300 appends e 300 leituras em sequência, mediana). Comparado com a seção "Medição: alternativas ao Mutex" de `.omc/research/gray-box/store-sqlite.md`: mesma direção (checkpoint no fechamento como causa do custo de abrir e fechar); os números absolutos diferem porque a carga da máquina variava.
- **O que muda no plano:** nada além do que o D-1 já listou. Condicional ao dono: se ele preferir manter 117 testes, assinaturas `&self` e `Hexlog: Send + Sync`, a (1c) é a alternativa sem exceção de memória, ao custo medido acima (append 1,6 a 4,6 vezes e leitura cerca de 70 vezes mais lentos em chamadas em sequência no mesmo processo; igual em CLI e hook). Nesse caso a seção 4.4 passa a `SqliteStore { path, busy_timeout }`, `connect()` aplica `busy_timeout` primeiro e `foreign_keys` em toda chamada, `Store` continua `Send + Sync` com `&self`, e a T5d mantém as 9 linhas; o `busy_timeout` dos testes de concorrência fica em 5 s com no máximo 8 threads.

### D-1 (decisão final do dono)

O dono escolheu a (1b): `Hexlog<'d>: Send`, escritas por `&mut self`, sem substituto para o teste removido (`concurrency` com 8 testes, total 116 antes de somar os 2 testes do D-0). Conferi a aplicação no plano: seção 4.4, E1 (linha 33), 168, 619, 421 (`Store: Send`, `append_with` e `ensure_tenant` com `&mut self`, `read` com `&self`), T5d e 16.4 a 16.6 estão coerentes com o D-1. Restos: as linhas 600, 601 e 650 (T1 e T6, "Pronto") ainda dizem `architecture` com 8 testes, e a 16.6 diz 10 (D-0); fica a de 16.6 como a vigente, mas o texto de T1 e T6 precisa ser trocado para 10 para o `executor` e o verificador não contarem errado. As linhas 934, 952 e 1097 (rodadas 8 a 13) citam o `Mutex` como histórico e podem ficar.

### D-2: congelar o corpo do record na T2 — um tipo `RecordBody<'a>` só, com `Cow`, para escrever e para ler, e documentos dourados das cinco formas comparados byte a byte; os 3 testes de `jcs_vectors` não congelam nada

- **Pergunta:** como a T2 prende o formato do corpo do record (mudar depois é tenant novo), já que nada nos 3 testes de `jcs_vectors` liga a forma de `RecordBody` ao formato? E que tipo escreve e lê?
- **Alternativas (congelamento):**
  1. Só a seção 4.3 em prosa (estado atual do contrato: costura 5 diz "seção 4.3 do plano, chave a chave").
  2. Teste de unidade na T2 que constrói `RecordBody` e compara o JCS e o hash com documentos dourados de cinco formas, valores do oráculo TS.
  3. Esperar a T5c (verify) ou a T5a (primeiro `decide`).
  4. (2) mais um teste de ponta a ponta na T5a que lê a coluna `body` crua depois de um `append` e compara com os mesmos dourados.
- **Alternativas (forma do tipo):** (i) `RecordBody<'a>` de empréstimos para escrever e outro tipo, ou `serde_json::Value`, para ler; (ii) um único tipo com `Cow<'a, str>` e `#[serde(borrow)]` que serve às duas direções; (iii) `Value` na leitura e extração de campo por nome.
- **Interferência no comportamento final:**
  - (1) deixa o executor livre onde a prosa é ambígua, e cada escolha muda o hash. Medi com o oráculo TS (`canonicalize` do `hexlog`, Node 24) cinco leituras razoáveis do mesmo texto, todas com hashes diferentes: `retirement:false` emitido ou omitido no node; lote sem `key` com ou sem o objeto `batch`; `model: null` em vez de ausente. As ambiguidades do 4.3: A1 `retirement` aparece como `false` no exemplo mas a regra diz "campo opcional ausente é omitido"; A2 o corpo de relation tem `type`? e `data`? (a regra só diz "troca `lineage`"); A3 `batch` existe em lote sem `key`? (no TS existe sempre, só a `key` é opcional); A4 grafia de `from_origin`/`to_origin` (a tabela de mutação usa `native` e `tenant`, mas nenhum texto fixa que a serialização é essa, em minúsculas); A5 `native_catalog` na retirement de relation; A6 `count` é inteiro JSON.
  - (2) e (4) fecham tudo: o dourado fixa chaves, presença, ordem JCS e bytes. O dourado da T2 precisa do tipo no contrato, porque o teste construirá o tipo por campo.
  - (3) deixa a forma sem prova até o dono já ter aprovado o resumo da T2; o erro sai como retrabalho de fixture depois do ponto de parada 3.
  - Forma (i): duas definições que podem divergir (escrever uma chave, ler outra); (iii): nome de campo por texto, sem o compilador; `Value` copia tudo; não detecta chave duplicada (a última vence, mas a recanonicalização acusa).
- **Escolhida e por quê:** congelamento (4) e forma (ii). Um tipo só, `RecordBody<'a>` com `Cow<'a, _>`, `Serialize` e `Deserialize` com `#[serde(borrow)]` e `#[serde(deny_unknown_fields)]`, campo opcional com `skip_serializing_if = "Option::is_none"`, o `lineage` como enum interno por `kind`. Escrever usa `Cow::Borrowed` dos dados do chamador, ler empresta do texto da linha. Razões: (a) o compilador impede que escrita e leitura divirjam; (b) a leitura do `verify` fica `serde_json::from_str::<RecordBody>(&row.body)` e depois `canonical(to_value(&body)) == row.body`; essa comparação sozinha acusa chave fora de ordem, espaço, escape desnecessário, `null` em campo opcional, campo desconhecido (`deny_unknown_fields`) e chave duplicada (erro de `serde`); todos viram `InvalidBody`; (c) os campos que não têm escape (ids, hashes, tenant, `at`, `def_hash`) saem como `Cow::Borrowed`; só `external_id`, `author` e `data` caem em `Cow::Owned` quando o texto traz escape.
- **Método:** crate descartável `.../scratchpad/critic/d2`.
  1. `golden.mjs` gera, com o `canonicalize` do TS e `sha256`, JCS e hash de cinco corpos (node completo, relation com `native_catalog`, retirement de node, retirement de relation, node de lote sem `key`): `golden_bodies.json`.
  2. `tests/golden.rs`: `RecordBody` de empréstimos (variante dois tipos) reproduz o JCS e o hash do oráculo nas cinco formas (3 testes verdes).
  3. `tests/unified.rs`: o tipo único com `Cow` reproduz os bytes do oráculo e lê de volta emprestando (`Cow::Borrowed` em `id` e `tenant`); clippy do crate limpo com os lints do D-0.
  4. `tests/read_probe.rs` (saída com `--nocapture`): campo desconhecido dá erro de parse; chave duplicada dá erro de parse (`duplicate field`); `retirement` ausente dá erro de parse; `external_id:null`, espaço após `:` e `5` no lugar de `5` leem, mas a reserialização canônica difere do texto (acusa `InvalidBody`); `client` com `\"` vira `Cow::Owned` e `tenant` segue `Cow::Borrowed`.
  5. `oracle.mjs`: cinco variantes de um mesmo corpo, quatro com hash diferente do base.
- **Trecho a trocar no contrato (costura 5, parágrafo "Corpo do record: seção 4.3 do plano, chave a chave"):**

```text
Corpo do record. Tipo interno (crate), `engine/model.rs`: `pub(crate) struct RecordBody<'a>`, um só para escrever e ler (Serialize + Deserialize, `#[serde(borrow)]`, `#[serde(deny_unknown_fields)]`, campos de texto `Cow<'a, str>`, `data: Option<Cow<'a, serde_json::Value>>`, opcionais com `skip_serializing_if = "Option::is_none"`). Campos e nomes JSON: v (u8, sempre 1), tenant, seq (u64), id, at (texto de `Clock::now()` sem alteração), type (JSON `"type"`; Rust `type_name`), lineage (enum interno por `kind`: node com `id`; relation com `relation_type`, `from`, `to`, `from_origin`, `to_origin`), revises, retirement (bool), data, author (agent, model, client), snapshot (kind, id), external_id, native_catalog, def_hash, batch (fingerprint, key, count). Tipos auxiliares `BodyLineage<'a>`, `BodyAuthor<'a>`, `BodySnapshot<'a>`, `BodyBatch<'a>`. `RecordBody::canonical(&self) -> String` = `chain::canonical(&serde_json::to_value(self))`.

Presença de cada campo por forma (sem `null`, nunca; ausente é omitido):

| campo | node (raiz ou revisão) | relation (raiz ou revisão) | retirement de node | retirement de relation |
|---|---|---|---|---|
| v, tenant, seq, id, at, author, def_hash | sempre | sempre | sempre | sempre |
| type | nome do tipo | nome do relation type (igual a `lineage.relation_type`) | copiado do latest | copiado do latest |
| lineage | kind node, id | kind relation, as cinco chaves | como node | como relation |
| revises | se há record anterior na lineage | idem | sempre | sempre |
| retirement | sempre presente, `false` | sempre presente, `false` | `true` | `true` |
| data | sempre presente (objeto; `{}` se vazio) | ausente | ausente | ausente |
| snapshot | regras S1 a S9 | ausente | ausente | ausente |
| external_id | opcional | opcional | ausente | ausente |
| native_catalog | ausente | presente só se algum extremo é native | ausente | copiado do latest, se havia |
| batch | só no primeiro record de todo lote, com ou sem `key`: fingerprint, count (nº de itens) e key se houver |

`from_origin` e `to_origin` valem `"tenant"` ou `"native"`. A fixture `tests/fixtures/golden_bodies.json` traz as cinco formas (node completo, relation com `native_catalog`, retirement de node, retirement de relation, node de lote sem `key`), cada uma com o JCS e o hash calculados pelo Hexlog TS (mesmo comando e commit da fixture dos records). Teste de unidade da T2 em `src/engine/model/tests.rs` (declarado por `#[cfg(test)] mod tests;` em `model.rs`; não pode ser filho de `engine` porque `it_should_declare_every_engine_child_module_with_restricted_visibility` acusa filho desconhecido): para cada forma, constrói `RecordBody` por campo e afirma `canonical()` igual ao JCS do dourado e `hash_record` igual ao hash; lê o JCS do dourado com `serde_json::from_str::<RecordBody>` e afirma que reserializa para o mesmo texto. Teste de ponta a ponta na T5a (`it_should_store_the_golden_body_for_each_record_shape`, arquivo `append`): com `FixedClock` em `2026-11-07T12:00:00.000Z` e `SeqIds` gerando os ids do dourado, grava as cinco formas e lê a coluna `body` crua pela conexão de `tests/support/tamper.rs`, comparando com o mesmo JCS.
```

- **O que muda no plano:** 4.3, regra "Regras do corpo", para trazer a tabela de presença (resolve A1 a A6); `engine/model.rs` ganha `RecordBody` e auxiliares como `pub(crate)`; a T2 acrescenta o teste de unidade fora da contagem (como o da T3) e a fixture `golden_bodies.json`; a T5a acrescenta 1 teste (`append`), então a contagem de nomes únicos sobe de 118 para 119 mais os que a T3 fixar. O dono vê as cinco formas no resumo da T2.
- **Escolha de formato que o dono vê no resumo da T2 (a contrato fixa; mudar depois é tenant novo):** `retirement` sempre presente (o exemplo do plano o mostra como `false`); `type` presente também no corpo de relation (uniformiza a cópia na retirement); `batch` presente em todo lote, com ou sem `key`, e com `count` (o TS não tem `count`). Alternativas: `retirement` ausente quando `false` (mais curto, e o `verify` precisaria aceitar a ausência como `false`); relation sem `type` (uma redundância a menos, e a coluna `type` sairia de `lineage.relation_type`); `batch` só com `key`. Recomendo as três escolhas da tabela acima; qualquer outra serve desde que a tabela diga.

### D-3: impressão (`fingerprint`) do lote — a forma do `AppendItem` está correta e completa; falta prender o valor, porque o TS só confere o hash do JSON, não a serialização do Rust

- **Pergunta:** a forma de `AppendItem` na costura 5, que entra no hash por `batch.fingerprint`, está correta e completa? Um hash do TS sobre o mesmo JSON basta? Algo depende de ordem ou de campo opcional?
- **Alternativas:** (a) manter o teste `it_should_derive_the_fingerprint_without_key_and_author` (só mostra invariância); (b) acrescentar valores dourados de impressão por forma de item, hash do TS sobre o JSON da costura 5, comparados com `chain::fingerprint(&[AppendItem])` construído pela API pública; (c) conferir só o hash do JSON bruto (o que o TS faz de fato).
- **Interferência no comportamento final:** (a) não prova valor algum, só que `key` e `author` não entram, o que já é verdade por construção: `AppendItem` não tem `author` nem `key` (ficam em `AppendRequest`). (c) confirma `sha256(JCS(texto))` e nada sobre a serialização do `AppendItem` em Rust: o TS tem outro `BatchItem` (`alias`, `type`, `target`, `relations`) e não conhece `body`, `base`, `snapshot`. (b) prende o que importa: nomes de chave, presença, `type` no lugar de `type_name`, `snapshot` aninhado.
- **Escolhida e por quê:** (b). `AppendItem` e os campos são públicos, então o teste de integração monta cada forma e chama `hexlog::chain::fingerprint`, sem tipo interno. O oráculo é o TS só para JCS e hash; a forma do JSON vem da costura 5, que é a fonte independente do Rust.
- **Dependências de ordem e de opcional (medido com o `canonicalize` do TS):** a ordem dos itens muda a impressão (`[a,b]` e `[b,a]` diferem), correto porque a ordem define o `seq`; `base` ausente e `base: null` dão hashes diferentes, então o Rust tem de omitir (`Option::None` omitido por `skip_serializing_if`, nunca `null`); os itens não levam `author` nem `key` por estrutura. Não há campo opcional que dependa de outro.
- **Hashes do oráculo** (cada item sozinho num array, como `fingerprint(&[item])`; ids e SHA como no `fp.mjs`): item node completo (`base`, `snapshot` de 40 `c`, `external_id: "x"`, `data: {"k":"v"}`) `1861ef8282ee3dd97d41d90654e175cc74cedd5dde014c4a75081a0cb3199d6e`; relation sem `base` `3a450227f029c1d6f5448d8b40a43d3563ddabb554a9259ef9e00399f7af9937`; retirement de node `6da64732030ce5e8912849b931803d9fb214eae040da2895f3529bde23cf958b`; retirement de relation `b903eaae5bc500d1e1b639b73058adf3151b9ed51a4e621f31121051f76d0979`; lista [node, relation] `6ab902b1f24d6ecf3319451858a993c3d8b08b0aa7caaec67a0f1e2d730ed4f4`; lista [relation, node] `714b3b12bfef88054c5d82cf97f8ac6b244305c1fd16569f5a6db55ea9b6468f`. Servem como exemplo e para conferir o `fp.mjs`; o `test-engineer` regera pelo script, não copia da prosa.
- **Lacuna de ordem (achado):** a impressão roda no passo 3, antes do passo 4 (regras estáticas, que recusam número fora de inteiro seguro). Um `data` com inteiro maior que 2^53 faz `canonical` falhar (D-4) e, na costura 3, `fingerprint` devolve `Err`; o contrato não diz o que a escrita faz com esse `Err`. Duas leituras: devolver o erro do `canonical` (código diferente do `INVALID_RECORD number-not-safe-integer` do passo 4, e sem o ponteiro), ou pular o replay e seguir ao passo 4, que recusa o mesmo número. A segunda mantém a taxonomia de erros e é segura, porque o `Err` só nasce de número que o passo 4 recusa. Também: `data: {"a":1.0}` tem a mesma impressão de `{"a":1}` (o JCS imprime `1`), então um lote repetido com `1.0` dá `replayed: true` em vez de `INVALID_RECORD`; é inofensivo (o dado já gravado era o inteiro), mas é comportamento observável. O texto abaixo fixa as duas coisas.
- **Trecho a trocar no contrato (costura 3, `fingerprint`, e costura 5):** na linha `pub fn fingerprint(items: &[AppendItem]) -> Result<Hash, HexlogError>; // sha256hex(JCS(items))` acrescentar: `// JCS do array de itens como na costura 5; Err só quando algum número de data é inteiro fora de ±2^53. A escrita (T5a) trata o Err como "não há replay possível" e segue ao passo 4. Em f64 inteiro (1.0) a impressão é a do inteiro (1).` E em `jcs_vectors` acrescentar o teste `it_should_match_the_ts_fingerprint_for_each_item_shape` (valores de `tests/fixtures/golden_fingerprints.json`, gerados pelo TS sobre o JSON da costura 5, com o comando e o commit anotados).

### D-4: fronteira numérica do `canonical` — RFC fiel (ES para `f64`, dígitos para inteiro até ±2^53, erro acima) com a política "só inteiro seguro" fora dele; `canonical(&Value)` com a cópia de `data` do `to_value`

- **Pergunta:** número fora de ±(2^53 − 1) ou com fração: recusar em `canonical` ou serializar pela regra do ECMA-262? E `canonical(&Value)` contra um `Serializer` próprio?
- **Onde número chega a `canonical` (leitura do plano e do relatório [jcs-rfc8785.md](../research/gray-box/jcs-rfc8785.md)):** (1) `data`, depois de passar pela regra do passo 4 só inteiro seguro; (2) `fingerprint`, no passo 3, **antes** dessa regra, sobre `data` arbitrário; (3) `def_hash` de uma definição de catálogo, cujo `schema` o subconjunto de JSON Schema limita a inteiros seguros em `minimum`, `maximum`, comprimentos, `enum` e `const` (json-schema-subset.md, proposta); (4) os vetores do RFC, que têm `f64` (Apêndice B) e o limite 2^53 exato.
- **Alternativas:**
  - α. `canonical` fiel ao RFC: `i64`/`u64` até ±2^53 como dígitos, acima disso erro; `f64` finito pela regra ES; não finito é erro (não existe num `Value`); a política "só inteiro seguro" fica na validação de `data`.
  - β. `canonical` recusa tudo que não for inteiro seguro (perfil de inteiros).
  - γ. converter tudo para `f64` e imprimir pela regra ES, como o `JSON.parse` do Node.
- **Interferência no comportamento final:**
  - β falha os vetores do Apêndice B com número fracionário (o brief exige que `canonical` passe nos vetores do RFC) e mudaria o `def_hash` de qualquer definição futura com fração; fora isso, os resultados de produção coincidem com α, porque `data` e catálogo só têm inteiro seguro.
  - γ funde valores diferentes: medido com o oráculo TS, `9007199254740993` e `9007199254740992` saem do `canonicalize` como o mesmo `9007199254740992`. Em Rust o `serde_json` lê `9007199254740993` como `u64` exato (confirmado) e convertê-lo para `f64` arredondaria em silêncio; duas entradas distintas teriam a mesma impressão. Hoje isso só afetaria entrada que o passo 4 recusa, mas deixaria o `canonical` com uma colisão silenciosa.
  - α fecha as duas coisas, ao custo de implementar `es_number` (já prototipada e validada pelo relatório: 35 vetores, 6.000.000 de linhas do arquivo oficial, 4.499.032 amostras contra o `JSON.stringify` do Node, 0 diferenças).
- **Escolhida e por quê:** α. Segue a decisão do dono (JCS próprio pelo RFC 8785), passa nos vetores, não colide e mantém a regra de produto (inteiro seguro em `data`) onde o plano já a coloca (linha 535), com o código `number-not-safe-integer` e o ponteiro, que `canonical` não tem como dar. Regra precisa para o contrato: o limite ±2^53 vale para `Number` que é `i64` ou `u64`; um `Number` que é `f64` (incluindo `9007199254740992.0`, `295147905179352830000`, `1e+21` dos vetores) sempre sai pela regra ES, e `-0.0` sai `0`. Sem essa frase um executor converte tudo para `f64` (γ) ou recusa todo `f64` (β).
- **Parse:** `serde_json` sem `float_roundtrip` lê 17+ dígitos com erro de 1 ulp. Medido aqui: `0.73479537756966477e-11` lê `4440593796052815094` sem a feature e `4440593796052815093` com ela (igual ao `str::parse::<f64>`). O `Cargo.toml` do `hexlog` hoje é `serde_json = "=1.0.154"` sem features. Em produção o efeito é nulo (qualquer float em `data` é recusado e o catálogo só tem inteiro), mas os vetores do cyberphone (`values`, `arrays`) têm literais de 17 dígitos, e o teste de vetores falha ou passa por sorte sem a feature. Correção: `serde_json = { version = "=1.0.154", features = ["float_roundtrip"] }`, e o teste de `architecture` do fecho de dependências acusa `arbitrary_precision` e `preserve_order` nas features resolvidas.
- **`canonical(&Value)` contra `Serializer` próprio (medido, release, o `RecordBody` do D-2):** `to_value(&RecordBody)` copia `data` uma vez e os textos curtos. Para `data` de 229, 4.029 e 16.037 caracteres canônicos: corpo inteiro (`to_value` mais `canonical`) 4,5 µs, 28,7 µs, 129,6 µs; só a cópia do `to_value` 2,1 µs, 7,0 µs, 45,9 µs; a canonicalização de `data` sozinha (que o teto de 16.000 caracteres já exige) 1,1 µs, 15,3 µs, 57,0 µs. A cópia é de 0,05 % a 1 % do tempo de uma transação (4,7 a 16 ms medidos na pesquisa do store); o pior lote possível, 50 itens de 16 KB, gasta cerca de 2,3 ms na cópia, perto de um terço do custo de canonicalizar o lote e bem abaixo do I/O do `commit` de 800 KB. Um `Serializer` próprio (com bufferização de mapas para ordenar por UTF-16) tiraria a cópia e dobraria a superfície de código a validar.
- **Decisão sobre a cópia:** manter `to_value` mais `canonical(&Value)`, e registrar que o formato são os bytes, não o caminho de código: o dourado do D-2 fixa os bytes, então uma otimização futura (reaproveitar o `canonical(data)` do teto como fragmento, ou um serializador) é troca interna sem mudar hash, e um teste dourado verde prova que não mudou. A regra "empreste em vez de copiar" fica atendida onde a cópia seria evitável sem custo (o `RecordBody` empresta os textos; só o `Value` do `to_value` é copiado, porque `serde_json::Value` é dono do que guarda).
- **Método:** `.../scratchpad/critic/d2`: `examples/cost.rs` (release, 2.000 repetições por tamanho), `examples/fr.rs` com e sem `float_roundtrip`, `fp.mjs` para a colisão do 2^53 + 1 no oráculo TS. Leitura de [jcs-rfc8785.md](../research/gray-box/jcs-rfc8785.md) (tabela de inteiros, armadilhas 2, 6 e 7, algoritmo).
- **Trecho a trocar no contrato (costura 3, linha de `canonical`):**

```text
pub fn canonical(value: &serde_json::Value) -> Result<String, HexlogError>; // RFC 8785
// Número: `Number` que é i64 ou u64 sai em dígitos se |n| <= 2^53, senão Err; `Number` que é f64 sai pela regra ES (Number::toString; -0.0 sai "0"). A política "só inteiro seguro" (±(2^53 - 1)) NÃO é do canonical: é do passo 4 da escrita. Err: ErrorCode::InvalidInput, details[0].code == "number-out-of-range", details[0].path == "". NaN e Infinity não existem em Value (Number::from_f64 devolve None), então os 2 vetores do Apêndice B com `input: null` saem de `rfc8785_vectors.json`; os 24 de número entram por `ieee754` convertido com `f64::from_bits` e `Number::from_f64`, e os 9 de texto (`cyberphone-*` e RFC §3.2.x) por `input` lido com `serde_json::from_str`.
```

### D-5: `details` de recusa em `parse` — `parse` devolve `path ""` e quem monta o ponteiro é quem conhece o documento (o chamador do `parse`, ou o `decide`); `HexlogError::at(prefix)` prefixa

- **Pergunta:** o `parse` devolve `path ""` e o chamador prefixa, ou o `parse` recebe o ponteiro? Quem monta `/items/3/body/id`?
- **Alternativas:** P1 `parse(text) -> Err` com `path ""` e `HexlogError::at(self, prefix: &str)` que prefixa cada `details[].path`. P2 `parse(text, at: &str)`. P3 ids com `Deserialize` e `serde_path_to_error`.
- **Interferência no comportamento final:**
  - Os ids chegam à `Hexlog` já tipados (`AppendItem` tem `SemanticId`, `TypeName` etc., plano 4.4). Logo o `decide` nunca vê o texto cru de um id; a recusa de formato nasce no `omhl`, ao montar o `AppendItem` a partir do JSON do agente, e só o `omhl` sabe em que documento (CLI, MCP) e em que posição o texto estava. O motor monta ponteiro apenas para o que ele mesmo recusa no passo 4 e 6 (`snapshot-required`, `number-not-safe-integer`, `endpoint-not-found`...), e esses ponteiros seguem a forma do `AppendRequest` (costura 5): `/items/<i>/snapshot`, `/items/<i>/body/data/<chaves>`.
  - P2 obriga todo ponto de construção, inclusive o `verify` lendo ids gravados e os testes, a passar um ponteiro que não existe; mistura "onde" com "o quê".
  - P3 perde o detalhe estruturado: `serde` devolve só texto.
- **Escolhida e por quê:** P1. `parse` e `Snapshot::new` devolvem `path ""` ("o próprio valor", já no contrato). Acrescentar `HexlogError::at(self, prefix: &str) -> HexlogError`: troca `path ""` por `prefix` e `path "/x"` por `prefix + "/x"`. Quem monta: o `decide` para o que ele recusa; o `omhl` para o que o `parse` recusou, com `.at("/items/3/body/id")`.
- **Escape do segmento (achado):** o ponteiro vem de chaves de `data`, e chaves podem ter `/` e `~`. RFC 6901: `~` vira `~0` e `/` vira `~1`. O dourado do D-2 tem a chave `a/b` para o caso. O contrato precisa de uma função única: `pub fn pointer_segment(segment: &str) -> String` (módulo `error`), usada pelo `decide`; sem ela cada regra do passo 4 escapa de um jeito. Teste de T2: `pointer_segment("a/b~c") == "a~1b~0c"`, e `at` com prefixo vazio mantém o caminho.
- **Método:** leitura de 4.4 (tipos de `AppendItem`) e das regras do passo 4; nenhum experimento de compilação (a escolha depende de quem tem a informação, não de comportamento do compilador). O dourado com a chave `a/b` do D-2 (JCS `"a/b"` sem escape de `/`) é o caso de teste do `pointer_segment`.
- **Trecho a trocar no contrato (costura 1, depois do parágrafo de `SemanticId::parse`):**

```text
Toda recusa de `parse`/`new`/`FromStr` dos tipos de id e de `Snapshot::new` devolve exatamente um `Detail` com `path == ""`. `impl HexlogError { pub fn at(self, prefix: &str) -> HexlogError }` troca cada `path` vazio por `prefix` e cada `path` não vazio por `prefix` mais o `path`. `pub fn pointer_segment(segment: &str) -> String` escapa `~` como `~0` e `/` como `~1` (RFC 6901). O ponteiro de erros de `data` é `/items/<i>/body/data/<chave escapada>...`.
```

### Achados no contrato de costuras da T2 (rascunho `omhl-v2-core-step3-seams.md`): o que o `test-engineer` não testa sem ler a implementação ou que o `executor` cumpre de duas formas

Cada item traz a correção exata. Os de D-2 a D-5 já têm o trecho nas decisões acima; aqui ficam os demais.

1. **Costura 1, formato dos outros nove tipos de id.** O contrato fixa só `SemanticId::parse` (`InvalidInput`, `semantic-id-format`) e diz "mesma forma" para `Name`, `TenantId`, `TypeName`, `RelationTypeName`, `RecordId`, `Hash`, `IdempotencyKey`, `ExternalId`. Cada um tem regra e código próprios no plano (X3 manda `INVALID_RECORD external-id-format`; o passo 1 manda `INVALID_INPUT` para `key`). Sem tabela, o teste e o executor escolhem códigos diferentes. Substituir "Mesma forma em ..." por:

```text
Formatos e recusas (todos `parse(text: &str) -> Result<_, HexlogError>`; `details[0].path == ""`; `message` nunca ecoa o valor):
| tipo | aceita | code | details[0].code |
| SemanticId | segmentos ^[a-z0-9][a-z0-9-]{0,62}$ separados por ".", até 200 caracteres, sem ponto inicial, final ou duplo | InvalidInput | semantic-id-format |
| Name, TenantId, TypeName, RelationTypeName | ^[a-z0-9][a-z0-9-]{0,62}$ (um segmento) | InvalidInput | name-format |
| RecordId | "<TenantId>:<uuid>", uuid minúsculo com hífens, versão 7, variante 8, 9, a ou b | InvalidInput | record-id-format |
| Hash | 64 hexadecimais minúsculos | InvalidInput | hash-format |
| IdempotencyKey | 1 a 200 unidades UTF-16 | InvalidInput | idempotency-key-format |
| ExternalId | 1 a 200 unidades UTF-16 | InvalidRecord | external-id-format |
Comprimento é sempre em unidades UTF-16 (`str::encode_utf16().count()`), igual ao Hexlog TS e à ordenação do JCS; vale também para `Author` (1 a 100) e `data_max_chars`. Teste com emoji (vale 2).
```

2. **Costura 4, o fake de ids precisa gerar uuid v7 válido.** `IdGenerator::next` devolve `uuid::Uuid` e `RecordId` exige versão 7 e variante RFC 4122; um `SeqIds` com `Uuid::from_u128(n)` (versão 0) gera `RecordId` que o próprio `parse` recusa, e o `verify` ou o teste de ids quebra. Acrescentar: `SeqIds::next` devolve `Uuid::from_u128(0x0192a000_0000_7000_8000_0000_0000_0000 | n)`, igual aos ids dos dourados do D-2.
3. **Costura 4, assinatura do fake de `NativeNodes`.** "`FakeNative` com `reserve(&mut self, id, type)` e relations nativas" não dá assinatura. Acrescentar: `FakeNative::new(version: &str)`, `reserve(&mut self, id: SemanticId, type_name: TypeName)`, `reserve_relation(&mut self, relation_type: RelationTypeName, from: SemanticId, to: SemanticId)`, `catalog_version()` devolve a `version` recebida (o dourado do D-2 usa `"v1"` em `native_catalog`).
4. **Costura 3, `anchor_doc` não falha.** O contrato diz `Result<String, HexlogError>`, mas as entradas (um `TenantId` e um texto) sempre canonicalizam; um `Result` sem erro possível deixa o executor inventar validação de `created_at` que o teste não vê. Trocar por `pub fn anchor_doc(tenant: &TenantId, created_at: &str) -> String; // JCS de {"created_at":created_at,"format":"hexlog-chain/1","tenant":tenant}, sem validar created_at`. A fixture `golden_records.json` ganha o `anchor_doc` e a âncora de um tenant, calculados pelo TS (`hashOfJcs` do `chain.ts`), junto aos 3 records; o teste `it_should_match_the_hexlog_ts_hash_for_each_golden_record` os confere.
5. **Costura 3, fixture de vetores.** Dois dos 35 vetores (`...7fffffffffffffff-not-json`, `...7ff0000000000000-not-json`) têm `input: null` e `expected: null`: NaN e Infinity, que não existem em `serde_json::Value`. Tirar da fixture; o texto está no trecho do D-4.
6. **Costura 3, `fingerprint` e `canonical` com `Err`.** Sem dizer o código nem o que a escrita faz com o `Err`, o teste não consegue afirmar nada. Texto no D-3 (comportamento) e no D-4 (código).
7. **Costura 5, `Clock::now` não é validado.** O corpo guarda `at` verbatim; um relógio que devolve texto fora de `YYYY-MM-DDTHH:MM:SS.mmmZ` grava corpo que o Hexlog TS recusaria e quebra o `ColumnMismatch` de `at`. Escolher e escrever: o `decide` valida o formato de 24 caracteres e devolve `Internal`, ou não valida e o contrato diz que o relógio é de confiança. Recomendo validar (um `Clock` é porta injetada, e o formato é de armazenamento).
8. **Plano, tabela de mutações (linhas 483 e 857):** "tipo `requires_snapshot`" se lê como o nome do tipo, e `_` não passa em `^[a-z0-9][a-z0-9-]{0,62}$`. Trocar por "tipo com `requires_snapshot = true` (nome `strict-note`)".
9. **Plano, linhas 600, 601 e 650 (T1 e T6):** `architecture` com 8 testes; a 16.6 manda 10. Corrigir o texto das duas tarefas.
10. **`float_roundtrip`:** ver D-4 (mudança de `Cargo.toml` e do teste de fecho de dependências).

### D-6: `sqlx` 0.9.0 no lugar de `rusqlite`: Facade síncrona com `block_on` confinado ao store, portas por `&mut`, consultas em tempo de execução e abertura de banco novo com retentativa no `journal_mode=WAL`

Contexto: o dono decidiu `sqlx` (plano 16.8); aqui só o encaixe técnico. Os experimentos estão em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/critic/d6` (lib `Store` com `SqliteConnection`, testes `basic`, `stress`, `tamper`, `props`, `reopen`, `ddl`, exemplos de compile-fail), `d6x` (conflito de `links`), `d6m` (`query!`) e `d6r` (linha de base com `rusqlite`). Copiei `clippy.toml` e `rust-toolchain.toml` (1.97.0); a lib passa no clippy com os lints do workspace, só um aviso `type_complexity` do meu fechamento.

**Versão e features exatas (conferidas por `cargo tree`):**

```toml
sqlx = { version = "=0.9.0", default-features = false, features = ["sqlite-bundled"] }
futures-executor = "=0.3.34"   # block_on; já é dependência de sqlx-sqlite 0.9.0
```

`sqlite-bundled` liga só `_sqlite`, `sqlx-sqlite/bundled` (`libsqlite3-sys 0.37.0`, SQLite 3.51.3 medido) e `sqlx-macros?/sqlite`; os defaults `any`, `macros`, `migrate`, `json` ficam fora. O fecho normal tem zero ocorrências de `tokio`, `async-std`, `hyper`, `smol`, `async-global-executor`, `reqwest`, `ureq`, `git2`, `gix`. Candidatos novos: `sqlx` 17.633 estrelas (empurrado em 2026-10-31), `futures-executor` (repositório `rust-lang/futures-rs`) 5.940 estrelas (2026-11-02), `flume` 3.086 estrelas (transitiva, 2026-10-15); todas passam o critério de 1000 estrelas (consulta à API do GitHub em 2026-11-07). O relatório `store-sqlx.md` do `esopo` confere com tudo que reproduzi (17.632 estrelas, SQLite 3.51.3, conflito de `links`).

#### (a) Facade síncrona com `block_on` ou Facade `async`

| Alternativa | Efeito no comportamento final |
|---|---|
| A1. Facade síncrona; `futures_executor::block_on` só dentro de `store/sqlite` | Os testes continuam `#[test]` puros; CLI e hook não ganham runtime; `Hexlog` não muda de assinatura. |
| A2. Facade `async fn` | Todo consumidor (testes, CLI, hook) precisa de executor; o plano inteiro (4.4, T4, T5, 118 testes) vira `async`; `decide`, que é CPU puro, passa a viver dentro de um futuro sem ganho. Não rodei A2: descartada por custo de contrato, não por falha medida. |

Experimento (A1): `Store::open` + `append_with` + `read` funcionam em `#[test]` sem nenhum runtime (teste `sync_open_append_read_without_runtime`). Dentro de um runtime estranho: em thread de trabalho de `tokio` multi-thread e em `tokio` `current_thread` o `block_on` funciona, porque `sqlx-sqlite` tem a própria thread de sistema (`sqlx-sqlite-worker-N`) e canal `flume`, e não usa `tokio`. Dentro de outro `futures_executor::block_on` entra em pânico (`cannot execute LocalPool executor from within another executor: EnterError`, `local_pool.rs:80`). Escolhida: A1. Regra de contrato: a Facade nunca é chamada de dentro de um `block_on` do `futures-executor`; o MCP, se tiver runtime, chama a Facade por `spawn_blocking`.

#### (b) Sem `tokio`, `async-std` ou `hyper`: o que quebra

Nada que o desenho use. Pelo código (`sqlx-core-0.9.0/src/rt/mod.rs`), `spawn`, `spawn_blocking`, `timeout` e `sleep` caem em `missing_rt` sem feature de runtime; quem os chama é o `Pool`. `SqliteConnection` direto, sem `Pool`, só usa a thread de worker e `block_on`. Provado: abrir, transacionar, ler, fechar e 32 threads abrindo conexões (item e) sem nenhum runtime no grafo. Consequência: a lista proibida (plano linha 191) segue intacta, sem `[DONO]`. Regra: **nunca `SqlitePool`** (traria runtime). Sugestão de acréscimo à lista de `it_should_not_depend_on_forbidden_crates`, só endurece: `smol`, `async-global-executor` e `sqlx-macros` (item d). Fica como proposta, aprovação do dono.

#### (c) Portas `WriteReads`, `QueryReads` e `VerifyReads`

O `Executor` do `sqlx` exige `&mut SqliteConnection` (ou `&mut Transaction`). Alternativas, com compilação real:

| Alternativa | Resultado medido |
|---|---|
| C1. Portas com `&self` | Não compila: `E0596 cannot borrow self.conn as mutable, as it is behind a & reference` (`examples/cf_self.rs`). Só compilaria com `Mutex`/`RefCell`, vetados pela regra do dono. |
| C2. `async fn` em trait | Não é objeto de trait: `E0038 the trait Reads is not dyn compatible` (`examples/cf_async.rs`); forçaria genéricos em `decide` e no `Store`. |
| C3. Portas síncronas com `&mut self`, objeto de trait `&mut dyn`, cada método faz o seu `block_on` | Compila; `&mut dyn StoreReader` faz upcast para `&mut dyn QueryReads` e `&mut dyn VerifyReads` (`examples/upcast.rs`, saída `(2, 3)`). |

Escolhida: C3. Cada método de porta faz `block_on` próprio; `decide` (CPU puro) roda fora de qualquer `block_on`. A transação do SQLite é `sqlx::Transaction` embrulhada num tipo do store (`ReadTx`), emprestada por `&mut`.

Efeito na Facade: as leituras deixam de ser `&self`. `get`, `lineage_records`, `list`, `relations`, `chain_head` e `verify_chain` passam a `&mut self`; `catalog(&self)` continua `&self`. Isso desfaz a parte "leituras por `&self`, `unchecked_transaction`" da D-1 final; o resto da D-1 vale (`Deps<'d>` por empréstimo, sem `Arc`/`Mutex`, `Hexlog<'d>: Send`). `SqliteConnection` é `Send` e também `Sync` (provado por `examples/sync_probe.rs`), mas como todo método exige `&mut`, ser `Sync` não permite nada útil: a regra "uma instância por thread, o compilador prova" continua. A alternativa que devolveria `&self` seria uma conexão por chamada (1c da D-1): não adotada, o dono escolheu 1b; custo medido pelo `esopo`: 0,6 a 1,7 ms por abertura de conexão.

Custo: 1000 appends (BEGIN IMMEDIATE, count, max seq, insert, COMMIT, `synchronous=FULL`, WAL, mesma máquina): `rusqlite` mediana 4,9 ms; `sqlx` mediana 6,1 ms (cerca de 1,2 ms a mais, dominado pelo `fsync`; p95 do `sqlx` oscila, 53 ms numa rodada, 8 ms noutra, ruído de disco do WSL2; não tirar conclusão do p95).

#### (d) `query!` ou `query`/`bind` em tempo de execução

| Alternativa | Efeito |
|---|---|
| D1. `query!` (feature `macros`) | Funciona sem runtime (`d6m`, 138 pacotes). Exige `DATABASE_URL` apontando para um banco já com o esquema, ou `.sqlx/` gerado por `cargo sqlx prepare`. Sem nenhum dos dois: `error: set DATABASE_URL to use query macros online, or run cargo sqlx prepare`; banco inexistente: `unable to open database file`. `cargo-sqlx` não está instalado aqui, então não testei o modo offline. Um agente sem banco não compila; o banco de compilação nasce de `schema.sql`, criando um passo de build e mais uma fonte de verdade. |
| D2. `sqlx::query`, `query_scalar`, `raw_sql` com `bind`, SQL em `const &'static str` | Zero passo de build, mesmo `schema.sql` embutido por `include_str!`. O `sqlx` 0.9 recusa `String` dinâmico: `E0277 dynamic SQL strings should be audited for possible injections` (só `&'static str` ou `AssertSqlSafe`), o que combina com a regra de SQL fixo. |

Escolhida: D2, feature `macros` desligada. Perde-se a checagem de tipos do SQL em compilação; ela é recuperada pelos testes da T4, que executam cada instrução contra o SQLite real, e pelo `schema.sql` rodado inteiro (provado: o DDL do plano 4.3, 43 linhas de tabelas e 7 triggers, executa por `raw_sql` e cria os 7 triggers; SQLite 3.51.3 tem `ENABLE_FTS5` e `json_extract` com caminho ligado devolve o valor). `u64`: o `sqlx` decodifica `u64` com `try_into` checado, mas não codifica; ligar sempre `i64::try_from`. Sem `FromRow` (`derive` fica desligada): leitura por `Row::try_get`.

#### (e) Abertura de banco novo concorrente: a sequência do plano falha, com `rusqlite` e com `sqlx`

A sequência de 4.3 (`busy_timeout`, depois `journal_mode=WAL`, depois `BEGIN IMMEDIATE`) foi medida N threads abrindo o mesmo caminho novo, cada uma com `busy_timeout` de 10 s, e depois um append:

| Variante | Falhas |
|---|---|
| `rusqlite`, WAL antes do `BEGIN IMMEDIATE` (plano) | 29 de 1600 aberturas (200 rodadas x 8 threads), erro `wal database is locked` |
| `rusqlite`, WAL depois do `COMMIT`, sem retentativa | 0 de 1600 (sorte do agendamento: ver abaixo) |
| `sqlx`, WAL no `SqliteConnectOptions` (plano) | 34 de 1200 (150 x 8), no `connect` |
| `sqlx`, WAL depois do `COMMIT`, sem retentativa | 20 de 1200 (150 x 8) e 47 de 1600 (100 x 16), no `PRAGMA journal_mode=WAL` |
| `sqlx`, WAL depois do `COMMIT`, retentativa em BUSY a cada 20 ms | 0 de 1200 (150 x 8), 0 de 3200 (200 x 16), 0 de 1600 (50 x 32) |

Leitura: o `busy_timeout` não vale para o `PRAGMA journal_mode=WAL` (falha mesmo com 10 s configurados); só a retentativa explícita é robusta, e isso é verdade para qualquer biblioteca (o `rusqlite` sem retentativa passou por acaso de escalonamento). O defeito é do plano e vale para o desenho com `rusqlite` também, não nasce do `sqlx`. O teste `it_should_open_a_fresh_database_from_many_threads_without_error` (plano linha 836) só pega isso se repetir rodadas: a taxa é de 2% por abertura, então cerca de 15% por rodada de 8 threads; fixar **50 rodadas de 8 threads**.

Sequência nova (substitui a de 4.3): (1) `SqliteConnectOptions::new().filename(p).create_if_missing(true).busy_timeout(d).synchronous(Full).foreign_keys(true)` e **sem `journal_mode`**: o `sqlx` 0.9.0 só emite o PRAGMA se pedido, e aplica `sqlite3_busy_timeout` antes de qualquer PRAGMA (`sqlx-sqlite-0.9.0/src/connection/establish.rs:181-184`); (2) `begin_with("BEGIN IMMEDIATE")`; (3) lê `user_version` e a lista de tabelas; banco vazio recebe `schema.sql` (`raw_sql`) e `PRAGMA user_version = 1`; senão confere a versão; (4) `COMMIT`; (5) `PRAGMA journal_mode = WAL` (com `fetch_one`), repetido a cada 20 ms enquanto o erro for `SQLITE_BUSY` (código primário 5, também 261 e 517) até o prazo `busy_timeout`; esgotado o prazo, `HexlogError` `Busy`. Banco que já está em WAL devolve `wal` na hora; o PRAGMA é idempotente.

`BEGIN IMMEDIATE` por `Connection::begin_with("BEGIN IMMEDIATE")` funcionou e serializou as escritas (a contagem final bateu 8 em todas as rodadas); `user_version` por `PRAGMA user_version` e `= 1` dentro da transação funcionou.

**Fechamento é parte do contrato.** `drop` de `SqliteConnection` só avisa a thread de worker; reabrir logo em seguida deu `database is locked` no `connect` em 3 de 6 execuções do laço (300 abre-escreve-solta-reabre), com `busy_timeout` de 50 ms, e o arquivo `-wal` ficou no disco 290 a 298 de 300 vezes. Com `impl Drop for Store` que faz `block_on(conn.close())` (campo `Option<SqliteConnection>`): 0 falhas em 6 execuções x 300 e `-wal` ausente 300 de 300. É o que mantém válida a regra "testes fecham e reabrem para trocar fakes" (D-1). O custo é o checkpoint: laço fecha-reabre de 300 iterações levou 6 a 7 s (cerca de 20 ms por ciclo, com o checkpoint do WAL).

#### (f) `tests/support/tamper.rs` e conflito de `links`

`rusqlite` e `sqlx` não coexistem no grafo, nem como `dev-dependency`: `package libsqlite3-sys links to the native library sqlite3, but it conflicts with a previous package` (`sqlx-sqlite` pede `libsqlite3-sys` 0.37.0, `rusqlite` 0.41.1 pede 0.39). Logo `rusqlite` sai do plano inteiro e a conexão crua do `tamper` é `sqlx`. Provado em `tests/tamper.rs`: com `SqliteConnectOptions::foreign_keys(false)` e `.pragma("ignore_check_constraints", "ON")`, um `INSERT` que viola `CHECK` e outro que viola chave estrangeira passam (`Ok`), `DROP TRIGGER` passa e o `UPDATE` seguinte passa; na conexão normal os mesmos comandos dão `CHECK constraint failed` (275), `FOREIGN KEY constraint failed` (787) e `append-only` do trigger (1811). `raw_sql` aceita só `&'static str`: o `tamper` usa constantes.

#### (g) D-1 e D-0

D-1: `Hexlog<'d>: Send` vale (`Store: Send` provado por asserção de compilação com o `SqliteConnection` dentro); o `Sync` não existe como requisito. A regra "um escritor por instância, o compilador prova" continua e agora vale também para leitura. D-0: nenhum tipo vetado entra no nosso código (`Option<SqliteConnection>` e `block_on` passam no clippy com os lints do workspace; `Arc`, `Mutex` e threads existem dentro do `sqlx` e `futures`, fora do alcance de `disallowed_types`, que só olha o código do workspace). Sem `Arc` e sem `Mutex` mesmo com `block_on`. `futures-executor` passa o critério de biblioteca (5.940 estrelas, repositório da organização `rust-lang`, atualizado em 2026-11-02).

#### Efeitos no pacote de dependências

22 pacotes (`rusqlite`, `bundled`, `serde_json`, `uuid`, `time`) viram cerca de 110 a 138 (esopo: 138 com `derive`; meus `cargo tree` da lib enxuta: 113 linhas únicas). `sha2` entra duas vezes: 0.10.9 (de `sqlx-core`) e 0.11.1 (a nossa); aceitável, mas fica registrado. SQLite 3.51.3 em vez de 3.54.1: o que o plano usa (STRICT, `json_extract` com caminho ligado, WAL, FTS5) está presente; a seção 3 do plano que cita 3.54.1 precisa de ajuste.

#### Decisão D-6 (resumo) e texto exato a trocar

Escolhas: Facade síncrona com `futures_executor::block_on` só em `store/sqlite`; `sqlx 0.9.0` sem runtime e sem `Pool`; portas e leituras da Facade por `&mut`; SQL em tempo de execução com literais fixos; abertura de banco novo com `journal_mode=WAL` depois do `COMMIT` e com retentativa em BUSY; `Drop` do store fecha a conexão. Método: compilação de variantes, 32 threads x repetições, medição de falhas, leitura do código-fonte do `sqlx-sqlite` e `sqlx-core`, DDL do plano executado de verdade.

Plano (`omhl-v2-core-step3.md`):

- Linha 33 (E1, alternativa A) e linha 79: a alternativa recomendada passa a ser `sqlx 0.9.0` com `sqlite-bundled`; trocar "`Connection` é `Send` e não `Sync`: a instância é dona da conexão e escreve por `&mut self`" por "`SqliteConnection` é `Send` e `Sync`, mas todo método exige `&mut`: a instância é dona da conexão e lê e escreve por `&mut self`, sem `Mutex`". Acrescentar o custo: 138 pacotes no fecho contra 22, `sha2` em duas versões, SQLite 3.51.3 em vez de 3.54.1 (a linha 1188 e a seção 3 que citam 3.54.1 viram 3.51.3).
- Linha 168: `# SqliteStore (dono da SqliteConnection), abertura, PRAGMAs, Drop que fecha a conexão`.
- Linha 191: trocar `rusqlite (bundled)` por `sqlx (=0.9.0, default-features = false, features = ["sqlite-bundled"])` e `futures-executor (=0.3.34)`; acrescentar "sem `macros`, `derive`, `json`, `migrate`, `any`; nunca `SqlitePool`"; a lista proibida fica igual (proposta do crítico, a aprovar: acrescentar `smol`, `async-global-executor`, `sqlx-macros`).
- Linha 195 (PRAGMAs na abertura): `journal_mode=WAL` sai da lista de PRAGMAs da abertura da conexão e vai para o passo 5 da sequência abaixo.
- Linha 197 (sequência de abertura): substituir pela sequência de cinco passos do item (e) acima (opções sem `journal_mode`; `BEGIN IMMEDIATE`; `schema.sql` e `user_version = 1` se vazio; `COMMIT`; `PRAGMA journal_mode = WAL` com retentativa a cada 20 ms em BUSY até `busy_timeout`).
- Linha 287 (4.4): trocar "o `SqliteStore` é dono da `rusqlite::Connection`, sem `Mutex`, e as escritas tomam `&mut self`" por "o `SqliteStore` é dono da `sqlx::SqliteConnection`, sem `Mutex`, e escritas e leituras tomam `&mut self`"; acrescentar: "a Facade é síncrona; `futures_executor::block_on` existe só em `store/sqlite`, nunca em `engine`, e a Facade não pode ser chamada de dentro de outro `futures_executor::block_on` (pânico); consumidor com runtime a chama por `spawn_blocking`".
- Assinaturas 4.4 (linhas 304-314): `get`, `lineage_records`, `list`, `relations`, `chain_head` e `verify_chain` passam de `&self` a `&mut self`; `catalog` continua `&self`.
- Portas (linhas 418-453): `fn read(&mut self, f: &mut dyn FnMut(&mut dyn StoreReader) -> Result<(), HexlogError>)`; `append_with(&mut self, tenant, decide: &mut dyn FnMut(&mut dyn WriteReads) -> Result<AppendPlan, HexlogError>)`; todos os métodos de `WriteReads`, `QueryReads` e `VerifyReads` passam de `&self` a `&mut self`; o upcast passa a ser de `&mut dyn StoreReader` para `&mut dyn QueryReads` e `&mut dyn VerifyReads` (provado). A frase "a Facade passa `&dyn QueryReads`" vira `&mut dyn QueryReads`.
- Linha 483 (tabela de mutações, helper `tamper`): "conexão `rusqlite` crua" vira "conexão `sqlx` crua (`SqliteConnection` com `foreign_keys(false)` e `.pragma("ignore_check_constraints", "ON")`)"; `raw_sql` só aceita literal fixo, então o `tamper` usa constantes.
- T4 (linha 619): trocar "`SqliteStore` dono da `Connection`, com `append_with` por `&mut self` e `Connection::transaction_with_behavior(Immediate)` e `read` por `&self` e `unchecked_transaction`" por "`SqliteStore` dono da `SqliteConnection` dentro de `Option` e `Drop` que faz `block_on(conn.close())`; `append_with` por `&mut self` com `begin_with("BEGIN IMMEDIATE")`; `read` por `&mut self` com `begin()`; SQL em `const &'static str`, ligado por `bind` com `i64`; sequência de abertura de cinco passos".
- Testes: `it_should_open_a_fresh_database_from_many_threads_without_error` (linha 836) repete 50 rodadas de 8 threads, cada rodada com banco novo; novo teste de `store` que abre, escreve, solta o `Hexlog` e reabre o mesmo caminho 100 vezes sem `database is locked` (cobre o `Drop` que fecha); `it_should_be_send` continua; `unchecked_transaction` some de toda menção. Contagem: +1 teste (127 passa a 128), a confirmar pela sessão principal.
- Seção 16.8 e o ADR da linha 889: trocar "SQLite via `rusqlite` com `bundled` e `Connection` dona dentro de `Hexlog<'d>` (`Send` ...)" por "SQLite via `sqlx` 0.9.0 (`sqlite-bundled`) com `SqliteConnection` dona dentro de `Hexlog<'d>`, Facade síncrona".
- Seção 16.5: a frase "reads `&self` via `unchecked_transaction`" (D-1 final) está superada pela D-6; as leituras são `&mut self`.
- Limite a documentar: nenhum PRAGMA `journal_mode` por opção do `sqlx` (o padrão do 0.9.0 não emite); `STRICT` e triggers inalterados.

Contrato de costuras (`omhl-v2-core-step3-seams.md`): a T2 não toca no store; a única linha afetada é a costura 4, que cita "`SeqIds(AtomicU64)`" e "reserve(&mut self, ...)", sem mudança. A costura da T4, a escrever, deve trazer: as portas por `&mut`, o `Drop` que fecha, a sequência de cinco passos e o teste de reabertura em laço.

`[DONO]`: nenhum bloqueante. Uma aprovação pequena: acrescentar `smol`, `async-global-executor` e `sqlx-macros` à lista proibida (só a endurece).

### D-7: validador próprio do subconjunto de JSON Schema — fechamento dos pontos em aberto (lista branca, corte, memo, suíte oficial, guarda de inteiro, testes, `Definitions`, `def_hash`)

Base: [json-schema-subset.md](../research/gray-box/json-schema-subset.md) ("Algoritmo proposto", "Testes próprios", "Pontos em aberto"), [catalog-schema.md](../research/gray-box/catalog-schema.md), o `hexlog` TS em `/home/gabriel/personal/hexlog` (commit `5d1e22b`) e um protótipo descartável do compilador e do avaliador (cerca de 300 linhas) em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/critic/d7` (`src/lib.rs`, `src/catalog.rs`, `tests/`, `examples/bench.rs`, `fixture/`), mais `d8`. Passa em `clippy` com os lints do workspace. A suíte oficial foi baixada no SHA pino `245a45abc90190c0bff04d97e6d2b541b4bb9f12` e filtrada com o script do relatório: reproduzi **21 arquivos, 124 grupos, 392 testes**, ~50 KB compactos (124 KB indentados).

#### Resultado do protótipo contra a suíte

O protótipo passa nos 392 testes, 0 falhas. Para saber se a suíte discrimina, mutei o avaliador seis vezes, e cada mutação foi pega: `not` ignorado (23 falhas), `oneOf` como `anyOf` (8), irmãos de `$ref` ignorados (1), `additionalProperties` olhando fora do `properties` do mesmo objeto (4), `minLength` em bytes (2), `enum` ignorado (26). Fraqueza: a suíte só confere válido ou inválido; ponteiro e `code` da violação não são conferidos por ela, então os testes próprios continuam necessários (item f).

**Achado F-1 (a suíte não roda pela API pública).** Dos 124 grupos, só 4 têm raiz `type: "object"`. A regra "raiz tem de ser objeto" do plano (teste da linha 678) recusa os outros 120 em `Catalog::new` (`root-not-object`). Rodei por `compile_any` (a compilação sem a regra de raiz): 392/392. Consequência: `it_should_pass_the_filtered_official_suite_without_skips` é **teste de unidade** em `src/catalog/validate.rs` (`#[cfg(test)]`), que chama o compilador interno e lê os JSON por `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/json-schema-test-suite")`; a regra de raiz fica numa função separada, chamada por `Catalog::new` antes do compilador. Embrulhar cada schema da suíte num objeto de raiz não serve: reescreve `$ref: "#"`.

**Achado F-2 (`validate_data` não é público).** Três dos 10 testes de `tests/catalog.rs` do plano (`it_should_list_every_violation_with_json_pointers_up_to_50`, `it_should_refuse_numbers_that_are_not_safe_integers_in_data`, `it_should_accept_data_that_satisfies_the_schema`) chamam a validação de `data`, mas o plano (linha 687) diz que a porta `Definitions` não é API pública e a T3 vem antes de qualquer `append`. Sem mudança o `test-engineer` não consegue escrever os três. Correção: `Catalog` ganha o método inerente público `validate_data(&self, name: &TypeName, data: &Value) -> Result<(), Vec<DataViolation>>`, e `DataViolation` é reexportado por `hexlog::*`; o `impl Definitions for Catalog` só delega. O mesmo vale para o `def_hash`, usado pelo teste da linha 680: `Catalog::type_hash(&self, &TypeName) -> Option<&Hash>` e `Catalog::relation_type_hash(&self, &RelationTypeName) -> Option<&Hash>`, públicos.

#### (a) Lista branca final e anotações

Método: (1) varri os schemas reais do dono, os arquivos `skills/hexlog-setup/references/*.json` e `owner-seat/types/*.json` do TS (oito schemas de node type); (2) contei palavra-chave no repositório TS inteiro (`test`, `docs`, `skills`, `dev`, `src`): `type` 88, `minLength` 15, `maxLength` 15, `properties` 13, `additionalProperties` 12, `required` 11, `format` 7, `enum` 3, `examples` 2, `$schema` 1; (3) a suíte filtrada cobre cada palavra da lista.

Lista final: a do relatório, sem mudança (`type`, `enum`, `const`, `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems`, `required`, `properties`, `additionalProperties`, `items`, `allOf`, `anyOf`, `oneOf`, `not`, `$ref` interno, `$defs`, `$schema` só na raiz e com o URI 2020-12). Anotações `title`, `description`, `$comment`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly`: entram, ignoradas, com conferência do tipo do valor (protótipo: `title: 1` dá `keyword-type`). Custo de uma linha cada; sem elas um modelo com `description` seria recusado sem motivo.

**`[DONO]` — `format`.** Seis dos oito schemas reais usam `"format": "attachment"` (marca de anexo do TS, D-16: `three-phase-model.example.json`, `minimal-model.example.json`, `owner-seat/types/evidence.json`, `premise.json`). O plano manda recusar `format` (relatório: "anotação por padrão no 2020-12; recusar evita falsa sensação de validação") e anexos estão fora do passo 3 (plano 4.5). Efeito: modelo escrito para o TS não carrega no `Catalog` do Rust. Alternativas: (i) recusar `format` (como está): o dono edita os modelos; (ii) aceitar `format` com qualquer string e ignorar: carrega tudo, mas `"format": "email"` passa sem checar (no TS o `ajv-formats` checaria: comportamento diverge em silêncio); (iii) aceitar só `"format": "attachment"`, tratado como string comum no passo 3, e recusar os outros valores com `format-not-supported`. Recomendação técnica: (iii), porque carrega os modelos reais e não cria falsa checagem; mas é decisão sobre compatibilidade com os modelos do dono. Até a resposta, a contagem de testes assume (iii) (um teste: `it_should_accept_format_attachment_and_reject_any_other_format`).

Palavra desconhecida segue recusada (`keyword-not-allowed`), contra o Core §4.3.1; o ADR registra o desvio. Os oito schemas reais só usam palavras da lista (mais o `format` acima), então o desvio não quebra nenhum modelo existente.

#### (b) Corte em 50

O TS responde: `capDetails` (`src/errors.ts:159-177`) devolve `unique.slice(0, budget)` mais o marcador, e `test/errors.spec.ts:306` e `:471`, `test/commands/register.spec.ts:349` e `:426` afirmam `toHaveLength(51)`. Ou seja, **50 itens reais mais `too-many-errors`, 51 no total**, apesar do comentário "corta em MAX_DETAILS". O relatório recomendava a leitura A (49 mais o marcador); o formato de referência é a leitura B. Protótipo: com 49, 50 e 51 violações devolve 49, 50 e 51 itens (o último é `too-many-errors` só no terceiro caso); com 60, 51. Parar a coleta em 51 achados limita o trabalho. Diferença do TS: a mensagem do marcador do TS traz a contagem (`N more errors omitted`); o Rust para de contar em 51, então a mensagem não leva número.

**Achado F-3 (corte feito duas vezes não fecha no lote).** O plano diz que o corte é feito só em `validate_data` e o engine repassa (linha 403). Com um lote de dois itens inválidos, cada chamada devolve até 51, e o `INVALID_RECORD` do teste da linha 702 junta "as violações dos dois": 102 detalhes, acima do teto de `details` (50). O TS corta de novo no lote (`capDetails` em `prepareBatch`). Regra: o engine junta as violações de todos os itens, em ordem de item, com o ponteiro prefixado pelo índice do item; descarta os marcadores `too-many-errors` vindos dos itens; se o total passar de 50 **ou** algum item trouxe marcador, corta em 50 e acrescenta um único `too-many-errors` com `path ""`. Teste novo na T5a: `it_should_cap_the_violations_of_a_batch_at_50_with_a_single_marker`.

**Ponteiro de `required` (ponto 2 do relatório).** O TS reporta o ponteiro da propriedade ausente, não o do objeto: `offendingProperty` (`src/adapters/validator.ts:18-27`) acrescenta `missingProperty` ao caminho, e `test/adapters/validator.spec.ts:524` espera `/text` com código `required`. Escolhido: igual ao TS, um achado por propriedade ausente, na ordem do array `required` do schema. O ponteiro do objeto (a spec, §12.4.2) perde a informação que o usuário precisa. Protótipo: `required: [m1, m2, "a/b~c"]` com `m1` e `m2` ausentes dá `/m1` e `/m2`. `additionalProperties: false` já aponta a chave excedente, como o TS.

Os demais pontos do relatório: (3) números do schema só inteiros seguros, mantido; (4) palavra desconhecida recusada, mantido; (5) `enum` vazio e com duplicatas aceitos, mantido; (6) o teto de profundidade foi medido, ver (c); (7) palavras baratas ficam fora até pedido do dono.

#### (c) Memoização: chave `std::ptr::from_ref(v) as usize`

| Alternativa | Medido (release, µs por chamada; "típico" = 20 campos, "DAG" = `d0..dn` com `allOf` ou `anyOf` duplo) |
|---|---|
| M1. Sem memo | DAG com `anyOf`, n=16: 9.349 µs; n=22: 596.485 µs (cresce 64x a cada 6 níveis, 2^n); n=30 não termina em tempo útil. `allOf` válido, n=22: 292 ms. Típico válido: 1,2 µs. |
| M2. Memo em todo (nó, posição), chave por endereço | DAG n=30: 7,9 µs (válido 7,3 µs, 92 avaliações); típico válido: 3,6 µs; recursivo com 120 níveis válido: 34,5 µs, inválido: 62 µs. |
| M3. Memo só em nós compartilhados (grau de entrada maior que 1 no grafo de "mesma posição") | DAG n=30: 3,9 µs; típico: 1,8 µs; recursivo inválido a 120 níveis: **678 µs** (10x pior que M2, porque o relatório reavalia o subárvore em cada nível). |
| M4. M3 na fase A e M2 na fase B (híbrido) | Em todos os casos medidos fica perto do melhor: DAG 3,9 µs, típico 1,6 µs, recursivo inválido 70 µs. |
| M5. Memo por caminho (`String` do ponteiro JSON) | Não implementei. Cada descida passa a alocar e fazer hash de texto de tamanho crescente; só seria exigido se a chave por endereço fosse proibida. |

A chave por endereço passa na regra de memória e nos lints: o protótipo compilou limpo com `unsafe_code = "forbid"`, `disallowed_types`, `disallowed_methods`, `needless_pass_by_value`, `needless_pass_by_ref_mut`, `redundant_clone`, `implicit_clone` e `large_types_passed_by_value` em `deny`; nenhum tipo vetado é usado (um `HashMap<(u32, usize), bool>` e um `HashSet`). `std::ptr::from_ref(v) as usize` só acusa `clippy::as_conversions` (restrição, desligada no workspace). Forma melhor, sem `as`: `std::ptr::from_ref(v).addr()` (estável desde o Rust 1.84, toolchain 1.97), que o protótipo usa. É estável porque `data` fica emprestado por `&` durante a chamada inteira; endereços de elementos de `Vec` e de `Map` são distintos e nada é movido ou liberado.

Escolhida: **M2**, a mais simples. M4 ganha até 28 µs por chamada contra M2 no pior caso medido, e custa cerca de 12 linhas (`shared`, troca de fase); contra a transação de 4,9 ms (item (c) da D-6) são 0,6%. O custo de M2 (cerca de 2 µs a mais no caso típico) fica abaixo do ruído do I/O. Registro: se um dia a validação virar caminho quente, M4 é a otimização medida.

Dois cuidados que o protótipo mostrou e o contrato deve fixar. (1) Estouro de profundidade não pode virar "inválido" dentro do `valid`: um `not` o transformaria em válido. O protótipo marca `overflow` no `Run`, e a validação inteira devolve uma única violação `max-depth-exceeded`. (2) **Teto de profundidade de avaliação: 1024, não 512.** Medi a pilha: com o teto desligado e uma thread de 2 MiB, em build de teste (debug), uma cadeia de `$ref` válida sobrevive a 2.000 níveis e estoura em 4.000; inválida (relatório), sobrevive a 2.000 e estoura em 3.000 (em release 10.000 sobrevive, 20.000 estoura). Um dado de 127 níveis, o máximo do `serde_json::from_str`, com recursão por `$ref` e dois `allOf` aninhados por nível passa com o teto em 1.024 e em 512; o teto de 512 já fica na beirada (cerca de 4 níveis por nível de dado) e um schema com 5 a 6 por nível recusaria documento legítimo. 1.024 deixa margem de 2x contra o estouro de uma thread de 2 MiB em debug. O teto só vale para avaliação; o teste da cadeia de 10.000 `$defs` passa a esperar `max-depth-exceeded`.

#### (d) Suíte oficial como fixture

Decisões (todas conferidas no SHA pino):
- Pasta: `crates/hexlog/tests/fixtures/json-schema-test-suite/`, com os 21 arquivos originais pelos nomes originais (`allOf.json`, `ref.json`...), compactos (~50 KB), mais `LICENSE` (MIT, Copyright (c) 2012 Julian Berman: a licença exige o aviso junto das cópias), `filter.py` (o script do relatório, mais o `gen.py` que escreve a pasta) e `MANIFEST.md`.
- `MANIFEST.md` guarda: repositório `json-schema-org/JSON-Schema-Test-Suite`, SHA `245a45abc90190c0bff04d97e6d2b541b4bb9f12` (2026-11-03), data da cópia, caminho de origem `tests/draft2020-12/`, o comando que regenera, e as contagens 21/124/392 por arquivo.
- Runner: o teste de unidade da F-1, que **afirma as contagens 21 arquivos, 124 grupos e 392 testes antes de rodar** e não tem ramo de pular: qualquer `Err` de compilação ou divergência de `valid` é falha. Isso fecha dois buracos: filtro que perde um grupo em silêncio e runner que pula.
- O varredor de vocabulário do plano (C7) lê só `crates/hexlog/src` e `schema.sql`; a pasta de fixtures fica fora dele. As descrições dos testes da suíte não têm as palavras vetadas (conferi `unit`, `directive`, `diretriz`, `work`).
- Os 12 grupos de `ref` que o relatório pede para conferir à mão: o protótipo os resolve todos por `compile_any`; os que a heurística do script aceitaria como "alvo que resolve" mas não é posição de schema ficaram fora do fixture por serem raros, e o teste próprio `it_should_reject_a_ref_to_a_non_schema_location` cobre essa recusa.

#### (e) Guarda de inteiro seguro

Uma só função, **`engine::limits::json_issues(value: &Value, limit: usize) -> Vec<(String, &'static str)>`**, `pub(crate)`, iterativa (pilha explícita, ordem de documento, ponteiro RFC 6901 já escapado, para em `limit` achados). Mora em `engine/limits.rs` (módulo `pub(crate)` do engine) e é usada por `decide` (via a validação de `data`) e por `catalog` (`Catalog::new` e `validate_data`); a dependência `catalog` para `engine` já existe e a trava de imports só proíbe o sentido contrário, e `catalog` não importa nada da lista proibida. Achados e códigos: `number-not-safe-integer` (número que não é `i64` ou `u64` dentro de ±(2^53 − 1); `1.0`, `1e2`, `0.5`, `-0`, `-0.0`, ±2^53 e `18446744073709551615` caem aqui, medido; `0`, ±(2^53 − 1) passam) e `max-depth-exceeded`.

Por que a profundidade entra na mesma função: o `Value` de um chamador, montado por código e não por `from_str`, não tem o limite de 128 níveis do parser; medi que o `Drop` recursivo do `serde_json::Value` estoura uma thread de 2 MiB entre 10.000 e 30.000 níveis. O guarda iterativo resolve 1.000.000 de níveis sem estourar a pilha, então é o lugar certo para recusar `data` e schema com mais de 128 níveis (127 contêineres mais o escalar, que é o que o `from_str` aceita: medi 127 arrays aninhados aceitos e 128 recusados) antes de qualquer outra coisa tocar o valor. Teste: `it_should_refuse_data_nested_deeper_than_the_parser_limit`.

Onde cada chamada entra: `Catalog::new` aplica a função **ao documento inteiro do schema**, não só a `minimum`/`maximum`/comprimentos/`enum`/`const` como o relatório propunha. Motivo: o `def_hash` é `canonical` do documento, e `canonical` devolve erro para inteiro acima de 2^53 (D-4); recusar número inseguro em qualquer lugar (inclusive `default` e `examples`) garante que o hash nunca falha por número e que existe uma só regra, em vez de uma por posição. Custo: `default: 0.5` é recusado; nenhum schema real do dono tem número em `default`. `validate_data` aplica a função ao `data` antes de avaliar o schema; havendo achado, devolve só esses (não avalia o schema sobre `data` com número inseguro). **Ordem no passo 4 da escrita** (para a D-3/D-4): guarda de `data` vem antes de `canonical(data)` do teto `data_max_chars`; senão um inteiro 2^60 viraria `INVALID_INPUT number-out-of-range` em vez de `INVALID_RECORD number-not-safe-integer`.

#### (f) Testes próprios: quais entram, com o nome exato

Dos 16 do relatório: 14 entram em `tests/catalog.rs` (API pública, schema com raiz `type: "object"`, dados por `validate_data`), 1 vira teste de unidade (a suíte, F-1) e 1 funde no teste que o plano já tem (o do corte). Nomes exatos, todos com o protótipo rodando o comportamento descrito:

| Teste | Onde | Afirma (valor medido no protótipo) |
|---|---|---|
| `it_should_reject_a_ref_cycle_without_descending_into_the_instance` | `tests/catalog.rs` | `$ref: "#"`, `allOf`, `anyOf`, `not` apontando `#`, e `a -> b -> a` em `$defs`: `INVALID_SCHEMA` com detalhe `ref-cycle` |
| `it_should_accept_a_recursive_ref_guarded_by_properties` | `tests/catalog.rs` | `properties.child = {"$ref": "#"}`, `items` e `additionalProperties` recursivos compilam |
| `it_should_validate_a_shared_ref_dag_in_polynomial_time` | `tests/catalog.rs` | `d0..d30` com `allOf` duplo e com `anyOf` duplo inválido; roda em thread com `recv_timeout` de 5 s (sem memo o caso `anyOf` com n=22 já leva 0,6 s e n=30 não termina; o timeout transforma travamento em falha) |
| `it_should_not_overflow_the_stack_on_deeply_nested_data` | `tests/catalog.rs` | 127 níveis de dado com schema recursivo por `$ref` e `allOf` aninhado, thread de 2 MiB, em build de teste |
| `it_should_report_max_depth_exceeded_on_a_very_long_ref_chain` | `tests/catalog.rs` | cadeia de 10.000 `$defs`: uma violação `max-depth-exceeded`, nada de pânico |
| `it_should_treat_property_names_as_names_not_keywords` | `tests/catalog.rs` | `properties` com chaves `pattern`, `$ref` e `$defs` compila |
| `it_should_resolve_ref_pointers_with_escapes_and_empty_tokens` | `tests/catalog.rs` | alvos `a~1b`, `c~0d`, vazio (`#/$defs/`), `x%20y`, `q%22` resolvem e validam |
| `it_should_reject_a_ref_to_a_non_schema_location` | `tests/catalog.rs` | `#/enum/0`, `#/properties`, alvo inexistente: `ref-unresolved`; `other.json#`, `http://x/y#` e `#foo`: `ref-not-internal` |
| `it_should_evaluate_ref_siblings` | `tests/catalog.rs` | `{"$ref": ..., "maxLength": 2}` reprova `"abc"` com `max-length` |
| `it_should_not_look_into_all_of_for_additional_properties` | `tests/catalog.rs` | `allOf` com `properties.a` e `additionalProperties: false` no pai: `a` é recusada |
| `it_should_report_a_single_violation_for_a_failing_any_of` | `tests/catalog.rs` | `any-of` e `one-of` no ponteiro do valor, sem erros dos ramos |
| `it_should_count_string_length_in_code_points` | `tests/catalog.rs` | `"💩"` com `maxLength: 1` passa |
| `it_should_use_a_pointer_with_tilde_escapes_in_violations` | `tests/catalog.rs` | chave `a/b~c` vira `/a~1b~0c` |
| `it_should_reject_non_safe_integers_in_the_schema` | `tests/catalog.rs` | `minimum: 1.5`, `enum: [1.0]`, `const` aninhado com `1e2`, `minimum: 9007199254740992`: `number-not-safe-integer` com ponteiro; `maximum: 9007199254740991` passa |
| `it_should_pass_the_filtered_official_suite_without_skips` | unidade, `src/catalog/validate.rs` | 21 arquivos, 124 grupos, 392 testes, 0 falhas |

Acréscimos meus, ainda sem nome no plano:

| Teste | Onde | Afirma |
|---|---|---|
| `it_should_name_the_missing_property_in_the_required_pointer` | `tests/catalog.rs` | `required` ausente dá ponteiro `/m1`, um achado por propriedade, ordem do `required` |
| `it_should_reject_a_keyword_outside_the_whitelist` | `tests/catalog.rs` | `requred`, `$id`, `$dynamicRef`, `uniqueItems`, `format: "email"`: `keyword-not-allowed` com ponteiro da palavra |
| `it_should_accept_annotation_keywords_and_check_their_type` | `tests/catalog.rs` | `title`, `description`, `$comment`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` aceitos; `title: 1` dá `keyword-type` |
| `it_should_not_add_too_many_errors_at_exactly_50_violations` | `tests/catalog.rs` | 49 e 50 violações devolvem 49 e 50 itens sem marcador; 51 e 60 devolvem 51 (50 mais `too-many-errors` em `""`) |
| `it_should_refuse_data_nested_deeper_than_the_parser_limit` | `tests/catalog.rs` | 128 níveis programáticos dão `max-depth-exceeded`; 127 passam |
| `it_should_accept_format_attachment_and_reject_any_other_format` | `tests/catalog.rs` | condicional à resposta do `[DONO]` sobre `format` |
| `it_should_match_the_ts_oracle_def_hash_for_each_definition_shape` | unidade, `src/catalog/types.rs` | seis definições, hashes do item (h) |
| `it_should_cap_the_violations_of_a_batch_at_50_with_a_single_marker` | T5a | F-3 |

Mudanças em testes que o plano já tem: `it_should_list_every_violation_with_json_pointers_up_to_50` passa a dizer "dado com 60 violações devolve 50 mais `too-many-errors`, 51 itens, nos 50 primeiros na ordem determinística"; `it_should_reject_a_malformed_equivalent_uri_and_accept_a_wellformed_one_without_resolving_it` ganha os casos do item (h). Contagem: `tests/catalog.rs` vai de 10 para 10 + 14 + 5 + o de `format` = 30 (29 se o dono escolher recusar `format`); testes de unidade da T3: 3 (o da porta `Definitions`, a suíte e o do `def_hash`), fora da contagem; T5a: +1. Delta em nomes únicos nas tabelas da seção 5: +21 (+20 sem o de `format`); o total de 128 vira 149 (148). O teste de unidade da guarda (`it_should_flag_unsafe_numbers_and_depth_in_document_order`, em `src/engine/limits.rs`, T2) fica fora da contagem, como os outros de unidade.

Funcionamento do teste de DAG com timeout: `recv_timeout` não mata a thread, ela vaza até o fim do processo; aceitável em teste porque a falha já aborta a execução.

#### (g) `Catalog: Send` e a forma de `Definitions`

Método: protótipo `src/catalog.rs` (um `HashMap<Name, Entry>` com `Entry { def, info, compiled }`) mais `tests/defs.rs`, que compila `&dyn Definitions`, segura o `&NodeTypeInfo` emprestado enquanto chama `validate_data`, e testa em outra thread.

| Alternativa | Resultado |
|---|---|
| G1. Retorno por dono: `Option<NodeTypeInfo>` (como o plano, linha 400) | Compila, mas cada consulta clona o `Hash` (uma `String` de 64 caracteres) e o `NodeTypeInfo` por item do lote. Contra a regra "empreste em vez de clonar". |
| G2. Retorno emprestado: `Option<&NodeTypeInfo>`, `Option<&RelationTypeInfo>` | Compila; `dyn Definitions` segue objeto de trait; o `&NodeTypeInfo` convive com `validate_data(&self)` (ambos `&self`); `Catalog` guarda o `info` pré-calculado uma vez em `Catalog::new`. `Catalog: Send + Sync` por construção (asserção `is_send_sync::<Catalog>()` verde; só `Vec`, `HashMap`, `String`, `Value`). |

Escolhida: G2. `validate_data` **precisa do `TypeName`**: é a chave do mapa de schemas compilados. O custo é uma segunda busca por nome por item (mapa de `String`, dezenas de nanossegundos; irrelevante contra a transação de ms). A alternativa de `node_type` devolver um objeto com `validate` própria muda a porta e não traz ganho medido. O `Run` (memo, `done`, `seen`, saída) é local a cada chamada, então `validate_data(&self)` não precisa de tipo interior mutável e vários threads podem validar com o mesmo `Catalog`. Assinaturas do contrato: `fn node_type(&self, name: &TypeName) -> Option<&NodeTypeInfo>`, `fn relation_type(&self, name: &RelationTypeName) -> Option<&RelationTypeInfo>`, `validate_data` inalterada.

#### (h) `Serialize`/`Deserialize` e `def_hash` estável

Método: protótipo de `TypeDef` e `RelationTypeDef` com serde; hash `sha256hex(to_string(to_value(def)))` (igual ao JCS para chaves ASCII e inteiros; o produto usa `canonical`); mesmo documento recalculado no oráculo TS (`canonicalize` 5.1.0 do `node_modules` do `hexlog`, commit `5d1e22b`) em `d7/oracle.mjs`. Seis formas, os 6 hashes iguais:

| # | Documento | `def_hash` (igual no Rust e no oráculo) |
|---|---|---|
| 0 | `{"name":"note","schema":{"type":"object"},"requires_snapshot":false}` | `dd42860a5d64e12ee33e608d711b6efaa53bdbf1d79eaa531b3587f82b754447` |
| 1 | o mesmo sem `requires_snapshot` e com as chaves em outra ordem | `dd42860a5d64e12ee33e608d711b6efaa53bdbf1d79eaa531b3587f82b754447` |
| 2 | schema com `é`, `minimum: -5`, `requires_snapshot: true`, `equivalent` | `817730a409826e94700ef25d541514a48aa6d486ece026be55c058713372ac75` |
| 3 | relation `after`, `from: [note, task]`, `to: [note]`, `acyclic: true` | `ff31663197ac307219e96d5cfcbdf9e7f70479248fb3e14dae404af5d2e404d5` |
| 4 | o mesmo com `from: [task, note]` | `fb3fa70df5f1b3060e97550892bf49498041c65bb823613a27e74498cabf5613` |
| 5 | relation `links` sem `from`, `to` nem `acyclic` | `16f4fa81d70a0010f92708972fcb19aa4b928b6772dbce53d61419810e2c1197` |

O que o contrato precisa fixar:
- Nomes de campo exatamente os do Rust (`name`, `schema`, `requires_snapshot`, `equivalent`, `from`, `to`, `acyclic`); `#[serde(deny_unknown_fields)]` nos dois (medido: `requires_snapshots` dá `unknown field`).
- `requires_snapshot` e `acyclic` com `#[serde(default)]` e **sempre escritos** na serialização: documento sem o campo e documento com `false` têm o mesmo hash (linhas 0 e 1).
- `equivalent`, `from` e `to` com `#[serde(default, skip_serializing_if = "Option::is_none")]`: ausente é omitido, nunca `null`; `null` na leitura vira `None`, igual a ausente (mesmo hash). `from: []` é diferente de ausente (hash diferente).
- A ordem das listas `from` e `to` faz parte do hash (linhas 3 e 4 diferem): o hash é da definição como escrita, sem ordenar nem deduplicar. Consequência a documentar: reordenar `from` muda o `def_hash` e portanto o que os records novos gravam.
- `TypeName` e `RelationTypeName` serializam como string simples (`#[serde(try_from = "String", into = "String")]`) e recusam nome inválido na leitura (medido: `"Bad Name"` falha).
- Chave duplicada: no nível do struct o `serde` recusa (`duplicate field`); dentro de `schema` (um `Value`) a última vence em silêncio e não é detectável depois do parse. Fica registrado como limite, já previsto no relatório.
- `[DONO]`-leve: relação com `from: []` ou `to: []` nunca aceita nenhum item. Recomendo `Catalog::new` recusar com `empty-endpoint-list` (definição que torna a relação inutilizável é erro de digitação, não intenção); sem confirmação, o comportamento é aceitar e a relação fica sem uso.

**`equivalent` com `url`** (plano, linha 561). `Url::parse` é permissivo (WHATWG): medi que aceita ` https://example.com ` (espaços nas pontas), `https://exa<TAB>mple.com` (o tab é removido), `https://example.com/a<LF>b`, `https://example.com/a b` (espaço virado `%20`), controle C0, `HTTP://EXAMPLE.com` e `javascript:alert(1)`; e recusa vazio, só espaço, `foo/bar`, `/a/b`, `http:`, `http://`, porta 99999, `http://[::1`. Como o `def_hash` cobre a **string original**, uma string que o `url` normaliza em silêncio entraria no hash diferente da URI que o `url` entende. Escolhido: `Url::parse(s).is_ok()` **mais** recusa de qualquer caractere menor ou igual a U+0020 ou igual a U+007F (`equivalent-uri-format`); não exijo `as_str() == s`, que recusaria host em maiúscula e IDN legítimos. Esquemas não são restritos (`urn:`, `mailto:` passam). Sem teto de comprimento no plano; `Url::parse` aceita 100.000 caracteres; fica sem teto, a decisão sobre teto é do dono se quiser.

### D-8: detecção de ciclo da T5b — `petgraph` 0.8.3 (`DiGraphMap` e `has_path_connecting`) no lugar da busca em profundidade própria

- **Pergunta:** a regra do dono manda preferir biblioteca apta para lógica já feita; a regra de menor código também pesa. O relatório [decide-rules.md](../research/gray-box/decide-rules.md) diz que a `petgraph` é apta (4.055 estrelas, MIT OR Apache-2.0, commit em 2026-10-31, 378 `#[test]`, CI com cinco workflows) e que a busca própria tem cerca de 14 linhas. Comparar comportamento e fecho de dependências e decidir.
- **Alternativas:**
  - A. Busca própria: `HashMap<&str, Vec<&str>>` de adjacência, `HashSet` de visitados e pilha explícita (iterativa); `closes_cycle(from, to)` é `from == to` ou existe caminho `to -> from`.
  - B. `petgraph`: `DiGraphMap<&str, ()>::from_edges(...)` e `has_path_connecting(&g, to, from, None)`.
- **Interferência no comportamento final:**
  - Equivalência: 791.598 veredictos comparados entre A e B em 20.000 grafos aleatórios (2 a 31 nós, até 59 arestas; inserção incremental com `add` e reconstrução do zero, incluindo nós que não estão no grafo e auto-aresta): **0 divergências** (157.447 deram ciclo). `has_path_connecting` com nó ausente devolve `false` sem pânico, e com `from == to` devolve `true`, que é a resposta certa para auto-aresta; então a chamada direta já cobre os casos de borda sem proteção extra.
  - Custo: reconstruir o grafo e consultar uma vez (como o `decide` faz por item de relation), cadeia de arestas: 100 arestas 17,5 µs (A) contra 21,5 µs (B); 1.000 arestas 278 µs contra 233 µs; 10.000 arestas 4,6 ms contra 4,0 ms. Empate; o relatório já dizia que nenhuma das duas evita montar o grafo por chamada (O(arestas)).
  - Código: A tem 14 linhas mais um tipo; B tem duas linhas (a construção e a consulta). A busca de `has_path_connecting` é iterativa (`Dfs`), sem recursão de pilha.
  - Memória e regra do dono: `DiGraphMap<&str, ()>` empresta os `&str` das relations (vindas de `live_relation_pairs` e dos itens do lote); nenhum `Rc`/`Arc`/`RefCell` no nosso código; o protótipo passa nos oito lints do workspace (`disallowed_types`, `disallowed_methods`, `needless_pass_by_value`, `needless_pass_by_ref_mut`, `redundant_clone`, `implicit_clone`, `large_types_passed_by_value`) sem aviso.
  - Fecho de dependências (partindo do `Cargo.toml` real do `hexlog`, 113 pacotes únicos de `cargo tree -e normal`): B soma **4 pacotes** (`petgraph` 0.8.3, `fixedbitset` 0.5.7, `foldhash` 0.1.5 e `hashbrown` 0.15.5), 117 no total; `indexmap` e `equivalent` já estão no fecho por causa do `sqlx`. Com as features padrão a diferença é a mesma (os mesmos 4 pacotes), então o corte `default-features = false, features = ["std", "graphmap"]` não economiza pacote; é só higiene. Sem `std`, `DiGraphMap` pede um terceiro parâmetro (o hasher) e não compila com dois: `std` é obrigatória. Nenhum crate da lista proibida entra (0 ocorrências de `tokio`, `async-std`, `hyper`, `reqwest`, `ureq`, `git2`, `gix`). Custo real: `hashbrown` passa a existir em três versões no `Cargo.lock` (0.15.5, 0.16.1, 0.17.1) e `foldhash` em duas.
- **Escolhida e por quê:** B, `petgraph`. A regra do dono pede biblioteca apta para lógica já feita, e a apta cobre o caso com a mesma correção (0 divergências em 791 mil veredictos), o mesmo custo e menos código (duas linhas contra quatorze mais um tipo e um teste de busca). O que pesa contra é só o fecho (+4 pacotes, todos de estrutura de dados, sem rede nem runtime); como o `sqlx` já trouxe `indexmap`, `hashbrown` e `hashlink`, o acréscimo marginal é pequeno. Se o dono preferir o fecho mínimo, A é aceitável e equivalente, mas é a alternativa que escreve código que uma biblioteca apta já escreveu.
- **Dependência a registrar** (plano, linha 191 e T1): `petgraph = { version = "=0.8.3", default-features = false, features = ["std", "graphmap"] }`. A trava de arquitetura (`it_should_not_depend_on_forbidden_crates`) não muda.
- **Método:** `d8` (`src/lib.rs` com as duas implementações, `examples/cmp.rs` com a comparação aleatória e o custo, `examples/absent.rs` com os nós ausentes; `cargo tree` antes e depois sobre o `Cargo.toml` do `hexlog`; `cargo clippy` com os lints do workspace). Leitura de [decide-rules.md](../research/gray-box/decide-rules.md), seção `petgraph`.
- **Trecho a trocar no plano (T5b, "ciclo"):** a busca de ciclo usa `petgraph::graphmap::DiGraphMap<&str, ()>` montado a cada item de relation `acyclic` com as relations vivas do tenant (`live_relation_pairs`), as de `native_relations` e as do próprio lote já aceitas, e decide com `petgraph::algo::has_path_connecting(&g, to, from, None)`; auto-aresta (`from == to`) é ciclo pela mesma chamada. Os testes da seção 5 que já cobrem o ciclo (`it_should_reject_a_cycle_on_an_acyclic_relation_including_native_relations` e `it_should_ignore_relations_with_a_retired_endpoint_when_detecting_cycles`) bastam; nenhum teste novo, porque a escolha da biblioteca não muda o comportamento (0 divergências medidas). Sugestão sem custo de contagem: o primeiro teste inclui os casos auto-aresta e aresta do próprio lote.

#### Trecho a colar no contrato de costuras (rascunho da T3, costura 6: catálogo)

```text
### Costura 6: catálogo e validação (API pública hexlog::*)

Pega: definições lidas de JSON, recusa de schema fora do subconjunto, hash de definição estável, violações de data com ponteiro. Deixa passar: o uso da porta Definitions pelo decide (T5a).

pub struct TypeDef { pub name: TypeName, pub schema: serde_json::Value, pub requires_snapshot: bool, pub equivalent: Option<String> }
pub struct RelationTypeDef { pub name: RelationTypeName, pub from: Option<Vec<TypeName>>, pub to: Option<Vec<TypeName>>, pub acyclic: bool, pub equivalent: Option<String> }
// Serialize + Deserialize + Debug + Clone + PartialEq nos dois. deny_unknown_fields. requires_snapshot e acyclic: default false na leitura, sempre escritos.
// equivalent, from, to: ausente na leitura = None; None é omitido na escrita (nunca null); null na leitura = None. Nomes: string simples, nome inválido falha na leitura.
pub struct DataViolation { pub pointer: String, pub code: String }   // Debug, Clone, PartialEq
impl Catalog {
    pub fn new(types: Vec<TypeDef>, relations: Vec<RelationTypeDef>) -> Result<Catalog, HexlogError>;
    pub fn type_def(&self, name: &TypeName) -> Option<&TypeDef>;
    pub fn relation_type_def(&self, name: &RelationTypeName) -> Option<&RelationTypeDef>;
    pub fn type_hash(&self, name: &TypeName) -> Option<&Hash>;                 // def_hash = sha256hex(canonical(to_value(def)))
    pub fn relation_type_hash(&self, name: &RelationTypeName) -> Option<&Hash>;
    pub fn validate_data(&self, name: &TypeName, data: &serde_json::Value) -> Result<(), Vec<DataViolation>>; // tipo ausente (proposta): Err com um achado "type-not-found" em ""; devolver Ok passaria dado sem validar, em silêncio
}
// Catalog: Send + Sync. Porta (crate-privada): node_type(&self, &TypeName) -> Option<&NodeTypeInfo>, relation_type(&self, &RelationTypeName) -> Option<&RelationTypeInfo>.
```

Recusa de `Catalog::new`: `ErrorCode::InvalidSchema`, um detalhe por problema, `details[i].path` em `/types/<nome>/schema<ponteiro>` (schema) ou `/types/<nome>/equivalent`, `/relations/<nome>/<campo>` (proposta, a confirmar pela sessão principal), `code` entre: `root-not-object` (raiz sem `type: "object"`), `keyword-not-allowed` (inclui typo, `$id`, `$dynamicRef`, `$async`, `uniqueItems`, `format` que não seja o permitido), `pattern-not-allowed` (`pattern`, `patternProperties`), `ref-not-internal` (`$ref` que não é `#` nem `#/...`), `ref-unresolved` (alvo inexistente ou que não é posição de schema), `ref-cycle` (ciclo de "mesma posição" por `$ref`, `allOf`, `anyOf`, `oneOf`, `not`; `properties`, `items` e `additionalProperties` recursivos são válidos), `keyword-type` (tipo errado do valor, inclusive anotação), `schema-uri` (`$schema` fora da raiz ou com URI diferente de 2020-12), `number-not-safe-integer` (qualquer número do documento do schema fora de ±(2^53 − 1) ou com fração/expoente), `max-depth-exceeded` (mais de 128 níveis), `equivalent-uri-format` (`url::Url::parse` falha ou há caractere menor ou igual a U+0020 ou U+007F). Lista branca: `type`, `enum`, `const`, `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems`, `required`, `properties`, `additionalProperties`, `items` (sem `prefixItems`), `allOf`, `anyOf`, `oneOf`, `not`, `$ref`, `$defs`, `$schema` (raiz, URI 2020-12), anotações `title`, `description`, `$comment`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` (só o tipo do valor é conferido). Chaves de `properties` e de `$defs` são nomes, nunca palavras-chave.

Violações de `validate_data`: todas, em ordem determinística (palavras na ordem `type`, `enum`, `const`, `minimum`, `maximum`, `min-length`, `max-length`, `min-items`, `max-items`, `required`, `properties`, `additionalProperties`, `items`, `allOf`, `anyOf`, `oneOf`, `not`, `$ref`; propriedades em ordem alfabética; arrays por índice), sem repetir o par `(pointer, code)`. `pointer` é o ponteiro RFC 6901 da instância (raiz `""`, `~` vira `~0` antes de `/` virar `~1`). `code` entre: `type`, `enum`, `const`, `minimum`, `maximum`, `min-length` e `max-length` (em pontos de código), `min-items`, `max-items`, `required` (ponteiro da propriedade ausente, um por propriedade), `additional-properties` (ponteiro da chave excedente; só olha `properties` do mesmo objeto), `false-schema`, `any-of`, `one-of`, `not` (um único achado no ponteiro do valor, sem erros dos ramos), `number-not-safe-integer`, `max-depth-exceeded`. `allOf` e `$ref` são transparentes. `type` que falha encerra a avaliação daquele nó. Se `data` tem número inseguro ou mais de 128 níveis, a resposta são só esses achados. Estouro do teto de avaliação (1.024 quadros) devolve um único `max-depth-exceeded` em `""`. Corte: no máximo 50 achados reais; havendo mais, os 50 primeiros mais um `too-many-errors` em `""` (51 itens); com exatamente 50 não há marcador.

Fontes independentes dos valores esperados:

- `tests/fixtures/json-schema-test-suite/`: 21 arquivos, 124 grupos, 392 testes da suíte oficial no SHA `245a45abc90190c0bff04d97e6d2b541b4bb9f12`, com `LICENSE`, `filter.py` e `MANIFEST.md`; roda como teste de unidade em `src/catalog/validate.rs`, afirma as contagens e não pula nada.
- `def_hash`: os seis documentos e hashes do item (h) da D-7, calculados com o `canonicalize` 5.1.0 do `hexlog` TS no commit `5d1e22b`, comando e commit anotados na fixture `tests/fixtures/def_hashes.json`; nenhum valor nasce do Rust.
- Violações e ponteiros: escritos à mão a partir da tabela acima (a suíte oficial só confere válido ou inválido).
```

#### Trechos a trocar no plano (D-7)

- Linhas 399-403 (`Definitions`): `node_type` e `relation_type` devolvem `Option<&NodeTypeInfo>` e `Option<&RelationTypeInfo>`; o comentário de `validate_data` passa a "até 50 achados reais, ponteiro JSON, e `too-many-errors` quando passa disso (51 itens); o engine junta os achados de todos os itens do lote e corta de novo em 50 com um único marcador (F-3)".
- Linha 403 e linha 681: "dado com 60 violações devolve 50 e `too-many-errors`" vira "devolve 50 mais `too-many-errors`, 51 itens, como o TS (`test/errors.spec.ts:306`)".
- Linha 535 (números em `data`): acrescentar "A guarda é `engine::limits::json_issues`, a mesma que o `Catalog` usa para o schema; recusa também mais de 128 níveis (`max-depth-exceeded`) e roda antes de `canonical(data)` do teto `data_max_chars`".
- Linha 561 (`equivalent`): "valida com `url::Url::parse` e recusa caractere menor ou igual a U+0020 ou U+007F (`equivalent-uri-format`)".
- Linha 563 (catálogo e schema): trocar "JSON Schema 2020-12 com raiz `type: "object"`; `pattern`... recusados" pela lista branca e pelos códigos do rascunho da costura 6; `$async` e `$id` caem em `keyword-not-allowed`.
- Linha 613 (T3, entrega): `validate_data`, `type_hash` e `relation_type_hash` são métodos públicos de `Catalog`; `DataViolation` reexportado; fixture `tests/fixtures/json-schema-test-suite/` e `tests/fixtures/def_hashes.json`.
- Linha 614 e tabela "Arquivo `catalog` (T3)": a lista de 10 passa a 30 (29 sem o de `format`) com os nomes do item (f); o teste da suíte e o do `def_hash` são de unidade.
- Seção 16 (decisões do dono sobre dependências): acrescentar `petgraph` 0.8.3 (D-8) e registrar o `[DONO]` de `format`.

### D-9: leituras e erros do `store/sqlite` que o plano não fixa (conferência das costuras 6, 7 e 8)

Método: leitura de `crates/hexlog/src/engine/ports.rs` (entregue pela T2) contra as costuras 6 a 8 e contra o plano (seções 4.3, 4.4, 4.5); experimentos em `scratchpad/critic/d6/tests/` (`errs`, `c1`, `c2`, `c3`) e `scratchpad/critic/amb.rs`, todos sobre o SQLite 3.51.3 embutido, com o `clippy.toml` e os lints do workspace.

#### (a) `where_eq`: `Bool` e `Int` colidem com `json_extract` puro

Medido: com `body` de `data.x` igual a `true`, `1` e `"1"`, `json_extract(body, ?) = ?` ligado a `1` devolve `true` e `1` (2 linhas). O plano (seção 4.5, `where_eq`) manda só `json_extract(body, ?)` e o teste `it_should_filter_by_a_top_level_scalar_field` afirma texto, inteiro e booleano sem separar os tipos.

| Alternativa | Efeito |
|---|---|
| `json_extract(body, ?) = ?` (plano) | `where_eq(x, Int(1))` casa dado booleano `true`; `Bool(false)` casa `0` |
| `json_type(body, ?1) = ?2 AND json_extract(body, ?1) IS ?3` (escolhida) | tipo e valor conferem juntos |

Escolhida a segunda. O tipo vem de literal fixo escolhido em Rust (`Str` dá `text`, `Int` dá `integer`, `Bool(true)` dá `true`, `Bool(false)` dá `false`); o valor liga como texto, `i64` e `1` ou `0`. Medido: `Bool(true)` devolve só o `true`, `Int(1)` só o `1`, `Bool(false)` só o `false`, `Int(0)` só o `0`, `Str("1")` só `"1"`. Valor `null` em `data` nunca casa.

#### (b) Código 2067 não separa fork de `records.id` duplicado

Medido (inserções diretas, uma por linha): raiz duplicada, `UNIQUE constraint failed: records.tenant, records.lineage_kind, records.lineage_key` (2067); segundo sucessor, `records.tenant, records.revises` (2067); `id` repetido, `records.id` (2067); `batches` com a mesma chave, `batches.tenant, batches.key` (**1555**, não 2067); `batches` com `first_seq` inexistente, 787; `seq` pulado, `prev_hash` errado e `seq` repetido, 1811 (trigger); `UPDATE`, 1811; `lineage_kind` inválido, 275.

Escolhido: `ForkRejected` só quando o código é 2067 **e** a mensagem do SQLite cita `records.revises` ou `records.lineage_key`; 2067 de outra coluna cai em `StoreError`. O texto é do SQLite 3.51.3 fixado pelo `Cargo.lock`; se um dia mudar, o teste de fork quebra alto (não passa calado). A frase "Violação de `batches` por chave vira `IdempotencyConflict`" da costura 7 sai: não está no plano nem na D-6, e dentro de `BEGIN IMMEDIATE` com a `decide` lendo `batch_by_key` ela só ocorreria por defeito da `decide`; `IdempotencyConflict` diria ao chamador que a impressão difere quando pode ser a mesma. A violação em `batches` vira `StoreError` com o código (1555).

#### (c) O que vai em `details[0].code` de `StoreError`

O plano diz "código do SQLite em minúsculas", que não significa nada para número. Escolhido: o código **estendido** em decimal como texto (`"1811"`, `"2067"`, `"787"`, `"1555"`, `"275"`), `path` vazio e `message` fixa por classe (`database error`), nunca a mensagem do SQLite (ela pode trazer caminho do banco). Erro que não é do banco (`sqlx::Error::Io`, `Configuration`, `Protocol`): `"io"`, `"configuration"`, `"protocol"`; qualquer outro: `"other"`. `unsupported-schema-version` continua o único código em palavras de abertura.

#### (d) Contrato de `limit` em `current_nodes`

O plano fixa `Page.next_after_seq`, mas não diz se o leitor devolve `limit` ou `limit + 1` linhas. Escolhido: o leitor obedece ao `limit` ao pé da letra (no máximo `limit` linhas, por `seq` crescente, `seq > after_seq`) e não sabe de "próxima página"; a Facade (T5a) pede `limit + 1` num `PageRequest` interno e tira o excedente para preencher `next_after_seq`. Assim o teste de leitura do store afirma contagem exata e o de paginação (T5a) afirma `next_after_seq`.

#### (e) `chain_head` de tenant desconhecido

`ChainHead` não tem como dizer "sem tenant". Escolhido: `Err` com `ErrorCode::TenantNotFound` (detalhe de `path` vazio, sem ecoar o tenant). Tenant existente e vazio: `seq` 0 e `hash` igual à âncora.

#### (f) Dois `tenant` em `StoreReader`

Medido (`amb.rs`, rustc 1.97): com `WriteReads` e `VerifyReads` ambas no escopo, `reader.tenant(..)` em `&mut dyn StoreReader` dá E0034 (vários itens aplicáveis). Chamada qualificada resolve (`WriteReads::tenant(reader, ..)`). A implementação do `SqliteStore` pode escrever as duas com o mesmo corpo; os testes de `src/store/sqlite/tests.rs` importam só o trait que usam ou qualificam a chamada.

#### (g) `tamper`

Medido (`c1`, `c2`): `sqlx::query(AssertSqlSafe(sql.to_owned()))` com `bind(*texto)` compila e roda; bind **de texto** em coluna `INTEGER` de tabela `STRICT` é coagido (`seq := '99'` altera 1 linha) e `WHERE seq = ?` com texto casa; `seq = seq + 1` e `NULL` entram como literal no SQL. Logo `binds: &[&str]` cobre toda a tabela de mutações. O `Tamper` do contrato passa `clippy` com `disallowed_*`, `needless_pass_by_value`, `needless_pass_by_ref_mut`, `implicit_clone` e `redundant_clone` em `deny`. Lacuna: a tabela exige ler `prev_hash` (para "hash recalculado" com `hexlog::chain::hash_record`) e achar o alvo por forma; `read_body` não cobre. Acrescentar `read_text(&mut self, sql: &str, binds: &[&str]) -> Vec<String>` (primeira coluna, `CAST(.. AS TEXT)` no SQL); `read_body` vira atalho dele. `Tamper::open` não cria o arquivo (o store já o criou).

### D-10: chave duplicada, noncharacters e profundidade em `canonical`, `fingerprint` e `data` (numerado D-10 porque a D-9 já registra a conferência das costuras 6 a 8)

Método: protótipo `scratchpad/critic/d10/` (binário que chama `hexlog::chain::canonical` e `sha256hex` do código real da T2, thread de 2 MiB, build de teste), bisseção por processo filho (estouro de pilha aborta o processo, código 134), `scratchpad/critic/d10/nc.mjs` com o pacote `canonicalize` do Hexlog TS (commit 5d1e22b, node v24.18.1) e um `Deserialize` estrito de 25 linhas (`src/strict.rs` do mesmo protótipo).

#### (a) Profundidade: medidas

| Operação (thread de 2 MiB) | Build de teste | Release |
|---|---|---|
| `canonical` recursivo | ok até 1.554 níveis, estoura a partir de 1.562 | ok até 6.531, estoura a partir de 6.587 |
| `serde_json::to_value` (o que `fingerprint` faz antes) | ok até 1.500, estoura a partir de 1.507 | idem `canonical` |
| `Drop` do `Value` aninhado | ok até 10.890, estoura a partir de 10.898 | não medido |
| `serde_json::from_str` | 127 contêineres ok, 128 devolve `Err` (recursion limit); 50.000 devolve `Err` sem estouro | idem |
| `from_str` de `{"data": <n arrays>}` (o corpo do record) | até 126 níveis de `data` ok, 127 devolve `Err` | idem |

Duas conclusões. **Um defeito da D-7 item (e):** a guarda recusa "mais de 128 níveis" e eu escrevi "127 contêineres mais o escalar, o que o `from_str` aceita". Isso vale para um documento solto, não para `data` dentro do `body`: o corpo acrescenta um objeto, então `data` com 127 contêineres passa na guarda, é gravado e depois **não relê** (`verify_chain` daria `InvalidBody`; `get` e `list` falhariam). O teto de `data` é 126 contêineres. **`fingerprint` está exposto:** `to_value` recursa sem limite e roda no passo 3, antes da guarda do passo 4; um `Value` montado em código com 1.507 níveis aborta o processo antes de qualquer erro. O `Drop` do `Value` do chamador (≥ 10.898 níveis) não é alcançável por nós: `append` recebe `&AppendRequest`, quem solta é o chamador.

| Alternativa | Efeito |
|---|---|
| Só documentar | `append` com 1.600 níveis aborta o processo; o caso 127 grava registro que não relê |
| Só limite dentro de `canonical` | não protege: `to_value` estoura antes de chegar nele |
| Só guarda antes de `fingerprint` (no `append`) | protege o `append`, mas `hexlog::chain::fingerprint` e `canonical` são públicos e continuam estourando para quem os chama direto |
| **Guarda no `append` (passo 1) + guarda em `fingerprint` + teto de pilha em `canonical` (escolhida)** | cada função pública fica segura sozinha; o produto recusa com erro nomeado |

Escolhido, com uma só constante de produto e uma de pilha:
- `engine::limits::JSON_NESTING_MAX = 126` (contêineres, isto é, arrays e objetos aninhados; escalar não conta), e `pub(crate) fn json_nesting_exceeds(value: &Value, max: usize) -> bool`, iterativa, sem alocar ponteiros, na mesma `engine/limits.rs` onde nasce `json_issues` (T3). A razão do 126 fica em comentário: o `from_str` aceita 127 e o corpo gasta um.
- `json_issues` passa a medir em contêineres e a recusar acima de `JSON_NESTING_MAX` (hoje: `depth` com o escalar contado e limite 128). Vale para `data` e também para o schema (uma regra só).
- Passo 1 do `append` ("Forma do lote"): para cada item `Node` com `data`, `json_nesting_exceeds(data, JSON_NESTING_MAX)` dá `INVALID_INPUT`, detalhe `max-depth-exceeded`, `path` `/items/<i>/data`. Roda antes do replay, e o limite é constante (não configurável), então reenviar um lote antigo continua devolvendo `replayed`.
- `fingerprint`: antes do `to_value`, o mesmo teste por item; falha dá `INVALID_INPUT`, `max-depth-exceeded`, `path` `/<i>/data` (relativo ao slice de itens). Lembrete da D-3: `Err` de `fingerprint` significa "sem replay possível"; agora vale para número fora de ±2^53 e para profundidade, e ambos são recusados depois com o código próprio (passo 1 para profundidade, passo 4 para número).
- `canonical`: contador de profundidade em `write_value`; acima de `CANONICAL_NESTING_MAX = 256` devolve `INVALID_INPUT`, `max-depth-exceeded`, `path` vazio. É só teto de pilha para quem chama direto (o `fingerprint` embrulha cada item em array e objeto, então precisa de folga sobre 126; a 256 níveis o uso de pilha é cerca de 340 KiB no build de teste). Não é a regra do produto.
- `verify`: sem mudança de código. Lê texto do banco por `from_str`, que limita a 127 e devolve `Err` sem estouro (medido até 50.000): vira `InvalidBody` ("legível", plano 4.5). Dois testes novos em T5c o provam (abaixo).
- Gravação: invariante "todo corpo gravado relê" vale porque `data` tem no máximo 126 e o envelope gasta 1. Teste de ida e volta no limite (abaixo).

#### (b) Chave duplicada em `data`

Medido: `serde_json::from_str` de `{"a":1,"a":2}` fica com `{"a":2}` sem erro; o `canonicalize` do TS sobre `JSON.parse` do mesmo texto também (`{"a":2}`). O `hexlog` recebe `Value`, que **não pode** conter duplicada: o `Map` já resolveu. Logo a borda de recusa não existe no passo 3; ela é de quem converte texto em `Value`, o `omhl` (passo 4, borda do MCP). O hash cobre o `Value` recebido, então nada muda no hash. Os dois pontos de texto que o próprio `hexlog` lê: (1) `body` do banco: uma duplicada gravada à mão não é canônica (a recanonicalização tem uma chave só e difere do texto), então `verify_chain` já diz `InvalidBody`; (2) `Deserialize` de `TypeDef`/`RelationTypeDef` (`schema` é `Value`): última vence, igual ao TS. Alternativa de recusa: `Deserialize` estrito, medido em 25 linhas (`src/strict.rs`): `{"a":1,"a":2}` e `{"x":[{"b":1,"c":2,"b":3}]}` dão `duplicate-key`, `{"a":1,"A":2}` passa (maiúscula é outra chave), o limite de 127 do `serde_json` continua valendo porque a recursão é dele. Escolhido: **aceitar e documentar** (paridade com o TS e com o RFC 8785, que cita a proibição mas não manda o `canonical` testar), sem código novo no passo 3 (menor entrega). Quem quiser recusa na borda usa o trecho do protótipo; fica como item do passo 4. Se o dono quiser que o produto recuse duplicada, é decisão dele e muda o `omhl`, não o `hexlog`: marco como informação, sem pedir resposta agora.

#### (c) Noncharacters (U+FFFE, U+FFFF, U+FDD0..U+FDEF, U+1FFFE, U+10FFFF)

Medido: a string `"a\u{FFFE}\u{FFFF}b\u{FDD0}\u{FDEF}\u{1FFFE}\u{10FFFF}z"` em `{"k": ..}` dá, no `canonical` do Rust e no `canonicalize` do TS, os mesmos bytes (`7b226b223a2261efbfbeefbfbf62efb790efb7aff09fbfbef48fbfbf7a227d`) e o mesmo `sha256hex` (`cccc4a026451b00f922cc2986c70a03ad855b8a0aabb65156dd148e8a610e6dd`). O `serde_json` também as lê escritas como `\ufffe`. Recusar mudaria o conjunto de dados aceitos em relação ao TS sem ganho de hash. Escolhido: **aceitar e documentar** (o RFC 8785 não proíbe; o RFC 7493 não é o perfil adotado). Surrogate solto (`\ud800`) já é recusado pelo `serde_json` na leitura e não existe em `String` de Rust; fica fora.

#### (d) O que muda no contrato, no plano e nos testes

Código (emenda pequena à T2, que já entregou `chain.rs` e `limits.rs`): `limits.rs` ganha `JSON_NESTING_MAX = 126` e `json_nesting_exceeds`; `chain.rs` ganha o contador em `write_value` (teto 256) e o teste por item em `fingerprint` antes do `to_value`. A T3 aplica `JSON_NESTING_MAX` em `json_issues` (contêineres, não escalar).

Plano e contrato: costura 6, tabela de recusas, linha `max-depth-exceeded`: "mais de 126 níveis de array ou objeto aninhados (o corpo do record gasta um dos 127 que o `from_str` aceita)". Costura 6, violações de `validate_data`: "Se `data` tem número inseguro ou mais de 126 níveis de contêiner, a resposta são só esses achados" (hoje diz 128). Costura 6, "Guarda de inteiro seguro": acrescentar "`json_issues` mede profundidade em contêineres, limite `engine::limits::JSON_NESTING_MAX` (126)". Plano 4.5, "Ordem de recusa da escrita", passo 1: acrescentar "`data` com mais de 126 níveis de contêiner: `INVALID_INPUT`, detalhe `max-depth-exceeded`, `path` `/items/<i>/data`". Plano 4.5, linha 535: trocar "mais de 128 níveis" por "mais de 126 níveis de contêiner". Documentar em `AppendItem.data`: "chave duplicada é resolvida por quem converteu o texto em `Value` (a última vence); noncharacters são aceitos".

Testes novos ou alterados (nomes exatos):

| Onde | Teste | Afirma |
|---|---|---|
| `tests/jcs_vectors.rs` (T2) | `it_should_emit_noncharacters_raw_like_the_ts_oracle` | `{"k":"a\u{FFFE}\u{FFFF}b\u{FDD0}\u{FDEF}\u{1FFFE}\u{10FFFF}z"}` dá os bytes `7b22..7d` e o `sha256hex` `cccc4a02..e6dd` acima, calculados pelo `canonicalize` do TS (comando e commit anotados no teste) |
| `tests/jcs_vectors.rs` (T2) | `it_should_refuse_a_value_nested_beyond_the_canonical_stack_limit` | thread de 2 MiB: 256 níveis ok; 257 dão `INVALID_INPUT` `max-depth-exceeded`; 5.000 níveis dão o mesmo erro sem abortar |
| `tests/jcs_vectors.rs` (T2) | `it_should_fail_the_fingerprint_for_data_nested_beyond_126_levels_without_overflowing` | `data` com 126 contêineres: `Ok`; 127: `Err` `max-depth-exceeded` com `path` `/0/data`; 5.000 em thread de 2 MiB: `Err`, sem abortar |
| `src/engine/limits.rs`, módulo de teste (fora da contagem) | `it_should_count_nesting_in_containers_and_not_in_scalars` | 126 arrays: `false`; 127: `true`; escalar solto: `false`; objetos e arrays misturados contam igual; 1.000.000 níveis sem estourar |
| `tests/catalog.rs` (T3), alterado | `it_should_not_overflow_the_stack_on_deeply_nested_data` | passa de 127 para 126 níveis de dado |
| `tests/catalog.rs` (T3), renomeado | `it_should_refuse_data_nested_beyond_the_record_body_limit` (era `..._deeper_than_the_parser_limit`) | 127 contêineres dão `max-depth-exceeded`; 126 passam |
| `tests/node_lifecycle.rs` (T5a) | `it_should_round_trip_data_nested_to_the_limit_and_refuse_one_level_more` | 126: `append` e `get` devolvem `data` igual; 127: `INVALID_INPUT` `max-depth-exceeded` em `/items/0/data` e `chain_head` não anda |
| `tests/node_lifecycle.rs` (T5a) | `it_should_refuse_data_nested_thousands_of_levels_without_overflowing_the_stack` | 5.000 níveis em thread de 2 MiB: `INVALID_INPUT`, sem abortar |
| `tests/chain_tamper.rs` (T5c) | `it_should_report_invalid_body_for_a_duplicate_key_in_the_stored_body` | corpo com chave repetida, hash recalculado: `InvalidBody` no alvo |
| `tests/chain_tamper.rs` (T5c) | `it_should_report_invalid_body_for_a_stored_body_nested_beyond_the_parser_limit` | corpo com `data` de 200 níveis, hash recalculado: `InvalidBody`, sem pânico nem estouro |

Contagem: +7 na contagem de nomes (3 em `jcs_vectors`, 2 em T5a, 2 em T5c; os de T3 são alteração) e +1 de unidade fora dela. Ficam fora da D-10: recusar duplicada na borda do `omhl` (passo 4), `Drop` recursivo do `Value` do chamador (≥ 10.898 níveis, alheio ao `hexlog`) e surrogate solto (impossível em `String`).

### D-11: `format`, listas de extremos vazias ou repetidas, e dois achados de simplicidade da T2

Método: leitura de `src/adapters/validator.ts` (84-115, 132-140), `src/domain/definitions.ts` (26-36), `src/commands/definition.ts` e `test/adapters/validator.spec.ts` (59, 145-205) do Hexlog TS (commit 5d1e22b); execução do `createValidator()` real do TS sobre 11 schemas e 7 dados (`scratchpad/critic/d10/fmt.mts` e `fmt2.mts`, node v24.18.1); varredura dos schemas reais do dono (já feita na D-7 item a); `grep` dos usos no crate Rust.

#### (a) `format`: aceitar só `"attachment"`, com as mesmas posições e a mesma conferência do TS

Medido no TS: `format: "attachment"` passa em `/properties/<campo>`, em `/properties/<campo>/items` e em `type: ["string","null"]`; é recusado (`invalid-schema`, `path` `<ponteiro>/format`) em objeto aninhado, em `anyOf`, em `$defs` e na raiz; `format` sem `type` string é recusado pelo modo estrito do ajv; `format: 1` dá `type`. No dado, o campo marcado exige 64 hexadecimais minúsculos (`"abc"`, 64 `A` e 63 `a` dão `code: "format"` em `/f`; `null` passa com `type: ["string","null"]`; `5` dá `type`). `email` e `git-sha` passam no TS (ajv-formats e catálogo próprio).

| Alternativa | Efeito |
|---|---|
| (i) recusar todo `format` | os 6 usos reais (`"type": "string", "format": "attachment"`, sempre em `/properties/<campo>`) não carregam; o dono reescreve os modelos |
| (ii) aceitar qualquer `format` e ignorar | carrega tudo, mas `"email"` passa sem checagem e um campo `attachment` aceita qualquer texto; diverge do TS em silêncio |
| (iii) aceitar só `attachment`, tratado como texto comum | carrega os modelos, mas o campo marcado aceita `"abc"`, que o TS recusa; o passo de anexos herdaria hash inválido gravado |
| **(iii') aceitar só `attachment`, com posição e conferência do TS (escolhida)** | carrega os 6 usos; o que o TS recusa, o Rust recusa; `email`, `git-sha` e demais ficam recusados (desvio declarado) |

Regra exata:
- Na compilação do schema (`Catalog::new`), em cada schema com `format`: (1) valor que não é texto: `keyword-type`; (2) texto diferente de `attachment`: `format-not-supported`; (3) `attachment` fora de `properties/<campo>` ou `properties/<campo>/items`, contados a partir da raiz do schema (por `$ref` ou `$defs` não vale): `format-not-supported`; (4) `attachment` em schema sem `type` igual a `"string"` ou lista de tipos que contenha `"string"`: `format-not-supported`. Um detalhe por ocorrência, a primeira condição que falha. `path` `/types/<nome>/schema<ponteiro do schema>/format`. Vale só nessas posições, não "em qualquer schema".
- Em `validate_data`: se o schema do ponto tem `format: "attachment"` e a instância é texto, ela tem de ter 64 hexadecimais minúsculos (mesma regra de `Hash::parse`, chamado como `Hash::parse(texto).is_ok()`); senão, achado `format` no ponteiro da instância. Instância que não é texto não gera `format` (quem acusa é `type`). Posição na ordem das palavras: depois de `max-length`.
- Códigos novos: recusa `format-not-supported`; violação `format`.
- Desvio declarado em relação ao TS: modelo que use `format: "email"`, `date`, `uri` etc. não carrega no Rust. Nenhum dos 8 schemas reais usa; vai ao ADR junto com o desvio de palavra desconhecida.

Testes (um comportamento cada; o `it_should_accept_format_attachment_and_reject_any_other_format` sai, trocado por quatro):
- `it_should_accept_format_attachment_on_a_top_level_property_its_items_and_a_nullable_string`
- `it_should_reject_a_format_other_than_attachment` (`email`, `git-sha`, `date`, texto vazio: `format-not-supported`; `format: 1`: `keyword-type`; `format` sem `type` string: `format-not-supported`)
- `it_should_reject_format_attachment_outside_the_top_level_property_positions` (aninhado, `anyOf`, `$defs`, raiz: `format-not-supported`, `path` com `/format`)
- `it_should_require_a_sha256_hash_in_a_field_marked_as_attachment` (64 `a` passa; `"abc"`, 64 `A` e 63 `a` dão `format` em `/f`; `null` com `type: ["string","null"]` passa; item `/l/1` de lista)

`it_should_reject_a_keyword_outside_the_whitelist` perde `format: "email"` e fica com `requred`, `$id`, `$dynamicRef`, `$async`, `uniqueItems` (`keyword-not-allowed`).

#### (b) Listas de extremos: recusar a vazia e a repetida

Lido: `definitions.ts:26-27`: "`from`/`to` são listas de nomes de tipo, não vazias e sem repetição. Omitida, a ponta aceita qualquer tipo; `[]` não quer dizer "nenhum tipo" e é recusada"; o `spec` afirma os quatro casos (`from: []`, `to: []`, repetido em `from`, repetido em `to`, linhas 336-339). Escolhido: **recusar as duas**, como o TS. `empty-endpoint-list` (já na tabela) e novo `duplicate-endpoint-type`; `path` `/relations/<nome>/from` ou `/to`; um detalhe por lista. A ordem das listas segue no hash, sem ordenar. Ausente e `null` continuam `None` (qualquer tipo). Também: o TS não confere se os tipos de `from`/`to` existem (`commands/definition.ts` não consulta o catálogo), então `Catalog::new` **não** confere (o teste do `def_hash` usa relation type sem os tipos).
Testes: `it_should_reject_a_relation_type_with_an_empty_endpoint_list` (`from: []` e `to: []`, ausente e `null` aceitos), `it_should_reject_a_relation_type_with_a_repeated_endpoint_type`.

#### (c) Achados de simplicidade da T2

- `Hash::from_lower_hex`: confirmado um uso (`chain.rs:62`). Manter. Abrir o campo dos 9 tipos como `pub(crate)` tiraria a validação de todos para economizar um construtor de um tipo. Melhoria de uma linha: `debug_assert!(is_hash(&hex))` dentro do construtor (`is_hash` está no mesmo arquivo), que pega o erro de quem chamar com texto errado.
- `SnapshotKind::as_str` e `Serialize` manual: manter. `as_str` não é de um uso: a T4 grava e filtra pela coluna `snapshot_kind` (`RecordColumns.snapshot`, `ListFilter.snapshot`) e `parse` (hoje sem uso) lê a coluna de volta. O `Serialize` manual usa `as_str`, então o texto `git-commit` fica num só lugar (derive com `rename` duplicaria o literal, e a palavra `git` só pode aparecer em `engine/snapshot.rs`).

#### (d) `it_should_match_the_ts_oracle_def_hash_for_each_definition_shape` em `tests/catalog.rs`: confirmo

Com `type_hash` e `relation_type_hash` públicos o teste não precisa ser de unidade e passa a contar. Condições: a fixture `tests/fixtures/def_hashes.json` guarda o documento, o `def_hash` e, no topo, comando e commit; o teste monta **um `Catalog` por documento** (os documentos 0 e 1 têm o nome `note`, 3 e 4 têm `after`); relation type sem os tipos de `from`/`to` precisa ser aceito (decisão (b)).

#### (e) Lista final de `tests/catalog.rs`: 36 nomes (10 do plano, 14 de (f), 5 acréscimos, 4 de `format`, 2 de extremos, 1 do `def_hash`)

O teste de unidade `it_should_expose_requires_snapshot_def_hash_and_acyclic_through_the_definitions_port` continua em `src/catalog/types.rs`.

Plano (10): `it_should_reject_a_schema_with_pattern_or_pattern_properties`, `it_should_reject_an_external_ref`, `it_should_reject_a_schema_whose_root_is_not_an_object`, `it_should_reject_a_malformed_equivalent_uri_and_accept_a_wellformed_one_without_resolving_it`, `it_should_compute_a_stable_def_hash`, `it_should_list_every_violation_with_json_pointers_up_to_50`, `it_should_refuse_numbers_that_are_not_safe_integers_in_data`, `it_should_accept_data_that_satisfies_the_schema`, `it_should_load_type_and_relation_type_definitions_from_json`, `it_should_reject_an_invalid_type_name_when_deserializing`.

D-7 item (f) (14): `it_should_reject_a_ref_cycle_without_descending_into_the_instance`, `it_should_accept_a_recursive_ref_guarded_by_properties`, `it_should_validate_a_shared_ref_dag_in_polynomial_time`, `it_should_not_overflow_the_stack_on_deeply_nested_data` (126 níveis, D-10), `it_should_report_max_depth_exceeded_on_a_very_long_ref_chain`, `it_should_treat_property_names_as_names_not_keywords`, `it_should_resolve_ref_pointers_with_escapes_and_empty_tokens`, `it_should_reject_a_ref_to_a_non_schema_location`, `it_should_evaluate_ref_siblings`, `it_should_not_look_into_all_of_for_additional_properties`, `it_should_report_a_single_violation_for_a_failing_any_of`, `it_should_count_string_length_in_code_points`, `it_should_use_a_pointer_with_tilde_escapes_in_violations`, `it_should_reject_non_safe_integers_in_the_schema`.

Acréscimos (5): `it_should_name_the_missing_property_in_the_required_pointer`, `it_should_reject_a_keyword_outside_the_whitelist`, `it_should_accept_annotation_keywords_and_check_their_type`, `it_should_not_add_too_many_errors_at_exactly_50_violations`, `it_should_refuse_data_nested_beyond_the_record_body_limit` (126 passa, 127 recusa, D-10).

`format` (4), extremos (2), `def_hash` (1): os sete nomes de (a), (b) e (d) acima.

### D-12: um só `decide` dentro do `append_with`, com o replay sob a trava; `at` por lote; ids só na montagem

Método: leitura de `createDecide` e `linkItems` do Hexlog TS (`src/commands/register/state.ts:255-345`, commit 5d1e22b); plano 4.5 (ordem de recusa) e contrato da T4 (plano sem records e sem lote não grava); medição de custo em `scratchpad/critic/d7/examples/bench50.rs` (validador da D-7, 50 itens de 19 KB: 40 ms em release, 178 ms no build de teste); compilação de um esqueleto em `scratchpad/critic/t5a.rs` com `#![deny(warnings)]` (módulo `pub(in crate::engine)`, `pub fn` com tipos `pub(crate)`, fechamento passado a `&mut dyn FnMut`, campos disjuntos de `self`: compila).

Plano 4.5 pede que a consulta de replay seja uma leitura fora da transação de escrita e que o passo 5 repita a consulta sob a trava. O TS faz tudo sob a trava: confere a cadeia, busca a `key` (replay devolve sem gravar, ou conflito) e só então as regras (`createDecide`); o `at` vem de uma só chamada de relógio por lote, repetida em todos os elos (`linkItems(..., deps.clock().toISOString())`); só o primeiro elo leva `batch`.

| Alternativa | Efeito |
|---|---|
| A. Leitura de replay fora da transação (plano) e `decide` só com os passos 5 e 6 | replay não toma a trava; toda escrita nova paga uma leitura a mais; dois lugares consultam a `key`; os passos 1 a 4 ficam em arquivo à parte |
| **B. `decide` roda os passos 1 a 6 dentro do `append_with` (escolhida)** | uma só consulta de `key`, sem janela entre consulta e gravação (o passo 5 do plano some); um replay toma a trava por um instante (leitura de uma linha mais `batch_records`); o plano vazio do contrato da T4 passa a ter dono |

Custo medido de B: o pior lote (50 itens perto de `data_max_chars`) segura a trava por cerca de 40 ms de validação em release; o `busy_timeout` padrão é 5.000 ms e a gravação com `synchronous=FULL` custa mais que isso. Aceito. Efeito para o chamador: sob contenção, um replay pode receber `BUSY` onde antes só leria; `it_should_write_once_when_the_same_key_races` (T5d) segue valendo.

Escolhido também: (1) `Clock::now()` é chamado uma vez por `append`, pela Facade, antes do `append_with`, e passado ao `decide` (que continua puro); o mesmo `at` vai em todos os records do lote, como no TS; (2) `IdGenerator::next` é chamado uma vez por record, só na montagem, depois de todas as recusas e fora do replay: lote recusado ou repetido não gasta id (o gerador de teste é determinístico e o hash dourado depende disso); (3) `fingerprint` é calculado no passo 3 mesmo sem `key` (o corpo do primeiro record guarda `batch.fingerprint` sempre); `Err` de `fingerprint` pula a consulta de `key` e segue ao passo 4, que recusa com o código próprio; se o passo 4 passar com `Err` guardado, é `Internal` `fingerprint-unreachable`; (4) o `outcome.chain_head` de um replay é o chain head **atual** do tenant, não o do momento do lote.

#### Texto do plano a trocar (D-12)
Seção 4.5, passos 3 e 5: "A consulta a `batches` é uma leitura fora da transação de escrita" sai; o passo 5 perde "nova consulta de `key`"; acrescentar "os passos 1 a 6 rodam dentro da transação, na ordem; o replay não verifica a cadeia".

### D-13: formato do instante de `Clock::now()`

Método: costura 4 (24 caracteres `YYYY-MM-DDTHH:MM:SS.mmmZ`), `toISOString()` do TS como origem do formato, tabela de erros (`Internal`).

Escolhido: o `decide` valida o instante só ao montar os records (depois das regras 1 a 6 e fora do replay), e `ensure_tenant` o valida antes de gravar o tenant. Forma exata: 24 bytes, dígitos nas posições 0 a 3, 5 e 6, 8 e 9, 11 e 12, 14 e 15, 17 e 18, 20 a 22; `-` em 4 e 7, `T` em 10, `:` em 13 e 16, `.` em 19, `Z` em 23. Só a forma, não o calendário (mês 13 passa): quem implementa o `Clock` responde pelo valor; o calendário exigiria `time` na validação sem ganho de hash. Falha: `ErrorCode::Internal`, detalhe `clock-format`, `path` vazio, `message` fixa, nada gravado. Alternativa descartada: validar logo ao chamar o relógio. Efeito: um relógio quebrado não esconde `TENANT_NOT_FOUND`, `IDEMPOTENCY_CONFLICT` nem o replay (que não usa `at`). Teste: `it_should_fail_with_internal_and_write_nothing_when_the_clock_returns_a_malformed_instant` (node_lifecycle) e `it_should_replay_even_when_the_clock_returns_a_malformed_instant` (idempotency).

### D-14: passo 1 (forma) e os caminhos de erro de `data`

Método: costura 1 do contrato (`/items/<i>/body/data/<chave escapada>`), D-10 e plano 4.5 (`/items/<i>/data`), `ItemBody::Node` em `engine/model.rs` (o `data` está dentro de `body`).

Conflito achado: a D-10 e o plano dizem `/items/<i>/data` para `max-depth-exceeded`; a costura 1 diz `/items/<i>/body/data`. O `AppendItem` serializa com `body` no meio, então o ponteiro verdadeiro é `/items/<i>/body/data`. Escolhido: **`/items/<i>/body/data` em tudo** (profundidade, violações de schema, tamanho). Troca nos textos e nos testes da D-10: `it_should_round_trip_data_nested_to_the_limit_and_refuse_one_level_more` passa a afirmar `/items/0/body/data`; plano 4.5, passo 1, idem. O `path` relativo do `fingerprint` (`/<i>/data`) não muda: é detalhe de uma função de baixo nível já entregue.

Passo 1, na ordem, devolve o primeiro problema (um só detalhe), `INVALID_INPUT`: (1) `items` vazio ou com mais de `limits.batch_items`: `batch-size` em `/items`; (2) `author.agent`, `author.client` ou `author.model` (se presente) fora de 1 a `limits.author_field_max` unidades UTF-16: `author-format` em `/author/agent`, `/author/client` ou `/author/model`; (3) por item, em ordem: corpo `Relation` ou `Retirement` de relation: `relation-items-not-supported` em `/items/<i>/body` (ramo da T5a que a T5b remove); `data` com mais de `JSON_NESTING_MAX` contêineres: `max-depth-exceeded` em `/items/<i>/body/data`. `key` e `Snapshot` e `ExternalId` já chegam validados pelo tipo.

### D-15: passo 4 (estáticas): duas passadas e o corte do lote

Método: plano 4.5 (passo 4, H:311), F-3 da D-7, tabela S1 a X4, `Definitions` em `ports.rs`.

Passada A, em ordem de item, devolve no primeiro achado, erro com código próprio: `Node` com tipo ausente do catálogo: `TypeNotFound` em `/items/<i>/body/type`, detalhe `type-not-found`; `Node` cujo id `NativeNodes::lookup` reconhece: `ReservedId` em `/items/<i>/body/id`, detalhe `reserved-id` (vale também para revisão de um id que virou nativo: o nativo prevalece; `Retirement` de node não é recusado, para o dono poder retirar o sombreado). Motivo da passada à parte: a validação de `data` precisa do tipo, e um item sem tipo não tem como ser validado.

Passada B, em ordem de item, acumula `INVALID_RECORD` (um erro, vários detalhes), por item nesta ordem: (1) `defs.validate_data(&tipo, &data)`: cada achado vira detalhe com `path` `/items/<i>/body/data` mais o ponteiro, `code` o do achado e `message` fixa por código (nunca o valor); os marcadores `too-many-errors` vindos do item são descartados; (2) se (1) não achou nada, tamanho: JCS de `data` com mais de `limits.data_max_chars` unidades UTF-16: `data-too-large`; (3) `snapshot`: `Node` com `requires_snapshot` e sem `snapshot`: `snapshot-required`; `Node` sem `requires_snapshot` e com `snapshot`: `snapshot-not-allowed`; `Retirement` com `snapshot`: `snapshot-not-allowed`, `path` `/items/<i>/snapshot`; (4) `external_id` em `Retirement`: `external-id-not-allowed`, `path` `/items/<i>/external_id`. Corte: concatenado o lote, mais de `limits.details_max` achados (ou algum item com marcador) dá os primeiros `details_max` mais um `too-many-errors` com `path` `""` (F-3). O tamanho só é medido depois de `validate_data` achar zero problema, para a guarda de número e profundidade vir antes de `canonical` (D-7 e).

### D-16: passos 2, 3, 5 e 6 (tenant, replay, cauda e histórico) e o `FORK_REJECTED` do mesmo lote

Método: plano 4.5 ("Regra de um vigente"), assinaturas de `WriteReads` em `ports.rs`, `createDecide` do TS (`state.ts:288-330`), handoff (pendência do `current` no mesmo lote).

Passo 2: `reads.tenant(..)` ausente: `TenantNotFound` em `/tenant`, detalhe `tenant-not-found` (não ecoa o tenant). Passo 3: com `key`, `batch_by_key`; impressão igual: plano de replay (`records` vazio, `batch: None`, `outcome.replayed = true`, ids de `batch_records(tenant, first_seq, item_count)`, `chain_head` atual); diferente: `IdempotencyConflict` em `/key`, detalhe `key-reused`.

Passo 5 (cauda, política `Tail`; `Full` só na T5c e na T5a vale como `Tail`): `chain_head`; se `seq > 0`, o `WriteReads` não tem leitura de "último record" e o plano não a prevê; usar `batch_records(tenant, head.seq, 1)` (a leitura é por faixa de `seq`, e a porta já existe, o que evita mexer em `ports.rs` e na T4 em andamento) e exigir `hash_record(prev_hash, body) == hash == head.hash`; senão `ChainBroken` com detalhe `tail-hash-mismatch`, `path` vazio. Alternativa descartada: método novo `tail_record` em `WriteReads` (mexe na T4 durante a execução, ganho só de nome). Anotar no contrato que `batch_records` serve a duas leituras.

Passo 6, por item em ordem; chave de lineage é o id do node:
1. Já visto no lote (conjunto de chaves, `HashSet`): `ForkRejected`, detalhe `lineage-in-batch`, `path` `/items/<i>/body`, `current` e `retired` ausentes (o primeiro item ainda não tem record; a decisão pedida pelo handoff). Vale para `Node` e `Retirement` na mesma lineage em qualquer ordem.
2. `latest_record(tenant, Lineage::Node(id))`.
3. `Node`: sem latest, `base` presente: `InvalidRecord` `base-unknown`, `/items/<i>/base`. Com latest e `base` ausente ou diferente do record id dele: `ForkRejected`, detalhe `not-current`, `path` `/items/<i>/base`, `current = Some(latest.record_id)`, `retired = Some(latest.retired)` (o `retired` é "o latest é uma retirement"). Com `base` certo e tipo diferente de `latest.type_name`: `InvalidRecord` `type-changed`, `/items/<i>/body/type`. Revisar sobre retirement é permitido.
4. `Retirement` de node: sem latest: `InvalidRecord` `lineage-not-found`, `/items/<i>/body/lineage`; `base` ausente ou diferente: `ForkRejected` `not-current` como acima; `base` certo e `latest.retired`: `InvalidRecord` `already-retired`, `/items/<i>/body/lineage` (decisão minha: o plano não diz; retirar o que já está retirado não muda estado e só alongaria a cadeia; o reenvio com `base` antigo já cai em `not-current`).
Montagem de cada record: `type` e `def_hash` do catálogo em `Node` e do latest em `Retirement`; `revises` = record id do latest quando há; `retirement` verdadeiro só na retirement; `data` só em `Node`; `batch` só no primeiro record (`fingerprint`, `count`, `key` se houver); colunas derivadas vêm do mesmo corpo; `BatchRow` só com `key`.

### D-17: Facade síncrona: abertura, leituras, página e visões

Método: plano 4.4, `Store`/`QueryReads` de `ports.rs`, contrato da T4 (`current_nodes` devolve no máximo `page.limit` linhas), `RecordBody` de `model.rs`.

- `Config::new(db_path: PathBuf) -> Config` com os padrões do plano (`Limits::default()`, `busy_timeout` 5 s, `VerifyPolicy::Tail`); os campos seguem públicos. `open` recusa `limits.batch_items` fora de 1 a 50 (o DDL tem `CHECK (item_count BETWEEN 1 AND 50)`), `limits.details_max` fora de 1 a 50 e `limits.page_max` fora de 1 a 200: `InvalidInput`, detalhe `limits-out-of-range`, `path` `/limits/<campo>`. Sem isso o defeito apareceria como `StoreError "275"` no meio de uma escrita.
- `ensure_tenant`: lê o relógio, valida o instante (D-13), monta `anchor_doc` e `anchor` por `hexlog::chain`, chama `Store::ensure_tenant`; devolve a linha guardada (tenant existente devolve o `created_at` e a âncora de quando nasceu, não os do relógio de agora) e `created`.
- Leituras (`get`, `lineage_records`, `list`, `chain_head`): tenant desconhecido é `TenantNotFound` em todas (`WriteReads::tenant` dentro da mesma `Store::read`). `get`: latest de `Lineage::Node(id)`; ausente ou retirado: `None`; senão `record_by_id` e `NodeView` montada **do `body`** (`data`, `author`, `at`, `snapshot`, `external_id`, `type`), mais `seq` e `record_id` da linha, `shadowed = native.lookup(id).is_some()`. Body que não lê ou campo que não parseia: `Internal`, detalhe `invalid-body` (quem acusa adulteração é `verify_chain`).
- `list`: `page.limit` fora de 1 a `limits.page_max`: `InvalidInput` `page-limit` em `/page/limit`. A Facade pede `limit + 1` a `current_nodes`, devolve `limit` itens e preenche `next_after_seq` com o `seq` do último item devolvido quando sobrou linha; senão `None`. `id_prefix` não é validado: texto que não é prefixo de id casa nada.
- `lineage_records` devolve `Vec` vazio para lineage sem records (não é erro) e `RevisionView.data` é `None` na retirement.

### D-18: `path` das recusas de `Catalog::new` (ref, pattern, tipo, cauda) e lacunas sem teste da T3

Método: leitura de `tests/catalog.rs` (o que cada teste já fixa: linhas 197-228, 429-452, 512-531, 595-624, 824-837), `engine/limits.rs` (`json_issues` devolve o ponteiro do contêiner que passa de 126), `json-schema-subset.md` (linha 94 e passo 1 do build), `Cargo.toml` (`serde_json` sem `preserve_order`: `Map` é `BTreeMap`, ordem alfabética), D-7 e D-8 (`petgraph`).

Regra única: o `path` aponta o valor que causa o problema, como `keyword-not-allowed` e `keyword-type` já fazem. Decisões:

1. `ref-not-internal`, `ref-unresolved`: ponteiro da palavra `$ref` (`/properties/a/$ref`). `ref-cycle`: **um detalhe por `$ref` que está num ciclo**, no ponteiro da palavra `$ref`. Definição por conjunto, sem depender da ordem de busca: o `$ref` é acusado quando a posição que o contém e o alvo dele estão na mesma componente fortemente conexa do grafo de "mesma posição" (arestas: `$ref`, `allOf`, `anyOf`, `oneOf`, `not`; auto-laço conta). Ordem dos detalhes: pelo ponteiro. Resultados: `$ref: "#"` na raiz, `/$ref`; `allOf: [{$ref: "#"}]`, `/allOf/0/$ref`; `anyOf`, `/anyOf/0/$ref`; `not`, `/not/$ref`; `a -> b -> a` em `$defs`, **dois** detalhes, `/$defs/a/$ref` e `/$defs/b/$ref`. Alternativa descartada: um detalhe por ciclo (depende de qual aresta a busca fecha, e o número de ciclos em SCC grande é ambíguo). `$ref` sem alvo (`ref-unresolved`) ou externo não cria aresta. A componente conexa tem de ser calculada sem recursão (`petgraph::algo::tarjan_scc`, iterativo): a cadeia de 10.000 `$defs` do plano roda em thread de 2 MiB.
2. Confirmo as cinco do test-engineer: `pattern-not-allowed` em `.../pattern` e `.../patternProperties` (não desce no valor); `keyword-type` em `.../title`, e também em posição de schema que não é objeto nem booleano (`properties.a = 5` dá `/properties/a`); `number-not-safe-integer` no ponteiro do número; `root-not-object` em `/types/t/schema` (ponteiro `""` relativo ao schema); `equivalent-uri-format` em `/relations/r/equivalent` e `/types/t/equivalent`. `root-not-object` encerra a análise daquele schema (um só detalhe, sem varrer o resto).
3. `root-not-object` para `{"type":["object","null"]}`, `{}`, `true` e `false`: confirmado (só vale `type` igual à string `object`). O teste existente da T3 já tem 4 raízes.
4. `max-depth-exceeded` de `validate_data`: no ponteiro do contêiner que passa de 126, como `json_issues` já devolve (o teste `it_should_refuse_data_nested_beyond_the_record_body_limit` já fixa `"/c" x 126`). Estouro do teto de 1.024 quadros de avaliação: um só achado em `""`.
5. 10.000 `$defs` encadeados: `Catalog::new` aceita (profundidade de JSON baixa, sem ciclo); só `validate_data` devolve `max-depth-exceeded` em `""`. Já fixado por `it_should_report_max_depth_exceeded_on_a_very_long_ref_chain`.
6. Lacunas: quatro viram teste, sem exceção, porque cada uma é comportamento que o executor poderia errar sem o teste ver: (a) `schema-uri`; (b) `deny_unknown_fields`; (c) `null` nos extremos; (d) o corte da recusa de `Catalog::new`. **Decisão nova em (d)**: o plano só cortava `validate_data`. A recusa de `Catalog::new` também corta em `limits.details_max`... o `Catalog::new` não recebe `Limits`, então o corte é fixo em 50: os 50 primeiros mais um `too-many-errors` com `path` `""` (51 itens); com exatamente 50 não há marcador. Ordem de visita: tipos na ordem do `Vec`, depois relations na ordem do `Vec`; dentro de um schema, palavras na ordem alfabética das chaves (`Map` é `BTreeMap`). Motivo: um schema de 10.000 palavras desconhecidas não pode devolver 10.000 detalhes (mesma razão do TS `capDetails`).

Testes novos (todos em `tests/catalog.rs`, +6, total 42): `it_should_accept_the_2020_12_schema_uri_only_at_the_root` (raiz com `https://json-schema.org/draft/2020-12/schema`, exato, sem `#` final, aceita; raiz com outro URI: `schema-uri` em `/$schema`; `$schema` em `properties/a`: `schema-uri` em `/properties/a/$schema`; valor não texto: `keyword-type`), `it_should_reject_unknown_fields_when_deserializing_definitions` (`TypeDef` e `RelationTypeDef` com campo extra: `Err`), `it_should_read_null_endpoint_lists_as_none_and_never_write_null` (`from: null` e `to: null` leem `None`; `equivalent: null` num `TypeDef` lê `None`; `serde_json::to_value` omite as chaves), `it_should_cap_the_refusal_of_a_catalog_at_50_details_with_a_single_marker` (60 palavras desconhecidas dão 51 itens, o último `("", "too-many-errors")`; 50 dão 50 sem marcador), `it_should_point_ref_pattern_and_type_refusals_at_the_offending_keyword` (tabela: `$ref` externo `/properties/a/$ref`, `$ref` sem alvo `/properties/a/$ref`, `pattern` `/properties/a/pattern`, `patternProperties` `/patternProperties`, `title: 1` `/title`), `it_should_report_a_ref_cycle_once_per_ref_on_the_cycle` (os cinco casos acima com os pares exatos).

Na T5a: a nova `reference_catalog()` (`note` com `text` string de `maxLength` 200 e `count` inteiro `minimum` 0, sem `additionalProperties: false`; `strict-note`; `task`; relations `after` e `owned-by`) não quebra nenhum teste: dado válido de `note` nunca usa `text` não texto nem `count` fora de inteiro natural, e os de profundidade usam outra chave; `type-changed` passa a usar `task`.

### D-18 (correção): `kosaraju_scc`, não `tarjan_scc`

Fato: no `petgraph` 0.8.3, `tarjan_scc` é recursivo (`src/algo/scc/tarjan_scc.rs:53`, "This implementation is recursive"; a linha 96 chama `self.visit(w, g, f)`), então a cadeia de 10.000 `$defs` estoura a pilha de 2 MiB. `kosaraju_scc` é iterativo (`src/algo/scc/kosaraju_scc.rs:19`, `DfsPostOrder` e `Dfs`). Muda: o contrato usa `petgraph::algo::kosaraju_scc` no lugar de `tarjan_scc` (item 1 da D-18). As componentes são as mesmas, então a regra de `ref-cycle` e os testes não mudam. A T3 passa a depender de `petgraph`. Eu havia afirmado que `tarjan_scc` era iterativo sem ler o código: erro meu.

### D-19: valor guardado que os tipos recusam: `corrupt-row` não pode sair de `verify_chain`

Método: leitura de `store/sqlite/tx.rs` (`record_from`, `columns_from`, `snapshot_of`, `retired_of`, `corrupt()`, linhas 183-361) contra a tabela de mutações (plano, linhas 485-520).

Achado imediato, não só hipótese: a linha `snapshot_kind` da tabela (`snapshot_kind := NULL`, `snapshot_id` intacto) cai em `snapshot_of` no braço `(None, Some(_))` e devolve `StoreError corrupt-row`. Pela tabela, `breaks[0]` deveria ser `ColumnMismatch` no `seq` do alvo; com o store de hoje, `verify_chain` devolveria `Err`. O mesmo vale para qualquer valor fora dos tipos (`retired := 2`, `lineage_kind := 'x'`, `hash` fora de 64 hexadecimais minúsculos, `type` fora do formato).

| Alternativa | Efeito |
|---|---|
| A. Manter `corrupt-row` | `verify_chain` devolve erro em vez de relatório nas adulterações que os tipos recusam; não diz qual `seq`; contradiz "quebra é resultado, não erro" (H:241) e a linha `snapshot_kind` da tabela |
| **B. `VerifyReads` lê texto cru (escolhida)** | toda coluna adulterada vira `ColumnMismatch` (ou `HashMismatch`/`DivergingSeq` onde a ordem de conferência manda), com o `seq` certo; custo: uma porta e um conjunto de linhas novos |
| C. B só para as colunas que a tabela adultera | deixa `retired := 2` e afins como `Err`; a regra fica "depende da coluna" |

Escolhido B, na **T5c** (a T5b não depende dela). Forma: `VerifyReads::records_from` devolve `StoredRow` e `batches` devolve `StoredBatch`, tipos em `engine/ports.rs` só com `String`, `i64` e `Option<String>` (as colunas como estão no banco: `snapshot_kind` e `snapshot_id` separados, `retired` como inteiro); `VerifyReads::tenant` também devolve texto cru (`anchor` adulterado para lixo não pode virar `Err`). `verify_chain` compara texto com o valor recalculado do corpo e nunca chama `Hash::parse`, `RecordId::parse` etc. nas colunas. As leituras de `WriteReads` e `QueryReads` continuam tipadas (uma linha ilegível ali é `StoreError corrupt-row`, correto: ninguém está verificando).

Junto, na política `Tail` (T5c liga `Full`): o passo 5 do `decide` lê o último record por `batch_records` (tipado). `StoreError` com detalhe `corrupt-row` nessa leitura vira `ChainBroken` com detalhe `tail-unreadable`, `path` `""`; qualquer outro `StoreError` segue como está. Motivo: a cauda ilegível é adulteração, e o chamador tem de receber o código que significa isso.

Testes (T5c): a tabela de mutações não muda de tamanho (a linha `snapshot_kind` passa a ser coberta de verdade); mais um teste em `tests/chain_tamper.rs`, `it_should_report_a_break_instead_of_a_store_error_when_a_stored_value_is_not_valid_for_its_type`, com quatro adulterações fora da tabela (`retired := 2` e `lineage_kind := 'x'` dão `ColumnMismatch`; `hash := 'ZZ'` dá `HashMismatch`; `type := 'Bad Type'` dá `ColumnMismatch`), todas no RC3, `breaks[0]` no `seq` do alvo; e em `tests/concurrency.rs`... não: um em `tests/chain_tamper.rs`, `it_should_fail_with_chain_broken_on_write_when_the_tail_record_has_an_unreadable_column` (`retired := 2` no último record; `append` dá `ChainBroken` `tail-unreadable`, nada gravado). T5c passa de N para N+2 em `chain_tamper`.

### D-20: regras de `Relation` e de `Retirement` de relation no `decide`

Método: plano 4.5 (ordem de recusa, "Relation e ciclo", tabela S/X) e seção 5 (`relations`); `ports.rs` e `tx.rs` (`live_relation_pairs`: devolve só pares com extremo de origem `native` ou node do tenant vigente, sem dizer a origem; `latest_record` devolve `LatestRow` sem origens nem `native_catalog`); costura 5 (tabela de presença); D-16; experimento `scratchpad/critic/d8/examples/absent.rs` com `petgraph` 0.8.3 (`has_path_connecting` com extremo fora do grafo devolve `false`, não entra em pânico; o ciclo `a->b->c->d->a` com arestas de três fontes é achado).

Ordem de recusa dentro do passo 6 para um item `Relation` (primeiro achado, um detalhe; a ordem é decisão minha, o plano só lista os motivos):

1. `lineage-in-batch` (`ForkRejected`, `/items/<i>/body`, sem `current`): chave de lineage é o par (tipo de lineage, chave) com a chave da relation `<relation_type>|<from>|<to>`, e vale entre `Relation` e `Retirement` da mesma lineage.
2. Lineage e `base`, como o D-16 para node: sem latest e com `base`: `base-unknown` (`InvalidRecord`, `/items/<i>/base`); com latest e `base` ausente ou diferente: `ForkRejected` `not-current`, `current = Some(latest.record_id)`, `retired = Some(latest.retired)`.
3. Extremos, `from` antes de `to`: resolução na ordem (a) `NativeNodes::lookup` (o nativo prevalece, como na D-15, mesmo que o tenant tenha um node de mesmo id), (b) sobreposição do lote (nodes dos itens anteriores: `Node` entra como vigente com o tipo dele; `Retirement` de node entra como retirado), (c) `latest_record(tenant, Node(id))`. Extremo sem resultado, ou do tenant e retirado: `InvalidRecord` `endpoint-not-found` em `/items/<i>/body/from` ou `/to`.
4. Os dois extremos nativos: `InvalidRecord` `native-only-relation` em `/items/<i>/body`.
5. Tipo do extremo contra `RelationTypeInfo.from` e `.to` (`None` = qualquer): o tipo vem do nativo (`NativeNode.type_name`), da sobreposição ou do latest; fora da lista: `InvalidRecord` `endpoint-type` em `/items/<i>/body/from` ou `/to`.
6. Ciclo, só se `RelationTypeInfo.acyclic`: aresta nova `from -> to` fecha ciclo se há caminho `to ~> from` no grafo do relation type formado por `live_relation_pairs`, `NativeNodes::native_relations` e as arestas dos itens `Relation` anteriores do lote com o mesmo tipo: `CycleRejected`, detalhe `cycle`, `/items/<i>/body`, mensagem fixa. Retirement dentro do lote **não remove** aresta do grafo (nem de relation nem de node): o lote nunca aceita ciclo por causa de uma retirada do próprio lote; quem quer isso manda dois lotes. Alternativa descartada: remover as arestas dos retirados do lote (mais código, caso raro, e o erro errado seria aceitar ciclo).

Revisão de relation (com `base`) valida extremos e ciclo de novo; revisar sobre retirement reinstala a relation (como em node) e passa por todos os passos. Estáticos (passo 4 do `decide`, D-15): `RelationTypeNotFound` entra na passada A, em `/items/<i>/body/relation_type`, detalhe `relation-type-not-found`; `self-relation` (`from == to`, qualquer origem) entra na passada B, `InvalidRecord`, `/items/<i>/body/to`; `snapshot` em relation: `snapshot-not-allowed` em `/items/<i>/snapshot` (S8); `external_id` em relation vale (X1).

`Retirement` de relation: sem catálogo (o relation type pode ter saído do catálogo; a retirada continua possível), sem validar extremos nem ciclo (retirar com extremo retirado é a limpeza esperada). Regras do latest como no node: sem latest, `InvalidRecord` `lineage-not-found` em `/items/<i>/body/lineage`; `base` errado, `not-current`; `latest.retired`, `already-retired`. O record copia do latest: `type`, `def_hash`, `from_origin`, `to_origin` e `native_catalog`. `LatestRow` não traz origens nem `native_catalog`: o `decide` chama `reads.record_by_id(latest.record_id)` e lê as duas origens das colunas (`RecordColumns`) e `native_catalog` do `body` (`RecordBody` desserializado; falha ou origem ausente em relation: `Internal`, detalhe `invalid-body`). Alternativa descartada: pôr as três coisas em `LatestRow` (muda `ports.rs` e `tx.rs` com a T4 verde, para economizar uma leitura só em retirement de relation).

Montagem de `Relation` (raiz ou revisão): `type` e `def_hash` do relation type do catálogo (`RelationTypeInfo.def_hash`); `lineage` com `from_origin` e `to_origin` da resolução do item (na revisão, recalculadas agora); `native_catalog` = `NativeNodes::catalog_version()` só se algum extremo é nativo; `data` e `snapshot` ausentes; `external_id` se vier; `revises` = latest quando há; colunas: `lineage_kind` relation, `relation_type`, `from_id`, `to_id`, origens. **Chave da lineage na coluna `lineage_key`**: `<relation_type>|<from>|<to>` com U+007C, igual a `lineage_columns` de `store/sqlite/tx.rs`. Duas implementações desencontradas deixam `latest_record` cego e o `FORK_REJECTED` some em silêncio; por isso a T5b **move `lineage_columns` de `tx.rs` para `engine/model.rs`** (`pub(crate) fn lineage_columns(lineage: &Lineage) -> (&'static str, Cow<'_, str>)`), e `tx.rs` e `decide` a usam. O teste `it_should_reject_a_second_root_for_an_existing_relation_lineage` pina isso.

Forma do código do ciclo (compila, medido): carregar antes e montar depois. Primeira passada: para cada relation type `acyclic` que aparece em itens `Relation` do lote, buscar `live_relation_pairs` e `native_relations` e guardar os `Vec<(SemanticId, SemanticId)>` num `HashMap<&RelationTypeName, ...>` completo. Segunda passada: um `DiGraphMap<&str, ()>` por tipo, montado emprestando desse mapa e de `req`, e as arestas do lote entram como `&str` de `req`. Montar o grafo enquanto o mapa ainda recebe inserções é autorreferência e não compila.

### D-21: `Hexlog::relations` e `RelationView`

Método: plano 4.4 (`RelationView`, `EndpointStatus`, `RelationQuery`), `tx.rs` (`latest_relations` ordena por `seq`, filtra por tipo, `from`, `to` e `include_retired`), princípio do plano "payload sempre lido de `body`".

Escolhido: `relations` roda dentro de uma `Store::read`: `WriteReads::tenant` (ausente: `TenantNotFound` em `/tenant`), `latest_relations`, e por linha a `RelationView` montada do **`body`** (`at`, `author`, `lineage`, `external_id`, `native_catalog`; origens do `lineage` do corpo, não das colunas; `retired` = `retirement` do corpo; `record_id` e `seq` da linha). Status de cada extremo, por origem guardada: `Native`: `NativeNodes::lookup` presente = `Current`, ausente = `MissingNative`; `Tenant`: `latest_record(tenant, Node(id))`, retirado = `Retired`, senão `Current`, e node sem nenhum record é `Internal` `invalid-body` (relation do tenant sem extremo é banco adulterado; `verify_chain` não confere existência de extremo, por decisão do plano). Corpo ilegível ou `lineage` que não é de relation: `Internal` `invalid-body`. Sem paginação: o resultado é todo o conjunto filtrado (o `RelationQuery` do plano não tem limite). Limite aceito, não decisão minha: quem chama filtra por `from`, `to` ou tipo.

### D-22: o teste de ponta a ponta do corpo cru não pode usar a fixture `golden_bodies.json` literalmente

Método: conferência campo a campo da fixture `golden_bodies.json` contra o que `append` produz (leitura do arquivo, costura 5, S1 a S8, D-12).

Três impedimentos, medidos na fixture: (1) `node_full` é `type: note` **com** `snapshot` e `node_keyless_batch_first` é `type: note` **sem**: pelas linhas S1/S3 um mesmo tipo não admite os dois; (2) `retirement_node` (seq 5) e `retirement_relation` (seq 6) não têm `batch`, mas `node_full` (seq 3) tem `count: 2` e fecha o lote com o seq 4, então o seq 5 abre lote e leva `batch` em qualquer `append`; (3) `def_hash` `aaaa...` e `fingerprint` `bbbb...` são marcadores, não os valores reais. A fixture continua certa para o teste de unidade de `model` (serialização por campo, T2).

Escolhido: o teste de ponta a ponta escreve o JSON esperado de cada um dos 6 records à mão, com `serde_json::json!`, a partir da tabela de presença da costura 5 (independente da implementação), com `def_hash` de `Catalog::type_hash`/`relation_type_hash` e `fingerprint` de `hexlog::chain::fingerprint(itens)` (a impressão tem teste dourado próprio em `jcs_vectors`). Cobre as cinco formas: node raiz sem chave (seq 1), node raiz com `snapshot` e `external_id` (seq 2), node revisão com chave e lote de 2 (seq 3), relation com origem nativa (seq 4), retirement de node (seq 5), retirement de relation (seq 6). Compara `canonical(esperado)` com a coluna `body` crua (`Tamper::read_body`) e confere `hash == hash_record(prev_hash, body)` em cadeia, a partir do chain head do tenant vazio.

Nota sobre o fim da D-19 (texto confuso acima, o log só cresce): os testes novos da T5c são exatamente dois, ambos em `tests/chain_tamper.rs`: `it_should_report_a_break_instead_of_a_store_error_when_a_stored_value_is_not_valid_for_its_type` e `it_should_fail_with_chain_broken_on_write_when_the_tail_record_has_an_unreadable_column`.

### D-23: dúvidas do executor da T3 (nome repetido, `type: []`, ordem das guardas, margem de pilha)

Método: leitura de `catalog/types.rs` (`refuse_repeated_names`, linhas 134-150) e de `catalog/validate.rs` (`type_mask`, linha 74; `required` e `enum`, linhas 345-367); TS `src/domain/ids.ts:41` (`refine` com a mensagem `duplicate type name` para lista de nomes de tipo); metaesquema 2020-12 (`type` array com `minItems: 1` e `uniqueItems: true`; `required` com `uniqueItems: true`; `enum` só `type: array`, `items: true`); medida do executor da T3 para a pilha (não repeti).

1. **Nome repetido no `Vec`: `duplicate-name` fica.** O TS recusa lista de nomes de tipo com repetição (`ids.ts:41`), e recusar é a única opção que não escolhe um vencedor em silêncio (o outro seria "o último vence"). Ajuste: hoje o código empurra um detalhe por ocorrência a mais (três cópias dão dois detalhes idênticos); passa a ser **um detalhe por nome repetido**, `path` `/types/<nome>` ou `/relations/<nome>`. Um tipo e uma relation com o mesmo nome convivem (são espaços de nomes separados). Testes novos em `tests/catalog.rs`: `it_should_reject_a_repeated_type_or_relation_name` (`[t, t, t]` dá exatamente `[("/types/t", "duplicate-name")]`; `[r, r]` dá `[("/relations/r", "duplicate-name")]`) e `it_should_accept_a_type_and_a_relation_type_with_the_same_name`.
2. **`type: []` passa a ser recusado; `required` repetido também; `enum` repetido e `required: []` ficam aceitos.** Pelo metaesquema, `type: []` e `required` com nome repetido são schema inválido (o ajv do TS os recusa ao compilar); `enum` não exige unicidade em 2020-12. `type` com nome repetido já é recusado hoje (a máscara de bits recusa sobreposição). Código: `keyword-type`, ponteiro da palavra (`/type`, `/required`). Cuidado do executor: rodar a suíte oficial (392) de novo; o filtro só admitiu grupos do subconjunto, então não deve haver `type: []` nela. Testes novos: `it_should_reject_an_empty_type_list_and_a_repeated_required_name` e `it_should_accept_a_repeated_enum_value_and_an_empty_required_list`.
3. **Guarda de `json_issues` achando algo: só esses achados saem e o `compile` não roda, por schema.** Fica, pelo mesmo motivo do lado do dado (`validate_data`): o `compile` não pode descer num documento fundo demais. O custo é a correção em duas rodadas (número inseguro agora, `pattern` depois). Vale por schema: o schema de outro tipo do mesmo `Catalog::new` segue sendo analisado. Teste novo (pina a decisão): `it_should_report_only_the_json_guard_findings_when_the_schema_also_has_other_problems` (schema com `minimum: 1.5` e `pattern` num campo: só `number-not-safe-integer`; o outro tipo, com `pattern`, ainda dá `pattern-not-allowed`).
4. **Margem de pilha 1,8x sobre o teto de 1.024 quadros: fica.** Estoura entre 1.792 e 2.048 quadros em debug com 2 MiB; em release a pilha por quadro é bem menor. O teto de 1.024 vem da D-7 e cobre 126 níveis de dado com até cerca de oito níveis de schema por nível de dado; schema que gasta mais que isso por nível recebe `max-depth-exceeded` com dado válido, e é o custo escolhido. Documentar na costura 6: "`validate_data` assume pilha de pelo menos 2 MiB em debug; thread com pilha menor não é suportada em debug". Sem teste novo: o `it_should_not_overflow_the_stack_on_deeply_nested_data` já roda na thread de 2 MiB.

### D-24: `verify_chain` (algoritmo, tipos e ordem)

Método: plano 4.5 (linhas 477-481 e tabela de mutações), seção 5 (C3), `ports.rs`, `tx.rs`, `schema.sql` (PK `(tenant, seq)`, `CHECK (seq >= 1)`, índices únicos de raiz e sucessor), D-10 e D-19.

- Tipos públicos em `engine/model.rs`, exportados em `lib.rs`: `ChainReport { ok, total_records: u64, chain_head: Hash, breaks: Vec<ChainBreak> (até 100), total_breaks: u64 }`, `ChainBreak { seq: Option<u64>, reason: BreakReason }`, `BreakReason` (os sete do plano). `chain_head` é o hash **recalculado** do último record (`hash_record(prev_hash armazenado, body armazenado)`), ou a âncora recalculada de tenant vazio: sempre um `Hash` válido, mesmo com a coluna adulterada, e igual ao armazenado na cadeia limpa. `seq` armazenado negativo ou fora de `u64` sai como `None` (borda que só um `tamper` cria).
- `verify_chain(reads: &mut dyn VerifyReads, tenant: &TenantId, expected: Option<&Hash>) -> Result<ChainReport, HexlogError>`, em `engine/verify.rs`. Tenant desconhecido: `TenantNotFound`. Leitura em páginas de 500 por `stored_records`, a partir do último `seq` armazenado (`after_seq`), com `None` na primeira: cobre `seq` 0 ou negativo adulterado.
- Tenant: um único `AnchorMismatch` (`seq` `None`) se `sha256hex(anchor_doc) != anchor` **ou** `anchor_doc != chain::anchor_doc(tenant, created_at)`. A âncora usada na cadeia é `sha256hex(anchor_doc)` recalculada.
- Record, em ordem de `seq` armazenado, parando no primeiro problema de cada um: (1) `seq` armazenado diferente de `seq` do anterior mais 1 (1 no primeiro): `DivergingSeq`; o contador segue o `seq` armazenado, não o esperado; (2) `prev_hash` diferente do `hash` armazenado do anterior (da âncora recalculada no primeiro), ou `hash` armazenado diferente de `hash_record(prev_hash armazenado, body armazenado)`: `HashMismatch`; (3) `body` não lê como `RecordBody`, ou `v != 1`, ou `body.canonical()` falha ou difere do texto: `InvalidBody`; (4) `body.tenant` diferente do tenant verificado, ou `body.seq` diferente do `seq` armazenado: `ColumnMismatch`; (5) cada coluna derivada, comparada como texto com o valor recalculado do corpo: `ColumnMismatch`. Colunas: `id`, `at`, `type`, `lineage_kind`, `lineage_key` (id do node; `<relation_type>|<from>|<to>` na relation), `retired` (0 ou 1 do `retirement`), `revises`, `relation_type`, `from_id`, `to_id`, `from_origin`, `to_origin` (NULL no node), `snapshot_kind` e `snapshot_id` (NULL sem snapshot), `external_id`, `def_hash`. `v != 1` é decisão minha (formato desconhecido não deve passar só porque o hash fecha); teste opcional abaixo.
- Lote: cada record com `body.batch.key` define um lote esperado `(key, fingerprint, count, seq do record)`; as linhas de `stored_batches` são comparadas por chave: linha sem lote esperado, lote esperado sem linha, ou `fingerprint`, `first_seq` ou `item_count` diferentes: `BatchIndexMismatch` (`seq` `None`), um por problema. Record com `InvalidBody` não alimenta o conjunto esperado.
- Ordem de saída: tenant, lotes, records por `seq`, e por fim `ChainHeadNotFound`: `expected` fora do conjunto {âncora recalculada} mais {`hash` armazenado dos records sem nenhum problema}. `breaks` guarda até 100 na ordem; `total_breaks` conta todos; `ok = total_breaks == 0`.

### D-25: leitura crua do `VerifyReads` (detalha a D-19) e testes do store que mudam

Método: leitura de `tx.rs` (`record_from`, `batch_from`, `tenant_from`) e de `store/sqlite/tests.rs` (todos os usos de `records_from`, `batches` e `VerifyReads`, linhas 413-419, 747, 1318-1321, 1357-1358, 1385, 1415, 1539); `schema.sql` (tabelas `STRICT`: o texto e o inteiro de cada coluna continuam legíveis como `String` e `i64` mesmo adulterados; o que a adulteração muda é o valor, não o tipo de armazenamento).

Portas novas em `engine/ports.rs` (os métodos tipados de `VerifyReads` saem; `WriteReads` e `QueryReads` não mudam):

```rust
pub(crate) struct StoredTenant { pub tenant: String, pub anchor: String, pub anchor_doc: String, pub created_at: String }
pub(crate) struct StoredRow { pub tenant: String, pub seq: i64, pub id: String, pub prev_hash: String, pub hash: String, pub body: String,
    pub at: String, pub type_name: String, pub lineage_kind: String, pub lineage_key: String, pub retired: i64, pub revises: Option<String>,
    pub relation_type: Option<String>, pub from_id: Option<String>, pub to_id: Option<String>, pub from_origin: Option<String>, pub to_origin: Option<String>,
    pub snapshot_kind: Option<String>, pub snapshot_id: Option<String>, pub external_id: Option<String>, pub def_hash: String }
pub(crate) struct StoredBatch { pub key: String, pub fingerprint: String, pub first_seq: i64, pub item_count: i64 }
pub(crate) trait VerifyReads {
    fn stored_tenant(&mut self, tenant: &TenantId) -> Result<Option<StoredTenant>, HexlogError>;
    fn stored_records(&mut self, tenant: &TenantId, after_seq: Option<i64>, limit: u32) -> Result<Vec<StoredRow>, HexlogError>;  // seq > after_seq, ordem de seq
    fn stored_batches(&mut self, tenant: &TenantId) -> Result<Vec<StoredBatch>, HexlogError>;                                      // ordem de first_seq
}
```

Efeito colateral bom: `VerifyReads` deixa de ter `tenant`, então o E0034 (`StoreReader` com dois `tenant`) some; a nota de risco da T5a/T4 fica obsoleta.

Testes de `sqlite_store` (30 passam a 32): mudam (a) o auxiliar `rows_from` (linha 413) passa a chamar `batch_records(tenant, from_seq, limit)`, que tem a mesma semântica (`seq >= from_seq`, ordem de `seq`), então `it_should_read_records_from_a_seq_up_to_a_limit`, `it_should_persist_nothing_when_decide_fails` e `it_should_write_nothing_for_an_empty_plan_from_a_replay` não mudam de texto; (b) `it_should_read_a_tenant_row_and_none_for_an_unknown_tenant`: o lado `VerifyReads::tenant` vira `stored_tenant` e compara os quatro campos como texto; (c) `it_should_list_the_batches_of_a_tenant`: `stored_batches`, `first_seq` e `item_count` como `i64`; (d) `it_should_append_records_and_batch_atomically`: a leitura de `records_from` (linha 1415) vira `batch_records(tenant, 1, 10)`. Novos: `it_should_read_stored_records_after_a_seq_as_text` (`after_seq: None` lê tudo, `Some(2)` lê só `seq > 2`, `limit` corta, cada campo igual ao do plano gravado, em texto) e `it_should_read_stored_rows_even_when_a_value_is_not_valid_for_its_type` (pela conexão `Raw` do arquivo, que mantém triggers e restrições, insere um record com cadeia certa e `hash` `ZZ` e `type` `Bad Type`, valores que o DDL aceita porque `hash` e `type` não têm restrição de domínio: `stored_records` os devolve como texto e `batch_records` devolve `StoreError` com detalhe `corrupt-row`). Sem `ignore_check_constraints` em `src/`: o varredor de vocabulário veta a palavra; os valores adulterados que dependem dela (`retired := 2`, `lineage_kind := 'x'`) ficam em `tests/chain_tamper.rs`.

### D-26: política `Full` e `Tail` ligadas na Facade e no `decide`

Método: plano 4.5, linha 481 ("Full roda a verificação completa antes de cada escrita e leitura"); `Store::append_with` entrega só `WriteReads` ao `decide` (`ports.rs`); trait upcasting estável no 1.97 do `rust-toolchain.toml`.

- **Leitura** (`get`, `lineage_records`, `list`, `relations`, `chain_head`) com `Full`: dentro da mesma `Store::read`, antes da consulta, `verify_chain(r, tenant, None)` (o `&mut dyn StoreReader` vira `&mut dyn VerifyReads` por upcasting); `total_breaks > 0` devolve `ChainBroken`, `details[0].code` = motivo da primeira quebra em kebab (`hash-mismatch`, `column-mismatch`, `diverging-seq`, `invalid-body`, `batch-index-mismatch`, `anchor-mismatch`, `chain-head-not-found`), `path` `""`, mensagem fixa. Tenant desconhecido: a verificação não roda, a consulta responde `TenantNotFound` como já fazia.
- **Escrita** com `Full`: antes de `append_with`, uma `Store::read` com a mesma verificação, e `ChainBroken` se houver quebra. Fica fora da transação de escrita, de propósito: `append_with` entrega só `WriteReads`, e acrescentar `VerifyReads` ali mudaria a porta e a T5a por um ganho que não existe (verificação não disputa corrida: um concorrente só acrescenta). Alternativa descartada. Consequência: com `Full`, cadeia quebrada e entrada inválida ao mesmo tempo dão `CHAIN_BROKEN`, não `INVALID_INPUT`; tenant desconhecido segue `TenantNotFound`.
- **`Tail`** na escrita: o passo 5 do `decide` (T5a) passa a mapear `StoreError` com detalhe `corrupt-row` da leitura da cauda para `ChainBroken` `tail-unreadable` (D-19).
- `Full` nunca se aplica a `verify_chain` nem a `ensure_tenant`.
- Testes novos em `tests/chain_tamper.rs`: `it_should_refuse_reads_with_chain_broken_when_the_policy_is_full_and_a_record_is_tampered` e `it_should_not_notice_a_tampered_middle_record_on_read_under_the_tail_policy`. A escrita sob `Full` e `Tail` fica nos testes de `concurrency.rs` (T5d), como no plano.

### D-27: T5d, o que cada corrida garante e como o teste prova `Hexlog: Send` sem `Arc` nem `Mutex`

Método: `clippy.toml` (proibidos: `Arc`, `Mutex`, `RwLock`, `OnceLock`, `Rc`, `Cell`; **permitidos**: `std::thread::scope`, `std::sync::Barrier`, `AtomicU64`, `mpsc`); `tests/support/mod.rs` (`FixedClock`, `SeqIds` com `AtomicU64`, `FakeNative` são `Sync`; `Harness` herda); D-6 (medida de 8 threads x 20 sem `BUSY` com 5 s; abertura concorrente) e D-12 (replay sob a trava); `tests/support/tamper.rs` (`Tamper::open` **remove os sete triggers ao conectar**).

Decisões:
1. Prova de `Send` em uso, não só em compilação: nos testes de corrida 1 a 3 as instâncias são abertas na thread do teste e **movidas** (`move`) cada uma para a sua thread de `std::thread::scope`; o compilador recusa se `Hexlog<'_>` não for `Send`. Os donos (`Harness`, com relógio, ids e nativo) são emprestados por `&` às threads, sem `Arc`. O teste `it_should_be_send` fica como asserção de compilação (`fn require_send<T: Send>()` instanciada com `Hexlog<'d>` numa função genérica sobre `'d`) mais uma ida e volta: abre, move para uma thread de `scope`, grava lá, devolve pelo `join`, lê na thread do teste.
2. Saídas determinísticas, sem `sleep`: `Barrier` solta as threads juntas, e `BEGIN IMMEDIATE` as serializa; o perdedor da corrida de revisão sempre chega depois do `COMMIT` do vencedor, então recebe `not-current` (e não o 2067 do índice, que só aparece sem a trava); o perdedor da corrida de `key` sempre vê o lote gravado e recebe `replayed: true` (D-12, replay sob a trava). Por isso os testes afirmam o detalhe `not-current` e `replayed`, e não apenas "um falha".
3. `busy_timeout` de 30 s só nos testes de `concurrency.rs`, por `Config { busy_timeout, ..harness.config() }` e `open_with`; o padrão de 5 s não muda (regra do handoff).
4. Abertura concorrente: cada thread abre dentro da thread (é o objeto do teste), com `Barrier` de 8 antes do `open`; um `TempDir` novo por rodada. A conferência do esquema precisa de uma conexão que **não** remova os triggers: `Tamper::open` os remove. T5d acrescenta a `tests/support/tamper.rs` um tipo `Inspect` (conexão `sqlx` crua somente leitura, `Inspect::open(path)` e `read_text(sql, binds)`, sem `DROP TRIGGER`). Alternativa descartada: ler `sqlite_master` antes de abrir o `Tamper` (a conexão que cria o banco é a do `Hexlog`, e a leitura depois da corrida é a que interessa).
5. Os três testes de adulteração usam `Tamper` depois de soltar o `Hexlog` que escreveu, e confirmam `rows_affected == 1`.

### D-28: o `omhl` da T6 não ganha comando; o diretório pai do banco é do passo 4

Método: leitura do plano (4.2: `omhl/src/main.rs # --version apenas`; T6 não cita `omhl` em Arquivos nem em Pronto; costura 7 diz "quem cria é o `omhl` (T6)"), de `crates/omhl/src/main.rs` (já entregue na T1: `--version` imprime `omhl <versão>` e sai com 0; qualquer outro argumento imprime o uso no stderr e sai com 2) e do mestre (o `omhl` de verdade é o passo 4).

| Alternativa | Efeito |
|---|---|
| A. T6 acrescenta ao `omhl` o caminho padrão do banco (`$XDG_DATA_HOME/omhl/omhl.db`) e `create_dir_all` | código sem comando que o chame; nada o exercita; vira dívida do passo 4 com decisão já tomada às cegas |
| **B. `omhl` fica como está; a costura 7 passa a dizer "o diretório pai é criado pelo chamador de `Hexlog::open`; o `omhl` do passo 4"** | nenhum código morto; a recusa (`StoreError` `"14"`) já está testada no `hexlog` |

Escolhido B. O que a T6 acrescenta é só a prova do contrato que já existe: dois testes em `crates/omhl/tests/cli.rs` (sem dependência nova; `env!("CARGO_BIN_EXE_omhl")` e `std::process::Command`).

### D-29: dependência declarada e não usada: `time`

Método: para cada crate de `[dependencies]` de `hexlog`, `grep -rlw` em `crates/hexlog/src` e `cargo tree -p hexlog -e normal -i time`. Resultado: `serde`, `serde_json`, `sha2`, `sqlx`, `futures_executor`, `uuid`, `thiserror`, `url` e `petgraph` têm uso; `time` aparece só como `std::time::Duration` (`store/sqlite/{mod,ddl,tests}.rs`), nenhum `use time::`; e `cargo tree -i time` mostra `hexlog` como único dependente, então nada mais no fecho exige o crate. O formato do instante é validado à mão (D-13) e o relógio entrega texto.

Escolhido: **remover `time = "=0.3.55"` de `crates/hexlog/Cargo.toml`** na T6 e regerar o `Cargo.lock`; sai também da lista de dependências do plano (linha 191) e do relatório de auditoria. Efeito: o fecho de dependências normais encolhe (`time-core`, `deranged`, `powerfmt`, `num-conv` e afins, se nada mais os pedir; conferir com `cargo tree`). Nada no código muda. Alternativa descartada: manter para uso futuro (o passo 4 declara o que precisar).

### D-30: remoção dos `allow` da T6, ensaio e regra para cada achado

Método: cópia da árvore de hoje (T3 e T4 prontas, T5a em andamento) para `scratchpad/critic/t6/`, remoção dos dois `allow` e `cargo clippy -p hexlog --lib`: **88 avisos**, todos em `engine/model.rs` (`RecordBody` e auxiliares), `engine/ports.rs` (portas e linhas sem consumidor) e `store/sqlite/{mod,ddl,tx}.rs` (o `SqliteStore` nunca construído leva junto as constantes de SQL e os auxiliares), mais `unused import: sqlite::SqliteStore` em `store/mod.rs`. Em `engine/limits.rs`, `chain.rs`, `snapshot.rs`, `ids.rs`, `error.rs` e `catalog/` **nenhum aviso** (itens alcançáveis por `pub use` de `lib.rs`). Os 88 são ruído de consumidor ausente (`decide` e Facade ainda não ligados), então a lista final só existe depois da T5c; o ensaio mostra o método e as áreas.

Regra para cada aviso que sobrar depois da T5c, nesta ordem: (1) item alcançável por `pub use`: não aparece, nada a fazer; (2) item só usado por teste de unidade dentro de `src/`: `#[cfg(test)]` no item; (3) campo de struct de porta nunca lido, método de porta nunca chamado em produção, função auxiliar sem chamador: **apagar**, junto com a atribuição no store e com a asserção do teste que o exercitava (o executor não edita teste: o executor lista, a sessão principal passa o teste ao test-engineer, e só depois o item é apagado); (4) não há "manter com motivo": nem `allow` nem `#[expect(dead_code)]` voltam (handoff). Candidatos prováveis, a confirmar rodando: `LatestRow.seq` (o `decide` usa `record_id`, `retired`, `type_name` e `def_hash`) e `TenantRow.anchor_doc` e `TenantRow.tenant` (a Facade devolve `anchor`, `created_at` e `created`; a conferência de `anchor_doc` usa `StoredTenant`). Os dois `allow` saem juntos com a linha de comentário `// removido na T6`.

## Decisões da sessão principal na execução

Decisões de processo ou de baixo risco que a sessão principal tomou sem passar pelo crítico. Todas entram no relatório final.

- **S-1 (T3b em cópia isolada):** a emenda da D-23 rodou numa cópia do workspace, porque o `tests/support/mod.rs` da T5a só compilava com a Facade pronta. Só `src/catalog/{types,validate}.rs` voltou para a árvore real.
- **S-2 (revisão da T3):** aplicados A.1 a A.4 e B.1 a B.7. Ficaram de fora A.5 (vetor ordenado no lugar do `HashMap` com chave `String`), porque a chave do mapa é um segundo dono legítimo e o revisor a marcou como opcional, e B.8 (`percent-encoding` como dependência direta no lugar do `percent_decode` de 19 linhas), porque seria dependência nova sem ganho de comportamento.
- **S-3 (teste de 5.000 níveis da T5a):** o estouro vinha do `Drop` recursivo do `serde_json::Value` que o próprio teste montava, não do motor. O executor mediu: 1.000 níveis passam em 2 MiB de debug e 2.000 estouram. O test-engineer reescreveu o teste no padrão da T2b: o valor nasce e morre numa thread de 64 MiB, e só o `append` roda numa thread com escopo de 2 MiB.
- **S-4 (ambiguidades do vermelho da T5a):** o suporte usa `open_with(&Config, Catalog)` por referência, porque o lint `needless_pass_by_value` acusa a passagem por valor. O `FixedClock` não distingue relógio lido uma vez de relógio lido N vezes, então o teste prova só que o instante é o mesmo em todos os records do lote. O limite foi aceito sem criar um relógio contador.
- **S-5 (desvios aceitos do executor da T5a):** `pub(crate) use rules::instant_is_well_formed` em `engine/mod.rs`, porque a Facade valida o instante do `ensure_tenant`, e `HexlogError::tenant_not_found()` em `error.rs`, porque a recusa é idêntica no `decide` e na Facade.
- **S-6 (revisão da T5a):** aplicados `at_item`, o inline de `clock_format` e de `snapshot_not_allowed`, o `NodeView` montado direto do corpo, `Subject.type_name: Name`, `Limits: Copy` e chamadas de método nas portas.
- **S-7 (ambiguidades do vermelho da T5b):** aceitas as cinco escolhas do test-engineer, todas dentro do contrato. O teste de reinstalação virou um cenário encadeado, e os 31 testes ficaram. O relation type local se chama `loose`. `lineage-in-batch` vem antes de `base-unknown`. O cenário de base velha usa duas revisões. O `append.rs` espera um id por `seq`.
- **S-8 (desvios aceitos do executor da T5b):** `endpoint_types_allowed` recebe `&str`, porque o tipo do extremo chega como `TypeName` ou como `Name` (a costura 13 foi emendada). `invalid_body` e `read_body` foram para `engine/model.rs`, porque o `engine` não importa da Facade.
- **S-9 (revisão da T5b):** aplicados o inline de `closes_cycle` (costura 13 emendada), o fim da duplicação de `Origin` em `tx.rs`, `ends_of_item` e o `if let` de `validate_item_shape`. O `origin_text` de `store/sqlite/tests.rs` fica, como oráculo do teste. O `LineageRef` emprestado, para os clones que só montam chave de busca, foi ao crítico e vira rodada própria depois da T5c.
- **S-10 (ambiguidades do vermelho da T5c):** aceitas as escolhas do test-engineer que dão dentes aos testes:
  - `to_id := a.two`, porque o RC4 já aponta para `n.other` e o valor da tabela não adulterava nada;
  - `body_seq` igual ao `seq` do RC6 mais 1, porque o 7 fixo coincide com o `seq` do RC6 quando há um nó extra;
  - o campo `Mutation.target` e os auxiliares `expected_seq` e `tamper_and_verify`;
  - alvos tirados de `outcome.records` em vez de consulta por forma, que dão o mesmo alvo sem depender da posição;
  - a cauda reescrita muda só `author.agent`, que nenhuma coluna espelha.

  `chain_properties` leva cerca de 48 s com os 24 casos do contrato.
- **S-11 (T5c inteira numa cópia isolada):** testes e implementação da T5c rodam na cópia `t5c-ws`. Assim o git-master versiona a árvore real verde sem pegar teste vermelho. Quando a T5c ficar verde, voltam os arquivos de teste e de `src/` que mudaram.
- **S-12 (git-master em opus):** por pedido explícito do dono, o git-master roda em opus, exceção à regra de subagentes `sonnet` ou `haiku`. A importação vai para `main`, e as entregas seguintes vão para o branch `feat/hexlog-core-step3`, um commit por entrega.
- **S-13 (`.omc/` no repositório):** entram os documentos de `.omc/` (`drafts`, `handoffs`, `plans`, `research` e `specs`), porque o dono tornou a pesquisa entregável. Ficam fora o resto de `.omc/` (estado, wiki, notepads) e `.ignore/`, com as notas locais do dono.
- **S-14 (uma quebra por lote):** a costura 14 dizia "um por problema" nos lotes. Ficou valendo a leitura do executor: uma `BatchIndexMismatch` por lote com problema, e não uma por campo. O `ChainBreak` de lote não carrega a chave (`seq` é `None`), então contar por campo não ajuda a achar o lote, e os testes só afirmam `breaks[0]`. A costura 14 foi emendada.
- **S-15 (`Full` com tenant desconhecido):** na escrita com `Full`, o executor rodava a verificação antes do `decide`. Com isso, um lote malformado para um tenant que não existe dava `TenantNotFound` com `Full` e `InvalidInput` com `Tail`. Decisão: com tenant desconhecido, a verificação não roda, como já acontecia na leitura (costura 16). `Full` passa a só acrescentar recusas `CHAIN_BROKEN`, sem trocar o código de recusa que `Tail` daria. Entra 1 teste em `tenant_isolation`, e o total vai a 240.
- **S-16 (testes da T6):** os testes da T6 foram escritos numa cópia do commit `8f5c88a` e passaram de primeira, como travas. Aceitas três leituras do test-engineer:
  - para ter um `closed` gravado, o teste do invariante grava `open`, revisa para `closed` e então volta para `open`;
  - o primeiro teste retira todas as relations e todos os nós de cada grafo, o que cobre "uma relation e um nó";
  - nenhum dos dois afirma recusa de ciclo, que `relations.rs` já prova.
- **S-17 (revisão da T5c):** veredito PASS nas três regras. Entram só os ajustes baratos que evitam deriva silenciosa: `BODY_VERSION` compartilhada, uma fonte para os textos `node` e `relation`, `into_owned` no lugar de `to_string` em `Cow` e o comentário de `CHECKED_FIELDS`. Ficam como estão:
  - a derivação de colunas em duas pontas (`record_columns` e `verify.rs`): unificar reabriria a T4 e a T5a e trocaria o `strip_prefix` sem alocação por um `format!` por relation; a tabela de mutações já pega a deriva;
  - os mapeadores crus e tipados de `tx.rs`, que existem por decisão (D-19, D-25);
  - o auxiliar `in_read` opcional, com 2 usos, abaixo do limiar.

### D-31: `LineageRef` emprestado nas portas `latest_record` e `revisions`

Pergunta (revisão de simplicidade da T5b): `Lineage::Node(id.clone())` e as três cópias de `Lineage::Relation` em `decide.rs` e `facade.rs` existem só para chamar `latest_record` e `revisions`, que pedem `&Lineage`. A raiz é a assinatura da porta.

Alternativas:

| Opção | Efeito no comportamento final | Veredito |
|---|---|---|
| A. Deixar como está (`&Lineage` nas portas) | nenhum; 5 clones de `SemanticId`/`RelationTypeName` por item mais um `Cow::Owned` por item | descartada: o dono pede o borrow checker ao máximo, e a cópia só existe para satisfazer a assinatura |
| B. Portas recebem `(kind, key)` já calculado (`&str`, `&str`) | o SQL vira o único consumidor; `decide` e a Facade precisam calcular a chave | descartada: espalha `lineage_columns` pelos chamadores e o contrato de `latest_record` deixa de falar em lineage |
| **C. `LineageRef<'a>`: `Copy`, `pub(crate)`, em `engine/model.rs`; as duas portas e `lineage_columns` o recebem por valor** | nenhuma mudança de comportamento: as mesmas consultas, as mesmas chaves, as mesmas recusas | escolhida |

Método: cópia da árvore de hoje (T5b verde, T5c em curso) para `scratchpad/critic/t7/` e a mudança aplicada de verdade: `cargo test -p hexlog --lib` 38 verdes antes e depois; `cargo test --workspace` 208 verdes (inclui `tests/architecture.rs`, o varredor); `cargo clippy -p hexlog --all-targets` sem aviso (`needless_pass_by_value` não acusa um tipo `Copy` de 24 bytes; `large_types_passed_by_value` fica longe do limite); `cargo fmt` sem diferença que sobrasse. Diferença: 124 linhas de `+`/`-` em `src/`, nenhuma em `tests/`. Os sete pontos de chamada que a rodada mencionava são na verdade **seis**, em dois testes de `store/sqlite/tests.rs`: `latest_record` nas linhas 834-836 e `revisions` nas linhas 1224-1226; cada um vira `LineageRef::from(&nome)`.

Achado que muda a conta: `SemanticId` e `RelationTypeName` não derivavam `Hash`. Com `Hash` derivado na macro `string_id!` (`ids.rs:58`), o `seen` do passo 6 passa a ser `HashSet<LineageRef<'_>>` e **o `format!` da chave de relation some do passo 6**. É equivalente à chave de texto: `is_name` aceita só `[a-z0-9-]` e o `.` do `SemanticId` (`ids.rs:13-25`), então `|` nunca aparece num id e `<tipo>|<de>|<para>` é injetiva; igualdade estrutural e igualdade de chave decidem o mesmo. Efeito colateral público: todos os ids ganham `Hash` (só acrescenta; nada quebra).

Sobre "calcular a chave uma vez" (nota menor da T5b): **não vale perseguir**. Medido com `rustc -O` (2 milhões de voltas, duas rodadas): 3 clones de `String` custam 56-66 ns; o `format!` da chave custa 152-166 ns; uma ida ao SQLite custa ordens de grandeza mais (D-5 e D-7). Depois da troca, restam um `format!` em `tx.rs` (a porta) e um em `record_columns` (a coluna `lineage_key` precisa de `String` própria): reaproveitar um no outro exigiria a porta receber a chave, que é a opção B. O ganho da D-31 é de contrato e de propriedade, não de velocidade, e a mensagem do commit não deve dizer o contrário.

Riscos medidos: (1) E0502: não ocorreu; `reads` é `&mut dyn WriteReads` e `LineageRef` empresta de `req`, que `reads` não toca; `LatestRow` devolvida é toda própria. (2) Tempo de vida: `lineage_columns(LineageRef<'a>) -> (&'static str, Cow<'a, str>)` devolve o `Cow` preso ao `'a` da referência, não à cópia local, então `HashSet`/`Cow` não prendem o laço. O `Vec<Cow<'_, Lineage>>` vira `Vec<LineageRef<'_>>` e o `Cow` deixa de ser necessário nesse ponto (o `use` de `Cow` continua pelos outros usos). (3) Varredor: `LineageRef` não contém palavra vetada; `architecture.rs` verde na cópia. (4) `Subject.lineage` e `body_lineage` passam a `LineageRef<'a>` (3 ocorrências de `lineage: &'a Lineage` em `decide.rs`). (5) O `match` sobre `ItemBody::Retirement { lineage: Lineage::... }` em `ends_of_item` continua com o `Lineage` próprio; por isso o `use Lineage` fica em `decide.rs`.

Ordem: a rodada fica **depois da T5c verde**, porque a T5c reescreve `store/sqlite/tests.rs` (as seis chamadas estão lá). Dentro da rodada a ordem é test-engineer primeiro (adapta as seis chamadas; o arquivo deixa de compilar contra a porta antiga, que é o vermelho), executor depois (troca as portas; verde). `revisions` entra na mesma troca: mesma raiz, mesma assinatura. Nenhum teste novo: o que o `seen` faz já tem a recusa `lineage-in-batch` coberta, e os 30 de `sqlite_store` provam as duas portas.

D-31 (correção): "ordens de grandeza mais (D-5 e D-7)" aponta para as decisões erradas (D-5 e D-7 não medem o SQLite). A medição é a da linha 290 deste log: um append com `COMMIT` e `synchronous=FULL` leva 4,9 ms (`rusqlite`) a 6,1 ms (`sqlx`), dominado pelo `fsync`. Os cerca de 0,1 µs por item que a D-31 economiza são ruído contra isso; a razão da D-31 continua sendo propriedade e contrato, como dito acima.
