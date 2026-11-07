# Consenso de linguagem, rodada 2: revisões independentes

Instantâneo revisado: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (1094 linhas). Versão anterior: [omhl-v2-core-step3.r5.md](../plans/omhl-v2-core-step3.r5.md). Glossary: [GLOSSARY.md](../../GLOSSARY.md). Rodada anterior: [round1-language-reviews.md](round1-language-reviews.md). Os revisores não viram um ao outro; condensado sem mudar o conteúdo. Esta revisão alimenta a **rodada 3, a última**.

## Resultado

- Architect: os 6 achados da rodada 1 estão resolvidos; 2 IMPORTANTE e 3 MENOR novos; nenhuma decisão do dono violada.
- Critic: ITERATE leve; os 10 achados da rodada 1 estão resolvidos (o 6 e o 8 pela metade); 2 IMPORTANTE e 3 MENOR novos. Com A e B abaixo corrigidos, passa a APPROVE.
- O veto a `work` no varredor foi aprovado pelos dois. `lineage_records` e o item 23 ([DONO], módulo `schema`) também.

## IMPORTANTE (os dois revisores apontaram, de forma independente)

**A. `src/lib.rs` não está em nenhuma tarefa** (linhas 604 e 609-610). T3 e T4 são paralelas e ambas precisam de `mod schema;`, `mod store;` e do `pub use` de `Catalog`, `TypeDef` e `RelationTypeDef` (o `tests/catalog.rs` só usa a API pública). Se as duas editarem o `lib.rs`, há o mesmo conflito que o plano evitou no `tests/support/mod.rs`.
Correção: a T2 declara `mod engine; mod schema; mod store;` com `schema/mod.rs` e `store/mod.rs` vazios; a T3 é dona do `lib.rs` para o `pub use` do schema; o `pub use` do que for público do store entra na T5a.

**B. O `#![allow(dead_code)]` temporário cobre só `engine/mod.rs`** (linha 598). Na T4, `SqliteStore` e o `ddl` ficam sem consumidor até a T5a (`Hexlog::open`); o `store` não é exportado, então o `cargo clippy --all-targets -D warnings` do "Pronto" da T4 falha ("never constructed").
Correção: o mesmo `allow` temporário em `store/mod.rs` (a T4 o adiciona); a conferência por grep da T6 cobre todo `crates/hexlog/src`. Também: o `pub(crate) use` do `engine/mod.rs` só entra junto com o consumidor, na T5a, para não gerar `unused_imports`, que o `allow(dead_code)` não cobre.

## MENOR

**C. Trava de imports, glob com `super`** (Architect C). `use super::*` em `engine/mod.rs` e `use super::super::*` importam a raiz do crate sem nomear `store` nem `schema`; hoje passam. Correção: acusar glob cujo prefixo seja cadeia de `super` que saia de `engine`, ou todo glob fora de `#[cfg(test)]`.

**D. `CHECKED_FIELDS` e `chain` são públicos** (Architect D). Um `pub(crate) use` não pode ser reexportado por `pub use` (erro E0364). Correção: `engine/mod.rs` usa `pub use verify::CHECKED_FIELDS;` simples, e o `lib.rs` reexporta daí; o filho `pub(in crate::engine)` não impede o item `pub` ser alcançado.

**E. Ambiguidade em "segmento de caminho"** (Critic E, linha 843). Uma variável local ou parâmetro chamado `store` ou `schema` em `engine/` poderia ser lido como falso positivo. Correção: dizer "só ao lado de `::` ou dentro de `use`" e acrescentar o caso aceito `let store = 1;` aos trechos embutidos.

**F. Teste de unidade da T3 referencia outro teste** (os dois; linha 679). "`def_hash` igual ao do teste `it_should_compute_a_stable_def_hash`": um teste não lê o valor de outro. Correção: comparar com `sha256hex(JCS(definição))` calculado no próprio teste. Além disso, a linha 861 diz que `--lib` roda só `sqlite_store`, mas agora roda também esse teste de unidade, e o comando de C1 (`--test catalog ...`) não o executa: ajuste o texto e o comando.

**G. "Arquivos" da T6 sem `src/engine/mod.rs`** (Critic C). O "Pronto" da T6 exige remover o `allow(dead_code)` (linha 642), mas o arquivo não está listado. A remoção pode expor código morto real. Correção: listá-lo e dizer que o executor remove itens sem uso e não reintroduz o `allow`.

**H. Justificativa da recusa da fusão de structs** (Architect (2), linha 189). A razão escrita não procede: fundir `NodeTypeInfo`/`RelationTypeInfo` com `TypeDef`/`RelationTypeDef` não exigiria o `engine` importar `schema`; bastaria mover `TypeDef` e `RelationTypeDef` para o `engine`. Mantenha a recusa, mas reescreva o motivo: as vistas carregam `def_hash`, que é derivado, e `TypeDef` carrega `schema` e `equivalent`, que o `engine` não deve conhecer.

**I. Tensão da attestation como node** (Architect E, Critic pergunta aberta). `list` e a pergunta `occurred` olham a current revision; uma revision para um snapshot novo apaga da vista corrente o desfecho do snapshot anterior, que só aparece por `lineage_records`. Registre no item 13 como tensão e como pergunta aberta [DONO]/passo 4 (cada snapshot tem o seu próprio semantic id, ou a attestation é uma revision por snapshot?). Não decida.

## Conferido sem problema

Contagens por script: `catalog` 10, `sqlite_store` 9, `concurrency` 9, `architecture` 8, `third_graph` 2, `semantic_id` 2, `snapshot` 2, `jcs_vectors` 3; 118 nomes de teste na tabela de §5, nenhum duplicado; tabela de mutações com 34 linhas e 33 `CHECKED_FIELDS`. "base nativa" zerado nas seções 1 a 7; carregador de extensions listado em "Não entra"; itens 20 (D30 e D31) e T5d corrigidos; `pub(crate) use` de item `pub` em módulo `pub(in crate::engine)` compila.
