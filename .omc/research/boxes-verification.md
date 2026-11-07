# Caixas pretas, caixas cinzas e design patterns no plano do passo 3

Plano verificado: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (986 linhas, `pending approval`, consenso na rodada 3). Plano mestre: [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md). Briefing dos verificadores: [boxes-brief.md](../drafts/boxes-brief.md). Quatro verificadores (sonnet, lentes independentes): caixas pretas, caixas cinzas, aninhamento e design patterns. Nenhuma classificação contradiz as 33 decisões do dono. Nada foi aplicado ao plano.

## Índice

1. [Árvore de caixas](#1-árvore-de-caixas)
2. [Onde os verificadores discordam](#2-onde-os-verificadores-discordam)
3. [Achados convergentes](#3-achados-convergentes)
4. [Achados de um verificador só](#4-achados-de-um-verificador-só)
5. [Limiar para dividir uma caixa](#5-limiar-para-dividir-uma-caixa)
6. [Limites desta verificação](#6-limites-desta-verificação)

## 1. Árvore de caixas

Preta: interface estável, implementação trocável, o dependente nunca olha dentro. Cinza: interior delegável e verificado só pela interface. Aberta: ainda sem fronteira clara.

| Nível | Caixa | Classe | Interface mínima | Padrão (alternativa recusada) | Mudança proposta |
|---|---|---|---|---|---|
| 0 | `hexlog` (crate) | cinza | `Hexlog` (10 métodos), `Catalog`, 14 códigos de erro, `Clock`, `IdGenerator`, `ForeignNodes` | Facade (expor `Store` e `decide`) | reduzir o que `lib.rs` reexporta |
| 1 | `engine` | cinza | `decide(&dyn TxRead, req) -> AppendPlan`, `verify_chain` | Ports and Adapters, núcleo funcional (`decide` gravando direto) | teste de que não importa `store` nem `schema` |
| 1 | `schema` | cinza | `Catalog::new`, `type_def`, `relation_def`, `validate_data` | Facade, smart constructor | filhos `catalog` e `validate` |
| 1 | `store` | cinza | `Store` (3 métodos) | Repository, Unit of Work por closure (`begin`/`commit` expostos) | dividir `TxRead` |
| 2 | `engine/chain` | preta | `canonical`, `sha256hex`, `hash_link`, `anchor`, `fingerprint` | Adapter sobre a crate de JCS; SHA-256 como função livre (trait de hash seria prematuro) | prova de troca já existe: vetores do RFC e 3 hashes do TS |
| 2 | `engine/link` (nova) | preta | `LinkBody::build/parse`, `columns_of(body)` | Newtype, Builder | função única de derivação de colunas, usada por `decide` e `verify` |
| 2 | `engine/decide` | cinza | `decide(...)` | Pipeline (forma, replay, estático, dependente de estado) | nenhuma |
| 2 | `engine/rules` | cinza | `check(item, ctx)` por regra | Strategy | dividir em `revision`, `state`, `edge`, `cycle`, `reserved` |
| 2 | `engine/verify` | cinza | `verify_chain(...) -> ChainReport` | função pura dirigida por tabela (`CHECKED_FIELDS`) | nenhuma |
| 2 | `engine/ports` | aberta | traits e linhas | Port | separar `ports/host` (`Clock`, `IdGenerator`, `ForeignNodes`) de `ports/storage` (`Store`, `TxRead`) |
| 2 | `engine/state`, `ids` | preta | newtypes `Name`, `SemanticId`, `Hash`, `State{kind,id}` | Newtype; enum fechado `#[non_exhaustive]` (Strategy por tipo de estado) | nenhuma |
| 2 | `store/sqlite` | cinza (ver §2) | `SqliteStore: Store` | Adapter sobre `rusqlite` (ORM) | suíte de contrato do `Store` |
| 2 | validação de JSON Schema | preta, sem contrato | `validate_data -> Vec<Violation>` | Adapter | corpus comum rodado contra duas crates |
| 2 | `Clock`, `IdGenerator` | preta, com defeito | `now()`, `next()` | Strategy injetada em `Deps` | tipar o formato; tirar `uuid::Uuid` da trait |
| 2 | `ForeignNodes` | cinza (ver §2) | `lookup`, `native_edges`, `catalog_version` | Port and Adapter | teste de contrato exportado |
| 1 (`omhl`, fora do passo 3) | `graphs/*`, `render`, `native` | cinza | uma interface por caixa | Strategy por grafo e por host; Adapter | nenhuma agora |
| 1 (`omhl`, ausente) | `app` | proposta | uma função por caso de uso | Application Service | reservar o nome; decidir no passo 4 |

## 2. Onde os verificadores discordam

- **`store/sqlite`:** as lentes de aninhamento e de patterns leem como preta (interface `Store` pequena); as de caixas pretas e cinzas leem como cinza. Argumentos pela cinza: `Mutex<Connection>` concreto na Facade, triggers que guardam invariantes, `json_extract` no `where_eq`, `LinkColumns` com 19 campos que espelha o DDL, `tamper` preso aos nomes dos triggers. **Classificação adotada: cinza** até existir a suíte de contrato; vira preta quando um segundo `Store` (nem que seja em memória) passar nela.
- **`ForeignNodes`:** preta por desenho (trait de 3 métodos) contra cinza porque o contrato (id reservado = `lookup` devolve algo; `lookup` estável durante a transação) só existe em prosa. **Adotada: cinza** até o teste de contrato exportado.
- **`schema` e `verify`:** pequenos e estáveis (pretas) contra regras internas que mudam (lista de recusas, ordem de conferência). **Adotada: cinzas.**
- **`engine/link`:** pode morar dentro de `chain`; um verificador a separa porque o motivo de mudança é o formato do corpo, congelado no P0.
- **`trait Store`:** abstração prematura (uma implementação, a Facade guarda o `SqliteStore` concreto) contra costura barata para teste. O que paga de fato é o `TxRead` com fake.
- **`app` no `omhl`:** pode ser escopo extra; o critério 6 do plano mestre (CLI e MCP devolvem o mesmo resultado) pede uma camada compartilhada.

## 3. Achados convergentes

Mais de um verificador apontou. Ordenados pelo número de lentes.

1. **Fronteira interna sem trava (4 lentes).** O plano diz que `engine` não importa `store` (§4.4; E3-A), mas os 5 testes de `architecture.rs` cobrem só vocabulário, `git`, crates proibidas e `omhl`. Proposta: um teste de imports entre `engine`, `schema` e `store`, mais visibilidade `pub(crate)` ou `pub(in crate::engine)` nos filhos, e que `tests/` usem só a API pública.
2. **`TxRead` largo demais (3 lentes).** 12 métodos para três usos (decidir, consultar, verificar); `LinkColumns` com 19 campos espelha o SQL. Proposta: dividir em leituras por uso (`LineageRead`, `QueryRead`, `ChainRead`, `BatchRead`) e escrever que o espelho do esquema é vazamento aceito.
3. **Derivação das colunas em dois lugares (2 lentes).** `decide` grava `LinkColumns` e `verify` recalcula; se forem duas funções, `verify` pode concordar com um bug do `decide`. Proposta: `columns_of(&LinkBody)` única em `engine/link`.
4. **API pública maior que a Facade (3 lentes).** `lib.rs` expõe `chain` e `CHECKED_FIELDS`; a Facade expõe `catalog() -> &Catalog`. Proposta: `#[doc(hidden)]` ou feature de teste para o que só os testes usam.
5. **Faltam suítes de contrato (2 lentes).** `Store` (rodada em `SqliteStore` e num `MemStore` de teste), validador de schema (corpus comum contra duas crates), `Clock`/`IdGenerator` (formato e unicidade) e `ForeignNodes` (determinismo de `lookup`, sem autoaresta em `native_edges`).
6. **Testes de adulteração presos ao DDL (2 lentes).** `tamper` remove sete triggers pelo nome e `mutations` usa nomes de coluna; reorganizar o `store` quebra o critério 3. Proposta: isolar a lista de colunas e triggers num módulo do `store`, para o contrato de troca ser "reexecutar a tabela de mutações".

## 4. Achados de um verificador só

- **Erro tipado na porta (patterns):** `FORK_REJECTED` é "a tradução do erro de índice", mas `engine` não pode importar `rusqlite`; a porta precisa de `StoreError::UniqueViolation{index}` e `Busy`, com a tradução no adapter.
- **`where_eq` com `Int(1)` e `Bool(true)` (pretas):** `json_extract` devolve `true` como `1`; o teste `it_should_filter_by_a_top_level_scalar_field` mistura os três tipos e não prova a separação. Possível defeito real, não só de desenho.
- **`Clock` e `IdGenerator` (pretas, patterns):** `Clock` devolve `String` com formato só em comentário; `IdGenerator` expõe `uuid::Uuid` na trait; as implementações de produção não aparecem no layout.
- **Replay em dois caminhos (patterns):** leitura fora da transação (§4.5 passo 3) e reconsulta dentro (passo 5); extrair uma função única de leitura da `key`.
- **Decorator do adaptador (patterns):** "ler e depois gravar" é seguro sem lock porque o `base` obrigatório dá concorrência otimista (`FORK_REJECTED` se a ponta mudou); documentar isso.
- **Divisão de arquivos (aninhamento):** `rules.rs` (5 motivos de mudança), `ports.rs` (portas do hospedeiro e de armazenamento) e `model.rs` (cerca de 20 tipos, escrita e leitura) já cruzam o limiar da §5.
- **Config acoplada ao SQLite (pretas):** `Config.db_path` e `busy_timeout` na interface pública da Facade.
- **Fora do escopo agora:** `TenantScope` (Proxy) e Builder de lote ficam para o `omhl`; não antecipar.

## 5. Limiar para dividir uma caixa

Proposto pelo verificador de aninhamento; qualquer um basta.

- Interface pública com mais de 7 métodos ou tipos.
- Implementação acima de cerca de 600 linhas, ou mais de um arquivo acima de 400.
- Dois ou mais motivos independentes de mudança.
- O dependente precisa ler mais de cerca de 150 linhas de interface.

Hoje `rules.rs`, `ports.rs`, `TxRead` e `model.rs` cruzam ao menos um. O plano não tem critério explícito de "o dependente cabe no contexto"; uma contagem de itens públicos por caixa poderia ser conferida pelo `architecture.rs`.

## 6. Limites desta verificação

- Um só modelo (sonnet) com quatro lentes: a independência é média, então a convergência é evidência moderada.
- As definições de caixa preta e cinza vieram de resumos secundários das ideias de Matt Pocock, não da fonte primária.
- Os verificadores leram o plano, não código: nada foi compilado nem executado. O comportamento de `jsonschema` e de `json_extract` segue como hipótese até os spikes da T1.
- O verificador de aninhamento não leu o relatório do Hexlog atual.
- A decisão 8 do plano mestre diz que os limites das caixas saem da análise do código (passo 7); esta árvore é uma proposta sobre o plano do passo 3, não a decisão final.
