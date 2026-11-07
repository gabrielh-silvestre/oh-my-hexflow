# Consenso de linguagem, rodada 1: revisões independentes

Instantâneo revisado: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (1054 linhas). Versão anterior: [omhl-v2-core-step3.r4.md](../plans/omhl-v2-core-step3.r4.md). Glossary: [GLOSSARY.md](../../GLOSSARY.md). Briefing: [ralplan-brief-language.md](ralplan-brief-language.md). Os revisores não viram um ao outro; condensado sem mudar o conteúdo.

## Decisão do dono tomada depois das revisões

**Attestation é um tipo de node** (do graph de checks); cada revision dele carrega o desfecho de um check em um snapshot. Não é um quarto tipo de record. O GLOSSARY.md já foi atualizado. Isso fecha o achado 6 do Architect e o achado 4 do Critic: reescreva os itens 12, 13 e 20 do plano e tudo que diz "record attestation" para "node attestation (node type do graph de checks)", com `requires_snapshot` como propriedade desse node type.

## Architect: sem BLOQUEANTE; 3 IMPORTANTE e 3 MENOR; nenhuma decisão do dono violada

Coerente com o glossary e com D34 e D35. Contagens e ordem T1 a T6 fecham. A porta `Definitions` (item 22) é necessária e bem posicionada; o grafo de dependências fica acíclico; manter. Antítese (não bloqueante): pares quase idênticos de structs (`NodeTypeInfo`/`TypeDef`, `RelationTypeInfo`/`RelationTypeDef`) e o corte de 50 violações feito duas vezes.

1. IMPORTANTE (linha 840): a trava de imports exige `store` ou `schema` logo após `crate`, `hexlog` ou `super`, e deixa passar `use crate::{store::X}`, `use crate::{engine::a, schema}` e `use crate::*`. Corrigir: acusar `store` ou `schema` (minúsculo) como segmento de caminho em qualquer posição, inclusive dentro de grupo `{}`, e provar um caso com chaves no trecho embutido.
2. IMPORTANTE (linhas 151 e 841): visibilidade dos filhos de `engine` não especificada. `ports`, `model`, `snapshot`, `limits` e `chain` precisam ser `pub(crate)` (porque `schema/catalog.rs` e `store/sqlite` os importam); `decide`, `rules` e `verify` podem ser `pub(in crate::engine)`, alcançados pela Facade por `pub(crate) use` em `engine/mod.rs`.
3. IMPORTANTE (linhas 299 a 304, 842): dizer quais portas são API pública. `Clock`, `IdGenerator`, `NativeNodes` e `NativeNode` aparecem em `Deps` e o `omhl` as implementa, então saem por `pub use`. `Store`, `TxRead`, `Definitions`, as linhas e `AppendPlan` não saem. Uma frase no 4.2 com as duas listas.
4. MENOR: `Definitions` só tem consumidor na T5a. Na T3, um teste de unidade `#[cfg(test)]` em `schema/catalog.rs` conferindo o que `Definitions` devolve (`requires_snapshot`, `def_hash`, `acyclic`); não muda a contagem de 10.
5. MENOR (linha 155): o layout de `ports.rs` não lista `NodeTypeInfo`, `RelationTypeInfo`, `DataViolation` nem `NativeNode`.
6. MENOR: attestation (resolvido pela decisão do dono acima).

## Critic: ITERATE (ajuste mecânico, sem mudança de arquitetura)

Conferido sem problema: contagens dos "Pronto" contra §5 (T2: 2/2/3; T3: 10; T4: 9; T5d: 9; T6: third_graph 2 e architecture 8); 8 testes de `architecture`; tabela de mutações (34 linhas, ids únicos, 33 CHECKED_FIELDS, 7 triggers); renomeios nas §1 a §7; citações H:n amostradas; itens [DONO] e P0 pendentes.

1. IMPORTANTE: "base nativa" aparece 17 vezes (linhas 15, 33, 68, 87, 101, 106, 120, 130, 198, 591, 675, 840 a 842, 880, 899, 905). O glossary manda evitar `base` para Native, e `base` já é o campo de revision. A §11 afirma que nenhum nome antigo sobrevive, e isso está incorreto. Trocar por "conteúdo native" ou "o native"; manter só o nome do arquivo `native-base-candidates.md`.
2. IMPORTANTE: a §4.1 "Não entra" não lista o carregador de extensions; a mitigação de R7 compara com essa lista. Acrescentar "carregador de extensions (arquivo para banco, D34)".
3. MENOR: `history()` (linhas 311, 616, 683): o glossary lista "history" em `_Avoid_` de Lineage. Renomear (por exemplo `lineage_records`) ou registrar a exceção na §11.
4. MENOR: attestation com dois sentidos (resolvido pela decisão do dono).
5. MENOR: "pub(crate) ou pub(in crate::engine)" sem dizer qual vale para cada filho (igual ao Architect 2).
6. MENOR: o teste de imports do engine não cobre import agrupado nem glob (igual ao Architect 1).
7. MENOR: o varredor não consegue vetar `work_item` porque "item" é legítimo. Sugestão: acrescentar "work" à lista (avalie o risco de falso positivo: "work" é palavra comum).
8. MENOR: na T2, `ports.rs` só tem tipos e traits sem uso; `clippy -D warnings` pode acusar `dead_code` até a T5a (confiança média; já existia na r4).
9. Faltando: o item 20 cita D28 para "uma attestation exige snapshot", mas a regra vem de D30 e D31. O comentário da T5d diz que todos os testes de `concurrency` dependem de `verify_chain` e `tamper`, o que é falso para `it_should_be_send_and_sync` (já existia na r4).
10. Pergunta aberta: o módulo `schema` guarda o Catalog, e o glossary lista "schema" em `_Avoid_` de Catalog; o nome vem de D11 e do mestre, então é decisão do dono (marque [DONO] no plano, sem renomear).
