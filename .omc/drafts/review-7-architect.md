# Consenso, rodada final: parecer do Architect (sonnet)

Plano avaliado (iteração 14): [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md), [omhl-v1-marcos.md](../plans/omhl-v1-marcos.md). Spec: [deep-interview-oh-my-hexflow-design.md](../specs/deep-interview-oh-my-hexflow-design.md). Par: [review-7-critic.md](review-7-critic.md).

- **Architect** (`dr-house-architect-omhl-final-round`): **SOLIDO COM RESSALVAS**, 4 bloqueantes, todos corrigíveis com texto, sem reabrir decisão do dono. O desenho é implementável pela API pública do Hexlog e do Claude Code sem patch que viole o Princípio 3. Não executou nada nem leu o parecer do Critic.

Abreviações: SLICE = plano principal; MARCOS = marcos; SPEC = spec; HEXLOG = `/home/gabriel/personal/hexlog`.

## Respostas aos 5 focos
1. **Resíduos de sandbox:** limpo, com uma sobra: SLICE:202 ainda diz "A1 a A8 e A16 a A19", mas A16 (sandbox) foi rebaixada (SLICE:472) e não tem spike no Step 0.
2. **Coerência entre os 3 arquivos:** sem erro de referência; links resolvem; contagem fecha (21 ACs na fatia, 8 `plan-added`: AC-13 a AC-19 e AC-22; AC-21 e AC-23 nos marcos). Inconsistência menor: SLICE:227 e a tabela de SLICE:421-428 dizem que restam medições "P0, S1, S2, S7, S8, S9", omitindo S3, S4 e S5; o S4(i) refutado é "pare e peça decisão" (SLICE:219).
3. **Plano x spec:** incoerência (B3).
4. **Juiz Haiku e log de auditoria:** implementáveis (ver "Viabilidade verificada").
5. **Step 0:** mede a incerteza principal antes de construir, mas as saídas têm lacuna (B4).

## Bloqueantes

### B1. O conjunto de tipos do `bootstrap_project` não inclui os tipos de que o N/A e o juiz precisam
- SLICE:138 (AC-1) fixa 7 tipos (unit, premise, evidence, closure, directive, annulment, granularity), igual a SLICE:257. SLICE:144 (AC-7) exige, na fatia, teste de servidor que registre `not-applicable` (alvo `na.<directiveId>`). MARCOS:63 grava `judge-verdict` (alvo `jv.<id>`) e uma relação derivada para o N/A. Nenhum está no AC-1. `create_process` fixa para sempre os tipos vigentes (HEXLOG `docs/tools.md`, `create_process`); o fork não expõe `define_*` (SLICE:150). Colide com a idempotência do AC-1, com `SEED_CONFLICT` (SLICE:252, só diretivas) e com o R9.
- Exigido: decidir e escrever, antes do Step 1, o conjunto completo de tipos, relações e gates da v1 (incluindo `not-applicable`, `judge-verdict` ou o `kind` de `premise` que os substitua, e o nome da relação do N/A). Atualizar AC-1 e o schema congelado do Step 1. Registrar no R9 e em 3.1 que definição adicionada depois só vale para processos novos.

### B2. O Step 7 usa o termo "verdict", que o teste `no-flow-terms` do Hexlog reprova em `src/omhl/`
- HEXLOG `test/no-flow-terms.spec.ts` (FLOW_TERMS, linhas 32-42) inclui `verdict`; a varredura cobre `src` inteiro. SLICE:242 adota `outcome` por isso. Os marcos reintroduzem: campo `verdict` (SLICE:310, MARCOS:66), tipo `judge-verdict` e alvo `jv.` (MARCOS:63), código `JUDGE_VERDICT_FINAL` (MARCOS:64), já na tabela de erros do Step 1 (SLICE:233).
- Exigido: renomear antes do congelamento (por exemplo `ruling`, `judge-ruling`, `JUDGE_RULING_FINAL`; a prosa em português pode seguir "veredito"). Mesmo cuidado com `plan`, `claim`, `critic`, `omc` em `src/omhl/`.

### B3. Precedência spec x plano: os adiamentos de 3.1 contradizem o spec e não constam na 8.1
- SPEC:14 (versão anterior às minhas edições) diz que vale o plano só para o que a 8.1 lista. Os adiamentos estão só em SLICE:164-186 (3.1) e contradizem o spec: quatro estados da unidade, unidade fechada com órfão reabre, tool `ancestors` e "versões melhores" das genéricas, processo sucessor de major, checks embutidos lint/filhas fechadas/artefato, registro `bypass` no Hexlog. SLICE:29, :417, :543, :545 ainda dizem "o dono atualizará o spec", mas o spec já tem as emendas. Restam no spec: `checkedAgainst` (ontologia), "verdict per directive", "comandos de não se aplica", itens abertos já resolvidos.
- Exigido: linha na 8.1 "Escopo da v1 (adiados, ver 3.1)" apontando para Q15 a Q17; reescrever o cabeçalho da 8.1 e os trechos do plano para "emendas já aplicadas ao spec; a coluna do meio é histórica"; limpar os 4 trechos obsoletos do spec.

### B4. A saída "parar e perguntar" do Step 0 só cobre o S4; falta o caso em que o `tool_response` das tools MCP não é utilizável
- A P0 roda primeiro (gate SLICE:209) e as fixtures vêm dela (SLICE:225). Mas a única saída para P0/S2 é o plano B do AC-9 (SLICE:219), que cobre só a projeção. Dependem do `tool_response` sem plano B: o espelho de unidade ativa (SLICE:288; fallback "ativa-desconhecida" faz o `Stop` bloquear sempre), o campo `anchors` (SLICE:240), `attested.directivesRead` (SLICE:141), o `branchRead` do juiz (MARCOS:61). A doc diz que o schema do `tool_response` "depende da tool"; em `PostToolUseFailure` o `error` é uma string. A P0 grava só um Bash com bloco delimitado de sucesso. O `omhl check` com `fail`/`error` pode chegar como `PostToolUseFailure` se sair com código != 0, e o plano não fixa o código de saída.
- Exigido: em SLICE:219, se o `tool_response` MCP for ilegível em `PostToolUse`, o espelho deriva de `tool_input` + tipo do evento; se nem o tipo distinguir sucesso de erro, parar e perguntar. Fixar que `omhl check` sai com 0 para pass/fail/error dentro do contrato. Incluir na P0 um Bash com saída longa, outro que falha e um erro de domínio MCP.

## Melhorias opcionais
- **M1 (alta).** Mover o S8 (detecção barata de leitura do segredo, 2 h) e os itens (i) a (iv) do S9 (subagente de plugin, `agent_type`, `SubagentStart`, reescrita do prompt) do marco para o Step 0. O Step 1 congela o schema com `attested.judge` e o segredo HMAC antes dessas medições (Princípio 2). A doc já confirma parte do S9 (`agent_type` escopado `plugin:agent`; `PreToolUse`/`PostToolUse` disparam em subagentes com `agent_id` e `agent_type`).
- **M2.** Juiz: `SendMessage` retoma um subagente sem nova chamada `Agent`, e `SubagentStart` dispara também na retomada. A reescrita do prompt e o teto de 2 invocações (MARCOS:59) só olham `Agent`. Negar `SendMessage` cujo `to` seja o juiz e contar o teto no servidor/hook por (unidade, diretiva).
- **M3.** Juiz: `SubagentStart` não carrega `tool_use_id`, só `agent_id`/`agent_type`, e não bloqueia a criação; MARCOS:60 diz "casado ao Agent aprovado" sem dizer como. Sugestão: fila de aprovações por sessão consumida pelo primeiro `SubagentStart` com o `agent_type` esperado. Definir a gramática de uma linha para `directive=<target>` no prompt original. O `hooks.json` do Step 4 (SLICE:325) precisa do matcher `Agent|SendMessage` e do evento `SubagentStart` ao entrar o marco 2.
- **M4.** A `key` de idempotência é por log de processo (HEXLOG `src/commands/process.ts:54-69`; o fingerprint ignora key, agent e model, `chain.ts`, D-06). O uso único de `human:<nonce>` só vale dentro de um processo; o plano compensa prendendo `process` na asserção (SLICE:107). Gap: `annul_directive` grava a premissa humana no mesmo lote da anulação (SLICE:110), isto é, no processo de diretivas, enquanto a asserção prende o `process` da unidade ativa. Declarar em qual processo cada registro cai e o que o servidor confere.
- **M5.** Corrigir SLICE:202 (A16) e a tabela de SLICE:421-428 (S3, S4, S5). SLICE:158 tem frase truncada. SLICE:5 diz "rev. 16" mas o plano e o changelog falam em 14 iterações. O Q7 acumula 5 decisões em uma linha (SLICE:497); considerar separar.

## Viabilidade verificada
- Hexlog: `execute` exportada (`src/mcp/kernel.ts:193`); `advertise` exportada (`:99`); `defineTool` (`:250`); `ToolDeps`/`CallContext` exportados (`:60`, `:67`); `ToolCall` não exportado (objeto inline serve por tipagem estrutural); `registerQueryTools` exportada (`src/mcp/tools/query.ts:354`); `scriptsBlock` proíbe `**/mcp/**` em `scripts/` (`eslint.boundaries.js:117-135`) e os blocos de camada não casam `src/omhl/`; `scripts/build.ts:19-26` aceita `entryPoints`; `dataDir` lê só `XDG_DATA_HOME` (`src/directory.ts:11-16`); `ajv` já é dependência (`package.json:75`).
- Claude Code (hooks e subagentes): `updatedInput` em `PreToolUse` e `tool_response` em `PostToolUse` existem; `UserPromptExpansion` traz `expansion_type`, `command_name`, `command_args`, `command_source`; `agent_id`/`agent_type` nos hooks de subagente; `SubagentStart` existe.
- Log de auditoria encadeado (AC-19): só Rust, `O_APPEND`, arquivo próprio; o servidor grava `auditHead` em campo de dados de `unit` e `closure`, sem patch no Hexlog. Ressalva da B1: o schema desses tipos precisa estar completo na v1.

## Consensus addendum
- **Antithesis:** sem sandbox, o agente tem Bash no mesmo uid e lê o segredo com o mesmo esforço com que forjaria um campo `omhl` sem MAC; o aparato (HMAC, JCS Rust x npm, digests, âncoras, fork em dois repositórios, 8 ACs `plan-added`) compra "forjar deixa rastro", e esse rastro depende de detectar a leitura (S8, só medido depois da fatia; `python -c`/`node -e` não casam com o guard por texto). Alternativa mais barata: o hook injeta um campo `omhl` sem MAC, o servidor recusa sem ele e o log registra.
- **Tensão:** congelar o schema completo no Step 1 (para os Steps 2 e 3 andarem em paralelo) contra medir S8 e S9 antes; com `create_process` fixando tipos para sempre (B1), errar custa mais.
- **Síntese:** manter o HMAC (separa "esquecimento do hook" de "ato deliberado e registrado"), mas fazer M1 e completar o conjunto de tipos (B1) antes do congelamento.
- **Violações de princípio:** Princípio 2 (S8/S9 depois do congelamento), gravidade média; Princípio 3 (B2 leva a patch de exceção no teste do upstream se `verdict` ficar), média, remediável por renomeação.
