# Rodada 2: revisões independentes do plano do passo 3

Instantâneo revisado: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (835 linhas). Histórico: [omhl-v2-core-step3.r1.md](../plans/omhl-v2-core-step3.r1.md). Rodada anterior: [round1-reviews.md](round1-reviews.md). Os revisores não viram um ao outro; condensado sem mudar o conteúdo.

## Architect: sem BLOQUEANTE; os 8 achados da rodada 1 resolvidos; 2 IMPORTANTE e 3 MENOR novos

- **N1, IMPORTANTE, §5 C7 contra §4.3:** o varredor inclui literais de string e proíbe a palavra inteira `check`, mas o DDL (linhas 194, 202, 204, 224) é literal em `src/store/sqlite/ddl.rs` e tem `CHECK (...)` quatro vezes; o teste reprova na T4. Preferência: tirar `check` da lista. Alternativas: isentar `ddl.rs` no teste, ou escrever `CONSTRAINT` sem a palavra.
- **N2, IMPORTANTE, §4.5 `where_eq` (linha 472):** o caminho `$."<nome>"` sobre o corpo inteiro lê campos do envelope (`type`, `state`, `tenant`, `id`), não de `data`. Correção: `$.data."<nome>"` e um teste de `where_eq` com nome igual a um campo do envelope, afirmando que casa só com `data`.
- **N3, MENOR:** o `Retire` de aresta não diz se copia `from_origin`, `to_origin` e `native_catalog` da ponta; `EdgeView` declara `Origin` não opcional e `LinkColumns` os tem como `Option`. O `Retire` copia origens e `native_catalog`.
- **N4, MENOR, §4.4 linha 383:** `Store::read<R>` é genérico, então o trait não é objeto de trait; a Facade não é genérica. Dizer que `Hexlog` guarda o `SqliteStore` concreto ou trocar `read` por método não genérico.
- **N5, MENOR:** o teste do invariante no adaptador é função local ao teste e só documenta; dizer isso na observação da linha 741.
- **Antítese e tensão:** pouco mudaram. O P0 antes da T2 trava a execução em 5 ou mais respostas do dono; a T1 pode rodar antes (linha 499).

## Critic: ITERATE; achados da rodada 1 resolvidos de fato; 3 maiores novos

- **A, MAIOR, ordem das tarefas quebrada (§4.6):** `verify.rs` e `verify_chain` só nascem na T6, que depende de T5b e T5c; mas a T5c exige `verify_chain` ok em `it_should_keep_seq_contiguous_when_many_threads_append`, a política `Full` em `it_should_fail_with_chain_broken_on_write_for_a_tampered_middle_link_under_the_full_policy` e "verde em 20 repetições"; a T5a entrega `Tail` na escrita, que também precisa recalcular hash. O "Pronto" da T5c é inalcançável. Mover `verify.rs`, `CHECKED_FIELDS` e `verify_chain` para uma tarefa antes da T5c (por exemplo T4b, depois de T2 e T4); a T6 fica só com os testes de C3 e C7.
- **B, MAIOR, a trava de vocabulário reprova o próprio DDL (§4.2, §4.3, §5 C7):** mesmo problema do N1 do Architect (`CHECK (seq >= 1)`, `CHECK (subject_kind IN ...)`, `CHECK (retired IN ...)`, `CHECK (item_count BETWEEN ...)`; T1 e T4 falhariam). Correção sugerida: guardar o DDL em `src/store/sqlite/schema.sql` carregado com `include_str!` (o literal vira só o caminho); alternativa: isentar explicitamente literais SQL de `ddl.rs` no varredor, com caso em `scanner_cases.txt`.
- **C, MAIOR (média), sem tabela de mutação e motivo para C3:** `it_should_detect_a_change_in_each_checked_field` e o proptest dizem "motivo esperado", mas §4.5 não mapeia campo → `BreakReason`; `prev_hash` está como "DivergingSeq/HashMismatch" e o enum não tem motivo de `prev_hash`. O helper `tamper` só remove triggers; alterar `id`, `revises`, `tenant`, `retired` ou `seq` esbarra em UNIQUE, FK, CHECK e PK, então o proptest "sempre" pode não conseguir gravar a mutação. Tabela única em `engine/verify.rs`: campo → valor adulterado válido para as constraints → motivo; dizer se `tamper` desliga FK e CHECK (`PRAGMA ignore_check_constraints`, `foreign_keys=OFF`); unificar `prev_hash` em um motivo.
- **3, MENOR:** `it_should_reject_an_edge_to_a_missing_node` espera `RELATION_NOT_FOUND` (código de relação ausente do catálogo); usar `INVALID_RECORD` com `endpoint-not-found`.
- **4, MENOR:** `AppendPlan` (em `store/mod.rs`) contém `AppendOutcome` de `facade.rs`: dependência circular entre módulos; mover `AppendOutcome` para `engine/model.rs`. `TipRow` e `LinkColumns` usam `String` onde deveria ser `TypeName`.
- **5, MENOR:** falta a linha X4 da tabela de `state` (revisão herda ou reenvia `external_id`); `NodeView.external_id` lê da ponta, então omitir na revisão o apagaria.
- **Conferido:** commit 5d1e22b existe em `/home/gabriel/personal/hexlog` e `hashLink` em `chain.ts:64-67` é `sha256hex(prevHash + jcs(sem prevHash))`.
- **Para APPROVE:** corrigir A, B e C; 3 a 5 são polimento.
- **Observação:** o P0 bloqueia a T2 até o dono responder 7 itens, sem padrão assumido; escolha consciente do plano, a cargo do dono.
