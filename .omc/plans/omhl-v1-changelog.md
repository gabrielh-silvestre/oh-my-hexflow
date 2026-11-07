# Histórico de revisões do plano omhl v1

Plano: [omhl-v1-vertical-slice.md](omhl-v1-vertical-slice.md). Marcos: [omhl-v1-marcos.md](omhl-v1-marcos.md). Este arquivo guarda o changelog de cada iteração (as menções ao sandbox, ao `scriptDigest`, ao `maxAge` e ao N/A por token humano são história; o plano vigente está no arquivo principal).

## Changelog da iteração 1

### Critic (ITERATE, 7 MAJOR)

| Achado | O que foi feito |
|---|---|
| M-1 integridade do recibo só do hook; R1(3) falso | **Atendido.** Princípio 1 reescrito; tabela "O que o desenho garante"; envelope HMAC verificado pelo servidor; MAC do recibo no `PostToolUse`; AC-13 e AC-6 (`RECEIPT_UNATTESTED`); R1(3) corrigido com o resíduo |
| M-2 fato errado do remoto | **Atendido.** F11 e o Step 4 corrigidos: `origin git@github.com:gabrielh-silvestre/hexlog.git`; fork segue `main` (Q6) e usa `upstream` |
| M-3 regras do spec sem AC | **Atendido.** AC-1 (idempotência no plugin), AC-2 (granularidade fixada, `MAJOR_MISMATCH` por serviço), `extends` no AC-8; o resto na seção 3.1 com justificativa (bypass no Hexlog, `grounded`/`executed`, reabertura, sucessão, `omhl detect`) e Q15 a Q17 |
| M-4 pilha de hooks real | **Atendido.** P0 e S2 com RTK, OMC, context-mode e `settings.json`; AC-18; delimitadores e plano B por `outbox` |
| M-5 contrato de `criteria` e CLI | **Atendido.** `criteria.schema.json` no Step 1; `omhl check` (a forma por stdin foi trocada por `--criteria-file` na iteração 2); AC-16 |
| M-6 projeção do bootstrap e exceção do init | **Atendido.** claim `bootstrap` presa ao `seedHash`; `PostToolUse` de `bootstrap_project` projeta (AC-9) |
| M-7 perna plugin do handshake | **Atendido.** `--plugin-contract` no `hooks.json`; AC-11 com a tabela de casos |
| Mudança 8 (verificação executável) | **Atendido.** Sem `just`/`mise`/`nextest`/`insta`/`hyperfine`; `scripts/check.sh` e `cargo test`; goldens à mão; latência por `Instant` |
| Mudança 9 (alternativas) | **Atendido.** D removida das opções; B reescrita honestamente (spikes primeiro); C com variante real de monorepo; T2 e T3 justificadas |
| Mudança 10 (ACs vagos) | **Atendido.** AC-10, AC-11, AC-12 e o e2e (passos exatos) concretos; declarado que "CI verde" real não é exercitado |
| Minor 1 (`allow` vs `ask`) | Atendido em S1(iv): só `allow` quando tudo validou, nunca `ask` |
| Minor 2 (descoberta de projeto/processo) | **Parcial:** o `process` vem da unidade ativa no estado da sessão e o projeto do `.omhl/`; o spec o deixa aberto; confirmar no Step 2 |
| Minor 3 (tipo `granularity` no AC-1) | Atendido no AC-1 |
| Minor 4 (árvore limpa no fechamento) | Atendido: `treeClean` e `TREE_DIRTY` (AC-7) |
| Minor 5 (MSRV) | Atendido como A22, confirmação no Step 1 |
| Minor 6 (versão mínima do Claude Code) | Atendido: `omhl setup` verifica >= 2.1.295 (R16) |
| Minor 7 (dono e cadência do `fork-drift`) | Atendido no Step 5 |
| Minor 8 (teto 5 não é do BMAD) | Atendido na seção 2.1 e em Q7 |
| Minor 9 (rótulos `spec`/`plan-added`) | Atendido na seção 3 |
| "Falta": gates de decisão por pergunta | Atendido: cada passo tem seu gate e a tabela Q tem prazo |
| "Falta": serialização do nonce; localização do segredo | Atendido: nonce de uso único checado contra registro do projeto; `OMHL_SECRET_FILE` no registro MCP. A atomicidade do check de nonce contra gravação concorrente fica coberta pelo lock por processo do Hexlog (a conferir no Step 3) |
| Opcionais (A17 como fato parcial; baseline do OMC) | Atendidos (A17; baseline descartado na 2.1 e na 3.1) |

### Architect (SÓLIDO COM RESSALVAS, 11 achados)

| # | O que foi feito |
|---|---|
| 1 servidor não autentica o que decide o fechamento | **Atendido.** Envelope HMAC, `ENVELOPE_REQUIRED`, recibo com MAC do hook, servidor sem escrita genérica (AC-13); R1(3) reescrito |
| 2 identidade do PR | **Atendido.** `headRef` na unidade, `attested.headRef`/`treeClean`, `SHA_NOT_OF_UNIT_PR` (AC-14). O `prNumber` não é guardado: deriva-se do `headRef` pelo provedor |
| 3 guarda do guarda | **Atendido.** AC-15 (settings user/managed, `~/.claude.json`, `hooks.json`, cache do plugin, binário e seu digest, diretório de stock); `omhl setup` humano grava no escopo user; managed como Q14. `.omhl/proposals/` fica desprotegido de propósito |
| 4 fork: patches e `no-flow-terms` | **Atendido.** Entrada própria sem editar o upstream (localizada em `src/omhl/server.ts` depois da iteração 2) (compondo `registerQueryTools` mais as de domínio, não `createServer`, para esconder as escritas; a entrada fica em `src/omhl/server.ts`); `XDG_DATA_HOME` no registro MCP; `outcome` no lugar de `verdict`; invariante do diretório de dados testada (AC-13). **Divergência do Architect (a justificativa da 1ª versão estava errada e foi corrigida na iteração 2):** ele sugere `src/omhl/server.ts` + `createServer`; `createServer` registra as 12 tools, incluindo as escritas, então o plano compõe `registerQueryTools` mais as de domínio. Na iteração 1 o plano pôs o entry em `scripts/omhl/` alegando que `src/` não pode importar `compose.ts`; isso descrevia a mensagem do lint, que só atinge blocos de camada, e o `scriptsBlock` na verdade proíbe `**/mcp/**` em `scripts/`. O entry vive em `src/omhl/server.ts` |
| 5 "major" mal definido | **Atendido** no Step 1 (definição, pré e pós-checagem, corrida); lacuna da sucessão como R9/Q15 para o dono |
| 6 validade só por SHA | **Atendido.** `issuedAt` e `maxAgeSec` (AC-7, AC-16); valor padrão em Q7 |
| 7 escopo do `contentHash` | **Atendido** na seção 2: payload canônico `{kind, project, process, target, data, relations}` com o texto capturado do prompt |
| 8 ordem dos passos (bypass tarde; bootstrap sem token) | **Atendido.** Bypass movido para o Step 2; claim `bootstrap` e projeção pelo `PostToolUse` do `bootstrap_project` |
| 9 contrato Rust/Node | **Atendido.** Schemas do zod no fork; cópia de `contract/` com `DIGEST` e `contract-sync.spec.ts` |
| 10 estado espelho | **Atendido.** "ativa-desconhecida" e lembrete no `resume` (AC-10, Step 2) |
| 11 spike de subagente e `-p`/`bypassPermissions` | **Atendido** em S1(v) e S5; A19 e R15 |
| Opção E (pacote de extensão) | **Atendida como risco R14**, sem reabrir o spec |
| Fato em conflito (remoto) | Resolvido por comando: o remoto existe (F11) |


## Changelog da iteração 2

Fonte: [review-2.md](../drafts/review-2.md). Estado dos pareceres: Critic ITERATE leve; Architect SÓLIDO COM RESSALVAS.

### Bloqueantes

| Achado | O que foi feito |
|---|---|
| N-1 e B2 (a asserção humana não prende alvo nem relações; `annul_directive` aceitaria premissa antiga) | **Atendido.** Seção 2 "O que a asserção humana prende": o hook deriva `tool`, `kind`, texto e ids dos `command_args` e sobrescreve ou nega; `target` derivado pelo servidor; whitelist de relações; uma asserção por tool; `annul_directive` com asserção própria presa a `{directiveId, scope, reason}`, gravando premissa e anulação no mesmo lote. AC-3 com negativos (a) a (d); AC-8 com a anulação de outro conteúdo recusada; A23 nova |
| N-2 (CLI `--criteria-stdin` contradiz o MAC de "uma invocação simples") | **Atendido.** `--criteria-file <path>` com o JSON em `.omhl/outbox/criteria/`; formas aceitas e rejeitadas testadas no AC-16 |
| N-3 (gates de decisão tarde) | **Atendido.** Q3, Q7, Q8, Q11 e Q13 foram para antes do Step 1 (tabela de Q reescrita e ordenada por prazo) |
| N-4 (verificação) | **Atendido.** `cargo test -- <filtros>`; `omhl setup --home <dir>` obrigatório no e2e e nos testes; passos de verificação para AC-1 (lado plugin, `init_idempotent`) e AC-19 (`sandbox-probe.sh`); `scripts/check.sh` marcado como nascido no Step 1; frase sobre o espelho AC-spec corrigida com o mapa exato e o `AC-E2E` |
| B1 (entry no lugar errado) | **Atendido, com correção de fato.** Entry em `src/omhl/server.ts`; justificativa corrigida no Step 1 e no F8: o motivo é o `scriptsBlock` (proíbe `**/mcp/**` em `scripts/`), não a regra de `compose.ts`, que só atinge blocos de camada; `knip.json` (1 linha) entra na lista de toques; build por `build({ entryPoints })` sem patch; `coverageThreshold` global de 90% exige testes em `test/omhl/`; A21 corrigida. Verifiquei os blocos de `eslint.boundaries.js`, `knip.json`, `scripts/build.ts` e `package.json` por leitura |
| B3 (identidade do PR) | **Atendido.** O recibo de `ci-status`/`review-threads` carrega `prNumber` e `prHeadRefOid` (resolvidos pelo `omhl check`); o servidor só compara; `open_unit` PR recusa `headRef` igual ao branch base (`HEAD_REF_IS_BASE`); primeiro `ci-status` com PR inexistente ou fechado devolve `error`; AC-14 reescrito com negativos |

### Melhorias

| Achado | Estado |
|---|---|
| N-5 (injeção de delimitadores) | Aplicada: `omhl check` captura e digere a saída do filho; hook usa o último bloco; teste no AC-5 |
| N-6 (bypass sem teste) | Aplicada: `bypass.json` com MAC do hook, ignorado sem MAC; sob bypass só o `Stop` libera (AC-12) |
| N-7 (conjunto semente) | Aplicada: `contract/seed-directives.json` com `seedHash` e cópia com `DIGEST` |
| N-8 (digest, `shlex`, `check.sh`) | Aplicada: `--refresh-digest`, R17, `shlex` na lista, `check.sh` no Step 1 |
| N-9 (`range-diff`) | Aplicada: `log upstream/main..HEAD` mais `range-diff` entre tags, a partir da 2ª |
| Fidelidade (critério 7) | Aplicada: AC-20 para `correct`/`revoke`; integrity herdado no AC-2; projeção de premissas por pedido, N/A no corpo do PR e `judge` na 3.1 |
| Cortável do AC-15 e lista de cortáveis | Aplicada: núcleo e cortável marcados no AC-15; lista de 7 itens na 3.1; R18 sobre a carga de infraestrutura |
| Architect 4 (expirações separadas) | Aplicada: envelope e `attested` 45 s, asserção humana 10 min (Q7) |
| Architect 5 (rotação do segredo) | Aplicada como `omhl setup --rotate`, cortável |
| Architect 6 (guarda do guarda) | Aplicada: caminho absoluto exato do binário, rc de shell e subcomandos de `claude` no guard; a parte cortável marcada |
| Architect 7 (aval em dois passos) | Aplicada no Step 5, com o plano B de um passo (cortável) |
| Architect 8 (recuperação "ativa-desconhecida") | Aplicada sem tool nova: `open_unit` idempotente por nome devolve a unidade existente e refaz o espelho (AC-2) |
| Architect 9 (mapa de definições) | Aplicada: mapa `definição -> versão` no `unit`, conferido contra o `manifest.fixed` |
| Architect 10 (encurtar o Step 1) | Aplicada: o envelope do Step 1 só tem `contract` e `bootstrap`; `human`, `attested` e `receipts` entram aditivamente nos Steps 2 e 3 |
| Architect 11 (`inputHash` sobre args crus) | Aplicada: `registerTool` próprio chamando `execute` (`kernel.ts:193`) com os `args` crus; `.default()` e transforms proibidos |
| Architect 12 (tabela de garantias) | Aplicada |
| Architect 13 (`read_attachment`) | Aplicada na lista "o que o plano acrescenta ao spec" |
| Architect 14 (justificativa de A sobre B e de E) | Aplicada |
| Architect 15 (atrito do digest) | Aplicada: `--refresh-digest` e R17 |

**Não atendido:** nenhum achado da iteração 2 ficou sem tratamento. Ressalva: o "~95% dos ACs concretos" do Critic foi preservado, mas o AC-9 depende do resultado da P0 (formato do `tool_response`), o que o plano declara como condicional.


## Changelog da iteração 3

Fonte: [review-3.md](../drafts/review-3.md). Estado dos pareceres: Critic ITERATE final (2 bloqueantes); Architect SÓLIDO COM RESSALVAS (3 bloqueantes menores).

### Bloqueantes

| Achado | O que foi feito |
|---|---|
| N-10 e parte N/A do B-II (N/A não prende a diretiva) | **Atendido.** A asserção de `not-applicable` prende `{directiveId, reason, process}`; o hook deriva do comando; o servidor deriva relação e `target` (`na.<directiveId>`) e não aceita `anchored-in` do agente nesse `kind`. Negativo (e) no AC-3 e teste de fechamento com N/A de outra diretiva no AC-7 |
| N-3 e Architect 5 (gates Q1, Q2, Q5, Q14 tarde; schema das asserções) | **Atendido.** Q1, Q2, Q5, Q14 (e a nova Q18) foram para antes do Step 2; o Step 1 **congela o schema completo das asserções** e a gramática dos comandos em `contract/commands.md`, para os Steps 2 e 3 andarem em paralelo |
| B-I (fontes de prompt não humanas) | **Atendido.** Verifiquei na doc que o `UserPromptSubmit` dispara para tarefa agendada, `/loop`, subagente em background e mensagem de outra sessão (F6). S7 enumera as fontes e o campo que distingue cada uma; regra de falha fechada (só origem interativa comprovada cunha asserção humana); núcleo do AC-15 nega `claude` pelo `Bash` e o agendamento de prompts com comando do `omhl`; negativo (f) no AC-3; R19 e A24 novos; a decisão de desligar asserções humanas sem sinal interativo virou Q18. **Superado na iteração 4:** a distinção não é por campo (o `UserPromptSubmit` não tem campo de origem, F13) e sim pela identidade do evento (`UserPromptExpansion`); a A24 e a Q18 foram reescritas |
| B-II (ciclo de vida da asserção) | **Atendido.** Uma asserção pendente por tool e por `session_id`; o novo comando substitui; `SessionEnd`/`SessionStart` limpam; descarte só no `PostToolUse` com sucesso; `nonce` como campo de topo verificado dentro do lock do processo; `process` e `project` entram nos campos derivados; regra única de normalização (sobrescrever o texto em silêncio, negar só por divergência semântica); ACs no item (g) do AC-3 (dois comandos pendentes, falha seguida de retry, duplo gasto) |
| B-III (`open_unit` idempotente) | **Atendido.** `UNIT_EXISTS_MISMATCH`; unidade fechada devolve `closed: true` sem reativar o espelho; `key` de idempotência do `register` derivada do nome; `reason` do `Stop` e lembrete do `SessionStart` reescritos; contradição resolvida: leitura = tool `list`, recuperação = `open_unit` repetido com o nome. O `systemMessage` mostra em que unidade a premissa será gravada. A opção `--unit <nome>` não foi adotada (o `systemMessage` basta) |

### Melhorias

| Achado | Estado |
|---|---|
| N-11 (`--criteria-stdin` na seção 8) | Aplicada |
| N-12 (listas "Criar" e "Verificação") | Aplicada: `tests/init_idempotent.rs` e `scripts/sandbox-probe.sh` no Step 4; AC-20 na verificação do Step 3; AC-1 plugin e AC-19 na verificação do Step 4. `claims` x `assertions`: "claim" só aparece em prosa com a nota de nomenclatura |
| N-13 (`read_attachment`, `inputHash`, FORK.md/ADR) | Aplicada: AC-13 lista `read_attachment`; `inputHash` no schema do envelope; conferência de `docs/FORK.md` e do ADR contra `test/directives.spec.ts`, lint e `cspell` no Step 1 |
| Architect 4 (pós-checagem do mapa) | Aplicada: lê o mapa de novo depois do `create_process`, compara com `DefinitionStore.versions`/`read` e repete uma vez |
| Architect 6 (AC do aval em dois passos; bootstrap negativo; sandbox-probe) | Aplicada: negativos nos AC-8 e AC-1; AC-20 e `sandbox-probe.sh` nas verificações |
| Architect 8 (identidade do PR) | Aplicada: `baseRef` explícito no lugar da lista de fallback (cobre o PR de release de `develop` para `main`); `headRef` só de `attested`, HEAD destacado recusado; o recibo carrega `repo`, `headRepository`, `baseRefName`; o PR aberto resolvido no `ci-status` é a proteção real; `REPO_MISMATCH` |
| Architect 9 (T1') | Aplicada: o hook nega a 1ª chamada e devolve o envelope assinado para os args exatos; o agente repete |
| Architect 10 (`exp` e RTK) | Aplicada: política `allow` explícita e `ENVELOPE_EXPIRED` com remontagem; a checagem do primeiro token usa o comando visto no `PreToolUse`, com plano B de isenção do RTK |
| Architect 11 (colisão do alvo) | Aplicada: `premise.<32 hex>` |
| Architect 12 (convenções do upstream) | Aplicada e verificada por leitura (F12): sem dependência nova, sem `arquivo:N` em docs, títulos `should ...`, ADR com numeração própria |
| Architect 13 (nomenclatura do envelope) | Aplicada: nota de nomenclatura na seção 2 |
| Architect 14 (sem campo `origin`) | Aplicada: o servidor deriva `origin` da asserção; AC-3 reescrito |
| Architect 15 (ordem de corte) | Aplicada: lista de cortáveis reordenada (`--rotate`/`--refresh-digest`, outbox, aval em dois passos primeiro) |
| Detalhe técnico (`ToolCall`/`ToolConfig` não exportados; `args` indefinido) | Aplicada no Step 1 |

**Não atendido:** nenhum achado ficou sem tratamento. Ressalva: o Step 2 continua o mais pesado (foi o ponto do Architect 15); em vez de dividi-lo, a lista de cortáveis define a ordem de corte. O plano não foi executado: lint, `knip`, `typecheck` e testes do Hexlog seguem não rodados, só lidos.


## Changelog da iteração 4

Fonte: [review-4.md](../drafts/review-4.md). Estado dos pareceres: Critic APPROVE (sem bloqueantes); Architect SÓLIDO COM RESSALVAS (2 bloqueantes pequenos).

### Bloqueantes

| Achado | O que foi feito |
|---|---|
| B-I (S7, A24 e Q18 procuravam um campo de origem que a doc não tem; o padrão "sem sinal, desliga tudo" derrubava `/omhl-init` e bypass) | **Atendido.** Reescrito pela **identidade do evento**: asserção humana só no handler de `UserPromptExpansion` com `expansion_type: slash_command` e `command_name` do plugin; o `UserPromptSubmit` nunca cunha; skills de comando humano com `disable-model-invocation: true`. S7, A24 (reescrita), A25 (`-p` aninhado) e Q18 reescritas; residual `-p` medido na S7 com discriminadores candidatos (cadeia de processos, variável de ambiente), tratado como freio e não barreira (S6 é a barreira). Carve-outs explícitos: `bootstrap` sem dependência de origem; bypass sob o gate de origem, com o teto do `omhl` (ou encerrar a sessão) como alternativa. `CLAUDE_CODE_DISABLE_CRON=1` virou a Q19. Verifiquei a doc: o exemplo de `UserPromptExpansion` tem os campos citados; a frase "user-typed" e as páginas de tarefas e mensagens vêm do relato do Architect (F13 marca isso) |
| B-II (nonce "dentro do lock" não implementável pela API pública) | **Atendido.** Uso único pela `key` de idempotência do `register`: `key = "human:<nonce>"`; lote idêntico devolve `replayed`; lote diferente devolve `IDEMPOTENCY_CONFLICT`, mapeado para `HUMAN_TOKEN_INVALID`; vale também para `annul_directive` e `create_directive`; o `nonce` segue como campo de topo para auditoria. Verificado por leitura: ordem de recusa com a `key` no nível 5 sem callback (`src/commands/process.ts`, 54-69) e `BATCH_KEY_MAX = 200` (`src/domain/chain.ts:10`). Teste (g) do AC-3 reescrito como (h) |

### Melhorias

| Achado | Estado |
|---|---|
| Architect 3 (`process` na cunhagem) | Aplicada: fixado na cunhagem e conferido no uso (AC-3 (g)) |
| Architect 4 (N/A só para `product` existente) | Aplicada: `NOT_APPLICABLE_FORBIDDEN` e `DIRECTIVE_NOT_FOUND` (AC-7); `directiveId` é o `target` da diretiva (Critic M-3) |
| Architect 5 (`IDEMPOTENCY_CONFLICT` no `open_unit`) | Aplicada: relê e compara `parent`, `granularity`, `headRef`, `baseRef` (Step 1, AC-2) |
| Architect 6 (`list` não distingue aberta de fechada) | Aplicada: dica de `query` por `closure` no lembrete e no runbook (AC-10) |
| Architect 7 (âncora do `repo`) | Aplicada como resíduo R20, com o tratamento para quando o GitHub entrar |
| Architect 8 (ordem de corte; dividir o Step 2) | Aplicada: lista reordenada (esqueleto do `gh` primeiro; S6 por último entre os de segurança); Step 2 dividido em 2a e 2b |
| Architect 9 (alinhar o changelog da iteração 3 com a A24) | Aplicada: nota "Superado na iteração 4" na linha do B-I da iteração 3 |
| Critic M-2 (códigos de erro) | Aplicada: `contract/errors.md` no Step 1; `ENVELOPE_EXPIRED` separado de `ENVELOPE_INVALID` no AC-13 |
| Critic M-3 (`na.<directiveId>`) | Aplicada |
| Critic M-4 (`claude` negado vs verificação do plano) | Aplicada: negação só em projetos com `.omhl/` e para `claude` que abre sessão; `--version`, `plugin validate`, `mcp list` liberados |
| Critic M-5 (`baseRef`; `omhl setup` idempotente) | Aplicada: `baseRef` é argumento do agente em `open_unit`, fora de `attested`; `omhl setup` preserva o segredo e `--rotate` o regenera |
| Critic M-6 (dividir o Step 2) | Aplicada |
| Critic M-7 (ACs condicionais ao corte) | Aplicada na 3.1 |
| Critic M-8 (token humano desligado e e2e; teste do `SessionEnd`) | Aplicada na seção 1 e no AC-3 (g) |
| Critic N-12 (terminologia) | Aplicada: "claim" só em prosa; no contrato e no código, `assertions` |
| Q5 e Q14 fora do gate do Step 4 | Aplicada |
| ADR e RALPLAN-DR final | Escritos na seção 9 |

**Adiada:** o terminal "pergunta em aberto" do Critic (a numeração própria do ADR do fork precisa passar em `test/directives.spec.ts`) continua como conferência do Step 1, não resolvida por leitura.

**Não atendido:** nenhum achado ficou sem tratamento. Lint, `knip`, `typecheck` e testes do Hexlog seguem não executados.


## Changelog da iteração 5

Fonte: mensagem de polimento final do líder (consenso: Critic APPROVE; Architect SÓLIDO COM RESSALVAS sem bloqueantes, B-I e B-II resolvidos). Todos os itens eram opcionais.

| Item | Estado |
|---|---|
| Critic 1 (nome `envelope-asserções`) | Aplicado: `envelope-assertions` |
| Critic 2 (`baseRef` lido de `attested`) | Aplicado: `baseRef` é argumento do agente; `headRef` e `repo` vêm de `attested` |
| Critic 3 (Q19 sem gate) | Aplicado: Q19 no gate do 2a e na frase do gate do Step 1 |
| Critic 4 (prós e contras de A, B, C; suavizar o "Why chosen") | Aplicado na seção 9 |
| Critic 5 (mapa passo-AC; `SessionEnd` no 2b) | Aplicado: AC-6 lado hook e AC-16 efeito no 2b, AC-14 lado `omhl` no 2a; `session_end` na lista de arquivos de hook do 2b |
| Critic 6 (frase de ordem) | Aplicado: Step 0 -> 1 -> 2a -> 2b -> 3 -> 4 -> 5 |
| Architect 7 (matriz do Q18) | Aplicado: matriz na seção 2 (a recomendação inverte para "desligar" só com S6 refutada e `-p` indistinguível) |
| Architect 8 (determinismo dos lotes; mapeamento por prefixo; AC-3 (h)) | Aplicado: construtor puro, `checkedAgainst` ordenado, mapeamento só para `human:`; (h) reescrito |
| Architect 9 (`key = "bootstrap:<seedHash>"`) | Aplicado no AC-1 e no Step 1 |
| Architect 10 (dependências 2a/2b; gates por metade; campo `source`) | Aplicado: no 2a só o parser puro (`shlex`); latência e formas sem MAC no 2b; gates 2a = Q10, Q14, Q19 e 2b = Q9, Q1, Q2, Q5, Q18; `source` definido como o nome do evento (`UserPromptExpansion`) |
| Architect 11 (códigos em `contract/errors.md`) | Aplicado: `EVIDENCE_MISSING`, `EVIDENCE_FAILED`, `EVIDENCE_ERROR`, `PRODUCT_DIRECTIVE_UNRESOLVED`, `CRITERIA_HASH_MISMATCH`, `SCRIPT_DIGEST_MISMATCH`, `RECEIPT_INVALID`, usados nos AC-6 e AC-7 |
| Architect 12 (ADR: redação, cabeçalho, follow-ups) | Aplicado |
| Architect 13 (ACs condicionais com Q18 desligado; sessão "ativa-desconhecida") | Aplicado junto da matriz |
| Architect 14 (F13) | **Aplicado em parte, com ressalva.** Li a doc de tarefas agendadas: ela confirma que prompt agendado não executa skills `disable-model-invocation: true` (nem comandos internos, nem prompts de MCP). A frase "user-typed command" da doc de hooks **não** apareceu no texto que li, então esse ponto continua como relato do Architect; o comportamento de linha com `/` em mensagem de sessão segue como medição da S7 |
| Architect 15 (ordem da tabela de riscos) | Aplicado: R17 e R18 antes de R19 e R20 |

**Adiado:** nada. **Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.


## Changelog da iteração 6 (rodada extra opus)

Fonte: [review-opus.md](../drafts/review-opus.md). Estado dos pareceres: Critic opus ITERATE (1 CRITICAL, 6 MAJOR); Architect opus SÓLIDO COM RESSALVAS (3 bloqueantes). Nenhum reabre o spec.

### Bloqueantes

| Achado | O que foi feito |
|---|---|
| C1 (âncoras de confiança fora do guarda) e A1 (sandbox sem `failIfUnavailable`) | **Atendidos.** AC-15 reescrito: caminho absoluto do binário no `hooks.json` instalado (placeholder `@OMHL_BIN@`), binário, PATH, `hooks.json` e cache do plugin, bundle do fork, `~/.claude.json` e `claude mcp|plugin|config` no **núcleo**; autodigest rebaixado a detector de reinstalação; negativo do `omhl` falso no PATH. `omhl setup` grava `sandbox.failIfUnavailable: true` e `allowUnsandboxedCommands: false`; o `PreToolUse` afirma `attested.sandbox` e o servidor recusa (`SANDBOX_NOT_STRICT`); AC-19 reescrito como pré-condição e **removido dos cortáveis** (a fatia para se o S6 refutar, Q14). Princípio 1, tabela de garantias e matriz do Q18 reescritos. Verifiquei na doc de sandboxing que, por padrão, o Claude Code roda sem sandbox quando a dependência falta (F14) |
| A2 (teto R11 enterrado) | **Atendido.** R11 promovido ao Goal (seção 1), ao Princípio 1 e à tabela de garantias; declarado que "CI verde" é auto-certificado no modo `self`; o digest da árvore de testes dos checks embutidos ficou como follow-up (3.1) |
| A3 (`command_source`) | **Atendido.** Cunhar exige `expansion_type == "slash_command"` **e** `command_source == "plugin"`; fixture negativa de homônimo local no AC-3 (f) |
| M1 (issue/epic ativa trava o `Stop`) | **Atendido.** O `Stop` vigia só PR; `open_unit` de issue/epic não ativa o espelho; depois do PR fechar não há unidade ativa; casos golden no AC-10 |
| M2 (árvore suja sem definição; `.omhl/` suja a árvore) | **Atendido.** `.gitignore` com `.omhl/state/`, `.omhl/outbox/` e `.omhl/proposals/` (AC-1); suja = `git status --porcelain --untracked-files=all` não vazio fora dos ignorados; negativo no AC-5 |
| M3 (bypass mais estreito que o spec) | **Atendido por pergunta:** Q22 com recomendação e consequências; o AC-12 aponta para ela |
| M4 (projeção da anulação) | **Atendido.** O `PostToolUse` de `annul_directive` grava `docs/directives/annulments/<directiveId>--<escopo>.md` (AC-9) |
| M5 (provedor falso sem regra) | **Atendido.** Provedor falso só atrás da cargo feature `fake-github`; sem provedor, `error` com `no-github-provider`; a fatia não fecha PR real até a Q10 (seção 1, R22, ADR) |
| M6 (consumo da asserção ambíguo) | **Atendido.** Só consome a chamada cujo texto normalizado é igual ao `textHash`; texto diferente segue como `agent`; lista dos campos que o agente ainda controla |

### Melhorias

| Achado | Estado |
|---|---|
| M7 (Q com recomendação, consequências, ACs) | Aplicada: tabela de Q reescrita; Q12 separada em Q12 (ADR) e Q23 (nome `outcome`); Q18 com regra única sem reler a matriz; ACs que pressupõem resposta marcados |
| A4 (R2/R14 com os números verificados; teste de contrato de API) | Aplicada: R2 e R14 reescritos (687 commits; `query.ts` 26, `ports.ts` 23, `kernel.ts` 17, `process.ts` 11, `compose.ts` 10, números que reconferi, o Architect citara 12 para `process.ts`); `test/omhl/upstream-api.spec.ts` no Step 1 |
| A5 (`cross-process-currency` x premissa herdada) | Aplicada como **Q21** antes do Step 3; a cita de `relations.ts` foi corrigida para a linha 252 |
| A6 (fatia mínima) | **Não cortei o token humano** (o spec o inclui). **Q20** com recomendação, consequência de cada resposta, ACs que viram fatia-2 e o efeito na ordem dos Steps; marco fatia-1 agent-only na seção 3 |
| T1' (usabilidade) | Registrada no texto do T1' como fragilidade de usabilidade se o S1 cair (a volta extra por chamada) |
| Critic MINOR 1 a 9 | Aplicados: `resume` no matcher do `SessionStart`; um só registro do MCP (`omhl setup`, `--scope user`, padrão `local` conferido em `claude mcp add --help`); gate do Step 3 e a Q4 no 2b; `sync-contract.sh` e `cspell`/`markdownlint` na lista de toques; cabeçalho da linha 4; F13/A17 e S7 atualizados; "só" do `knip.json` corrigido; S1 com servidor de eco; `isLegacy` no `ToolDeps` do entry |
| Fidelidade ao spec | Aplicada: projeção da anulação (AC-9); bypass (Q22); "versões melhores das genéricas" e checks `lint`/"artefato"/"comentário" na 3.1 com justificativa |
| Escopo (adiar `repo`/`baseRefName`, `--rotate`, aval em dois passos) | Aplicada na ordem de corte (3.1) |
| Fatos | Corrigidos na iteração 6: `knip.json`; `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP` confirmado na doc (F14). **Correção na iteração 7:** a nota da iteração 6 sobre F13/A17/Q19 estava errada; `CLAUDE_CODE_DISABLE_CRON` e as duas frases da doc de hooks existem (ver o Changelog da iteração 7) |

**Nota corrigida na iteração 7:** afirmei aqui que não localizei as frases "user-typed command" e "`/skill` digitado contorna o `PreToolUse`" e que `CLAUDE_CODE_DISABLE_CRON` não constava da página de variáveis; as três existem na doc oficial (confirmado por leitura direta das páginas). Lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.


## Changelog da iteração 7

Fonte: [review-6.md](../drafts/review-6.md). Estado dos pareceres: Critic APPROVE; Architect SÓLIDO COM RESSALVAS (2 bloqueantes pequenos).

### Bloqueantes

| Achado | O que foi feito |
|---|---|
| B1 (S6 ainda condicional em 5 lugares; `bwrap` e `socat` ausentes) | **Atendido.** As 5 linhas reescritas (decisões de saída do Step 0, Step 4, verificação do Step 4, seção 6, R1): o S6 sempre roda e, se refutar, o Step 0 termina em "parar e perguntar a Q14". F10 registra que `bwrap` e `socat` não estão instalados, que `~/.claude/settings.json` não tem chave `sandbox` e que `max_user_namespaces` é 95447 (verifiquei por comando). S6 reescrito com o pré-requisito e as sondas (a) a (h): falha no startup, `dangerouslyDisableSandbox`, `excludedCommands` de projeto, `claude -p` aninhado dentro do sandbox, `!omhl setup` fora do sandbox, allowlist de rede do `gh` e visibilidade das flags de `claude --settings` |
| B2 (Q14 e AC-19 omitem o escopo user) | **Atendido.** Q14 lista (a) escopo user não é admin-required (`excludedCommands` de projeto vale), (b) escopo global com allowlist de rede vazia, (c) `failIfUnavailable` só no startup; recomenda o lançador `claude --settings` (admin-required, sem root). O `attested.sandbox` afirma que não há `excludedCommands`; o AC-19 amarra a asserção ao digest da configuração gravado no `SessionStart` e manda reiniciar a sessão depois do `omhl setup`. Confirmei as frases da doc de sandboxing por leitura direta. A opção `--accept-no-sandbox` foi **retirada** (o spec veda variável de ambiente de escape e ela não tinha mecanismo): não existe modo sem sandbox |

### Fatos corrigidos
`CLAUDE_CODE_DISABLE_CRON` consta de env-vars e de scheduled-tasks; as frases "when a user-typed command expands into a prompt" e "typing `/skillname` directly bypasses `PreToolUse`" estão na doc de hooks. Confirmei as quatro por leitura direta das páginas oficiais. F13, A17, Q19 e a nota final do changelog 6 foram corrigidos.

### Melhorias

| Achado | Estado |
|---|---|
| Architect 4 (`ts-jest` com `isolatedModules`) | Aplicada: o `fork-drift` roda também `npm run typecheck` e `npm run lint` |
| Architect 5 (marco fatia-1 sem trabalho humano antes) | Aplicada: o 2b foi dividido em 2b-agent (antes do Step 3) e 2b-humano (depois do marco); a ordem e os gates foram atualizados |
| Architect 6 (`--accept-no-sandbox` sem mecanismo) | Aplicada pela retirada da opção |
| Architect 7 (atrito de autohospedagem) | Aplicada no Step 2a (você instala por `!` e roda `--refresh-digest`) |
| Architect 8 (Q21 x regra do `process`) | Aplicada: exceção declarada na seção 2 e na Q21 |
| Architect 9 (ADR e leitura) | Aplicada: "Positivas" corrigida; "Leitura rápida" com as 5 decisões no topo |
| Critic 1 (instalação do plugin com `@OMHL_BIN@`) | Aplicada: o `omhl setup` renderiza `plugin/` num diretório protegido e imprime o caminho para `--plugin-dir`; o AC-1 e o runbook usam esse diretório; o placeholder nunca é carregado direto |
| Critic 3 (`SANDBOX_NOT_STRICT` sem tarefa) | Aplicada: item no Step 1 (verificação de sandbox no servidor) e testes nas verificações do Step 1 e do 2b-agent |
| Critic 4 (testes do escopo da Q22 no AC-12) | Aplicada, marcados condicionais à Q22 |
| Critic 6 (fragilidade do T1') | Aplicada no texto do T1' |
| Critic 8 (resumo de uma página) | Aplicada no topo da seção 7 |
| Critic 9 (F14 sem fonte) | Aplicada: F14 cita as páginas e agora está confirmada por leitura direta |
| Critic 10 e 11 (pontuação; reinício depois do `setup`) | Aplicadas |

**Adiado:** nada. **Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos; `bubblewrap` e `socat` precisam ser instalados por você antes do S6.


## Changelog da iteração 8

Fonte: mensagem final do líder (Architect: B1 e B2 resolvidos, 1 bloqueante novo N1; Critic: 1 bloqueante, o gate da Q5, mesma raiz).

| Item | Estado |
|---|---|
| N1 (a cunhagem `bootstrap` ficava no 2b-humano, depois do marco, e o e2e do marco começa por `/omhl-init`) | **Atendido.** Novo `src/hook/bootstrap_prompt.rs` no 2b-agent, que cunha só a asserção `bootstrap` (constante `seedHash`, sem dependência de origem); o `user_prompt.rs` do 2b-humano fica só com as asserções de origem humana (premissa, N/A, anulação, diretiva, bypass). AC-1 lado hook entra na verificação do 2b-agent |
| Gate da Q5 | **Atendido.** A Q5 passou para "antes do 2b-agent" na tabela e nos gates do Step 1 e do 2b-agent; saiu do gate do 2b-humano; o gate do Step 4 ("Q5 e Q14 já fecharam antes") agora é verdadeiro |
| Varredura dos passos anteriores ao marco | **Feita por leitura.** Dependências de código ou decisão posteriores ao marco encontradas e corrigidas: (1) a cunhagem `bootstrap` (N1); (2) o gate da Q5; (3) o `SessionEnd` no `hooks.json` do Step 4, que agora só entra com o 2b-humano. Sem outras: o Step 3 valida asserções por fixtures de envelope (sem hook humano); a projeção do `bootstrap_project` está no 2b-agent; Q1, Q2 e Q21 fecham antes do Step 3; Q18 e Q22 só importam depois do marco; o e2e do marco não usa bypass |
| (a) S7 x S6(d) | Aplicado: a regra de decisão é a sonda S6(d) com a Q18; o S7 só mede se o `-p` aninhado expande o comando e se há discriminador |
| (b) `disable-model-invocation` é fato | Aplicado (confirmado na página de tarefas agendadas) |
| (c) Q20 | Aplicado: "2b-humano some"; e o marco isola o lado do hook, não o do servidor |
| (d) Q18 na lista das que mudam o plano | Aplicado: 7 pedem decisão sua; a Q18 é regra automática; as outras 15 seguem as recomendações |
| (e) ponto no AC-12 | Aplicado |
| (f) `bubblewrap` e `socat` no Step 0 | Aplicado: pré-requisito no Step 0 e nas decisões de saída |

**Adiado:** nada. **Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.


## Changelog da iteração 9

Fonte: mensagem final do líder (Critic APPROVE; Architect SÓLIDO COM RESSALVAS, sem bloqueantes). Só texto: sem mudar estrutura, ACs, ordem dos passos nem gates.

| Item | Estado |
|---|---|
| 1. Armazenamento e descarte da asserção pendente num módulo compartilhado | Aplicado: tipo `assertion` em `src/state/` (Step 2a), usado por `bootstrap_prompt.rs` e `user_prompt.rs`; redação do `user_prompt.rs` ajustada |
| 2. `hooks.json` por fase | Aplicado: antes do marco só eventos com handler existente (`UserPromptExpansion` apenas com o `bootstrap_prompt`); `UserPromptSubmit` e `SessionEnd` entram com o 2b-humano |
| 3. Evolução do conjunto semente | Aplicado: o `bootstrap_project` ignora diretivas que já existem por `target` e recusa com `SEED_CONFLICT` se o conteúdo divergir; o servidor compara o `seedHash` com a própria cópia |
| 4. Sem bypass antes do marco | Aplicado no gate do Step 4: a única saída do `Stop` nos Steps 3 e 4 (agent) é o teto do `omhl` |
| 5. R19 e o `claude -p "/omhl-init"` | Aplicado: aceito porque o conteúdo é constante, a operação é idempotente e o servidor confere o `seedHash` |
| Rótulo "(N1)" solto | Aplicado: substituído por uma explicação |

**Adiado:** nada. **Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.


## Changelog da iteração 10

Fonte: [owner-decisions.md](../drafts/owner-decisions.md) (decisões do dono de 2026-10-31).

| Decisão | O que mudou no plano |
|---|---|
| **Q14: sem sandbox (opção C), com observabilidade** | Saíram o sandbox como pré-condição, o S6 bloqueante (virou medição opcional, nível futuro), o AC-19 antigo, `SANDBOX_NOT_STRICT`, `attested.sandbox`, `failIfUnavailable`, `allowUnsandboxedCommands`, `bubblewrap` e `socat` dos pré-requisitos, o R21 antigo, a Q14 e a Q18 antigas e o item "não cortáveis" da 3.1. Reescritos: Leitura rápida, Goal (seção 1: "freio, não barreira"), Princípio 1, tabela de garantias, AC-15 (guard como freio com registro de toda negação), R1, R21, ADR. **Acréscimos:** AC-19 (log de auditoria só-acréscimo encadeado, na fatia), AC-21 e Step 6 (`omhl report`, critérios D1 a D4 e níveis futuros de barreira, marco logo depois da fatia), spike S8 (detecção barata de leitura do segredo), F15, A28 e A29. Justificativa do escopo: a coleta (um ponto único, `audit::record`) entra na fatia porque precisa existir desde o primeiro dia; a análise fica num marco depois para não inflar os 20 ACs |
| Q9: aceito | Marcada como decidida |
| Q10: API do GitHub em Rust, sem `gh` e sem MCP | Removidos o `gh` e o esqueleto do provedor `gh`; o provedor real é um cliente de API (candidatos a verificar no crates.io); A11 reescrita; token na **Q10b** (recomendação: somente leitura, arquivo 0600 fora do repositório; consequências listadas) |
| Q19: manter o cron ativo | Removidos `CLAUDE_CODE_DISABLE_CRON`, `permissions.deny` de `CronCreate`/`ScheduleWakeup`/`loop`, a negação de agendamento no guard e `CronCreate`/`ScheduleWakeup` do matcher; regra: se o S7 achar fonte agendada indistinguível, desligar as asserções humanas |
| Q20, Q21, Q22: aceitas | Marcadas como decididas; gates dos passos atualizados (2a = Q10b; 2b-agent = Q4, Q5; Step 3 = Q1, Q2; 2b-humano = resultado do S7) |
| Demais questões (Q1 a Q8, Q11 a Q13, Q15 a Q17, Q23) | Seguem como recomendação do plano, pendente da confirmação do dono |

**Conflito com o spec:** conferi o spec e ele não menciona sandbox; ele já chama o estado local de "freio, não barreira". Nenhum conflito a registrar. **Resíduo proposital:** os changelogs das iterações 1 a 9 mencionam o sandbox como história e não foram reescritos.


## Changelog da iteração 11

Fonte: [owner-decisions.md](../drafts/owner-decisions.md) (atualizações do dono).

| Decisão | O que mudou no plano |
|---|---|
| **Q10b: token do GitHub por variável de ambiente** | O `omhl check` lê `OMHL_GITHUB_TOKEN` (nome próprio, para não vazar para outras ferramentas); o arquivo 0600 saiu do plano. **Resíduo declarado** na tabela de garantias (o agente pode lê-la por `env`, `printenv`, `/proc/*/environ`); mitigação: token fine-grained somente leitura (`checks:read`, `pull_requests:read`, `metadata:read`). **Observabilidade:** o guard nega e registra a leitura explícita (classe `token`) e **marca** a genérica (classe `env`); `env` entrou no AC-19, no AC-15 (3b e negativos), no AC-21 e no critério D2 (10 marcações `env` em 14 dias: revisar). Q10b fechada |
| **Produto aprovado: Q3, Q5, Q6, Q12, Q15, Q16, Q17** | Marcadas como decididas na tabela, no resumo do topo da seção 7, no ADR (item i) e nos gates: Q5 deixou de ser gate do 2b-agent; Q3, Q6 e Q12 deixaram de ser gates do Step 1; o 2a não tem gate aberto |
| **Técnicas pendentes: Q1, Q2, Q4, Q7, Q8, Q11, Q13, Q23** | Seguem como recomendação do plano; o resumo do topo da seção 7 traz agora, para cada uma, o tipo, a recomendação e o **impacto de aceitar** (o dono pediu o impacto antes de aceitar) |

**Adiado:** nada. **Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.


## Changelog da iteração 12

Fonte: reavaliação da opção C (Critic: 1 bloqueante de texto; Architect: sem bloqueantes).

| Item | Estado |
|---|---|
| **Bloqueante: AC-12 tratava a Q22 como aberta** | **Atendido.** "Pendente da Q22" e "condicionais à Q22" saíram; o escopo é o decidido e os testes são **obrigatórios** (sob bypass: `Write` fora das âncoras e `Bash` genérico liberados; âncoras e exigência de envelope continuam; expirado, tudo volta a negar). Varredura: nenhum outro AC ou passo trata como aberta uma Q já decidida; saíram também Q20, Q21 e Q22 dos follow-ups do ADR |
| 1. `anchor-integrity` circular | Aplicado: a conferência do digest do binário e do `hooks.json` passou para o **servidor do fork** (`src/omhl/anchors.ts`, processo à parte), que a reporta no campo `anchors` das respostas; a tabela de garantias e o D1 foram ajustados; o texto diz que a conferência **não detecta troca deliberada** que regrave também o registro (para isso, `omhl audit verify --anchors` com a cópia confiável do dono) |
| 2. Log de auditoria | Aplicado: o diretório do log é âncora `anchor-hard`; `attested.auditHead` (`seq` e `hash`) gravado pelo servidor nos registros `unit` e `closure`, `head-ack` no log, arquivo `.head` conferido no `SessionStart` seguinte (`audit-gap`); AC-19 diz que a cadeia detecta adulteração **descuidada**, não deliberada; `<repo-id>` definido; lock de escrita no orçamento da A14 |
| 3. S8 | Aplicado: linhas `legit-read` cercando as aberturas legítimas e `watch-heartbeat` a cada 60 s (ausência de eventos não vira "tudo ok") |
| 4. D1 a D4 | Aplicado: `anchor-hard` e `anchor-soft` separados (só a primeira conta no D2); observável do D3 definido; limiares D2 a D4 rotulados provisórios e arbitrários |
| 5. Redução de escopo | Aplicado: AC-15 com teste dirigido por tabela (rota x classe) e uma fixture de `Bash` por classe; a classe `token` passou de **negar** para **marcar** (negar não compensa o atrito, porque `env \| grep` contorna); o digest do binário cobre troca não coordenada |
| 6. Cliente de GitHub | Aplicado: 403/429 e `Retry-After` viram `error` `rate-limited` sem consumir o retry; paginação de `check-runs` e `reviewThreads`; permissões "Checks", "Pull requests" e "Metadata" somente leitura; teste real manual no primeiro repositório com GitHub (R22) |
| 7. `omhl audit verify` e Q1/Q2 | Aplicado: o Step 6 ficou só com `omhl report`; o Step 1 diz que o schema congelado assume as recomendações de Q1 e Q2 e que outra resposta reabre o schema |

**Adiado:** nada. **Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.


## Changelog da iteração 13

Fonte: [owner-decisions.md](../drafts/owner-decisions.md), seção "Decisões técnicas do dono". Aplicado depois do ajuste da iteração 12.

| Decisão | O que mudou no plano |
|---|---|
| **Q1: OK, com abstração** | Interface mínima do agente: `{kind, statement, directives?, restsOn?, supersede?, revoke?}`; o servidor deriva `origin`, `target`, relações e `checkedAgainst` (de `applicable_directives`); campos enviados a mais: `FIELD_NOT_ACCEPTED`; `DIRECTIVE_NOT_APPLICABLE`. AC-4 reescrito; AC-3 (b) ajustado; Step 3 reescrito |
| **Q2: mudança de spec, N/A só pelo juiz Haiku** | Saiu o N/A como asserção humana (AC-3 (e), conjunto de condicionais de Q18, `contract/commands.md`). Novo **Step 7** (juiz como subagente de plugin com `model: haiku` e só leitura; prompt-modelo fixo; `branchRead` observado pelo hook; `attested.judge` afirmado só quando o `agent_type` é o do juiz; formato do veredito; falha fechada) e **AC-23**. **Decisão de escopo:** o juiz é **marco logo depois da fatia**, porque a fatia não tem diretivas `product` (Q3), a fatia fica mínima e o juiz é o maior risco; a fatia traz só o lado do servidor, testado com envelopes fixture. Verifiquei na doc de hooks que `PreToolUse`/`PostToolUse` de subagentes trazem `agent_id` e `agent_type` (F16, **fato**); a string exata de `agent_type`, o subagente de plugin com Haiku, a leitura do `tool_response` e a qualidade do Haiku são **suposições** (A30 a A32) medidas no novo **S9**. **Seção 8.1 "Emendas ao spec"** |
| **Q7** | Envelope de 2 min (era 45 s); `ENVELOPE_REPLAYED` (o servidor recusa `nonce` repetido na janela; consequências de segurança descritas na Q7); a evidência vale até o próximo commit: saíram `maxAgeSec`, `issuedAt`, `EVIDENCE_EXPIRED`, o item de corte (5) e os ACs/testes de validade por tempo; R4 reavaliado |
| **Q11** | O fluxo de proposta e aval vale para premissas (`--approve <proposalId>`), com ciclo de vida da proposta (`proposed`, `approved`, `consumed`, `expired`) na seção 2 e testes no AC-3 (e) |
| **Q13** | Saíram `scriptDigest`, `SCRIPT_DIGEST_MISMATCH` e o digest fixado no `criteria`. Entram instrução ao agente, verificação por git e **gate humano** no fechamento: novo **AC-22** (`attested.checkFiles`, asserção `check-edit-ok` presa a `{process, headSha, hash da lista}`, comando `/omhl:confirm-check-edit`, `closure.checkEdits`). R11 continua como resíduo |
| **Q4, Q8, Q23** | Fechadas como decididas |
| **Resumo e gates** | O resumo do topo da seção 7 diz que as 23 questões estão decididas ou são regra e lista só as **medições** que ainda condicionam o plano (P0, S1, S2, S7, S8, S9); os gates dos passos ficaram sem questão aberta |

**ACs novos:** AC-22 (na fatia), AC-23 (Step 7, fora da fatia). **ACs com partes removidas:** AC-3, AC-4, AC-6, AC-7, AC-16. A fatia passa a ter 21 ACs (8 `plan-added`). **Risco novo:** R23 (juiz Haiku).

**Não executado:** lint, `knip`, `typecheck` e testes do Hexlog seguem só lidos.

## Changelog da iteração 14

Fonte: mensagem final do líder (Critic APPROVE; Architect SÓLIDO COM RESSALVAS, 2 bloqueantes só do Step 7).

| Item | Estado |
|---|---|
| B1 (id escopado do subagente) | Aplicado no marco 2: conferência do `agent_type` por **igualdade exata** com `omhl:omhl-judge`, nunca por sufixo nem pelo nome simples; negativo do subagente local homônimo no AC-23; `.claude/agents/**` e `~/.claude/agents/**` entraram no AC-15 (`anchor-soft`); A30 ajustada |
| B2 (juiz repetido até acertar) | Aplicado: o primeiro veredito `applicable`/`inconclusive` por (unidade, diretiva) é terminal (`JUDGE_VERDICT_FINAL`), teto de 2 invocações por par (`JUDGE_CAP_EXCEEDED`), tudo registrado no log; negativos no AC-23 |
| 3. Juiz: `subagent_type`, `model`, `run_in_background`, `agent_id`, entrada observada, hook `ask` do dono | Aplicado no marco 2 (Step 7, S9, R23) |
| 4. Q1 abstrata não pode mentir | Aplicado: campo derivado renomeado para `applicableAtRegistration` (`derived: true`); `attested.directivesRead` ⊇ `product` aplicáveis (`DIRECTIVES_NOT_READ`, cortável); `register_premise` como união discriminada por `kind`, com o ramo `not-applicable` só com `attested.judge` |
| 5. Q13 | Aplicado: `git diff --name-only --no-renames` contra `origin/<baseRef>`, falha fechada (`BASE_UNRESOLVED`); `skip-worktree`/`assume-unchanged` no R11; `.omhl/check-files.txt` é `anchor-hard`; o nome do gate promete mais do que cobre |
| 6. Q7 e R4 | Aplicado: nonces por processo do servidor; o resíduo do R4 vale também para `review-threads` |
| 7. Q11 | Aplicado: aval de premissa em um passo registrado como escolha aceita do dono, com o aval em dois passos como alternativa |
| 8. Seção 8.1 | Aplicado: 5 linhas novas (escopo do bypass, CLI de uso manual, genéricas de escrita, `.omhl/outbox` e `.omhl/proposals` escritos pelo agente, renomeação do campo) |
| 9. Medição de que cada passo depende | Aplicado: uma linha "Medições de que depende" em cada passo |
| 10. Changelogs para outro arquivo | Feito: este arquivo |
| 11. Marcos para outro arquivo | Feito: [omhl-v1-marcos.md](omhl-v1-marcos.md) (Step 6, Step 7, AC-21, AC-23, R23, S8, S9, F15, F16, A28 a A32); a fatia mantém o campo `attested.judge`, o lado do servidor do juiz e o AC-19 |
| 12. Condensação | Feita: seção 2.1 em uma tabela, "Viable Options" em uma tabela, questões decididas em uma linha cada. Nenhuma decisão, passo ou AC foi retirado |

## Changelog da iteração 15 (rodada final)

Fontes: [review-7-critic.md](../drafts/review-7-critic.md) (ITERATE, 2 bloqueantes e 2 majors) e [review-7-architect.md](../drafts/review-7-architect.md) (SÓLIDO COM RESSALVAS, 4 bloqueantes). Sem decisão nova do dono ([owner-decisions.md](../drafts/owner-decisions.md) não foi reaberto). O plano segue `pending approval`.

### Decisões para o dono

Itens que **não** apliquei, com a recomendação do Planner:

| Item | O que é | Recomendação |
|---|---|---|
| Architect M1 | Mover o S8 (detecção barata de leitura do segredo, 2 h) e o S9(i)-(iv) (subagente de plugin, `agent_type`, `SubagentStart`, reescrita do prompt) dos marcos para o Step 0, porque o Step 1 congela o schema com `attested.judge` e o segredo HMAC antes dessas medições (Princípio 2) | **Mover o S9(i)-(iv) para o Step 0; deixar o S8 e o S9(v)-(vii) nos marcos.** O S9(i)-(iv) é barato, só lê hook e payload, e valida o campo `attested.judge` que o Step 1 congela (e agora também a fila de aprovações e a negação do `SendMessage`); o S8 só decide se o Step 6 tem watcher, sem tocar o schema. Se o dono preferir não aumentar o Step 0, o custo é aceitar que um S9 refutado reabra o campo `attested.judge` num schema já congelado |
| Architect, antítese | Trocar o HMAC por um campo `omhl` simples, sem MAC: sem sandbox, quem lê o segredo forja o campo com o mesmo esforço; o aparato (HMAC, JCS Rust x npm, digests, âncoras, 8 ACs `plan-added`) compra "forjar deixa rastro", que depende de detectar a leitura (S8, só medido depois da fatia). O Architect mesmo sintetiza: manter o HMAC | **Manter o HMAC**, como o dono já escolheu (freio com HMAC). Registrado aqui apenas que a alternativa foi considerada: o campo sem MAC é mais barato, mas perde a distinção entre "o hook esqueceu" e "alguém forjou deliberadamente", que o MAC dá de graça no servidor. A tensão real é a dependência do S8; fica coberta pela recomendação do M1 e pelos critérios D1 a D4 do Step 6. Reavaliar se o AC-17 (JCS Rust x npm) custar mais que o previsto: o plano B sem JCS já existe |
| Critic, pergunta aberta 1 | `UserPromptExpansion` entrega `command_name` namespaced (`omhl:open`) ou simples? | **Tratada como medição.** Entrou como S4(v); o conjunto de comandos reconhecido pelo hook guarda a forma medida. Sem decisão do dono |
| Critic, pergunta aberta 2 | Depois do `omhl setup --refresh-digest`, a janela em que o hook nega tudo exige bypass? Conflita com "sem bypass antes do marco" | **Tratada como medição mais regra de runbook.** S4(vi) mede se o `!` do dono passa pelo `PreToolUse`; a regra no esqueleto do runbook (Step 4) é rodar o `--refresh-digest` por `!` ou fora do `claude`, então não há bypass. Sem decisão do dono |

Duas escolhas que tomei dentro do que o texto permitia, para o dono vetar se quiser: (1) o resultado do juiz e o N/A em **um tipo só**, `judge-ruling`, com o **primeiro registro por (unidade, diretiva) final, seja qual for o `ruling`** (na iteração 14 só `applicable` e `inconclusive` eram terminais; um N/A registrado agora também não é trocado); (2) a anulação (premissa humana e `annulment`) cai no **processo de diretivas**, e `annul_directive` com `scope = unit` recusa unidade que já tem `closure`.

### Critic

| Achado | O que foi feito |
|---|---|
| B1 servidor de `create_directive` e `annul_directive` só no Step 5 | **Aplicado (opção preferida do Critic).** O Step 3 passa a criar `create-directive.ts` e `annul-directive.ts` (lado servidor, envelopes fixture) e a verificar AC-3 (c) e (h) e AC-8 (servidor); o Step 5 ficou com o aval em dois passos, os comandos e a projeção. Ajustados "Criar" e "Verificação" do Step 3, o Step 5, o AC-8 (separado em lado servidor e lado hook), a Leitura rápida e o §6 |
| B2 e2e do marco fatia-1 só nasce no Step 5 | **Aplicado.** O Step 4 cria a variante agent-only de `tests/slice.rs` e `scripts/e2e.sh` e o esqueleto de `docs/e2e-runbook.md`; `cargo test --test slice` entra na Verificação do Step 4 como **porta do marco fatia-1**; o Step 5 estende os três para o caminho humano; AC-E2E e "Marco fatia-1" reescritos |
| M1 Q21 dentro e fora da fatia | **Aplicado: entra.** Saiu a linha de 3.1; `--in <ancestral>` entrou na gramática de `contract/commands.md` e `inProcess?` no schema `human` congelado; o AC-20 ganhou os testes do ancestral aberto, do ancestral fora de `ancestors` (`PROCESS_NOT_ANCESTOR`) e do ancestral fechado (`ANCESTOR_NOT_OPEN`); os dois códigos entraram em `errors.md`; "depende da Q21" e "Q21 aberta" removidos |
| M2 tipos fora da semente | **Aplicado**, junto com o B1 do Architect (abaixo) |
| Melhoria 1 (Q20 e Q21 tratadas como abertas) | **Aplicado:** marco fatia-1, ordem dos passos, "vira a Q20" do ADR |
| Melhoria 2 (`setup.rs` 2a x renderização do Step 4) | **Aplicado:** o 2a entrega segredo, digest, marcador, `deny` e `mcp add`, com teste `tests/setup.rs`; a renderização do `plugin/` é do Step 4 |
| Melhoria 3 (quem executa o Step 0; grep `a verificar`) | **Aplicado:** bloco "Quem executa" (o dono digita P0, S1, S4 e S7(i); o resto em `-p` ou `tmux`); o grep passou a olhar só P0, S1 a S5 e S7, com S6, S8 e S9 liberados (§6 e Step 0) |
| Melhoria 4 (A16 no Step 0) | **Aplicado:** "A1 a A8 e A17 a A19" |
| Melhoria 5 (`npm run spell` e `lint:md`) | **Aplicado** na Verificação do Step 1 e no §6 |
| Melhoria 6 (resíduos do spec) | **Aplicado** (ver Architect B3) |
| Melhoria 7 (pontuação do AC-22, datas) | **Aplicado:** frase do AC-22 refeita; plano e ADR datados de 2026-10-31 |
| Melhoria 8 (`range-diff` na 1ª tag) | **Aplicado:** `PREV` vazio na 1ª tag e `range-diff` só se `PREV` existir |
| Melhoria 9 (fonte inline do R23) | Não aplicado (a precedência foi conferida pelo Critic na doc de hooks; sem número novo a citar) |
| Perguntas abertas | Ver "Decisões para o dono" |

### Architect

| Achado | O que foi feito |
|---|---|
| B1 conjunto de tipos, relações e gates | **Aplicado, a opção mais simples.** Conjunto completo e congelado em `contract/definitions.json` no Step 1: **8 tipos** (`unit`, `premise`, `evidence`, `closure`, `directive`, `annulment`, `granularity`, `judge-ruling`), **6 relações** (`anchored-in`, `rests-on`, `extends`, `evidence-of`, `annuls`, `ruled-on`, todas `derivesFrom`) e **nenhum gate** (o spec põe a lógica fixa no `close_unit`). O N/A **é** o `judge-ruling` com `ruling = not-applicable`. Justificativa: o servidor precisa ver todo resultado do juiz para tornar o primeiro final, então um tipo cobre os dois fatos; `kind` de `premise` misturaria a premissa do agente com a decisão de um juiz; um registro por alvo `jr.<directiveId>` faz de "final" uma checagem de existência. AC-1 atualizado (lista e prova com `describe_type` contra `definitions.json`); R9 e 3.1 registram que definição adicionada depois só vale para processos novos e que o processo de diretivas, único, nunca a recebe |
| B2 termo `verdict` | **Aplicado.** `verdict` virou `ruling`; `judge-verdict` virou `judge-ruling`; alvo `jv.` virou `jr.`; `JUDGE_VERDICT_FINAL` virou `JUDGE_RULING_FINAL` (tabela de erros, AC-23, R23, Step 7). Varri também `phase`, `fase`, `plan`, `plano`, `critic`, `milestone`, `claim` e `omc`: nenhuma ocorrência em nomes de contrato ou de `src/omhl/` (a prosa em português segue com "veredito"). Entrou um `grep -w` sobre os contratos no Step 1 e no §6; para `src/omhl/` vale o `no-flow-terms.spec.ts` do próprio Hexlog |
| B3 precedência spec x plano | **Aplicado.** 8.1 reescrita ("emendas já aplicadas ao spec; a coluna do meio é histórica"), com as linhas novas "Não se aplica (Q2)" e "Escopo da v1 (adiados, ver 3.1)" (Q15 a Q17); 3.1 abre dizendo que são adiamentos, não emendas; "o dono atualizará o spec" removido (§7 e título da 8.1). No spec: `checkedAgainst` da ontologia, "verdict per directive", "comandos de não se aplica" (agora skill do agente com juiz), "envelope em toda tool de escrita" e os itens abertos já resolvidos limpos; a transcrição ganhou notas de histórico; o cabeçalho aponta para 8.1 e 3.1 |
| B4 `tool_response` MCP ilegível | **Aplicado.** Plano B em "Decisões de saída" do Step 0 (espelho por `tool_input` mais o tipo do evento; se nem o tipo distinguir sucesso de erro, parar e perguntar ao dono); `omhl check` sai com 0 para `pass`, `fail` e `error` (AC-5 e Verificação 2a); a P0 grava também um `Bash` de saída longa, um `Bash` que falha e um erro de domínio MCP |
| M1 (S8 e S9 para o Step 0) | **Não aplicado**; ver "Decisões para o dono" |
| M2 e M3 (juiz) | **Aplicado** no [marco 2](omhl-v1-marcos.md): `SendMessage` ao juiz negado; teto de 2 contado por (unidade, diretiva) fora da tool `Agent` (aprovações e retomadas); fila de aprovações por sessão consumida pelo primeiro `SubagentStart` com o `agent_type` esperado; gramática de uma linha para `directive=<target>`; matcher `Agent|SendMessage` e evento `SubagentStart` no `hooks.json` ao entrar o marco 2 (Step 4); A31 e S9 atualizados |
| M4 processo de cada registro | **Aplicado.** Tabela "Em que processo cada registro cai" na seção 2 (o que o servidor confere por tool; a anulação e a premissa humana caem no processo de diretivas; `annul_directive` com `scope = unit` confere que a unidade existe e não tem `closure`) |
| M5 (A16, tabela de medições, AC-22, "rev. 16", datas) | **Aplicado:** A16 fora do Step 0; a tabela do §7 ganhou S3, S4 e S5 e a lista do Step 1 diz "P0, S1 a S5, S7, S8, S9"; AC-22 refeito; "rev. 16" virou "iteração 15"; datas 2026-10-31. A separação do Q7 em linhas não foi feita (opcional) |

### Verificações, contagens e riscos

- **ACs:** 21 na fatia (AC-1 a AC-20 e AC-22; 8 `plan-added`: AC-13 a AC-19 e AC-22), mais o AC-E2E; AC-21 e AC-23 nos marcos. Nenhum AC novo; AC-1, AC-5, AC-7, AC-8, AC-20 e AC-23 ganharam texto de prova.
- **Itens novos:** `contract/definitions.json`; códigos `PROCESS_NOT_ANCESTOR` e `ANCESTOR_NOT_OPEN`; renomeado `JUDGE_VERDICT_FINAL` para `JUDGE_RULING_FINAL`; campo `inProcess?` no schema `human`.
- **Resíduo de risco:** o conjunto de definições é agora o ponto de maior custo de erro (R9): errá-lo exige processo novo de diretivas ou a sucessão adiada (Q15).

## Changelog da iteração 16 (rodada final)

Fontes: [review-8-critic.md](../drafts/review-8-critic.md) (ITERATE, 2 bloqueantes leves e 1 major) e [review-8-architect.md](../drafts/review-8-architect.md) (SÓLIDO COM RESSALVAS, 1 bloqueante e 5 ressalvas). Sem decisão nova do dono ([owner-decisions.md](../drafts/owner-decisions.md) não foi reaberto). O plano segue `pending approval`. Arquivos editados: o plano principal, [omhl-v1-marcos.md](omhl-v1-marcos.md), o [spec](../specs/deep-interview-oh-my-hexflow-design.md) e este changelog.

### Decisões para o dono

| Item | O que é | Recomendação |
|---|---|---|
| **Q24** (nova, no §7 do plano) | Mover o S9(i)-(iv) (subagente de plugin com `model: haiku`, `agent_type`, `SubagentStart`, reescrita do `prompt`, `SendMessage`) dos marcos para o Step 0 (Architect M1, já recomendado na iteração 15) | **Aplicar.** Não aplicada: é do dono. Não bloqueia o início do Step 1, porque só muda **quando** o S9(i)-(iv) roda e não o conteúdo do Step 1 (o campo `attested.judge` é congelado de qualquer modo); precisa de resposta antes de congelar o schema de `attested`, o fim do Step 1 |

Escolhas que tomei dentro do que o texto permitia, para o dono vetar: (1) a **confirmação extra** da anulação de projeto é o segundo passo do aval, com o hash armado a partir do que o próprio humano digitou (sem arquivo do agente) e uma só asserção que traz o `proposalId`; (2) o provedor falso de GitHub varia **pelo dado** (`headRef` terminado em `-ci-red`), não por canal; (3) a `key` do lote do juiz é `jr:<directiveId>`; (4) a premissa humana e a anulação do mesmo lote **não** têm relação nomeada; (5) premissa repetida: `created: false`, ou promoção a `human` por `supersedes`; (6) nome da unidade com teto de 58 caracteres; (7) `details` do recibo truncado em 2.000 caracteres e `changedFiles` do juiz truncado em 50 nomes de 200 caracteres.

### Architect

| Achado | O que foi feito |
|---|---|
| B-A "segunda asserção" não cabe no schema congelado | **Aplicado, opção 2 (reuso do aval em dois passos).** A "confirmação extra" do spec é o segundo passo do aval: o 1º comando (`annul <id> --scope project --reason ...`) mostra o conteúdo canônico e arma o hash completo de `{directiveId, scope, reason}`; o 2º, com `--approve <hash completo>`, cunha **uma única** asserção presa a esse conteúdo e ao `proposalId`. O schema não ganha campo (`proposalId?` já existia; passou a ser obrigatório nessa anulação) e o ciclo de vida de uma asserção pendente por tool não muda. O servidor recusa `scope = project` sem `proposalId` (`PROJECT_SCOPE_NEEDS_APPROVAL`). Corrigidos: tabela de processos e parágrafo do aval (§2), AC-8 (servidor e hook), cortáveis (o aval de anulação de projeto **não** é cortável), Step 1 (`commands.md`, schema `human`), Step 3 e Step 5, ADR, spec. Nenhuma menção a "segunda asserção" sobra como regra |
| R1 "primeiro ruling é final" não é atômico | **Aplicado** (ver Critic M1): `key = "jr:<directiveId>"`, com a prova de que a `key` vale por processo |
| R2 códigos de erro faltantes | **Aplicado:** `DIRECTIVE_ALREADY_ANNULLED`, `UNIT_CLOSED`, `UNIT_NOT_FOUND`, mais `PROJECT_SCOPE_NEEDS_APPROVAL` e `UNIT_NAME_INVALID`, em `errors.md` (congelado no Step 1) e usados no AC-8, na tabela de processos e no Step 3 |
| R3 8.1 incompleta; notas históricas | **Aplicado:** três linhas novas na 8.1 (Q21 `--in`; anulação com `scope = unit` recusando unidade fechada e a confirmação extra; primeiro ruling final) e os mesmos itens em "o que o plano acrescenta ao spec"; spec editado onde cabe (Premise, Directive, Not applicable, Comandos). Removidas dos marcos as notas "Aperto em relação à iteração 14" e "(escolha da iteração 15…)" |
| R4 `pattern`/`patternProperties` e teto de 16.000 | **Aplicado:** `definitions.json` sem `pattern` nem `patternProperties` (teste `jq` no §6 e no Step 1); o AC-1 compara de forma exata (o arquivo não tem esses campos). **Correção de um detalhe do parecer:** o catálogo de `format` do Hexlog só tem `git-sha` (7 a 40 caracteres) e `attachment`, então hash de 64 caracteres não tem `format`: usa `type: string` com `minLength` e `maxLength` 64, conferido pelo servidor; só o SHA de commit usa `git-sha`. Cada tipo também precisa caber em 16.000 caracteres canônicos (o `evidence` embute o recibo). Teste de teto por registro no Step 3 e no AC-7 (maior `evidence` com 3 tentativas, maior `closure`), com `details` truncado em 2.000 caracteres por tentativa |
| R5 `unit-<process>` e `directiveId` | **Aplicado:** nome da unidade (e do processo) com teto de 58 caracteres (`UNIT_NAME_INVALID` em `open_unit`), para `unit-<process>` caber no segmento de 63; `directiveId` é um `Name` único **sem pontos** (§2, tabela de tipos, gramática do juiz nos marcos) |
| Renomear `kind: "not-applicable"` | **Aplicado:** o ramo de `register_premise` do juiz é `kind: "ruling"` (§2, Step 3, marcos, AC-23) |
| `applicableAtRegistration` na premissa da anulação | **Aplicado:** campo opcional, ausente na premissa humana da anulação |
| M1 do Architect (S9(i)-(iv)) no §7 | **Aplicado como Q24** (ver "Decisões para o dono") |
| "Dois primeiros" do Step 1 | **Aplicado:** AC-17 é o primeiro **teste**; as definições, o recibo e o `criteria` são o primeiro grupo de **schemas** |
| `--in` ausente cai em `cross-process-currency`; unidade de `applicableAtRegistration` | **Aplicado:** a recusa nativa traz `details.hint` mandando usar `--in <ancestral>` (sem código novo); `applicableAtRegistration` vem da unidade **ativa**, não do ancestral (AC-20, Step 3) |

### Critic

| Achado | O que foi feito |
|---|---|
| B1 porta do marco fatia-1 não executável | **Aplicado.** Porta: `cargo test --features fake-github --test slice` (AC-E2E, "Marco fatia-1", Step 4, Step 5, §6, ADR). **Provedor falso:** no build de produção (sem a feature) continua proibida a seleção por variável de ambiente ou arquivo, e o binário não contém o provedor (teste `! grep -qa 'fake/repo' target/release/omhl`); no build com a feature ele é fixo e determinístico, função só do que o `omhl check` já recebe. **Negativo "CI vermelho":** o provedor devolve `failure` quando o `headRef` da unidade termina em `-ci-red`; o e2e ganhou esse quarto negativo (nenhum AC novo: é o `EVIDENCE_FAILED` do AC-7 no caminho completo). **Por que esta opção:** uma segunda feature (`fake-github-red`) exigiria dois builds e duas rodadas de teste para um negativo só; um argumento de teste mudaria a forma única de `omhl check` aceita para o MAC (AC-16); a convenção de branch não acrescenta flag, variável, arquivo nem schema. **Pré-requisitos do `slice.rs`:** build do fork (`npm run build` e `scripts/omhl/build.ts`, forma exata a confirmar no Step 1) feito antes pelo `e2e.sh`; caminho do bundle em `OMHL_FORK_BUNDLE` (lida só pelo teste e pelo `e2e.sh`, nunca pelo `omhl`), ausente = falha; `node` no PATH; `slice` com `required-features = ["fake-github"]`, para o teste sem a feature falhar em vez de passar com 0 casos |
| B2 segunda asserção | **Aplicado** (ver Architect B-A) |
| M1 `JUDGE_RULING_FINAL` não atômico | **Aplicado.** O lote do juiz usa `key = "jr:<directiveId>"`. **Prova de escopo:** o `register` decide a `key` sob o lock **do processo** (`src/commands/process.ts:54-69`: "grava o lote numa única linha do log de `process`, sob o lock só da origem"; ordem de recusa com a `key` no nível 5), o fingerprint ignora `key`, `agent` e `model` (`src/domain/chain.ts`), e o `judge-ruling` mora sempre no processo da unidade (um processo por unidade); logo `jr:<directiveId>` é único por (unidade, diretiva) e `judge:<unit>:<directive>` seria redundante (e gastaria o teto de 200 caracteres). Cabe em `BATCH_KEY_MAX` (3 + até 63). A checagem de existência saiu: a `key` cobre o caso sequencial e o concorrente. Mapeamento por prefixo no §2 (`human:`, `jr:`, `unit:`, `bootstrap:`); teste de duas gravações concorrentes (`Promise.all`) no Step 3 e no AC-23 |
| Melhoria 1 (Step 4 "parte humana") | **Aplicado:** removida da ordem; o Step 4 só tem a parte agent-only, e o que depende do handler humano ficou com o 2b-humano (`UserPromptSubmit`, `SessionEnd`, bypass) e o Step 5 (`/directive`, `/annul`, `confirm-check-edit`, `--in`) |
| Melhoria 2 (`checkFiles`, `BASE_UNRESOLVED`, `directivesRead`) | **Aplicado:** o hook emite `checkFiles: []` no 2b-agent; o servidor recusa o campo ausente (`ASSERTION_INVALID`); `directivesRead` fica no 2b-agent (AC-4 lado hook entrou na verificação); `checkFiles`/`BASE_UNRESOLVED` ficam no 2b-humano; o gate de edição está inativo no marco fatia-1 |
| Melhoria 3 (códigos do AC-8) | **Aplicado** (ver Architect R2) |
| Melhoria 4 (`definitions.json` circular) | **Aplicado:** `definitions.json`, `receipt.schema.json` e `criteria.schema.json` congelados juntos, no mesmo commit, com teste de igualdade dos subschemas embutidos |
| Melhoria 5 (AC-1 e `pattern`) | **Aplicado** (ver Architect R4) |
| Melhoria 6 (assinador TS e vetores de envelope) | **Aplicado:** `contract/fixtures/envelope-vectors.json` no Step 1; `test/omhl/support/sign-envelope.ts` (só teste) no Step 3 |
| Melhoria 7 (nomes no juiz) | **Aplicado:** ramo `kind: "ruling"`; `changedFiles` são **todos** os arquivos alterados da unidade (mesmo diff do AC-22, sem filtrar pelos padrões de check); como nome de arquivo é canal controlado pelo agente, a lista é truncada em 50 nomes de 200 caracteres, escapada como JSON e tratada como dado pelo `omhl-judge.md` |
| Melhoria 8 (S9(i)-(iv) só no changelog) | **Aplicado como Q24 no §7**, não aplicada; "Step 1: nenhum aberto" reescrito (ver "Decisões para o dono") |
| Pergunta aberta 1 (anulação x premissa humana) | **Registrada como decisão do plano:** sem relação nomeada, vínculo por `nonce` e lote atômico; uma 7ª relação só para auditoria congelaria uma definição que nada lê (§4, "Definições"; confirmar antes de congelar as 6 relações) |
| Pergunta aberta 2 (`premise.<hash>` repetido) | **Respondida:** o `applicable_directives` não lê `premise` (lê `directive`, `annulment` e `judge-ruling`), então a repetição não o afeta; para a premissa da unidade o servidor não grava duplicata (`created: false`, ou promoção a `human` por `supersedes`); na anulação a repetição é recusada por `DIRECTIVE_ALREADY_ANNULLED` |

### Verificações, contagens e riscos

- **ACs:** 21 na fatia (AC-1 a AC-20 e AC-22; 8 `plan-added`), mais o AC-E2E; AC-21 e AC-23 nos marcos. **Nenhum AC novo.** AC-1, AC-7, AC-8, AC-20, AC-22, AC-E2E e AC-23 ganharam texto de prova.
- **Códigos novos em `errors.md`:** `DIRECTIVE_ALREADY_ANNULLED`, `UNIT_CLOSED`, `UNIT_NOT_FOUND`, `UNIT_NAME_INVALID`, `PROJECT_SCOPE_NEEDS_APPROVAL`. **Arquivo novo de contrato:** `contract/fixtures/envelope-vectors.json`.
- **Resíduos:** o aval em dois passos protege a anulação de projeto como freio, não barreira (o servidor confere que o `proposalId` é o SHA-256 do JCS de `{directiveId, scope, reason}`, desde a iteração 17, mas não que houve dois comandos; o agente com o segredo forja, como em tudo o mais, Q14); o nome de branch `-ci-red` é uma convenção de teste que só existe no build com a feature.

## Changelog da iteração 17 (passada final de melhorias)

Fontes: [review-9-critic.md](../drafts/review-9-critic.md) (APPROVE com ressalvas, sem bloqueante; M-a e M-b) e [review-9-architect.md](../drafts/review-9-architect.md) (SÓLIDO COM RESSALVAS, sem bloqueante; 5 ressalvas). Sem decisão nova do dono ([owner-decisions.md](../drafts/owner-decisions.md) não foi reaberto). O plano segue `pending approval`; a Q24 continua pendente do dono. Arquivos editados: o plano principal, [omhl-v1-marcos.md](omhl-v1-marcos.md) (uma frase sobre o replay do juiz) e este changelog.

### Critic

| Achado | O que foi feito |
|---|---|
| M-a `check.sh` x `slice` | **Aplicado, opção 2.** O `check.sh` roda `cargo test --features fake-github --lib --bins` mais um `--test` por `tests/*.rs`, **menos `slice`** (lista montada pelo script). O `slice` é só do `scripts/e2e.sh`, a porta oficial do marco, que reconstrói o bundle; o `slice.rs` também falha se o bundle for mais antigo que o commit do `HEAD` do fork. Escolhida por ser a mais simples: a opção 1 faria o `check.sh` (verificação de todo passo) pagar o build do fork a cada execução. Corrigidos: Step 0 (definição), AC-E2E, marco fatia-1, Step 4, Step 5, §6 |
| M-b prazo da Q24 | **Aplicado.** Resposta "até o fim do Step 0 (no mais tardar, com tempo de rodar o S9(i)-(iv) antes de congelar o `attested`)"; ponto de verificação no Step 1 ("perguntar ao dono a Q24 antes de congelar o `attested`"); alinhados o gate do Step 1 e a linha da Q24 no §7 |
| Melhoria 1 (`proposalId` sem checagem) | **Aplicado:** o servidor recomputa `SHA-256(JCS{directiveId, scope, reason})` e exige `proposalId` igual (`HUMAN_ASSERTION_MISMATCH`); novo negativo no AC-8, tabela de processos e Step 3; resíduo da iteração 16 corrigido |
| Melhoria 2 (estado `armed`) | **Aplicado:** tipo `armed` em `src/state/`; o 1º passo do aval **só arma e não substitui** a asserção pendente da mesma tool (substitui quem cunha); `armed` expira em 10 min e no `SessionEnd` |
| Melhoria 3 (`hooks.json` no 2b-humano) | **Aplicado:** bullet no "Criar" do 2b-humano |
| Melhoria 4 (`init_idempotent`) | **Aplicado:** subcomando determinístico `omhl init` (a skill `/omhl-init` o chama e depois chama `bootstrap_project`); o teste executa `omhl init` 2x |
| Melhoria 5 (`bootstrap:`) | **Aplicado:** frase em `errors.md` com o mapeamento por prefixo |
| Melhoria 6 (detalhes do e2e) | **Aplicado:** branch `feature-ci-red` no repositório temporário; `decision` com `restsOn` da `premise` para gerar o órfão (`ORPHAN_DEPENDENTS`) no negativo "premissa revogada"; `cargo build --release` antes do `grep` |
| Melhoria 7 (replay do juiz) | **Aplicado:** o `judge{agentId,...}` entra no fingerprint; um 2º juiz com o mesmo parecer dá `JUDGE_RULING_FINAL`, não `replayed` (plano e marcos) |

### Architect

| Achado | O que foi feito |
|---|---|
| 1 `no-github-remote` / `no-github-token` sem teste | **Aplicado:** `select_provider(remote, token)` é função pura, sem `cfg`, testada por tabela em todo build; a feature só injeta o falso depois, na composição |
| 2 guard do binário de produção | **Aplicado:** `cargo build --release && ! grep -qa 'fake/repo' target/release/omhl` no `check.sh`, no §6 e na Verificação 2a |
| 3 `clippy --all-targets` sem a feature | **Aplicado:** `cargo clippy --all-targets --features fake-github -- -D warnings` |
| 4 teto de 16.000 do tipo | **Aplicado:** teste Rust `definitions_size` mede o JCS de cada tipo contra `RECORD_TYPE_MAX_CHARS` (no `check.sh` e no §6); sem `jq`, porque `tojson` não é JCS |
| 5 premissa repetida (ler e gravar) | **Aplicado, mutex em vez de só declarar o resíduo:** mutex assíncrono em memória por (projeto, processo) (`src/omhl/with-lock.ts`) em `register_premise` (premissa/decisão) e `annul_directive` (também lê e grava); teste com `Promise.all`. Key permanente descartada (bloquearia regravar depois de `revoke`). Resíduo **R24**: dois servidores no mesmo projeto não compartilham o mutex |
| Melhoria (handshake com features) | **Aplicado de forma mais simples que o handshake:** a feature mora no binário Rust, não no fork; o `omhl setup` recusa, via `cfg!`, rodar sem `--home` num binário com `fake-github`, e o `omhl --version` lista as features. O handshake com o fork fica como follow-up (não é preciso) |

### Verificações, contagens e riscos

- **ACs:** sem AC novo (21 na fatia, mais o AC-E2E; AC-8 ganhou um negativo; AC-1 e o AC-E2E, texto de prova). **Risco novo:** R24. **Resíduos:** o mutex não cobre dois servidores; o `proposalId` verificado não prova que houve dois comandos (Q14).
