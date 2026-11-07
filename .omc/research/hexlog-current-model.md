# Hexlog atual: modelo de referência para a reescrita em Rust

- **Data da leitura:** 2026-11-06, somente leitura, repositório `/home/gabriel/personal/hexlog`, branch `develop` no commit `5d1e22b` (versão `1.2.3`, `package.json:4`).
- **Para quê:** base do passo 1 do [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md) (extrair o que portar, mudar e descartar) e consulta futura.
- **Convenção de citação:** todo `caminho:linha` é relativo à raiz do repositório do Hexlog. Onde a afirmação vem de documento e não de código, o documento é citado; a fonte da verdade do estado atual é o código (`docs/AGENTS.md`, seção Dependencies).
- **Tamanho medido:** 57 arquivos `.ts` em `src/` (9.468 linhas) e 119 specs em `test/`.

## Sumário

1. [Visão geral e mapa de diretórios](#1-visão-geral-e-mapa-de-diretórios)
2. [Arquitetura: camadas e fronteiras](#2-arquitetura-camadas-e-fronteiras)
3. [Modelo de dados](#3-modelo-de-dados)
4. [Cadeia de registros](#4-cadeia-de-registros)
5. [Processos e definições fixadas](#5-processos-e-definições-fixadas)
6. [Gates](#6-gates)
7. [Anexos](#7-anexos)
8. [Tools MCP](#8-tools-mcp)
9. [Persistência](#9-persistência)
10. [Códigos de erro](#10-códigos-de-erro)
11. [Testes e convenções](#11-testes-e-convenções)
12. [Motor genérico versus fluxo de desenvolvimento](#12-motor-genérico-versus-fluxo-de-desenvolvimento)
13. [Portar, mudar ou descartar](#13-portar-mudar-ou-descartar)
14. [Lacunas e dúvidas](#14-lacunas-e-dúvidas)
15. [Índice de arquivos](#15-índice-de-arquivos)

---

## 1. Visão geral e mapa de diretórios

O Hexlog é um servidor MCP stdio em TypeScript (Node `^24.18.1`, `package.json:8`) em que agentes gravam registros tipados num log append-only por processo, encadeado por hash sha256 sobre JCS (`CONTEXT.md:3`, `src/domain/chain.ts:64-67`). Expõe exatamente 12 tools (`src/installation.ts:50`, `src/mcp/server.ts:14-21`). Não há CLI para o agente: só scripts em `scripts/` (relatórios de leitura e utilitários do fluxo deste repositório) e o instalador (`AGENTS.md`, seção Purpose).

**`src/domain/`.** Domínio puro, sem I/O: esquemas zod de ids, registro, relação, elo da cadeia, manifesto e definições, mais as funções que decidem sobre dados já carregados (vigência, regras de relação, herança, gate, classificação de mudança de definição). Só importa `zod`, `canonicalize`, `es-toolkit`, `node:crypto` e `src/errors.ts` (`docs/directives/fronteiras.md:15`).

**`src/shared/`.** O carregador único do log (`loader.ts`: `parseLog`, `isValidLine`, `verifyProcess`, `loadVerified`, `checkExpectedHead`), o tipo `Logger`, `latestVersions` e o corte de texto por caracteres (`src/shared/loader.ts:81`, `:116`, `:201`, `:234`, `:260`; `src/shared/logger.ts:1-9`).

**`src/ports.ts`.** As portas do núcleo: `ProcessReader`/`ProcessStore`, `DefinitionReader`/`DefinitionStore`, `AttachmentReader`/`AttachmentStore`, `Validator`, `SearchIndex` e `Telemetry` (`src/ports.ts:37-262`). Leitura e escrita separadas por interface; `queries/` só recebe os `*Reader` (`src/ports.ts:56`).

**`src/commands/`.** Serviços de escrita: `definition.ts` (`define_type`, `define_relation`, `define_gate` sobre um motor único de versão imutável), `process.ts` (`create_process` e `register`), `attachment.ts` (`attach`), e `register/` com as etapas do `register` (estática antes do lock, `decide` sob o lock, anexos, mensagens de regra).

**`src/queries/`.** Serviço de leitura (`query-service.ts`): `queryRecords`, `evaluateGate`, `verifyChain`, `list`, `readAttachment`, `describeType`; mais `read.ts` (leitura por alcance e marcador), `select.ts` (filtros, vigência, relações de saída), `cursor.ts` (cursor opaco). Nunca grava (`src/queries/query-service.ts:188-263`).

**`src/adapters/`.** Implementações das portas: `fs/` (stores de processo, definição e anexo, lock por processo, escrita atômica, formato em disco), `validator.ts` (ajv 2020-12 estrito), `search.ts` (MiniSearch com cache por processo) e `telemetry/` (OpenTelemetry opcional).

**`src/mcp/`.** `kernel.ts` (envelope `execute` de toda tool, validação zod, erro estruturado, log) e `tools/` com as quatro famílias de tools (processo, definição, anexo, consulta).

**Raiz de `src/`.** `compose.ts` (única raiz de composição), `server.ts` (entrada do bundle), `errors.ts` (catálogo de erros e fábricas), `directory.ts` (resolve `<D>`), `version.ts`, e o subgrafo de instalação (`installation.ts`, `guard.ts`, `archive.ts`), usado só por `scripts/install.ts` (`src/AGENTS.md`, seção Dependencies > Internal).

**`hook/`.** `bash-guard.ts`: hook PreToolUse do Claude Code que nega ao agente acesso Bash ao diretório de dados (`test/AGENTS.md`, linha de `bash-guard.spec.ts`).

**`scripts/`.** Instalador e build (`install.ts`, `build.ts`), relatórios de leitura (`export.ts`, `timeline.ts`, `insights.ts`, `seat-report.ts`) e `scripts/flow/`, a automação do fluxo de desenvolvimento deste repositório (critic, reancoragem, sync de diretrizes, gate, marcador de pré-PR) (`docs/directives/fronteiras.md:30-37`).

**`skills/`.** Skills distribuídas pelo instalador: `hexlog` (bootstrap e diagnóstico), `hexlog-flow` (uso diário) e `hexlog-setup` (configurar um repositório de destino, com pacote opcional "cadeira do dono") (`skills/*/SKILL.md`, frontmatter).

**`.hexlog/`.** Fonte versionada das definições do fluxo deste repositório (8 tipos, 6 nomes de relação, 2 gates), ensaiada por `test/flow-definitions.spec.ts` (`AGENTS.md`, linha da tabela de `.hexlog/`).

**`.claude/`.** Hooks, skills e regras do fluxo de desenvolvimento deste repositório (`.claude/hooks/flow-hooks.ts:1-3`).

**`docs/`.** Diretrizes vivas e ADRs 0007 a 0015 (`docs/directives/`), referência das tools (`docs/tools.md`), layout de dados (`docs/dados.md`), tetos (`docs/tetos-dominio-v1.md`) e pesquisa congelada (`docs/pesquisa/`).

**`test/`.** Jest + ts-jest; unit, property-based (`fast-check`), integração pelo servidor MCP real em memória e e2e pelo bundle real via stdio (`test/AGENTS.md:7`).

---

## 2. Arquitetura: camadas e fronteiras

### 2.1 Camadas

| Camada | Papel | Pode importar |
|---|---|---|
| `domain/` | Regras puras | `errors.ts` (exceção declarada), `zod`, `canonicalize`, `es-toolkit`, `node:crypto` (`docs/directives/fronteiras.md:15`) |
| `shared/` | Carregador do log, logger | `domain/`, portas (`docs/directives/fronteiras.md:16-17`) |
| `ports.ts` | Contratos | tipos de `domain/` |
| `commands/` (escrita), `queries/` (leitura) | Serviços sobre as portas | `domain/`, `shared/`, portas; nunca `adapters/`, nunca um ao outro (`docs/directives/fronteiras.md:17`) |
| `adapters/` | Implementação das portas | builtins, libs de infra; nunca `commands/`, `queries/`, `mcp/` (`docs/directives/fronteiras.md:19`) |
| `mcp/` | Tools sobre os serviços | serviços, `shared/logger.ts`, de `ports.ts` só `Telemetry` e `CallProbe`; nunca `adapters/`, builtins nem `compose.ts` (`docs/directives/fronteiras.md:20`, `:26`) |
| `compose.ts` | Única raiz de composição | tudo; só `server.ts`, scripts e testes o importam (`docs/directives/fronteiras.md:30`) |

Dependências entram por injeção (`createXService(deps)`): `createProcessService` recebe `store`, `definitions`, `attachments`, `validator`, `clock`, `newUuid` e `logger` (`src/commands/process.ts:134-144`); `compose` liga os adaptadores reais e passa `randomUUIDv7` como `newUuid` (`src/compose.ts:1`, `:85`). Logger e telemetria são as duas únicas "capacidades transversais", lista fechada (`docs/directives/fronteiras.md:21`).

Toda operação de porta é síncrona, exceto `ProcessStore.write`, que é `async` só porque a espera do lock dorme com `setTimeout`; a regra de negócio da escrita fica num `decide` síncrono que o adaptador chama sob o lock (`src/ports.ts:25-28`, `:74`). Esse desenho "adaptador só sabe de bytes, lock e prefixo `\n`; o serviço decide a linha" é o ponto de costura entre regra e persistência.

### 2.2 Como as fronteiras são impostas

- **ESLint:** `eslint.boundaries.js` define `boundaryBlocks` (`eslint.boundaries.js:176`): builtins e `ajv`/`ajv-formats`/`minisearch` barrados nas camadas puras (`:9-21`, `:13`), SDK do MCP só em `mcp/` e `adapters/**` barrado nos serviços (`:46-49`), `compose.ts` só na raiz (`:27-29`), `@opentelemetry/*` só em `adapters/telemetry/` (`:33-35`), tipos de telemetria fora do núcleo (`:40-43`), import dinâmico barrado (`:55-59`), `max-lines` 800 (`:73`), `mcp/kernel.ts` sem importar `tools/` (`:215`).
- **Spec de fronteira:** `test/boundaries.spec.ts` roda a API do ESLint sobre `test/fixtures/boundaries/` e prova os dois lados de cada trava (`test/AGENTS.md`, linha de `boundaries.spec.ts`).
- **Vocabulário de fluxo:** `test/no-flow-terms.spec.ts` varre `src/`, `hook/` e `scripts/` (menos `scripts/flow/`) contra `FLOW_TERMS` (`phase`, `fase`, `plan`, `plano`, `critic`, `milestone`, `verdict`, `claim`, `omc`) (`test/no-flow-terms.spec.ts:8-10`, `:27-36`).
- **Convenção sem trava:** `errors.ts` e `ports.ts` dentro de `domain/` (`docs/directives/fronteiras.md:15`), `import()` em `adapters/` e na raiz (`:44`), só `scripts/flow/writer.ts` chamar `compose()` (`:45`).

---

## 3. Modelo de dados

### 3.1 Identificadores e nomes

| Conceito | Regra | Onde |
|---|---|---|
| `Name` (projeto, processo, tipo, nome de relação, gate, alias) | `^[a-z0-9][a-z0-9-]{0,62}$` | `src/domain/ids.ts:4-5` |
| `Hash` | 64 hex minúsculos | `src/domain/ids.ts:8` |
| `Instant` | ISO datetime, precisão de ms | `src/domain/ids.ts:11` |
| `Target` | segmentos `Name` separados por `.`, até 200 caracteres | `src/domain/ids.ts:15-18` |
| `RecordId` | `<process>:<uuidv7>`; atribuído pelo servidor, nunca pelo agente | `src/domain/ids.ts:21-24` |
| `Marker` | `Record<Name, RecordId \| null>`: uma entrada por processo lido | `src/domain/ids.ts:32` |
| Nomes reservados de processo | `types`, `relations`, `gates`, `attachments`, `archive` | `src/domain/ids.ts:56-62` |

O id carrega o processo dono: `processOf(id)` é o trecho antes de `:` (`src/domain/ids.ts:47-49`). O uuid é gerado no `decide` (`src/commands/register/state.ts:120`) e é opaco: "nenhuma regra lê o tempo dele" (`src/commands/process.ts:140-141`).

**Target** diz "sobre o que" é o registro; prefixo casa só na fronteira do `.`: `a.b` casa `a.b` e `a.b.c`, não `a.bc` (`src/domain/gate.ts:61-64`). O target é independente do id: vários registros (de tipos diferentes) compartilham um target, e uma linhagem de `supersedes` exige mesmo tipo e mesmo target (`src/domain/relations.ts:253-254`).

### 3.2 Registro

`HexRecord` (`src/domain/record.ts:110-118`), objeto estrito:

| Campo | Tipo | Observação |
|---|---|---|
| `id` | `RecordId` | do servidor |
| `type` | `Name` | precisa estar fixado no processo |
| `at` | `Instant` | relógio do servidor, um só valor por lote (`src/commands/register/state.ts:323`) |
| `target` | `Target` | |
| `author` | `{agent, model?, client}` | 1 a 100 caracteres bem formados; `client` vem do envelope MCP, nunca do agente (`src/domain/record.ts:58-68`, `src/mcp/kernel.ts:181-184`) |
| `data` | objeto JSON | até `DATA_MAX_CHARS` = 16.000 caracteres canônicos JCS (`src/domain/record.ts:11`, `:108`) |
| `relations` | `Relation[]` | até `RELATIONS_MAX` = 100 (`src/domain/record.ts:17`, `:117`) |

Na entrada (`BatchItem`, `src/domain/record.ts:121-127`) o agente envia `alias?`, `type`, `target`, `data`, `relations?`; não envia `id`, `at` nem `author`.

### 3.3 Relações

Relação gravada: `{kind, to, as?}` com `kind` sempre resolvido e `to` sempre um id (`src/domain/record.ts:71-77`). Relação de entrada: `to` pode ser `@alias` de um item anterior do lote, e vem `kind`, `as` ou os dois (`src/domain/record.ts:79-101`).

**As oito espécies (`kind`)**, fixas em código (`src/domain/record.ts:43-52`):

| `kind` | Regra de escrita | Leitura | Herança |
|---|---|---|---|
| `supersedes` | só mesmo processo, mesmo tipo e mesmo target; destino vigente (`src/domain/relations.ts:250-256`, `:283-291`) | tira o destino de vigência; forma a linhagem (`src/domain/relations.ts:55`, `:61`) | nunca herda (`src/domain/inheritance.ts:25`) |
| `revokes` | só mesmo processo; destino vigente; target pode diferir (`src/domain/relations.ts:250-256`; ADR 0007, emenda de 2026-10-16 em `docs/directives/adr-0007-dominio.md:88`) | tira o destino de vigência sem sucessor (`src/domain/relations.ts:56`) | nunca herda |
| `supports` | destino vigente (`stale-destination`), sem `contradicts` para o mesmo destino no mesmo registro (`src/domain/relations.ts:269-291`) | gate `approved`; `needsReview` (apoio morto) (`src/domain/gate.ts:226`, `src/domain/relations.ts:142-189`) | sobe o destino para a versão atual (`src/domain/inheritance.ts:15`) |
| `contradicts` | sem `supports` para o mesmo destino no mesmo registro | gates `approved` e `no_open_contradiction`; corta herança entre linhagens (`src/domain/gate.ts:230`, `:282-284`; `src/domain/inheritance.ts:112-113`) | nunca herda |
| `answers` | nenhuma regra própria | só via `resolvedBy.kind` do gate `no_pending` e filtros (`docs/directives/adr-0007-dominio.md:87`) | sobe a origem (`src/domain/inheritance.ts:18-23`) |
| `derivesFrom` | nenhuma | idem | sobe a origem |
| `complements` | nenhuma | idem | sobe a origem |
| `reopens` | nenhuma | idem | sobe a origem |

O ADR 0007 resume: o motor entende `supersedes`, `revokes`, `supports` (e, pelo texto original, `answers`); `contradicts` aponta conflito; `derivesFrom`, `complements`, `reopens` são guardadas sem regra e consultáveis por gate; "o projeto nomeia relações, mas não cria semântica" (`docs/directives/adr-0007-dominio.md:32`). A emenda de 2026-10-09 registra que `answers` também não tem regra em código (`docs/directives/adr-0007-dominio.md:87`).

**Nomes de relação (`as`).** Definição do projeto: `{name, kind, from?, to?}`, onde `from`/`to` são listas não vazias de tipos permitidos em cada ponta; omitida, a ponta aceita qualquer tipo (`src/domain/definitions.ts:25-39`). O agente pode mandar só `as`; mandando `kind` e `as`, o `kind` precisa bater com o fixado (`kind-mismatch`) (`src/domain/relations.ts:240-248`). Tipo da ponta fora da lista dá `endpoint-type` (`src/domain/relations.ts:258-266`). O nome gravado vai em `Relation.as`, e as contagens de relação na leitura agrupam por `as ?? kind` (`src/queries/select.ts:206-215`).

**Ordem das regras estruturais** (primeira violação vence): autorrelação, nome, sucessão (processo, tipo, target), pontas por tipo, conflito entre relações do mesmo registro, vigência do destino (`src/domain/relations.ts:293-310`). Os onze `RuleCode` estão em `src/domain/relations.ts:192-203`; as mensagens em `src/commands/register/errors.ts:5-18`.

### 3.4 Vigência

- **Vigente:** registro sem relação de entrada `supersedes` nem `revokes`; id desconhecido conta como vigente (`src/domain/relations.ts:38-43`, `:61`).
- **Versão atual:** `currentOf(id)` percorre os sucessores até o único vigente da linhagem, ou `null` se a linhagem foi revogada; memoizado (`src/domain/relations.ts:63-95`).
- **Linhagem:** registros ligados por `supersedes`; a raiz é calculada por união-busca com compressão de caminho (`src/domain/inheritance.ts:56-80`).
- **Escopo:** a vigência de um registro se calcula só com o processo dele, porque `supersedes`/`revokes` não cruzam processos (`docs/directives/adr-0007-dominio.md:38`). Na leitura de alcance processo, destino de outro processo é desconhecido e conta como vigente (`src/queries/select.ts:226-229`).
- **Ciclo:** `hasCycle` (Kahn sobre as arestas `supersedes`) recusa o lote que fecharia ciclo (`src/domain/relations.ts:100-123`, `src/commands/register/state.ts:321`).

### 3.5 Supersedes, revokes e fork

- `supersedes` sobre destino não vigente, ou `revokes` sobre não vigente, é a violação `not-current`, que sai como `FORK_REJECTED` (não `INVALID_RECORD`) com `details[0].current` = versão atual da linhagem ou `null` (`src/domain/relations.ts:283-291`, `src/commands/register/errors.ts:33-39`).
- A recusa existe para que nenhuma linhagem tenha dois vigentes ("bifurcação", `CONTEXT.md:135-137`). Dois `supersedes` concorrentes ao mesmo destino dão um aceito e um `FORK_REJECTED`, porque o segundo `decide` relê o log sob o lock (`test/AGENTS.md`, linha de `register.concurrency.spec.ts`).
- Dentro do lote: `supersedes`/`revokes` leem a vigência dos itens anteriores do lote; `supports` lê a vigência depois do lote inteiro, o que recusa `[supersedes → E, supports → E]` e `[supports → E, supersedes → E]` (`src/commands/register/state.ts:145-183`).
- Mudar um fato de target: `revokes` no antigo e registro novo com `derivesFrom` para ele (`src/commands/register/errors.ts:11-12`).
- A leitura tolera bifurcação forjada: `currentOf` segue o menor id de cada ramo e devolve o primeiro vigente, determinista (`src/domain/relations.ts:45-49`).

### 3.6 Herança de relações pela linhagem

Derivada só na leitura, nunca gravada (`src/domain/inheritance.ts:7-12`). Vale só em processo cujo manifesto tem `lineageInheritance: true`, gravado por todo `create_process` novo (`src/commands/process.ts:155`, `src/domain/manifest.ts:13-14`). Regras (`src/domain/inheritance.ts:82-159`):

- `supports` sobe o destino para a versão atual, desde que a origem seja vigente.
- `answers`, `derivesFrom`, `complements`, `reopens` sobem a origem.
- `contradicts` vigente entre as duas linhagens, em qualquer direção, corta a herança do par inteiro.
- Aresta direta idêntica (mesmo `kind`, `as`, origem, destino) vence a herdada; cada herdada carrega `origin` (a aresta física).

### 3.7 Apoio morto (`needsReview`)

Um `supports` A→B "está vivo" enquanto a versão atual de A apoia a versão atual de B (diretamente ou por herança). Quando uma ponta saiu de vigência sem a versão atual regravar o apoio, a ponta vigente ganha `needsReview.staleIn` ou `staleOut`. Só marca, não bloqueia (`src/domain/relations.ts:125-189`). No alcance processo, alertas que cruzam processos não são emitidos (`src/domain/relations.ts:134-137`).

### 3.8 Idempotência por `key`

- `key` opcional, 1 a 200 caracteres bem formados (`src/domain/chain.ts:9-15`, `src/mcp/tools/process.ts:18`).
- A impressão do lote (`fingerprint`) é o sha256 do JCS dos itens de entrada, sem `key` e sem autor (`src/domain/chain.ts:59-62`, `src/commands/process.ts:205`).
- Sob o lock, o `decide` indexa os lotes por `key` (a primeira ocorrência vale, `src/shared/loader.ts:185-194`). Mesma `key` e mesma impressão: devolve os ids já gravados, `replayed: true`, sem gravar, e o marcador da cabeça atual. Mesma `key` com impressão diferente: `IDEMPOTENCY_CONFLICT` (`src/commands/register/state.ts:100-104`, `:307-314`).
- O replay acontece antes das regras dependentes de estado (nível 5 de 6), então reenviar um lote cujo destino deixou de ser vigente ainda devolve `replayed` (`src/commands/process.ts:56-62`).
- Sem `key`, reenviar duplica o lote; a tool `register` é anunciada com `idempotentHint: false` por isso (`src/mcp/tools/process.ts:95-96`).
- O replay emite o log `batch-replayed` (`src/commands/process.ts:212-220`).

### 3.9 Lote (`batch`)

1 a 50 itens (`BATCH_MAX`, `src/domain/record.ts:14`), gravados numa só linha do log, tudo ou nada (`CONTEXT.md:31-33`). Aliases únicos; `@alias` só para item anterior (`src/commands/register/static.ts:32-63`). Só o primeiro elo leva o campo `batch` (`fingerprint`, `key?`, `aliases?`), dentro do hash (`src/domain/chain.ts:17-28`, `src/commands/register/state.ts:255-291`).

### 3.10 Limites de tamanho (domínio e tools)

| Limite | Valor | Onde |
|---|---|---|
| `data` | 16.000 caracteres JCS | `src/domain/record.ts:11` |
| Itens por lote | 50 | `src/domain/record.ts:14` |
| Relações por registro | 100 | `src/domain/record.ts:17` |
| `key` | 200 | `src/domain/chain.ts:10` |
| `aliases` | 50 | `src/domain/chain.ts:12` |
| Schema de tipo | 16.000 JCS | `src/domain/definitions.ts:9` |
| Nome de relação e gate (objeto inteiro) | 16.000 JCS | `src/domain/definitions.ts:12` |
| Perguntas por gate | 50 | `src/domain/definitions.ts:15` |
| `records.jsonl` por processo | 64 MiB | `src/adapters/fs/process-store.ts:42` |
| Anexo | 1 MiB | `src/adapters/fs/attachment-store.ts:21` |
| `details` por erro | 50, com `too-many-errors` | `src/errors.ts:129`, `:159-177` |
| Custo do 2º passe do validador | 25.000 | `src/adapters/validator.ts:209` |
| `text` da `query` | 200 | `src/queries/query-service.ts:69` |
| Cursor | 65.536 caracteres | `src/queries/cursor.ts:11` |
| Página | 24.000 caracteres (`PAGE_CHARS_CAP`) | `src/queries/read-attachment.ts:14` |
| `limit` da `query` | padrão 50, máx. 200 | `src/queries/query-service.ts:64`, `src/mcp/tools/query.ts:31` |
| `ids`, `where`, `fields`, `targets`, chaves do marcador | 200, 50, 50, 50, 200 | `src/mcp/tools/query.ts:40-47` |
| `changes` e `evidence` por lista no envelope | 100 | `src/mcp/tools/query.ts:34`, `:37` |
| Aninhamento dos args | 64 níveis | `src/mcp/kernel.ts:131` |

`docs/tetos-dominio-v1.md:9-36` registra quais números são medidos, quais herdados do 0.x e quais palpite. Ponto relevante para o port: como `isValidLink` reaplica os schemas na releitura, apertar um teto invalida linha já gravada, e por isso é mudança de formato (`src/domain/record.ts:6-10`, `docs/tetos-dominio-v1.md:7`).

---

## 4. Cadeia de registros

### 4.1 Elo, hash e âncora

- **Elo (`Link`)** = `HexRecord` + `seq` (inteiro lógico contíguo por processo) + `prevHash` + `batch?` (`src/domain/chain.ts:30-36`).
- **Hash do elo:** `sha256hex(prevHash + JCS(elo sem prevHash))` (`src/domain/chain.ts:64-67`). O JCS (RFC 8785) vem do pacote `canonicalize`; valor sem forma JSON vira `INTERNAL` em vez de hashear `''` (`src/domain/chain.ts:41-48`).
- **Âncora:** `sha256hex(JCS(manifesto))`; é o `prevHash` do primeiro elo e o `head` de um processo vazio (`src/domain/chain.ts:69-72`, `src/shared/loader.ts:202`).
- **Linha do log:** `{"links":[...]}` + `\n`, de 1 a 50 elos, um lote por linha (`src/shared/loader.ts:39-40`, `:93-96`).
- **Predicado único:** `isValidLine` é usado na leitura e na escrita; o `decide` valida a linha que montou antes de devolvê-la (`src/shared/loader.ts:76-91`, `src/commands/register/state.ts:324-327`). `isValidLink` confere forma (`invalid-line`), `seq` (`diverging-seq`) e `prevHash` (`hash-mismatch`) (`src/domain/chain.ts:89-104`).

### 4.2 Linhas rasgadas e quebras

`parseLog` percorre os segmentos por `\n` (`src/shared/loader.ts:110-140`):

- Segmento que não é JSON é "rasgado" e fica pendente. Se uma linha válida vier depois, os rasgados viram `repairedLines` (não quebram); se vier uma linha rejeitada, viram quebras `invalid-line`; no fim do arquivo, rasgados pendentes são ignorados (não consomem `seq`).
- Linha JSON que não ocupa a posição esperada vira quebra com as razões; a cadeia segue do `next` quando todos os elos têm forma de `Link`.
- O escritor acrescenta `\n` de prefixo se o arquivo não termina em `\n` (`src/adapters/fs/process-store.ts:153-155`).

### 4.3 `verify_chain`

`verifyChain` carrega e verifica o processo inteiro, mais os anexos citados (`src/queries/query-service.ts:568-586`):

- Saída: `ok`, `totalRecords`, `head` (hash do último elo contado, ou a âncora), `breaks` (até `MAX_BREAKS` = 100), `totalBreaks`, `repairedLines` (até 100), `attachmentBreaks`, `totalAttachmentBreaks` (`src/shared/loader.ts:17-37`).
- Quebra é resultado (`ok: false`), não exceção.
- `expectedHead` (um `head` guardado antes) detecta cauda apagada ou reescrita: precisa ser o hash de algum elo válido ou a âncora; senão entra a quebra `head-not-found` (`src/shared/loader.ts:227-257`). Sem ele, truncar o fim do log não é detectável (`docs/directives/invariantes.md:13`).
- Toda leitura de serviço (query, gate) também verifica a cadeia e falha fechada com `PROCESS_CORRUPTED` `broken-chain` (`src/queries/read.ts:104-110`); o `register` também (`src/commands/register/state.ts:304-305`).

### 4.4 Marcador e cabeça

Dois conceitos distintos (`CONTEXT.md:21-23`, `:109-111`):

- **Marcador (`marker`):** id do último registro lido de cada processo (`null` para vazio). É o que tools devolvem e aceitam (`register`, `query`, `evaluate_gate`).
- **Cabeça (`head`):** hash do último elo; só o `verify_chain` a expõe; o cursor a guarda em `markerHashes`.

**Leitura a partir de um marcador:** `verifyProcess(raw, marker)` corta o log no elo marcado, inclusive no meio de uma linha (lote parcial), e a cadeia devolvida conta só o prefixo (`src/shared/loader.ts:166-183`, `:196-225`). Id ausente: `MARKER_NOT_FOUND`. Regras de alcance (`src/queries/read.ts:29-51`):

- Alcance processo: o marcador precisa ter exatamente a entrada do processo lido; ausente ou extra é `MARKER_NOT_FOUND` (ler como vazio faria o gate passar por vacuidade).
- Alcance projeto: processo que o marcador não nomeia é lido como vazio (nasceu depois); processo nomeado que não existe é `MARKER_NOT_FOUND`.
- Se o elo marcado sumiu por adulteração, a leitura relê o processo inteiro e responde `broken-chain` em vez de `MARKER_NOT_FOUND` (`src/queries/read.ts:53-74`).

Usos: `changesSince` da `query` (o que entrou e saiu do resultado), `marker` do `evaluate_gate` (reproduzir a avaliação sobre o que existia então) e o cursor (fixa o marcador da página 1) (`src/queries/query-service.ts:81-88`, `:150-151`, `:537-547`).

---

## 5. Processos e definições fixadas

### 5.1 Projeto e processo

- **Projeto:** espaço nomeado que guarda definições e processos (`CONTEXT.md:9-11`). Não há tool que crie projeto: ele passa a existir quando a primeira definição é gravada (`create_process` recusa projeto sem definição, `src/commands/process.ts:128-132`).
- **Processo:** log append-only com as definições que fixou ao nascer (`CONTEXT.md:13-15`). Alcance de leitura: um processo ou o projeto inteiro (`CONTEXT.md:17-19`).

### 5.2 `create_process`

`createProcess({project, process})` (`src/commands/process.ts:147-180`):

1. Recusa nome reservado (`RESERVED_NAME`).
2. Tira o retrato da versão vigente (a maior) de cada tipo, nome de relação e gate do projeto (`src/commands/process.ts:82-104`).
3. Projeto sem nenhuma definição: `TYPE_NOT_FOUND` `unknown-name` em `/project` (`src/errors.ts:442-450`).
4. Monta o manifesto com `lineageInheritance: true` e os hashes de cada grupo (`hashOfJcs` do mapa por nome) e cria.
5. Idempotente por nome: processo existente volta intacto com `created: false` e `stale` listando as definições cujo conteúdo vigente difere do fixado (`src/commands/process.ts:112-122`).

### 5.3 Manifesto

`process.json`, schema estrito (`src/domain/manifest.ts:9-21`): `project`, `process`, `createdAt`, `lineageInheritance?`, `fixed.{types,relations,gates}` (o conteúdo de cada definição por nome, não a versão) e `hashes.{types,relations,gates}`. É imutável e é a raiz da cadeia (âncora). Consequência: depois de um `breaking: true`, o caminho é um processo novo (`docs/directives/invariantes.md:65`).

### 5.4 Definições versionadas

Motor único `defineVersioned` para os três `define_*` (`src/commands/definition.ts:148-196`):

- Versão `major.minor` sem zero à esquerda, até 6 dígitos por segmento; primeira versão `1.0` (`src/domain/definitions.ts:55-82`, `src/commands/definition.ts:63`).
- Definição idêntica à vigente (mesmo `hashOfJcs`) é replay: `created: false` (`src/commands/definition.ts:172-174`).
- Classificação de mudança (`src/domain/definitions.ts:84-154`):
  - Tipo: só é compatível acrescentar propriedade (sem pôr em `required`) ou valor de `enum`, fora de `not`/`if`/`anyOf`/`allOf`/`oneOf`; o resto é quebra.
  - Nome de relação: alargar `from`/`to` é compatível; trocar `kind` ou estreitar é quebra.
  - Gate: o servidor não detecta quebra; toda mudança é minor, salvo `breaking: true` (`src/commands/definition.ts:124-129`).
- Quebra sem `breaking: true`: `BREAKING_CHANGE` (`src/commands/definition.ts:136-146`).
- Concorrência: a versão é gravada por `link` exclusivo; se outro escritor ocupou a versão alvo, relê e decide de novo, até 10 tentativas (`exclusive-write-exhausted`); `divergentVersions` lista o que outros gravaram a partir da mesma base (`src/commands/definition.ts:65-67`, `:176-195`).

### 5.5 Validação do schema dos tipos

- O schema é JSON Schema 2020-12 validado por ajv estrito com `ajv-formats` (`src/adapters/validator.ts:134-138`); a raiz precisa ser `type: "object"` (`src/commands/definition.ts:107-110`).
- `pattern` e `patternProperties` são recusados (`pattern-not-allowed`) e, num tipo legado que os tenha, ignorados na validação: nenhum regex vindo do schema é construído nem executado (`src/adapters/validator.ts:94`, `:127`, `:163`; `docs/directives/invariantes.md:52-53`). Formato de campo vem de um catálogo fechado (`git-sha`, `src/domain/formats.ts:1-13`) ou de `minLength`/`maxLength`.
- `format: "attachment"` só é aceito em `/properties/<campo>` ou `/properties/<campo>/items` (`src/adapters/validator.ts:85-113`).
- `$id` já conhecido e `$async` são recusados (`src/adapters/validator.ts:180-192`).
- Validação de `data` no `register`: primeiro passe `allErrors: false`; se reprova e `expandedSize(schema) * countNodes(dado)` cabe em 25.000, segundo passe com todas as violações; acima, `violations-capped` (`src/adapters/validator.ts:209`, `:253-312`; `src/domain/schema-walk.ts:107-159`).
- `describe_type` devolve o schema sem `pattern`/`patternProperties` (`src/domain/schema-walk.ts:63-73`).

### 5.6 Fluxo do `register` (seis níveis de recusa)

`register` (`src/commands/process.ts:192-222`), ordem documentada em `src/commands/process.ts:55-71`:

1. Forma do lote (`INVALID_INPUT`) — `checkBatchShape` (`src/commands/register/static.ts:36-63`).
2. Manifesto: `PROCESS_NOT_FOUND`, `PROCESS_CORRUPTED` `unreadable-manifest`.
3. Estáticas, antes do lock: tipo fixado de cada item (`TYPE_NOT_PINNED`, primeiro erro), depois num só `INVALID_RECORD` as violações de schema de todo o lote e, por relação, `unknown-relation-name`, `kind-mismatch` e `cross-process-currency` (`src/commands/register/static.ts:128-160`).
4. Sob o lock: cadeia íntegra (`broken-chain`).
5. `key`: replay ou `IDEMPOTENCY_CONFLICT`.
6. Destinos de outro processo (lidos uma vez, sem lock), anexos, regras de relação, ciclo (`src/commands/register/state.ts:316-321`).

Depois monta um elo por item, valida a linha e devolve `{line, result}`; o adaptador grava e dá `fsync` (`src/commands/register/state.ts:323-332`). Resposta: `records` (`{alias?, id}` na ordem de entrada), `replayed`, `marker` com o último id gravado (`src/commands/register/state.ts:60-67`).

Destino em outro processo ausente ou corrompido: `RELATION_NOT_FOUND` (`missing` ou `destination-corrupted`) (`src/commands/register/state.ts:187-208`). A vigência de `supports` cruzado é melhor esforço: o destino pode ser superado entre a leitura e a gravação, porque só a origem é travada (`docs/directives/adr-0007-dominio.md:42`).

---

## 6. Gates

No Hexlog, **gate é uma consulta declarativa sobre os registros, avaliada sem gravar nada** (`CONTEXT.md:141-143`). Não é "texto de instrução" nem gatilho de evento: isso diverge do "grafo de gates" do plano v2 ([omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md), seção 1, item 3).

### 6.1 `define_gate`

`Gate = {name, questions[1..50]}` (`src/domain/definitions.ts:41-50`). Cada pergunta é uma união discriminada por `kind` (`src/domain/gate.ts:36-58`), com `scope?` (`process`, padrão, ou `project`):

| `kind` | Campos | Passa quando |
|---|---|---|
| `approved` | `of: Selector`, `by?: Selector`, `inherit?: true` | há ao menos um vigente que casa `of`, cada um tem origem vigente de `supports` (que casa `by`) e nenhum tem origem vigente de `contradicts` (`src/domain/gate.ts:213-243`) |
| `occurred` | `select: Selector`, `min?` (padrão 1) | ao menos `min` vigentes casam `select` (`src/domain/gate.ts:245-251`) |
| `no_pending` | `pending: Selector`, `resolvedBy: {kind, from?: TypeNames}`, `inherit?` | todo vigente que casa `pending` tem origem vigente pela relação `resolvedBy.kind` (de um tipo de `from`, se dado) (`src/domain/gate.ts:253-276`) |
| `no_open_contradiction` | `of?: Selector` | nenhum vigente que casa `of` (padrão: todos) tem origem vigente de `contradicts` (`src/domain/gate.ts:278-286`) |

`Selector = {type?, targetPrefix?, where?}`; `where` é igualdade sobre campos escalares de primeiro nível de `data`; `targetPrefix` omitido herda o `target` da avaliação (`src/domain/gate.ts:12-24`, `:66-78`). Só registro vigente conta, inclusive como origem (`src/domain/gate.ts:333-336`). `inherit: true` soma apoio ou resolução herdados pela linhagem e lista em `evidence.inherited` os que contam só por herança (`src/domain/gate.ts:186-204`).

### 6.2 `evaluate_gate`

`evaluateGate` (`src/queries/query-service.ts:443-490`):

- O gate precisa estar fixado no manifesto do processo (`GATE_NOT_FOUND`).
- Se alguma pergunta tem `scope: "project"`, lê o projeto; senão, só o processo. O marcador devolvido cobre exatamente o que foi lido.
- `marker` de entrada reproduz a avaliação sobre o prefixo marcado.
- `targets` (1 a 50) avalia vários targets numa leitura, com um `results` por target e um só marcador; exclusivo com `target` e incompatível com `evidence: full` (`src/queries/query-service.ts:452-462`).
- `evidence`: `full` (listas completas), `counts` (só tamanhos) ou `failing` (tamanhos mais as listas de diagnóstico de pergunta reprovada) (`src/domain/gate.ts:112-129`, `:301-331`). O serviço assume `full`; a tool assume `failing` sem `targets` e `counts` com `targets` (`src/mcp/tools/query.ts:383-389`) e corta cada lista em 100 ids com `omitted` (`src/mcp/tools/query.ts:319-351`).
- A `View` (vigência, relações de entrada, herança) é montada uma vez por alcance (`src/domain/gate.ts:341-369`).

Exemplos reais, do fluxo deste repositório: o gate `gaps` (`no_pending` de `gap` resolvido por `answers` de `directive` ou `premise`, alcance projeto) e o `pre-pr` (`no_pending` de `finding` com `severity: URGENT` resolvido por `answers` de `decision`) (`.hexlog/gates/gaps.json`, `.hexlog/gates/pre-pr.json`).

---

## 7. Anexos

- **O que é:** texto guardado à parte, endereçado pelo sha256 dos próprios bytes UTF-8 e citado por um campo do registro (`CONTEXT.md:47-49`, `src/ports.ts:130-143`).
- **`attach`:** exatamente um de `text` ou `path` (`src/commands/attachment.ts:56-67`). `path` só `.md`/`.txt` (`src/commands/attachment.ts:18`), dentro de `realpath(cwd)` do servidor e fora de `realpath(dataDir)`, arquivo regular de um link só, aberto com `O_NOFOLLOW` e conferido por `/proc/self/fd` (`src/adapters/fs/attachment-store.ts:97-179`). Texto não vazio, sem surrogate solto, até 1 MiB (`src/commands/attachment.ts:36-41`, `src/adapters/fs/attachment-store.ts:246-247`). Saída `{hash, bytes, deduplicated}`; blob existente é só conferido, nunca sobrescrito (`src/adapters/fs/attachment-store.ts:205-217`).
- **`format: "attachment"`:** marca no schema do tipo que diz que o campo guarda hash de anexo, em propriedade de primeiro nível ou nos `items` de um array de primeiro nível (`src/domain/definitions.ts:156-169`). No `register`, cada hash em campo marcado precisa existir e estar íntegro (`ATTACHMENT_NOT_FOUND`/`ATTACHMENT_CORRUPTED`); hash de anexo guardado em campo **sem** a marca é recusado com `unmarked-attachment` (`src/commands/register/attachments.ts:18-58`).
- **Leitura:** `read_attachment` devolve uma página de até 24.000 caracteres por `offset`, sem partir par surrogate (`src/queries/read-attachment.ts:32-55`, `src/shared/pages.ts:11-21`). A `query` anota `attachmentStatus` por hash citado (`src/queries/query-service.ts:390-405`), e o `verify_chain` lista anexos ausentes ou corrompidos (`src/queries/query-service.ts:574-585`). O blob é re-hasheado a cada leitura, com memo por `ino`/`size`/`mtime`/`ctime` (`src/AGENTS.md`, linha de `attachment-store.ts`).
- **Limites conhecidos:** sem teto total por projeto e sem coleta de lixo (`docs/tetos-dominio-v1.md:31`).

---

## 8. Tools MCP

Comum a todas (`src/mcp/kernel.ts:232-308`):

- Ordem do `execute`: `isLegacy()` → `LEGACY_DATA`; chave `__proto__` em qualquer nível → `INVALID_INPUT` `reserved-key`; aninhamento > 64 → `too-deep`; validação zod → `INVALID_INPUT` com `details[{path,code,message}]`; só então o serviço.
- `HexlogError` vira `{code, message, details}` em `content[0].text` com `isError: true`, sem `structuredContent`; outra exceção vira `INTERNAL` sem stack (`src/mcp/kernel.ts:81-92`, `:115-124`).
- Sucesso devolve `structuredContent` e a mesma coisa em texto (`src/mcp/kernel.ts:211-219`).
- O SDK anuncia o JSON Schema do zod, mas quem valida é o `execute` (`advertise`, `src/mcp/kernel.ts:94-104`).
- Log `tool` com `name`, `ms`, `code?`, nunca a entrada; aviso `tool-over-cap` acima de 48.000 caracteres (`src/mcp/kernel.ts:170-176`, `:259-273`).
- Anotações: escrita `{readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false}`; leitura `{readOnlyHint: true, openWorldHint: false}` (`src/mcp/kernel.ts:30-39`).
- Schemas de entrada são `z.strictObject`: chave desconhecida é `INVALID_INPUT` (`src/AGENTS.md`, linha de `mcp/tools/attachment.ts`).

### 8.1 `create_process` (escrita)

- **Entrada:** `{project, process}` (`src/mcp/tools/process.ts:9`).
- **Saída:** `{project, process, created, lineageInheritance?, pinned: {types, relations, gates}, stale?: [{kind, name, current}]}` (`src/mcp/tools/process.ts:22-31`).
- **Erros:** `RESERVED_NAME`, `TYPE_NOT_FOUND` (projeto sem definição), `INVALID_INPUT` (nome), `IO_ERROR`, `INTERNAL`.
- **Invariantes:** idempotente por nome; manifesto existente nunca é tocado (`src/ports.ts:58-63`).

### 8.2 `register` (escrita)

- **Entrada:** `{project, process, agent, model?, key?, records: BatchItem[1..50]}` (`src/mcp/tools/process.ts:13-20`).
- **Saída:** `{records: [{alias?, id}], replayed, marker}` (`src/mcp/tools/process.ts:33-37`).
- **Erros:** `INVALID_INPUT`, `PROCESS_NOT_FOUND`, `PROCESS_CORRUPTED`, `TYPE_NOT_PINNED`, `INVALID_RECORD`, `FORK_REJECTED`, `IDEMPOTENCY_CONFLICT`, `RELATION_NOT_FOUND`, `CYCLE_REJECTED`, `ATTACHMENT_NOT_FOUND`, `ATTACHMENT_CORRUPTED`, `PROCESS_TOO_LARGE`, `LOCK_TIMEOUT`, `IO_ERROR`, `INTERNAL`.
- **Invariantes:** seis níveis de recusa (seção 5.6); tudo ou nada; `IO_ERROR` depois do `fsync` deixa o resultado incerto e o reenvio com a mesma `key` resolve (`src/mcp/tools/process.ts:60-61`). É a única tool com `_meta` `anthropic/alwaysLoad` (`src/mcp/tools/process.ts:97`).

### 8.3 `define_type` (escrita)

- **Entrada:** `{project, name, schema, breaking?}` (`src/mcp/tools/definition.ts:26-31`).
- **Saída (`Defined`):** `{name, version, hash, created, previousVersion?, divergentVersions?}` (`src/mcp/tools/definition.ts:17-24`).
- **Erros:** `INVALID_SCHEMA` (schema inválido, `pattern-not-allowed`, raiz não objeto), `BREAKING_CHANGE`, `INVALID_INPUT`, `INTERNAL` (`exclusive-write-exhausted`, `unreadable-definition`), `IO_ERROR`.

### 8.4 `define_relation` (escrita)

- **Entrada:** `{project, name, kind, from?, to?, breaking?}` (`src/mcp/tools/definition.ts:34`).
- **Saída:** `Defined`. **Erros:** `INVALID_INPUT` (forma ou teto), `BREAKING_CHANGE`, `INTERNAL`, `IO_ERROR`.

### 8.5 `define_gate` (escrita)

- **Entrada:** `{project, name, questions, breaking?}` (`src/mcp/tools/definition.ts:36`).
- **Saída:** `Defined`. **Erros:** `INVALID_INPUT`, `INTERNAL`, `IO_ERROR`. Não há `BREAKING_CHANGE` automático para gate.

### 8.6 `attach` (escrita)

- **Entrada:** `{project, text?, path?}` (`src/mcp/tools/attachment.ts:8-15`).
- **Saída:** `{hash, bytes, deduplicated}` (`src/mcp/tools/attachment.ts:17`).
- **Erros:** `INVALID_INPUT` (`bad-args`, `bad-extension`, `lone-surrogate`, `too-big`, `outside-allowed-root`, `inside-data-dir`, `not-found`, `not-regular`, `invalid-utf8`), `ATTACHMENT_CORRUPTED`, `IO_ERROR` (`src/ports.ts:154-168`).

### 8.7 `read_attachment` (leitura)

- **Entrada:** `{project, hash, offset?, maxChars?(1..24000)}` (`src/mcp/tools/attachment.ts:20-25`).
- **Saída:** `{text, next?, status: "ok"}` (`src/mcp/tools/attachment.ts:27-31`).
- **Erros:** `ATTACHMENT_NOT_FOUND`, `ATTACHMENT_CORRUPTED`, `INVALID_INPUT` (`out-of-range`, `mid-surrogate-pair`, `invalid-hash`), `IO_ERROR`.

### 8.8 `query` (leitura)

- **Entrada:** `{project, process?, scope?, includeNonCurrent?, type?, targetPrefix?, where?, text?, ids?, fields?, relatedTo?, relations?(counts|full|none), limit?, cursor?, changesSince?}` (`src/mcp/tools/query.ts:76-99`).
- **Saída:** `{records, cursor?, marker, changes?: {entered, left: [{id, reason}], marker, omitted?}, hint?}` (`src/mcp/tools/query.ts:129-159`). Cada registro: `id`, `type`, `at`, `target`, `author`, `data?` (recortado por `fields`), relações no modo pedido, `needsReview?`, `attachmentStatus?` (`src/queries/query-service.ts:107-115`, `:511-522`).
- **Erros:** `INVALID_FILTER` (`/process` obrigatório no alcance processo, `/limit`, `/text` `too-long` ou `no-terms`), `INVALID_CURSOR`, `MARKER_NOT_FOUND`, `PROJECT_NOT_FOUND`, `PROCESS_NOT_FOUND`, `PROCESS_CORRUPTED`, `PROCESS_TOO_LARGE`, `IO_ERROR` (`src/queries/query-service.ts:200-205`).
- **Invariantes:**
  - Ordem de saída: alcance processo por `seq`; alcance projeto por (`at`, processo, `seq`); com `text`, por relevância e o mesmo desempate (`src/queries/select.ts:33-45`, `:93-124`).
  - Padrão `includeNonCurrent: false` (só vigentes) (`src/queries/select.ts:103-108`).
  - Página: `limit` registros e, na tool, até 24.000 caracteres; o 1º registro sai inteiro mesmo acima do teto (`src/queries/query-service.ts:524-533`, `src/mcp/tools/query.ts:302-312`).
  - Cursor `base64url(JSON)` sem autenticação; prende alcance, projeto, processo, hash dos filtros e `changesSince`, marcador e hash de cabeça de cada processo e `lastId`; a barreira contra forja é a releitura (`src/queries/cursor.ts:13-55`, `src/queries/query-service.ts:328-372`).
  - `changes` só na 1ª página e só com `changesSince`; `limit: 0` só nesse caso (`src/queries/query-service.ts:274-282`, `:548-551`).
  - `relations` padrão `counts` na tool, `full` no serviço (`src/mcp/tools/query.ts:308`).
  - `hint` quando `targetPrefix` quase casou sem fronteira de `.` (`src/queries/select.ts:126-142`).
  - `text`: MiniSearch com `AND`, prefixo e `fuzzy` 0.1 sobre `type`, `target` e strings de `data`, com fallback `OR` (`src/adapters/search.ts:23-24`, `:60`).

### 8.9 `evaluate_gate` (leitura)

- **Entrada:** `{project, process, gate, target?, targets?(1..50), marker?, evidence?}` (`src/mcp/tools/query.ts:101-110`).
- **Saída:** `{passed, questions?: [{index, kind, passed, counts?, evidence?, omitted?}], results?: [{target, passed, questions}], marker}` (`src/mcp/tools/query.ts:161-184`).
- **Erros:** `INVALID_INPUT` (`with-target`, `full-with-targets`), `PROCESS_NOT_FOUND`, `GATE_NOT_FOUND`, `MARKER_NOT_FOUND`, `PROCESS_CORRUPTED`, `PROCESS_TOO_LARGE`, `IO_ERROR` (`src/queries/query-service.ts:216-223`).

### 8.10 `verify_chain` (leitura)

- **Entrada:** `{project, process, expectedHead?}` (`src/mcp/tools/query.ts:112-116`).
- **Saída:** `{ok, totalRecords, head, breaks: [{index, reason}], totalBreaks, attachmentBreaks: [{id, hash, reason}], totalAttachmentBreaks, repairedLines}` (`src/mcp/tools/query.ts:186-196`).
- **Erros:** `PROCESS_NOT_FOUND`, `PROCESS_CORRUPTED` (só `unreadable-manifest`), `PROCESS_TOO_LARGE`, `IO_ERROR`. Quebra da cadeia não é erro.

### 8.11 `list` (leitura)

- **Entrada:** `{project?, process?}` (`src/mcp/tools/query.ts:118`).
- **Saída:** sem `project`, `projects: [{name, processes}]`; com `project`, `project: {name, processes: [{name, createdAt}], types, relations, gates}` (cada definição com `version` e `versions`); com `project` e `process`, `process: {name, createdAt, lineageInheritance?, pinned, hashes}` (`src/queries/list.ts:13-29`, `:37-76`).
- **Erros:** `INVALID_INPUT` (`process` sem `project`), `PROJECT_NOT_FOUND`, `PROCESS_NOT_FOUND`, `PROCESS_CORRUPTED`, `IO_ERROR`.

### 8.12 `describe_type` (leitura)

- **Entrada:** `{project, type, process?, version?}` (`src/mcp/tools/query.ts:121-126`).
- **Saída:** `{name, version?, schema}`; com `process`, o tipo fixado e sem `version` (`src/queries/describe-type.ts:17-23`).
- **Erros:** `INVALID_INPUT` (`process-with-version`, `invalid-version`), `PROCESS_NOT_FOUND`, `TYPE_NOT_PINNED`, `PROJECT_NOT_FOUND`, `TYPE_NOT_FOUND`, `PROCESS_CORRUPTED`, `INTERNAL`, `IO_ERROR` (`src/queries/query-service.ts:252-262`).

Observação: não existe tool `timeline`; o alcance projeto da `query` a substitui (`src/AGENTS.md`, linha de `mcp/tools/query.ts`). Não existe tool de escrita destrutiva: nada reescreve nem apaga registro, definição ou anexo (`docs/directives/invariantes.md:13`).

---

## 9. Persistência

### 9.1 Layout em disco

`<D>` = `$XDG_DATA_HOME/hexlog` se a variável é absoluta, senão `~/.local/share/hexlog` (`src/directory.ts:5-16`). Tudo do dado 1.0 mora em `<D>/.v1/` (`src/adapters/fs/data-format.ts:22-25`):

```text
<D>/.v1/<projeto>/
  types/<nome>/<major>.<minor>.json        # versão imutável de tipo
  relations/<nome>/<major>.<minor>.json    # versão imutável de nome de relação
  gates/<nome>/<major>.<minor>.json        # versão imutável de gate
  attachments/<sha256>                     # blob imutável
  <processo>/
    process.json                           # manifesto (âncora)
    records.jsonl                          # uma linha por lote
    records.jsonl.lock/holder              # lock transitório
```

Fontes: `src/adapters/fs/data-format.ts:27-75`, `docs/dados.md:13-23`. Modos: diretório `0700`, arquivos `0600` (`src/adapters/fs/process-store.ts:93`, `:209-211`).

### 9.2 Escrita

- **Log:** abre em modo `'a'`, escreve a linha do lote, um `fsync` por lote, fecha; replay sem linha também dá `fsync` (`src/adapters/fs/process-store.ts:88-100`). Teto de 64 MiB conferido com `stat` antes de ler e antes de gravar (`src/adapters/fs/process-store.ts:55-67`, `:157-159`).
- **Manifesto, versões de definição e blobs:** `writeFileAtomic` (temporário, `fsync`, depois `link` exclusivo ou `rename`, e `fsync` do diretório) (`src/adapters/fs/atomic.ts:29-62`). O `link` exclusivo é o que torna "versão nunca sobrescrita" e "blob nunca sobrescrito" garantias do sistema de arquivos (`src/adapters/fs/definition-store.ts:99-113`, `src/adapters/fs/attachment-store.ts:205-217`).
- **Criação de processo:** cria o diretório, o `records.jsonl` vazio e só então o manifesto exclusivo; `EEXIST` = já existia (`src/adapters/fs/process-store.ts:200-224`).

### 9.3 Locking e concorrência

- **Um lock por processo**, só da origem; o `register` A→B não espera o lock de B (`src/commands/register/state.ts:51`, `test/AGENTS.md` linha de `register.concurrency.spec.ts`).
- **Publicação:** diretório temporário com `holder` (`{pid, token, bootId}`) gravado com `fsync`, e `rename` para `records.jsonl.lock`, que só vence se o destino não existe (`src/adapters/fs/lock.ts:115-133`).
- **Órfão:** dono de outro boot (`/proc/sys/kernel/random/boot_id`), ou pid morto (zumbi conta como morto), ou pid deste processo com token que ele não segura; órfão é movido de lado e roubado; dono vivo nunca (`src/adapters/fs/lock.ts:91-113`, `:224-230`).
- **Espera:** polling de 10 ms até 15 s, depois `LOCK_TIMEOUT` `lock-busy` com o pid do dono (`src/adapters/fs/lock.ts:12-15`, `:200-238`). `holder` ilegível: `holder-unreadable`, não retentável, com instrução de destravamento manual (`src/adapters/fs/lock.ts:207-216`).
- **Antes de gravar:** `confirm` reconfere o token; sem posse, `lock-lost` sem gravar (`src/adapters/fs/lock.ts:250-255`, `src/adapters/fs/process-store.ts:160`).
- **Seção crítica:** ler o log, `decide` (síncrono) e gravar; só o `await confirm` entra no meio (`docs/directives/invariantes.md:16`). Premissa: todos os escritores no mesmo namespace de pid (`src/adapters/fs/lock.ts:101-108`).
- **Só Linux:** hard link, `fsync` de diretório e `/proc` são pressupostos (`src/adapters/fs/atomic.ts:34-38`).
- **Custo conhecido:** o lock da origem fica preso durante a leitura de cada processo-destino de relação cruzada (`src/commands/process.ts:182-191`).

### 9.4 Leitura e cache

Toda leitura lê o log inteiro e verifica a cadeia (`src/shared/loader.ts:259-266`). O único estado em memória é o índice de busca: MiniSearch por processo, validado por contagem e impressão do último registro, LRU de 24 milhões de caracteres; alcance projeto nunca é guardado (`src/adapters/search.ts:51`, `:100-130`; `docs/tetos-dominio-v1.md:118-121`).

### 9.5 Dado legado 0.x

Qualquer entrada de `<D>` que casa o padrão de nome (fora `.v1` e `archive`) é dado 0.x; toda tool responde `LEGACY_DATA` até `scripts/install.ts --archive-0x` arquivá-lo num `.tar` (`src/adapters/fs/data-format.ts:77-85`, `src/errors.ts:111-119`, `src/archive.ts`).

---

## 10. Códigos de erro

`ErrorCode` tem 25 membros (`src/errors.ts:24-49`). Todo erro é `HexlogError {code, message, details[]}`; cada `Detail` tem `path` (JSON Pointer), `code`, `message` e extras opcionais `current`, `process`, `pid`, `versions`, `kind` (`src/errors.ts:12-21`). `details` é deduplicado e cortado em 50 (`src/errors.ts:151-177`).

| Código | Quando | Onde é lançado |
|---|---|---|
| `INVALID_INPUT` | entrada fora do schema da tool, `__proto__`, aninhamento, nome inválido, forma do lote, regras de `attach`/`read_attachment`/`list`/`describe_type`/`targets` | `src/mcp/kernel.ts:204-210`, `src/errors.ts:91-94`, `src/adapters/fs/io.ts:26-27`, `src/commands/register/static.ts:36-63`, `src/commands/attachment.ts:36-66` |
| `INVALID_FILTER` | `process` ausente no alcance processo, `limit`, `text` longo ou sem termo | `src/queries/query-service.ts:265-298` |
| `RESERVED_NAME` | processo com nome reservado | `src/errors.ts:106-109`, `src/commands/process.ts:124-126`, `src/adapters/fs/process-store.ts:203` |
| `INVALID_SCHEMA` | schema de tipo inválido, `pattern-not-allowed`, raiz não objeto | `src/commands/definition.ts:82-115`, `src/errors.ts:452-460` |
| `PROJECT_NOT_FOUND` | projeto inexistente no alcance projeto, `list`, `describe_type` | `src/errors.ts:342-345`, `src/queries/read.ts:46`, `src/queries/list.ts:48` |
| `PROCESS_NOT_FOUND` | manifesto ausente | `src/errors.ts:280-285`, `src/adapters/fs/process-store.ts:129`, `:147` |
| `TYPE_NOT_FOUND` | definição de tipo sem versão; projeto sem definição no `create_process` | `src/errors.ts:305-320`, `:347-350`, `:442-450` |
| `PROCESS_CORRUPTED` | manifesto ilegível (`unreadable-manifest`) ou cadeia quebrada (`broken-chain`) | `src/errors.ts:96-104`, `:287-296`; `src/queries/read.ts:108`; `src/commands/register/state.ts:305` |
| `PROCESS_TOO_LARGE` | log acima de 64 MiB, na leitura ou no lote que cruzaria o teto | `src/errors.ts:298-303`, `src/adapters/fs/process-store.ts:62-65`, `:157-159` |
| `ATTACHMENT_NOT_FOUND` | blob ausente | `src/errors.ts:253-256`, `src/commands/register/attachments.ts:50`, `src/adapters/fs/attachment-store.ts:266` |
| `ATTACHMENT_CORRUPTED` | blob que não bate com o hash | `src/errors.ts:258-266`, `src/commands/register/attachments.ts:51`, `src/adapters/fs/attachment-store.ts:216`, `:269` |
| `TYPE_NOT_PINNED` | tipo fora do manifesto | `src/errors.ts:367-375`, `src/commands/register/static.ts:70-75`, `src/queries/describe-type.ts:37` |
| `LOCK_TIMEOUT` | `lock-busy`, `lock-lost`, `holder-unreadable` | `src/errors.ts:244-251`, `src/adapters/fs/lock.ts:194-197`, `:207-216`, `:253` |
| `IO_ERROR` | errno do fs, só o código em minúsculas | `src/errors.ts:239-242`, `src/adapters/fs/io.ts:9-24` |
| `INTERNAL` | exceção inesperada, definição ilegível ou inválida, escrita exclusiva esgotada, linha montada inválida | `src/errors.ts:86-89`, `:322-340`, `:467-475`; `src/mcp/kernel.ts:115-124`; `src/commands/register/state.ts:325-327` |
| `BREAKING_CHANGE` | mudança de tipo/relação que quebra sem `breaking: true` | `src/errors.ts:462-465`, `src/commands/definition.ts:144` |
| `INVALID_RECORD` | violação de schema ou de regra de relação (exceto `not-current`), `unmarked-attachment` | `src/errors.ts:224-227`, `src/commands/register/static.ts:154`, `src/commands/register/errors.ts:33-39`, `src/commands/register/attachments.ts:53` |
| `GATE_NOT_FOUND` | gate não fixado no processo; versão de gate inexistente na leitura de definição | `src/errors.ts:352-360`, `src/queries/query-service.ts:464`, `src/adapters/fs/definition-store.ts:35-39` |
| `IDEMPOTENCY_CONFLICT` | mesma `key` com outra impressão | `src/errors.ts:432-440`, `src/commands/register/state.ts:101-104` |
| `LEGACY_DATA` | dado 0.x em `<D>` | `src/errors.ts:114-119`, `src/mcp/kernel.ts:204` |
| `INVALID_CURSOR` | cursor longo, malformado, de outra consulta ou de outro conteúdo | `src/errors.ts:395-414`, `src/queries/cursor.ts:46-55`, `src/queries/query-service.ts:340-380` |
| `MARKER_NOT_FOUND` | id do marcador ausente, entrada faltando ou sobrando, processo inexistente | `src/errors.ts:377-393`, `src/shared/loader.ts:182`, `src/queries/read.ts:34-51` |
| `FORK_REJECTED` | `supersedes`/`revokes` sobre destino não vigente (`not-current`) | `src/commands/register/errors.ts:36-37` |
| `CYCLE_REJECTED` | lote fecharia ciclo de `supersedes` | `src/errors.ts:427-430`, `src/commands/register/state.ts:321` |
| `RELATION_NOT_FOUND` | destino de relação ausente ou corrompido; versão de nome de relação inexistente | `src/errors.ts:416-425`, `src/commands/register/state.ts:187-253`, `src/adapters/fs/definition-store.ts:35-39` |

Códigos de `details` que não são `ErrorCode`: os onze `RuleCode` (`src/domain/relations.ts:192-203`), `too-many-errors` e `violations-capped` (`src/errors.ts:131-147`), `reserved-key` e `too-deep` (`src/mcp/kernel.ts:148-162`), os de lock, anexo, cursor e marcador listados acima.

---

## 11. Testes e convenções

### 11.1 Como o projeto testa

- **Ferramentas:** Jest 30 com ts-jest em CJS (`package.json:26-77`), `fast-check` para propriedades, Stryker para mutação (fora do CI), piso de cobertura de 90% em `src/**` (`package.json:51-58`, `docs/directives/qualidade-e-testes.md:10`, `:15`).
- **Níveis:**
  - Unitário de domínio puro (`test/domain/*.spec.ts`), com ids determinísticos `recordId(process, n)` (`test/AGENTS.md:12`).
  - Propriedade: vigência (`test/domain/vigency.property.spec.ts`), herança (`test/domain/inheritance.property.spec.ts`), `expectedHead` (`test/shared/loader.property.spec.ts`), busca e paginação (`docs/directives/qualidade-e-testes.md:49`).
  - Serviço sobre fakes (`test/commands/fakes.ts`) ou sobre os adaptadores de disco num diretório temporário (`test/queries/query-setup.ts`, com `tamper` e `tamperLine` para adulterar o log).
  - Integração MCP: `test/mcp/environment.ts#createEnvironment` sobe o servidor real sobre `compose` com transporte em memória; nunca se mocka o servidor (`docs/directives/qualidade-e-testes.md:43`).
  - e2e caixa-preta: `test/stdio.e2e.spec.ts` constrói o bundle real e fala JSON-RPC por stdio com `StdioClientTransport` (`test/stdio.e2e.spec.ts:1-46`, `:298-514`).
  - Concorrência real em processos filhos: `test/commands/register.concurrency.spec.ts`, `test/adapters/lock.spec.ts` (fixtures `lock-holder.ts`, `crash-writer.ts`).
  - Orçamento de tempo (`*.budget.spec.ts`, só em `npm run test:budget`): carregar 5.000 registros ≤ 500 ms, `query` ≤ 500 ms, herança de 8.000 versões ≤ 1 s, entre outros (`docs/directives/qualidade-e-testes.md:34-36`).
- **Nomes:** `it('should …')`, AAA, `describe` pelo alvo; spec unitário tem o nome do arquivo de produção, e `test/spec-naming.spec.ts` reprova arquivo sem spec fora de uma lista de exceções (`docs/directives/qualidade-e-testes.md:40-42`). Sem rótulos de ID em títulos (`:50`).
- **Fixtures relevantes:** `test/fixtures/records-corpus.ts` (corpus de registros), `test/fixtures/chain-line.ts` (linha de cadeia montada à mão), `test/fixtures/domains/omc.ts` e `rdsc.ts` (fluxos de exemplo só por definições), `test/fixtures/legacy-0x/` (dado 0.x).
- **Coerência de documentação:** `test/directives.spec.ts`, `test/skill-coherence.spec.ts`, `test/skill-model.spec.ts` conferem links, contagens e modelos das skills contra o código (`docs/directives/qualidade-e-testes.md:20`).

### 11.2 O que serve de suíte de aceitação do port

Os specs são TypeScript e acoplados aos módulos, então não rodam contra um binário Rust como estão. O que se aproveita:

| Fonte | Uso no port |
|---|---|
| `test/stdio.e2e.spec.ts` | O único spec caixa-preta. Se o binário Rust mantiver nomes e formas das tools, o harness (cliente MCP stdio sobre um executável) roda trocando `command`/`args` (`test/stdio.e2e.spec.ts:38-46`). |
| `test/domain/vigency.property.spec.ts`, `test/domain/inheritance.property.spec.ts`, `test/domain/relation-graph.ts` | Propriedades a reescrever com `proptest`: independência da ordem, no máximo um vigente por linhagem, herança só de pontas vigentes, `contradicts` corta. O oráculo `lineages` é portável. |
| `test/shared/loader.spec.ts`, `test/shared/loader.property.spec.ts`, `test/queries/tamper.spec.ts`, `test/queries/verify-chain.spec.ts` | Casos de adulteração, linha rasgada, cauda apagada e `expectedHead`: viram testes do critério 3 do plano v2. |
| `test/commands/register.spec.ts`, `test/commands/register-state.spec.ts` | Matriz de recusa por nível e por regra de relação: tabela de casos a portar (entrada → código esperado). |
| `test/commands/register.concurrency.spec.ts` | Semântica a preservar sob SQLite: dois `supersedes` ao mesmo destino → um aceito e um `FORK_REJECTED`; mesma `key` em paralelo → uma gravada e um `replayed`. |
| `test/domain/gate.spec.ts`, `test/queries/gate.spec.ts` | Só se o motor v2 mantiver gates avaliáveis; o plano v2 tira avaliação da v1. |
| `test/fixtures/records-corpus.ts` | Corpus para orçamentos de desempenho comparáveis. |

---

## 12. Motor genérico versus fluxo de desenvolvimento

### 12.1 Genérico (nenhuma semântica de domínio de desenvolvimento)

- Ids, nomes, target, marcador (`src/domain/ids.ts:1-65`).
- Registro, lote, alias, idempotência, cadeia, verificação (`src/domain/record.ts`, `src/domain/chain.ts`, `src/shared/loader.ts`).
- Tipos de registro como JSON Schema definido por dados (`src/domain/definitions.ts:17-23`, `src/commands/definition.ts:82-115`).
- Nomes de relação definidos por dados, com restrição de tipo por ponta (`src/domain/definitions.ts:25-39`).
- Versionamento imutável de definições e fixação no processo (`src/commands/definition.ts`, `src/commands/process.ts:147-180`).
- Vigência, linhagem, ciclo (`src/domain/relations.ts:37-123`).
- Leitura por alcance, cursor, `changesSince` (`src/queries/`).
- Persistência, lock, anexos (`src/adapters/fs/`).
- O código de `src/` é travado contra termos de fluxo (`test/no-flow-terms.spec.ts:27-36`).

### 12.2 Genérico, mas com semântica embutida em código

Não citam desenvolvimento, mas fixam um vocabulário epistêmico (prova, contradição, aprovação) que o plano v2 quer como dado:

- **As oito espécies de relação** são enum de código (`src/domain/record.ts:43-52`). O projeto só nomeia; não cria espécie (`docs/directives/adr-0007-dominio.md:32`).
- **Regras por espécie:** sucessão (`src/domain/relations.ts:250-256`), conflitos `supports`/`contradicts` e `supersedes`/`revokes` (`src/domain/relations.ts:268-281`), vigência de `supports` (`src/domain/relations.ts:283-291`).
- **Herança por espécie** (`src/domain/inheritance.ts:14-25`).
- **Apoio morto** (`needsReview`), que só faz sentido com `supports` (`src/domain/relations.ts:125-189`).
- **Perguntas de gate** `approved`, `occurred`, `no_pending`, `no_open_contradiction` (`src/domain/gate.ts:36-58`): `approved` presume "aprovação = apoio sem contradição".
- **`Author` com `agent`/`model`/`client`** (`src/domain/record.ts:58-68`): específico de agente de IA e de MCP.
- **Formato `git-sha`** no catálogo fechado (`src/domain/formats.ts:1-11`): único formato extra, e é de repositório git.
- **`lineageInheritance` sempre ligado** em processo novo (`src/commands/process.ts:155`).

### 12.3 Específico do fluxo de desenvolvimento (fora de `src/`)

| Onde | O que é |
|---|---|
| `.hexlog/types/*.json` | Tipos `decision`, `directive`, `doc`, `evidence`, `finding` (com `severity` URGENT/NORMAL/MINIMAL), `gap`, `opening` (com `mode`, `route`, `branch`, `commit` em `git-sha`), `premise` (com `answeredBy`) |
| `.hexlog/relations/*.json` | `about-gap`, `anchored-in`, `rests-on` (`derivesFrom` a partir de `decision`), `closes-gap`, `fills-gap`, `resolves-finding` (`answers`) |
| `.hexlog/gates/*.json` | `gaps` e `pre-pr` (seção 6.2) |
| `scripts/flow/` | Critic da "cadeira do dono", reancoragem de decisões numa geração nova de diretrizes, sync de `docs/directives/` com o processo de diretrizes, gate e marcador de pré-PR (`scripts/flow/*.ts`, linha 1 de cada) |
| `scripts/seat-report.ts` | Relatório da auditoria da cadeira do dono; conhece `gap`, `premise`, `audit`, `answeredBy` (`docs/directives/fronteiras.md:32`) |
| `.claude/hooks/flow-hooks.ts`, `flow-command.ts` | Bloqueio de abertura de PR sem marcador de pré-PR (`.claude/hooks/flow-hooks.ts:1-12`) |
| `skills/hexlog-flow/`, `skills/hexlog-setup/` | Mecânica da "unidade de trabalho", modelo declarativo de fases, pacote `owner-seat` (tipos `audit`, `evidence`, `gap`, `premise`; gate `gaps`) (`skills/hexlog-setup/references/owner-seat/`) |
| `docs/directives/fluxo-hexlog.md`, `cadeira-do-dono.md`, `estrategia.md` | Regras do fluxo deste repositório |
| `test/fixtures/domains/omc.ts`, `rdsc.ts` | Fluxos de exemplo (plano pronto do OMC, prova vencida do rdsc) |
| `hook/bash-guard.ts`, `src/guard.ts`, `src/installation.ts` | Integração com o Claude Code (`settings.json`, PreToolUse, `claude mcp add`) |
| `src/errors.ts:112` | Mensagem de `LEGACY_DATA` cita `node scripts/install.ts --archive-0x` |

---

## 13. Portar, mudar ou descartar

Legenda: **portar** = levar o comportamento quase igual; **mudar** = levar a ideia com outro desenho; **descartar** = não entra na v1 do motor.

| Item | Decisão | Justificativa (seção) |
|---|---|---|
| Cadeia sha256 + JCS, âncora no manifesto, `seq` contíguo | portar | Atende o critério 3 do plano v2 e é independente de armazenamento (4.1). Em SQLite, o elo vira linha de tabela com `seq`, `prev_hash`, `hash`; o hash continua sobre o JCS do conteúdo, não sobre bytes do arquivo. |
| `verify_chain` com `expectedHead` | portar | Detecta adulteração e truncamento de cauda (4.3). |
| Linhas rasgadas e `repairedLines` | descartar | Artefato do JSONL; transação do SQLite não deixa linha parcial (4.2). |
| Lote atômico de 1 a 50, `@alias`, `key` + `fingerprint` | portar | Semântica de escrita segura para agente; `key` resolve `IO_ERROR` incerto (3.8, 3.9). Em SQLite, uma transação por lote. |
| `RecordId` `<process>:<uuidv7>` | mudar | O plano v2 quer id semântico estável do nó e id técnico por revisão (plano, seção 1, item 6). O `RecordId` atual corresponde ao id técnico da revisão. |
| `target` separado do id | mudar | Linhagem exige mesmo tipo e target (3.1, 3.5); com id semântico, a linhagem passa a ser "revisões do mesmo id", e o `target` pode virar o próprio id ou um atributo. Ponto aberto do plano (seção 5). |
| Linhagem por `supersedes` | mudar | É o equivalente atual de "revisões de um nó"; a raiz da linhagem (`lineageRootOf`, `src/domain/inheritance.ts:56-80`) é o que o plano chama de `externalId` para a primeira revisão. Em v2 a revisão pode ser implícita (gravar o mesmo id), e o `supersedes` explícito sai. |
| `supersedes`/`revokes` só no mesmo processo | mudar | Conflita com "projeto sobrepõe nó nativo pelo mesmo id" se base e projeto forem cadeias diferentes (3.5, `docs/directives/adr-0007-dominio.md:38`). O desenho da sobreposição precisa decidir se a base é um processo do mesmo armazenamento ou uma camada de leitura. |
| `FORK_REJECTED` (no máximo um vigente por linhagem) | portar | A regra continua necessária com revisões por id: gravar revisão sobre uma que não é a vigente deve recusar (3.5). Sob SQLite, a checagem roda dentro da transação. |
| Oito espécies de relação fixas em código | mudar | O plano quer relações como dados (seção 1, item 1). Proposta: manter em código só o que tem semântica de motor (revisão/revogação) e tornar o resto dados (3.3, 12.2). |
| Nomes de relação com `from`/`to` por tipo | portar | Já é dado e já é agnóstico (3.3). |
| Herança pela linhagem, `needsReview` | descartar (v1) | Semântica de prova específica de `supports`/`contradicts`; nada no escopo v1 do plano a pede (3.6, 3.7). |
| Tipos como JSON Schema, ajv estrito, sem `pattern` | portar | Tipos definidos por dados (critério 7). Em Rust, crate de JSON Schema 2020-12; manter a recusa de `pattern` evita ReDoS (5.5). |
| Versionamento `major.minor` de definição e `BREAKING_CHANGE` | mudar | Útil, mas a classificação de mudança de tipo é heurística com limites declarados (`src/domain/definitions.ts:99-101`). Decidir se a v1 precisa dela ou se uma definição é só mais um nó revisado. |
| Fixação de definições no manifesto do processo | mudar | Garante validação estável por processo (5.3), mas torna o processo imutável a mudanças de tipo. Com base nativa sobreposta por projeto, a fixação precisa ser repensada. |
| Gates avaliáveis (`evaluate_gate`, quatro perguntas) | descartar (v1) | O plano v2 tira avaliação e bloqueio da v1; "gate" no v2 é nó de texto de instrução (seção 6). Guardar o desenho para o marco futuro de execução de gates. Atenção à colisão de nome. |
| Anexos endereçados por hash, `format: attachment` | mudar | Útil para texto longo de instrução, mas a v1 do plano guarda só texto de instrução; avaliar se cabe em coluna `TEXT` do nó (16.000 caracteres atuais) antes de portar blobs (7). |
| `query` (filtros, alcance, `fields`, `relations`) | mudar | Portar filtros `type`, `targetPrefix`, `where`, `ids`, `relatedTo`, `includeNonCurrent`; trocar o motor por SQL (8.8). |
| `text` por MiniSearch com cache LRU | mudar | SQLite FTS5 substitui o índice em memória e o cache (9.4). |
| Cursor opaco preso ao conteúdo, `changesSince` | mudar | Com SQLite, `seq`/rowid dão paginação estável; manter "cursor recusa se o conteúdo lido mudou" se o requisito continuar (8.8). |
| Marcador por processo | mudar | Com um só armazenamento, um marcador global (último `seq`) pode bastar (4.4). |
| Lock por diretório, pid, `bootId` | descartar | SQLite resolve exclusão com transação (`BEGIN IMMEDIATE`) e `busy_timeout` (9.3). |
| Escrita atômica por `link`, `fsync` manual | descartar | Coberto pelo journaling do SQLite (9.2). |
| Teto de 64 MiB por processo | mudar | Era defesa contra ler o arquivo inteiro a cada chamada; com SQLite a leitura é indexada. Rever os tetos por registro (`data`, relações) e manter os de entrada (3.10). |
| Envelope MCP: `execute`, `__proto__`, profundidade, erro estruturado | portar | Contrato de erro `{code, message, details[]}` com JSON Pointer é bom para agente (8, 10). Em Rust, `serde` não tem o problema de `__proto__`, mas o teto de profundidade continua útil. |
| Catálogo de erros | mudar | Portar o formato e os códigos que sobrevivem; descartar `LEGACY_DATA`, `LOCK_TIMEOUT` (vira `busy`), `PROCESS_TOO_LARGE` (10). |
| Separação serviço de escrita / serviço de leitura com portas | portar | Encaixa direto na Facade do plano (seção 1, item 8): CLI e MCP chamam os mesmos serviços, como `server.ts` e `composeReader` já fazem (2.1). |
| `decide` síncrono sob exclusão, adaptador só de bytes | portar | Mesmo padrão com transação SQLite: o motor decide, o store grava (2.1). |
| `Author` (`agent`, `model`, `client`) | mudar | Útil na auditoria, mas acoplado a MCP; com CLI, `client` precisa de outra origem (12.2). |
| Telemetria OpenTelemetry | descartar (v1) | Observabilidade está fora da v1 do plano (seção 2). |
| Dado 0.x, `archive.ts`, `LEGACY_DATA` | descartar | Migração de formato antigo (9.5). |
| `installation.ts`, `guard.ts`, `hook/bash-guard.ts` | descartar | Integração específica do Claude Code e do layout em disco; a v2 tem hook de `SessionStart` próprio (12.3). |
| `scripts/flow/`, `.hexlog/`, skills de fluxo | descartar do motor | São conteúdo opinativo; candidatos a virar grafos do plugin (decisões, gates) (12.3). Os tipos `decision`, `premise`, `gap` são referência para o grafo de decisões. |
| Trava `no-flow-terms` e travas de fronteira | portar | Vale reproduzir como teste de arquitetura no crate do motor (critério 7) (2.2). |

---

## 14. Lacunas e dúvidas

1. **"Fork" na descrição do `register`.** A descrição diz que um id vigente errado "do mesmo tipo e target é aceito e bifurca a linhagem" (`src/mcp/tools/process.ts:67-68`), mas o código recusa toda escrita que deixaria dois vigentes numa linhagem (`src/domain/relations.ts:283-291`). Minha leitura: superar o vigente de **outra** linhagem do mesmo tipo e target une linhagens ou ramifica a errada, sem recusa. Não encontrei teste que confirme; ficou sem verificar.
2. **Um registro pode superar dois destinos.** Nada impede `supersedes` para dois registros vigentes no mesmo item (as regras de conflito só cruzam `supersedes` com `revokes`, `src/domain/relations.ts:268-274`). Não verifiquei se é intencional (fusão de linhagens) nem como `currentOf` e a herança se comportam nesse caso além do comentário de determinismo (`src/domain/relations.ts:45-49`).
3. **`answers` "entendida" ou não.** O item 2 do ADR 0007 diz que o motor entende `answers`; a emenda de 2026-10-09 diz que não há regra em código (`docs/directives/adr-0007-dominio.md:32`, `:87`). O relatório segue o código.
4. **Não li por inteiro:** `src/adapters/validator.ts` (só trechos e o `AGENTS.md`), `src/adapters/search.ts` (trechos), `src/adapters/fs/attachment-store.ts` (por grep), `src/adapters/telemetry/*`, `src/installation.ts`, `src/guard.ts`, `src/archive.ts`, `hook/bash-guard.ts`, `scripts/**`, `skills/**` (só frontmatter e o pacote `owner-seat` por listagem), ADRs 0008 a 0015, `docs/tools.md`, `docs/uso.md`, `docs/dados.md` (só layout) e o corpo dos specs (só títulos e o `test/AGENTS.md`). As afirmações sobre esses arquivos vêm de trechos lidos ou da documentação do próprio repositório.
5. **Não rodei nada:** nem testes, nem o servidor, nem medições. Todos os números de desempenho são os documentados (`docs/tetos-dominio-v1.md`, `docs/directives/qualidade-e-testes.md`).
6. **Projeto sem tool de criação (confirmado).** O projeto passa a existir com a primeira definição ou anexo gravado: `writeFileAtomic` cria os diretórios pais com `mkdirSync(dir, { recursive: true, mode: 0o700 })` (`src/adapters/fs/atomic.ts:46`). Não existe tool para criar, renomear nem apagar projeto.
7. **Uso real das espécies `complements` e `reopens`.** Nenhum tipo ou nome de relação de `.hexlog/` as usa; não sei se algum fluxo real depende delas.
8. **Ordem por `at` no alcance projeto.** `at` vem do relógio do servidor; com relógio fora de ordem entre sessões, a ordem do alcance projeto pode não refletir causalidade (`src/queries/select.ts:33-45`). Não encontrei tratamento para isso.

---

## 15. Índice de arquivos

### `src/`

| Arquivo | Responsabilidade |
|---|---|
| `src/server.ts` | Entrada do bundle: compõe, sobe o servidor stdio, liga telemetria e encerramento |
| `src/compose.ts` | Raiz de composição; `compose` (leitura e escrita) e `composeReader` (só leitura) |
| `src/directory.ts` | Resolve `<D>` por `XDG_DATA_HOME` |
| `src/version.ts` | Versão do servidor |
| `src/errors.ts` | `ErrorCode`, `HexlogError`, `Detail` e todas as fábricas de erro |
| `src/ports.ts` | Portas de processo, definição, anexo, validador, busca e telemetria |
| `src/domain/ids.ts` | `Name`, `Hash`, `Instant`, `Target`, `RecordId`, `Marker`, nomes reservados |
| `src/domain/record.ts` | `HexRecord`, `Author`, `Relation`, `RelationKind`, `BatchItem`, tetos |
| `src/domain/chain.ts` | `Link`, `Batch`, `hashLink`, `anchor`, `fingerprint`, `isValidLink` |
| `src/domain/manifest.ts` | Schema do `process.json` |
| `src/domain/definitions.ts` | `RecordType`, `RelationName`, `Gate`, semver, classificação de mudança, campos de anexo |
| `src/domain/relations.ts` | Vigência, ciclo, apoio morto, regras de relação |
| `src/domain/inheritance.ts` | Herança de relações pela linhagem |
| `src/domain/gate.ts` | Perguntas de gate, seletores, avaliação, resumo de evidência |
| `src/domain/schema-walk.ts` | Percurso de JSON Schema, custo estimado, remoção de `pattern` |
| `src/domain/formats.ts` | Catálogo fechado de formatos (`git-sha`) |
| `src/shared/loader.ts` | Parse e verificação do log, corte por marcador, `expectedHead` |
| `src/shared/latest.ts` | Versão vigente de cada definição |
| `src/shared/pages.ts` | Corte de texto sem partir surrogate |
| `src/shared/logger.ts` | Tipo `Logger` |
| `src/commands/definition.ts` | `defineType`, `defineRelation`, `defineGate` |
| `src/commands/process.ts` | `createProcess`, `register` |
| `src/commands/attachment.ts` | `attach` e suas recusas de entrada |
| `src/commands/register/static.ts` | Recusas do `register` antes do lock |
| `src/commands/register/state.ts` | `decide` do `register` sob o lock |
| `src/commands/register/attachments.ts` | Conferência de anexos citados |
| `src/commands/register/errors.ts` | Mensagens e recusa das regras de relação |
| `src/queries/query-service.ts` | `QueryService` (seis operações) |
| `src/queries/read.ts` | Leitura por alcance e marcador |
| `src/queries/select.ts` | Ordem, filtros, relações de saída, `hint`, motivo de saída |
| `src/queries/cursor.ts` | Cursor opaco |
| `src/queries/list.ts` | `list` |
| `src/queries/describe-type.ts` | `describeType` |
| `src/queries/read-attachment.ts` | `readAttachment`, `PAGE_CHARS_CAP` |
| `src/adapters/fs/data-format.ts` | Nomes de pasta e arquivo do dado 1.0, detecção de 0.x |
| `src/adapters/fs/io.ts` | `IO_ERROR`, `safeName`, helpers de leitura |
| `src/adapters/fs/atomic.ts` | Escrita atômica e `fsync` |
| `src/adapters/fs/lock.ts` | Lock por processo |
| `src/adapters/fs/process-store.ts` | Store de processo (manifesto, log, escrita sob lock) |
| `src/adapters/fs/definition-store.ts` | Store de versões de definição |
| `src/adapters/fs/attachment-store.ts` | Store de blobs de anexo |
| `src/adapters/validator.ts` | Validador ajv |
| `src/adapters/search.ts` | Índice MiniSearch com cache |
| `src/adapters/telemetry/*.ts` | Telemetria OTLP opcional e `noopTelemetry` |
| `src/mcp/kernel.ts` | `execute`, `defineTool`, `advertise`, anotações |
| `src/mcp/server.ts` | `createServer` com as 12 tools |
| `src/mcp/tools/process.ts` | `create_process`, `register` |
| `src/mcp/tools/definition.ts` | `define_type`, `define_relation`, `define_gate` |
| `src/mcp/tools/attachment.ts` | `attach`, `read_attachment` |
| `src/mcp/tools/query.ts` | `query`, `evaluate_gate`, `verify_chain`, `list`, `describe_type`, cortes do envelope |
| `src/installation.ts` | Instalação versionada do artefato, `TOOLS_COUNT` |
| `src/guard.ts` | Regras de deny e hook no `settings.json` do Claude Code |
| `src/archive.ts` | Arquivamento do dado 0.x |

### Fora de `src/`

| Arquivo ou pasta | Responsabilidade |
|---|---|
| `hook/bash-guard.ts` | Hook PreToolUse que isola `<D>` do Bash do agente |
| `scripts/install.ts`, `scripts/build.ts` | Instalador e build dos bundles |
| `scripts/export.ts`, `timeline.ts`, `insights.ts`, `seat-report.ts` | Relatórios de leitura pelo `composeReader` |
| `scripts/flow/` | Automação do fluxo de desenvolvimento deste repositório |
| `scripts/release-notes*.ts` | Corpo do GitHub Release |
| `skills/hexlog/`, `skills/hexlog-flow/`, `skills/hexlog-setup/` | Skills instaladas |
| `.hexlog/` | Definições do fluxo deste repositório |
| `.claude/` | Hooks, skills e regras do fluxo deste repositório |
| `eslint.boundaries.js`, `eslint.config.js` | Travas de fronteira e lint |
| `docs/directives/` | Diretrizes vivas e ADRs 0007 a 0015 |
| `docs/tools.md`, `docs/uso.md`, `docs/dados.md`, `docs/tetos-dominio-v1.md` | Referência de tools, uso, layout de dados e tetos |
| `test/` | Suíte Jest (119 specs), helpers e fixtures |
| `CONTEXT.md` | Glossário do produto |
