# Deep Interview Spec: oh-my-hexflow (design aprofundado)

## Metadata
- Interview ID: ohf-2026-10-30
- Rounds: 8 (modo `--frontier`), depois de uma entrevista prévia de 38 decisões
- Final Ambiguity Score: ~13.6% (estimativa)
- Type: greenfield
- Generated: 2026-10-30
- Threshold: 0.2
- Threshold Source: default
- Initial Context Summarized: yes (resumo do design anterior)
- Status: PASSED
- Base anterior: [design-decisions.md](../../.ignore/design-decisions.md). Onde houver conflito, **este spec vale**.
- Emendas de 2026-10-31 **já aplicadas neste spec** (decisões do dono após o consenso): [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md), seção 8.1 (a coluna do meio dela é histórica). O que a v1 só **adia** (reabertura por órfão, estados `grounded`/`executed`, tool `ancestors`, sucessão de major, registro `bypass` no Hexlog, checks embutidos `lint`/filhas fechadas/artefato/comentário, "não se aplica" no corpo do PR) não é emenda: está na seção 3.1 do plano (Q15 a Q17), e o spec segue descrevendo o alvo. Em conflito com o plano, vale o plano para o que a 8.1 e a 3.1 listam.

## Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.90 | 0.40 | 0.360 |
| Constraint Clarity | 0.88 | 0.30 | 0.264 |
| Success Criteria | 0.80 | 0.30 | 0.240 |
| **Total Clarity** | | | **0.864** |
| **Ambiguity** | | | **0.136** |

Scores são estimativas do entrevistador, sem rodada de pontuação separada por modelo.

## Topology
| Component | Status | Description | Coverage / Deferral Note |
|-----------|--------|-------------|--------------------------|
| model-hexlog | active | Tipos, relações, processos por unidade, regra de major, `closure` | Coberto: R1 a R3, R6 |
| harness-omhl | active | Binário Rust: hooks, estado local protegido, tokens, `check`, bypass | Coberto: R1 a R4, R7, R8 |
| plugin-surface | active | Comandos e skills | Coberto: R7, R8 |
| directive-projection | active | `docs/directives` projetado pelo hook | Coberto: R2 |
| premise-integrity | active | Modo `self`, `applicableAtRegistration` derivado pelo servidor; juiz Haiku só para o "não se aplica" | Coberto: R5. Cadeira de dono e promoção ficam fora da v1 (emendas 8.1) |
| level-gates | active | Gates de epic, issue e PR | Coberto: R3 a R6 |

Nenhum componente adiado.

## Goal
Entregar, em Rust, um framework de desenvolvimento para projetos pessoais, guiado por **directives** (regras atemporais e imutáveis) e **premises** (afirmações locais a **units** de trabalho recursivas), com o Hexlog como fonte única da verdade. A v1 é uma **fatia vertical mínima** que atravessa as três peças (fork do Hexlog, binário `omhl`, plugin do Claude Code): `open_unit` → `register_premise` → `omhl check` (CI verde) → `close_unit` para a granularidade PR, com o hook `Stop` e o token humano.

## Constraints
- Nomenclatura de tipos, relações, campos, comandos, flags e valores **em inglês**.
- Sem retrocompatibilidade nem migração do `hexflow`; ele fica intocado.
- **Três peças que sobem juntas**, com a mesma major e handshake: (a) **fork do Hexlog** que segue o upstream (diffs mínimos documentados, rebase periódico, melhorias genéricas devolvidas por PR quando forem melhores que a original); (b) binário **`omhl`**; (c) **plugin fino** do Claude Code.
- O fork tem duas camadas: **tools de domínio** que substituem as genéricas para o agente (`open_unit`, `close_unit`, `register_premise` com `correct` e `revoke`, `create_directive`, `annul_directive`, `ancestors`, `applicable_directives`, `record_evidence`, `bootstrap_project`, `version`) e **versões melhores** das tools genéricas. As 12 genéricas seguem por baixo, e um hook impede o agente de chamá-las direto no projeto hexflow.
- O `omhl` **nunca fala com o Hexlog**: só o agente fala MCP. A leitura do grafo (ancestrais, diretivas aplicáveis, projeção) é código do fork.
- Hexlog: um processo por unidade, filhos com a mesma major do pai (a minor pode diferir); um processo único e duradouro de diretivas. Major do pai sobe por processo sucessor. Filhos abertos terminam na major antiga.
- Config do projeto como registros no Hexlog. Estado local protegido em `.omhl/state/` (ignorado pelo git, por worktree).
- Estado local protegido (`.omhl/state/`) **só é escrito por hooks**, nunca por tool do agente. É freio, não barreira, porque o agente tem `Bash`. Exceção: `.omhl/outbox/` e `.omhl/proposals/` são escritos pelo agente (conteúdo por hash ou recibo não confiável).
- **Sem sandbox** (decisão do dono): as garantias são freio, não barreira; envelope HMAC, recibo com MAC e guard servem de evidência de adulteração, e um log de auditoria só-acréscimo mais `omhl report` medem se vale algo mais forte.
- O servidor do fork não expõe as tools genéricas de escrita nem `attach`/`read_attachment`. O `omhl check` fala com o GitHub por cliente Rust, token em variável de ambiente (sem `gh`, sem MCP).
- O servidor Hexlog nunca executa shell.

## Model
- **Unit:** recursiva, um tipo, no máximo um pai. Estados `open` → `grounded` → `executed` → `verified and closed`. Guarda `parent` e a cadeia de ancestrais, a versão da granularidade fixada na abertura e o `integrity mode` (`self` por padrão; indicado a cada abertura, imutável pelo agente, herdado pela subárvore).
- **Granularity:** `epic`, `issue`, `PR`; versionadas por `supersedes`/`revokes`. Mudar a granularidade só afeta unidades abertas depois.
- **Premise:** `origin = human | agent | seat`. Herdada para baixo. Corrige-se por `supersedes` no mesmo target (decisões apoiadas seguem pela herança de linhagem); a premissa herdada é corrigida no processo do ancestral **aberto**, por `--in <ancestral>` no `/premise` e invalida-se por `revokes`, sem cascata automática. Premissa ou anulação de origem `human` só muda com token humano.
- **Directive:** um único nível, imutável (só criar e anular), `extends` como única relação, `kind = mechanical | product`, `appliesTo` com as granularidades. **Criação**: o agente propõe e você avaliza; o aval gera um token **preso ao hash do conteúdo** (regra, `criteria`, granularidades) que o hook injeta em `create_directive`. **Anulação**: só por premissa `human`, com motivo, escopo padrão unidade e subárvore (outro nível: projeto inteiro, com confirmação extra, que é o segundo passo do aval: arma e aprova com o hash completo, gerando uma só asserção; o escopo unidade recusa unidade já fechada).
- **Not applicable:** o agente declara, para diretiva `product`, **somente com o veredito do juiz Haiku** (subagente do plugin, só leitura, que lê o ramo de diretivas); sem token humano. Vale para a unidade e a subárvore; aparece no registro de fechamento e no corpo do PR. O primeiro resultado do juiz por (unidade, diretiva) é final, seja qual for (`not-applicable`, `applicable` ou `inconclusive`). O juiz entra como marco logo após a fatia.
- **Closure:** registro gravado por `close_unit` com o resultado (`outcome`) por diretiva. Lógica **fixa dentro do `close_unit`** (não há gate dinâmico genérico). Exige, por diretiva aplicável: `mechanical` → evidência válida para a versão do sujeito; `product` → decisão ancorada ou "não se aplica"; e nenhum órfão apoiado em premissa revogada. Unidade fechada com órfão reabre.

## Checks (diretivas mechanical)
- O `criteria` é verificável e reproduzível: comando livre cujo retorno é **JSON estruturado**, com contrato mantido junto do `omhl` (`status` pass|fail|error, `subject` com a versão, `details`). Saída fora do contrato vira `error`.
- **Checks embutidos** reutilizáveis entre projetos (testes, lint, CI, filhas fechadas, artefato existe, comentário existe) e **checks do projeto** em `.omhl/checks/`, com o mesmo contrato.
- A ferramenta (jest, pytest, eslint...) é **explícita nos parâmetros da diretiva**; o `omhl detect` só sugere o valor ao criar.
- `omhl check` emite um **recibo** (comando, commit SHA, status, hash da saída). `record_evidence` guarda o recibo; o hook `PreToolUse` confere que ele bate com o do estado protegido.
- **Validade:** a evidência fica presa à versão do sujeito, sem prazo por tempo. No PR, o SHA da cabeça (commit novo invalida). Em issue e epic, os registros de fechamento das filhas.
- **Arquivos de check editados:** instrução ao agente + verificação por git; se foram editados na unidade, o fechamento exige um gate humano (conferir que era trabalho legítimo), registrado no `closure`.
- **Retry:** padrão 0; a diretiva liga o retry; toda tentativa vai no recibo; teto de 3 imposto pelo `omhl`.
- `fail` = reprovou; `error` = timeout, contrato quebrado ou infra. Ambos bloqueiam o fechamento.

## Gates iniciais (carga mínima)
- **PR:** CI verde no commit da cabeça; testes locais passam no mesmo SHA; nenhum comentário de review sem resposta. ("Pelo menos uma revisão" ficou fora.)
- **Issue:** todos os PRs filhos fechados; comentário na issue, posterior ao último PR fechado, que cita cada PR filho com uma seção por PR. A checagem é estrutural; a qualidade do texto fica com a auditoria humana.
- **Epic:** todas as issues respondidas e fechadas; relatório final em `docs/reports/<epic>.md`, gerado a partir de um **template versionado** (`docs/templates/epic-report.md`) preenchido pela projeção dos registros. O check regenera e compara byte a byte o arquivo commitado.

## Harness
- **Hooks:** `PreToolUse` (injeta token humano, bloqueia forja de `origin = human`, edição da projeção, escrita no estado protegido e chamadas diretas às tools genéricas), `PostToolUse` (grava a projeção a partir da resposta de `create_directive`/`annul_directive`, espelha o resultado do fechamento no estado local), `Stop` (vigia **uma unidade ativa por sessão**, bloqueia enquanto o último fechamento não passou, com teto de iterações e liberação em contexto cheio ou interrupção), `SessionStart` (handshake de versão).
- **Projeção:** `docs/directives/*.md`, versionada no PR da unidade, cabeçalho em comentário, só criar/anular disparam. Premissas só por pedido explícito.
- **Falha das peças:** as proteções fecham; o `Stop` é só um freio (aviso com teto de 5 bloqueios), e a garantia fica no servidor (`close_unit`) e no `PreToolUse`. A saída é o **bypass temporário** com token humano e motivo, gravado no estado local e, quando o Hexlog voltar, como registro `bypass` citado no PR. O bypass libera o `Stop` e as negações sobre ferramentas genéricas, nunca o envelope no servidor nem as âncoras do guard. Sem variável de ambiente de escape.
- **Canal autenticado:** envelope HMAC do hook (validade 2 min) em toda tool de domínio, exceto `version`; asserção humana (10 min, uso único por `key = human:<nonce>`) cunhada só no `UserPromptExpansion` por comando do plugin, nunca no `UserPromptSubmit`.

## Plugin
- **Comandos:** `/omhl-init` (bootstrap idempotente: tipos, relações, processo de diretivas, granularidades e diretivas mínimas, hooks, `.omhl/state`), `/open`, `/close`, `/premise` (humana, com `--supersede` e `--revoke`, e `--in <ancestral>` para corrigir premissa herdada), `/directive`, `/annul`, mais o comando de bypass (o "não se aplica" não é comando humano: é uma skill do agente que usa o juiz Haiku, Q2). Só o init leva prefixo, para não colidir com o `/init` do Claude Code.
- **Skills:** trabalhar numa unidade; registrar premissa e decisão; fechar unidade; propor e anular diretivas.
- **Integridade `self`:** o agente passa só o conteúdo da premissa (`kind: decision` incluído) e `applicable_directives`; o servidor **deriva** `applicableAtRegistration`, `target` e as relações (`anchored-in`). A prova de que o agente "olhou" todas deixa de existir; a contradição que escapar fica para a auditoria humana. O humano também pode aprovar uma premissa pré-sugerida pelo agente (`--approve <proposalId>`).

## Non-Goals (v1)
- Modo `judge` completo (o juiz Haiku entra só para o "não se aplica"), integração nativa da cadeira de dono, promoção de premissa a diretiva.
- Migração de `own-envy` e `own-finances`.
- CLI de uso geral para o dia a dia; criar o repositório no GitHub (por enquanto git local). Exceção: `omhl report`, `omhl audit verify` e `omhl watch` (observabilidade).
- Gate dinâmico genérico no Hexlog; retry automático por padrão; variável de ambiente de escape.

## Acceptance Criteria
- [ ] `/omhl-init` roda duas vezes sem estragar nada e deixa tipos, processo de diretivas, três granularidades e as diretivas mínimas no Hexlog.
- [ ] `open_unit` cria o processo com a major do pai e a cadeia de ancestrais; recusa major divergente.
- [ ] `register_premise` com `origin = human` sem asserção humana válida é recusada; o servidor deriva `applicableAtRegistration` das diretivas `product` aplicáveis.
- [ ] `omhl check` devolve recibo no contrato; saída fora do contrato vira `error`; retry 0 por padrão.
- [ ] `record_evidence` recusa recibo que não bate com o estado protegido; evidência de PR perde a validade com commit novo.
- [ ] `close_unit` (PR) passa só com CI verde, testes locais e threads respondidas no SHA atual, decisão ancorada ou "não se aplica" (veredito do juiz) para cada `product` aplicável, e nenhum órfão; grava `closure`.
- [ ] `create_directive` só aceita token preso ao hash do conteúdo; não existe caminho de editar diretiva; `annul_directive` exige premissa `human` com escopo.
- [ ] A projeção de `docs/directives` é gravada pelo hook a partir da resposta e não pode ser editada pelo agente.
- [ ] O `Stop` bloqueia enquanto o último fechamento da unidade ativa não passou e libera por contexto cheio, interrupção ou bypass.
- [ ] O handshake bloqueia major divergente entre fork, `omhl` e plugin.
- [ ] A fatia vertical roda de ponta a ponta para a granularidade PR antes de issue e epic.

## Assumptions Exposed & Resolved
| Assumption | Challenge | Resolution |
|------------|-----------|------------|
| O binário Rust faz tudo | Ele não lê o Hexlog; só o agente fala MCP | Fork do Hexlog com tools de domínio; `omhl` só hooks e scripts |
| Gates do Hexlog bastam | Gates são estáticos e as diretivas mudam | Lógica fixa em `close_unit`, registro `closure` |
| Evidência é afirmação do agente | O agente pode afirmar o que não rodou | Recibo de `omhl check` conferido pelo hook |
| Diretivas se editam | Edição quebra a atemporalidade | Só criar e anular; criação com aval preso ao hash |
| Estado local é barreira | O agente tem `Bash` | É freio, não barreira; bypass registrado |
| Retry mascara flakiness | Retry escondido enfraquece a prova | Padrão 0, a diretiva liga, tudo registrado |
| Sandbox é necessário | O objetivo é disciplina do agente, não barreira contra burla deliberada | Sem sandbox; freio + evidência de adulteração + auditoria para decidir depois com dados |

## Technical Context (greenfield)
- Hexlog atual: servidor MCP Node (stdio) com 12 tools e logs append-only por processo, em `~/personal/hexlog`. ADRs fixam o contrato de 12 tools; o fork amplia esse contrato.
- Lições das minerações que moldaram o design: `rests-on` e `anchored-in` são `derivesFrom` (revogar não marca dependentes); reescrever em cascata é caro e arriscado; `supersedes` no mesmo target preserva dependentes pela herança de linhagem; processos de diretivas versionados deixaram 112 decisões presas; o Ralph do OMC bloqueia pela flag `active`, que o agente consegue limpar.

## Ontology (Key Entities)
| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| Unit | core domain | granularity, parent, ancestors, integrity mode | tem Premises, fecha com Closure |
| Granularity | core domain | name, version | define gates por `appliesTo` |
| Premise | core domain | origin, applicableAtRegistration (derivado pelo servidor) | herdada pela subárvore |
| Directive | core domain | kind, appliesTo, criteria | `extends` outra Directive |
| Annulment | core domain | scope, reason | anula Directive por Premise human |
| NotApplicable | supporting | ruling `not-applicable` do juiz, reason (registrado como `judge-ruling`) | dispensa Directive product |
| Evidence | core domain | receipt, subject version | responde a Directive mechanical |
| Closure | core domain | outcome per directive | fecha Unit |
| Token | supporting | hash, uso único | autoriza ação humana |
| Bypass | supporting | reason, expiry | libera hooks por tempo limitado |

## Ontology Convergence
O conjunto de entidades estabilizou na rodada 3. Não calculei razão de estabilidade por rodada.

## Interview Transcript
Resumo das decisões por rodada (o transcrito completo está na conversa):
- **R0:** topologia de seis componentes, incluindo a entrevista dos níveis.
- **R1:** esclarecimento de que gates mecânicos são verificáveis e reproduzíveis; config no Hexlog; percurso do grafo por código; surgiu o fork do Hexlog.
- **R1b/R1c:** novas tools no fork; fork que segue o upstream; tools de domínio mais versões melhores das genéricas.
- **R2:** `omhl check` com recibo; hook escreve a projeção; token humano validado pelo servidor; quatro grupos de tools.
- **R3:** lógica fixa em `close_unit`; evidência presa à versão; critério aberto com retorno estruturado; criação de diretiva com aval.
- **R4:** checks embutidos e do projeto; aval preso ao conteúdo; gates do PR e da issue.
- **R5:** parâmetros explícitos; relatório por esqueleto e projeção; retry por check; completude de `checkedAgainst` (histórico: hoje o servidor deriva `applicableAtRegistration`, emenda Q1).
- **R6:** retry padrão 0; template versionado; `product` com âncora ou "não se aplica"; versão única com handshake.
- **R7:** "não se aplica" só seu (histórico: hoje só pelo juiz Haiku, emenda Q2); `/omhl-init`; quatro skills; falha fecha tudo, sem travar.
- **R8:** fatia vertical mínima; N/A para unidade e subárvore; só bypass temporário; só o init com prefixo.

## Itens abertos para a implementação
- Placeholders do template do relatório do epic (gate de epic, depois da fatia).
- Nomes finais dos comandos dependem da medição do namespace de plugin (S4 do plano).

Resolvidos pelo plano ([omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md)): como o `omhl` descobre projeto e processo (unidade ativa no estado da sessão, Step 2); detalhes do token (envelope HMAC e asserções, Step 1) e contrato JSON dos checks (`check-output.schema.json`); ADRs do fork (Q12, numeração própria); nomes dos comandos de "não se aplica" (não há comando humano, Q2) e de bypass (Q5).
