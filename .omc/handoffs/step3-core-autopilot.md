# Handoff: executar o passo 3 (núcleo `engine` e `store`) com autopilot

Para uma sessão nova, sem o histórico desta conversa. Tudo que ela precisa está neste arquivo e nos três que ele referencia. Data: 2026-11-07. Projeto: `/home/gabriel/personal/oh-my-hexflow` (não é um repositório git).

## Comece por aqui

1. Abra uma sessão nova neste diretório e rode: `/oh-my-claudecode:autopilot` com o texto do bloco abaixo.
2. Antes de qualquer edição, leia na ordem: este arquivo, [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (o plano executável, 1180 linhas), [GLOSSARY.md](../../GLOSSARY.md) (vocabulário obrigatório) e [p0-answers.md](../drafts/p0-answers.md) (as 7 respostas do dono ao portão P0).

```
Execute o plano .omc/plans/omhl-v2-core-step3.md, tarefas T1, T2, T3/T4, T5a, T5b, T5c, T5d e T6, nesta ordem, seguindo o handoff .omc/handoffs/step3-core-autopilot.md (regras, travas e pontos de parada). Subagentes só em sonnet ou haiku, no máximo 16 em paralelo. Código de produto só por executor e test-engineer. Pare e mostre o impasse ao dono quando o handoff mandar.
```

## O que está decidido (não rediscutir)

- O plano passou por 3 rodadas de consenso (Planner, Architect, Critic), por uma rodada de linguagem (3 rodadas) e por uma conferência final de contagens (APPROVE). O dono pediu esta execução com autopilot: isso é a aprovação para executar. Troque o `Status` do plano de `pending approval` para `approved for execution` na primeira edição.
- As decisões 1 a 35 do plano mestre [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md) valem. O glossary prevalece sobre qualquer nome antigo (decisão 35).
- Respostas do dono depois do consenso: `TxRead` virou `StoreReader` (soma de `WriteReads`, `QueryReads` e `VerifyReads`); o módulo `schema` virou `catalog` (arquivos `catalog/{mod,types,validate}.rs`); `work_item` é o tipo base e o PR é um tipo filho; o item 6 (conteúdo só no banco em máquina nova) está fora de escopo; os termos em português do glossary serão revistos depois.
- Respostas do P0: o motor confere só o formato do snapshot (40 ou 64 hex) e o `omhl` lê o git; relation com origem nativa é permitida; `Retirement` entra no passo 3; tabela de regras de `snapshot` da seção 4.5 confirmada; `alias` e `batch.aliases` **cortados por completo**; vocabulário estrutural aceito como estrutura do motor; snapshot `{kind, id}` com `kind` enum fechado.

## Regras do dono para esta execução

- **Subagentes:** só `sonnet` ou `haiku`; no máximo 16 em paralelo (T3 e T4 são as únicas tarefas paralelas). Preferir agente OMC nomeado: `oh-my-claudecode:executor`, `oh-my-claudecode:test-engineer`, `oh-my-claudecode:verifier`, `oh-my-claudecode:code-simplifier`, `oh-my-claudecode:debugger`. Um hook gera o nome do subagente: a primeira chamada falha com o nome sugerido; repita a mesma chamada com esse nome. Todo prompt de subagente pede mensagem de conclusão com o resumo. Rode `TaskStop` em cada subagente em background depois de usar o resumo, e nunca antes da notificação.
- **Código de produto só por `executor` (e `test-engineer` para testes).** A sessão principal edita só markdown, config e arquivos de harness. Os hooks `subagent-standards` injetam `code-standards` e `test-standards` sozinhos.
- **Idioma:** código, identificadores, mensagens de erro e nomes de teste em inglês; só comentários podem ser em português. Nomes de teste no formato `it_should_...`.
- **Verificação:** depois de cada trecho de código de produto, rode um `code-simplifier` instruído a invocar `verifier-standards` com escopo `entrega` ou `trecho-entrega` e a só reportar. Um FAIL volta ao `executor` pela sessão principal, no máximo 2 vezes; depois disso, pare e mostre o impasse ao dono.
- **Sem fingir conclusão:** nenhum `test.skip`, `#[ignore]`, teste-esqueleto nem ramo `todo!()`/`unimplemented!()`. O único `allow(dead_code)` permitido é o temporário do plano (em `engine/mod.rs` na T2 e em `store/mod.rs` na T4), e a T6 o remove.
- **Git:** o diretório não é um repositório. Não rode `git init`, commit nem push sem o dono pedir. Se precisar de worktree, use o Worktrunk (`wt`), nunca `git worktree`.
- **Dúvida que é do dono:** uma pergunta por vez, com `AskUserQuestion`. Não assuma.

## Regras de ouro do dono (acrescentadas durante a execução; prevalecem sobre o plano)

1. **TDD com agentes isolados.** Em cada tarefa, o `test-engineer` escreve os testes primeiro, a partir das tabelas da seção 5 do plano e do contrato de costuras da tarefa, sem ler a implementação, e relata a evidência de vermelho. Depois o `executor` implementa até o verde. O `executor` nunca escreve nem edita teste; o `test-engineer` nunca edita código de produto; teste que pareça errado volta pela sessão principal ao `test-engineer`. Valor esperado vem do plano ou de fonte independente, nunca da implementação.
2. **Sem exceção na memória do Rust.** Sem `unsafe` (o compilador recusa), `Box::leak`, `mem::forget`, `ManuallyDrop`, `MaybeUninit`, `transmute`, `static mut`, `Rc`, `Arc`, `Weak`, `RefCell`, `Cell`, `Mutex`, `RwLock`, `OnceLock`, `LazyLock`. Uma exceção só entra se nenhuma alternativa plausível existir, com aprovação do dono, comentário `memory-exception: <motivo>; alternativas descartadas: <quais>` no local e registro na seção 16 do plano. O subagente que achar que precisa de uma para e relata; não a abre por conta própria.
3. **Borrow checker ao máximo.** Empreste em vez de copiar: `&T`, `&mut T`, `&str`, `&[T]`, parâmetros de lifetime, structs de serialização que emprestam, iteradores em vez de `collect` intermediário. Valor consumido entra por move. `clone()`, `to_owned()` e `to_string()` só onde dois donos são de fato necessários ou onde o dado nasce ali, nunca para calar o compilador. Assinatura interna de porta pode trocar retorno ou parâmetro dono por empréstimo; assinatura pública muda só com registro na seção 16 do plano.
4. **Pesquisa antes de caixa cinza (conceito de Matt Pocock).** Antes de executar uma caixa cinza (`catalog`, `store/sqlite`, `engine/decide`, `engine/rules`, `engine/verify`), subagentes pesquisam se a lógica já existe em biblioteca ou ferramenta externa. O que não for viável fica documentado como gap interno em [gray-box-reuse.md](../research/gray-box-reuse.md). Uma biblioteca só é apta com no mínimo 1000 estrelas no GitHub, atualizações relevantes e bom status de saúde (documentação, testes e badges do GitHub). Para cada gap sem biblioteca apta, um subagente pesquisa papers, RFCs e documentação técnica similar, e a implementação se baseia nisso; a referência fica registrada no mesmo arquivo. Toda frente de pesquisa termina em relatório próprio em `.omc/research/gray-box/`, com as referências navegáveis, para outras sessões não refazerem a descoberta; o arquivo acima é o índice.
5. **Cadeira do crítico.** Perguntas puramente técnicas vão ao `critic` (sonnet), não ao dono. O crítico registra cada decisão em [omhl-v2-core-step3-critic-log.md](../plans/omhl-v2-core-step3-critic-log.md): alternativas, como cada uma interfere no comportamento final, a escolhida e o porquê, e o método usado para concluir que era a melhor. Subagente com dúvida técnica fora do plano para e relata à sessão principal, que a leva ao crítico. Ao dono vai só o que não é técnico, além dos pontos de parada abaixo.

## Ordem e portões

Grafo: T1; T2 (depende de T1); T3 e T4 (paralelas, ambas dependem de T2); T5a (T3, T4); T5b; T5c; T5d; T6. Cada tarefa fecha com os três comandos limpos: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, mais o "Pronto" da própria tarefa na seção 4.6 do plano. Os comandos por critério estão em "Comandos de verificação" da seção 5. Toolchain da máquina: cargo e rustc 1.97.0 (o plano usa upcast de trait, estável desde o Rust 1.86).

Pontos de parada obrigatórios (pare e mostre ao dono; não decida sozinho):

1. **Spike (a) da T1 reprova** (`serde_json_canonicalizer` contra os vetores do RFC 8785): aplicar E2-C do plano e avisar o dono.
2. **Spike (c) da T1:** `cargo tree -p hexlog -e normal` lista crate da lista proibida (`git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std`): trocar `jsonschema` por `boon` ou outro validador, como o plano manda, e registrar a troca.
3. **Fim da T2:** o hash dourado e a fixture do record gravam o formato. Mostre ao dono o resumo da T2 antes de seguir para T3 e T4. Depois disso, mudar o corpo do record é mudar o formato (tenant novo, decisão 12).
4. **Impasse após 2 devoluções ao `executor`** em qualquer tarefa.
5. **Qualquer pergunta que o plano marque como `[DONO]`** e que a execução não consiga contornar.

**Delegação do dono (2026-11-07, depois do fim da T2):** "Vai fechando as lacunas com o critic ou outras estratégias, você está por conta", e em seguida "Até a entrega da v1". Os pontos 4 e 5 e as lacunas da execução passam a ser resolvidos pelo crítico (ou por medição, protótipo ou pesquisa), sem parar para perguntar. Cada decisão fica no log do crítico e entra no relatório final. A pergunta sobre `format` (D-7) foi a primeira devolvida ao crítico.

## Armadilhas que a execução precisa conhecer

- **O varredor de vocabulário veta, em identificadores e literais de `crates/hexlog/src`, as palavras `unit`, `directive`, `diretriz` e `work`.** Por isso `work_item` **não pode aparecer no motor** (`engine`, `store`, `catalog`); o motor é agnóstico e o tipo `work_item` vive só como dado do catalog e no `omhl`. Nomes de teste e fixtures de `tests/` ficam fora do varredor, mas o texto de `src/` não.
- **`pub use` das quatro portas públicas** (`Clock`, `IdGenerator`, `NativeNodes`, `NativeNode`) entra já no `lib.rs` da T2, e não só na T5a: o `tests/support/mod.rs` nasce na T2 com fakes que as implementam, e o `tests/catalog.rs` da T3 provavelmente o usa. Sem isso o teste da T3 não compila.
- **`src/lib.rs` tem dono por tarefa:** a T2 declara `mod engine; mod catalog; mod store;` com `catalog/mod.rs` e `store/mod.rs` vazios (o módulo é `catalog`; trechos antigos do plano, nas seções 8 a 13, ainda dizem `schema`); a T3 acrescenta só o `pub use` de `Catalog`, `TypeDef` e `RelationTypeDef`; a T4 não edita o `lib.rs`; T5a e T5c acrescentam os seus.
- **`pub(crate) use decide::decide`** exige `pub fn decide`; qualquer visibilidade menor dá erro E0364. `CHECKED_FIELDS` sai por `pub use` simples. Os `pub(crate) use` do `engine/mod.rs` entram só com o consumidor (T5a e T5c), para não gerar `unused_imports`.
- **Teste de imports do `engine`** depende de reconhecer `#[cfg(test)]` por tokenização. Se o varredor ficar frágil, a saída é limitar o glob a `crate`, `hexlog` e `super` nos arquivos que não são de teste.
- **T6, remoção do `allow(dead_code)`:** antes de apagar um item "sem uso", confira se ele é público por `pub use` (por exemplo itens de `Limits`). Não reintroduza o `allow`.
- **T5d, 20 repetições:** em CI lento pode falhar; aumente o `busy_timeout` só nos testes, nunca o padrão.
- **`FORK_REJECTED` no mesmo lote:** sem `alias`, dois itens da mesma lineage no mesmo lote dão `FORK_REJECTED`. O plano não diz o que vai em `details[0].current` nesse caso (o primeiro item ainda não tem record gravado) e não tem teste para `Retirement` ou relation nessa situação. Decida na T5a e registre; é menor.
- **Teste acrescentado pelo Planner** `it_should_reject_a_second_item_for_the_same_lineage_in_the_same_batch` fica, salvo veto do dono (sem ele o total cai em 1 e `batch` de 3 para 2).
- **`tenant` está em `WriteReads` e em `VerifyReads`:** uma chamada direta a `&dyn StoreReader` é ambígua; chame por `&dyn WriteReads` ou `&dyn VerifyReads`.
- **Estado velho do OMC:** há 8 arquivos de estado de autopilot parados em `~/.omc/state/sessions/` (sessões `edge-sess-*` e `sess-1` a `sess-6`, ativos há cerca de 900 h). São lixo de uma execução morta. Não os retome; deixe a limpeza (`/oh-my-claudecode:cancel`) para o dono decidir.

## Fora do passo 3 (não fazer; só não bloquear)

Carregador de extensions (arquivo para banco) e projeções; gate nativo de PR e as perguntas `occurred`/`no_open_contradiction` (passo 4); o mecanismo pelo qual um tipo filho declara que especializa o `work_item` (passo 4); a tensão da attestation (uma revision para um snapshot novo apaga o desfecho anterior da vista corrente; passo 4); a idempotência do carregamento das extensions (antes do passo 5); a forma final de `Store::read` (soma `StoreReader` ou dois métodos); a revisão dos termos em português do glossary.

## Pronto quando

1. T1 a T6 cumprem o "Pronto" da seção 4.6, com `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` limpos e os comandos por critério da seção 5 verdes.
2. Contagens finais batem com a seção 16 do plano (que substitui os 117 nomes e os 8 de `architecture` originais), tabela de mutação com 34 linhas e 33 `CHECKED_FIELDS`.
3. Um `verifier` independente (sonnet) confere a evidência por script e não por relato; um `code-simplifier` com escopo `entrega` passa sem FAIL pendente.
4. Nenhum `allow(dead_code)` temporário em `crates/hexlog/src` (confira por `grep`; o de `tests/support/mod.rs` fica).
5. Relatório final ao dono: o que foi construído, contagens, o que ficou fora do passo 3 e qualquer decisão tomada na execução (as menores acima), em pt-BR, com o próximo passo.
