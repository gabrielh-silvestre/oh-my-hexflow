# Pesquisa de reuso antes de cada caixa cinza

Índice das pesquisas de 2026-11-07. Regras do dono em [step3-core-autopilot.md](../handoffs/step3-core-autopilot.md); plano em [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md); árvore de caixas em [boxes-verification.md](boxes-verification.md), seção 1. Consulte estes relatórios antes de pesquisar de novo: cada um traz candidatos, critérios, veredito, gaps e referências navegáveis.

Critério de biblioteca apta: no mínimo 1000 estrelas no GitHub, atualizações relevantes e boa saúde (documentação, testes e badges).

## Decisões do dono sobre as dependências

| Crate | Estrelas | Decisão |
|---|---|---|
| `serde_json_canonicalizer` | 23 | sai; JCS próprio pelo RFC 8785 |
| `jsonschema` | 833 | sai; validador próprio de um subconjunto de JSON Schema 2020-12, com lista branca de palavras-chave |
| `proc-macro2` | 946 | exceção documentada: só em teste, e já vem por `serde_derive` e `thiserror` |
| `rusqlite` | 4467 | apta, mas o dono escolheu `sqlx` (17632 estrelas) |

## Relatórios

| Relatório | Caixa | Resultado |
|---|---|---|
| [dependency-audit.md](gray-box/dependency-audit.md) | dependências do plano | 10 aptas, 3 reprovadas (acima) |
| [chain-verify.md](gray-box/chain-verify.md) | `hexlog`, `engine`, `engine/verify` | nenhum candidato apto; gap interno, com fundamentos (Haber e Stornetta 1991, Crosby e Wallach 2009, RFC 6962, RFC 9162, RFC 8785) |
| [versioned-graph.md](gray-box/versioned-graph.md) | `hexlog` e `engine` como um todo | nenhum substitui o miolo; `redb` fica como plano B de armazenamento |
| [decide-rules.md](gray-box/decide-rules.md) | `engine/decide`, `engine/rules` | sem biblioteca apta para o pipeline; ciclo por `petgraph` (apta), escolhida na D-8 do [log do crítico](../plans/omhl-v2-core-step3-critic-log.md) |
| [catalog-schema.md](gray-box/catalog-schema.md) | `catalog` | `url` apta; validador, recusa de palavras-chave e guarda de inteiro seguro são gaps internos |
| [store-sqlite.md](gray-box/store-sqlite.md) | `store`, `store/sqlite` | avaliação de candidatos e medições das alternativas ao `Mutex<Connection>`, feitas sobre `rusqlite` |
| [store-sqlx.md](gray-box/store-sqlx.md) | `store/sqlite` com `sqlx` | escolha do dono: como usar sem runtime, API, erros, armadilhas e medições |
| [jcs-rfc8785.md](gray-box/jcs-rfc8785.md) | `engine/chain` | fundamento do JCS próprio: RFC 8785, ECMA-262, vetores e armadilhas |
| [json-schema-subset.md](gray-box/json-schema-subset.md) | `catalog/validate` | fundamento do validador próprio: especificação 2020-12, subconjunto, suíte oficial e armadilhas |
