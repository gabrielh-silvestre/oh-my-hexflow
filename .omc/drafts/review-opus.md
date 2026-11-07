# Rodada extra em opus: pareceres do Critic e do Architect

Plano avaliado (722 linhas): [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md). Pareceres anteriores (sonnet): [review-4.md](review-4.md).

- **Critic opus** (`mandy-critic-omhl-opus`): **ITERATE** (1 CRITICAL, 6 MAJOR; corrigíveis por edição de texto, nenhum reabre o spec). 47 afirmações conferidas: 43 certas, 2 erradas, 2 sem fonte. As citações do Hexlog conferem no `main` (76eafc8), não só no `develop`.
- **Architect opus** (`dr-house-architect-omhl-opus`): **SOLIDO COM RESSALVAS** (3 bloqueantes). Crédito: cada premissa make-or-break de hook/MCP tem plano B que falha fechado (updatedInput -> T1', tool_response -> outbox, Stop -> freio + servidor, origem -> matriz Q18, onFailure ausente -> servidor recusa sem envelope). A única premissa sem fallback fechado é o sandbox.

## Bloqueiam a aprovação

### C1 [CRITICAL, Critic] As âncoras de confiança estão fora do guarda; a tabela de garantias (l.53) fica falsa
- (a) `hooks.json` usa `"command": "omhl"` resolvido pelo PATH (l.315). Neste ambiente `~/.local/bin` está na posição 2 do PATH e `~/.cargo/bin` na 13. Um `omhl` falso gravado em `~/.local/bin` pela tool `Write` (fora do sandbox; o AC-15 não cobre esse caminho) vira o hook, roda fora do sandbox e lê o segredo.
- (b) "Os hooks conferem o digest do próprio binário" (l.173, l.315) não protege nada: o binário trocado simplesmente não confere.
- (c) O bundle do fork (`node <bundle>`) e o `~/.claude.json` (comando, `XDG_DATA_HOME` e `OMHL_SECRET_FILE` do servidor) estão fora do núcleo; o `~/.claude.json` está até marcado como cortável.
- (d) `claude mcp add` não abre sessão, então o núcleo não o nega; a justificativa do cortável é falsa. O agente pode trocar o servidor por um que aceite tudo.
- Exigido: o `omhl setup` grava no `hooks.json` (ou num wrapper em `${CLAUDE_PLUGIN_ROOT}/bin`) o caminho absoluto do binário; o binário, o bundle do fork, o `~/.claude.json` e os diretórios do PATH à frente do binário entram no NÚCLEO do AC-15 (deny de Write/Edit e denyWrite do S6); o guard nega `claude mcp|plugin|config` no núcleo; o autodigest vira só detector de reinstalação; a tabela de garantias ganha a linha "controlar o binário do hook ou a configuração do servidor = mesmo efeito de ler o segredo"; novo negativo no AC-15: um `omhl` falso à frente no PATH não é executado como hook.

### A1 [CRITICAL, Architect] A garantia inteira do Princípio 1 repousa no sandbox do Bash, e o plano nunca seta `sandbox.failIfUnavailable`
- Fato verificado na doc (code.claude.com/docs/en/sandboxing): quando bubblewrap/socat falham, o Claude Code roda comandos SEM SANDBOX e EM SILÊNCIO; `failIfUnavailable: true` faz falhar fechado. No WSL2 essas deps externas quebram com atualização de distro ou kernel. Grep no plano: `failIfUnavailable` tem 0 ocorrências. S6/AC-19 é o ÚLTIMO item cortável (3.1). Sem sandbox o Bash lê o segredo, cunha envelope/asserção, forja recibo+MAC e fecha a unidade, e a degradação "freio+auditoria" da seção 2 é invisível.
- Exigido: `omhl setup` seta `failIfUnavailable: true` e `allowUnsandboxedCommands: false`; o servidor e o PreToolUse RECUSAM (não degradam) se o sandbox não está ativo; remover S6/AC-19 dos cortáveis; reescrever a linha "sandbox ausente -> freio + auditoria" para "-> falha fechado".

### A2 [MAJOR, Architect] Teto R11 enterrado
- Mesmo COM sandbox e SEM o segredo, o agente fecha a unidade PR com "CI verde" sem forjar nada: ele escreve o código do PR E os testes que o `omhl check` roda. `scriptDigest` é fixado "só para check de projeto" (Step 2a, linha 280), então `cargo test`/`jest` dos checks embutidos rodam os testes do agente SEM digest. `it('works',()=>expect(true).toBe(true))` fecha a unidade. Está só no parêntese da linha 53, enquanto Goal/Princípio 1 vendem "forjar exige o segredo".
- Exigido: promover o R11 ao Goal e ao Princípio 1; ou digest da árvore de testes nos checks embutidos, ou declaração explícita de que "CI verde" é auto-certificado por código do agente (fronteira do modo `self`).

### A3 [MAJOR-leve, Architect] A cunhagem da asserção humana não prende `command_source`
- O envelope grava só `source: UserPromptExpansion`, mas a doc mostra `command_source` ("plugin") como campo distinto. Um comando local/de projeto homônimo (`/premise`) chegaria ao mesmo handler; hoje só o sandbox nega `.claude/commands`.
- Exigido: a condição de cunhar exige `command_source == "plugin"` E `expansion_type == "slash_command"`; AC-3(f) com fixture negativa de homônimo não-plugin.

### M1 [MAJOR, Critic] Unidade `issue`/`epic` ativa trava o Stop
O e2e abre o pai `issue` (l.180); o `close_unit` de issue devolve `GRANULARITY_NOT_SUPPORTED_IN_V1` (l.165); o Stop bloqueia "unidade aberta" (l.168). Se a sessão parar entre abrir a issue e abrir o PR, ou se a unidade ativa voltar ao pai depois do PR, dá `non-convergence` em toda sessão. Exigido: na v1 o Stop vigia só unidades PR (ou `open_unit` de issue/epic não vira a unidade ativa); dizer o que é a unidade ativa depois que o filho fecha; caso golden no AC-10.

### M2 [MAJOR, Critic] Árvore suja sem definição, e os arquivos do próprio `omhl` sujam a árvore
O agente grava `.omhl/outbox/criteria/` e o `omhl check` grava `.omhl/outbox/<hash>.json` (l.174, l.277); só `.omhl/state/` é ignorado (l.219, spec l.48). O 2º `omhl check` do e2e devolve `dirty-worktree` e o `close_unit` dá `TREE_DIRTY`; se os untracked forem ignorados, um teste não rastreado altera o resultado sem mudar o SHA. Exigido: o `/omhl-init` grava no `.gitignore` `.omhl/state/`, `.omhl/outbox/` e `.omhl/proposals/`; "suja" = `git status --porcelain --untracked-files=all` não vazio fora dos ignorados; negativo no AC-5.

### M3 [MAJOR, Critic] O bypass ficou mais estreito que o spec, sem pergunta ao dono
Spec l.77 e l.129: "a saída é o bypass" e "libera hooks por tempo limitado"; plano l.170: "sob bypass vigente só o Stop libera". Uma falha do `PreToolUse` (binário ausente, digest depois de `cargo install`, R17), com `onFailure: block` e matcher em Bash, Write e Edit, deixa a sessão sem Bash e sem edição, sem saída na própria sessão. Exigido: nova Q com recomendação (por exemplo, o bypass libera também as negações do `PreToolUse` sobre ferramentas genéricas, nunca a exigência de envelope no servidor) e uma linha na 3.1, ou um AC que cubra o caminho.

### M4 [MAJOR, Critic] A projeção da anulação sumiu
Spec l.75-76: o `PostToolUse` grava a projeção a partir de `create_directive`/`annul_directive`. O AC-9 (l.167) e o Step 5 só fazem `create_directive` e `bootstrap_project`, enquanto a l.24 promete projeção nas duas. Exigido: o `PostToolUse` de `annul_directive` grava a projeção (definir o formato para a anulação de escopo unidade), ou uma linha na 3.1 com Q.

### M5 [MAJOR, Critic] O provedor falso não tem regra de seleção; o custo de uso real não está escrito
Só o trait e o provedor falso entram; o esqueleto `gh` é o 1º corte (l.201, l.278). Nada diz como o e2e escolhe o falso nem o que o binário de produção faz sem provedor. Escolha em tempo de execução (env, `.omhl/`) vira atalho de "CI verde"; se o binário só tiver o falso, o `ci-status` de produção mente. Com git local nenhum PR fecha de verdade: o Stop chega a `non-convergence` em toda sessão real. Exigido: provedor falso só atrás de cargo feature de teste (`cargo test --features fake-github` nas verificações); sem provedor o check devolve `error` com `details.reason`; declarar na seção 1 e no ADR que a fatia não fecha PR real até a Q10.

### M6 [MAJOR, Critic] O consumo da asserção humana pendente é ambíguo
"O texto é normalizado e sobrescrito em silêncio; negada apenas por divergência semântica" (l.97): lido assim, qualquer `register_premise` do agente com uma asserção pendente vira a premissa do humano com texto trocado, e relações e `checkedAgainst` do agente entram sem estar presos. Exigido: só consome a chamada cujo texto normalizado é igual ao `textHash`; texto diferente segue como premissa `agent`, sem asserção; dizer quais campos o agente ainda controla na premissa humana.

## Melhorias (não bloqueiam)

- **M7 [MAJOR, Critic] Questões ao dono:** em cada Q, recomendação + consequência de cada resposta + ACs afetados; separar a Q12 em duas; tirar da Q18 a dependência de reler a matriz; marcar os ACs que pressupõem a resposta (AC-6/Q13, AC-3 e AC-7/Q2). Q5, Q10, Q11, Q2 sem recomendação explícita.
- **A4 [MAJOR-manutenção, Architect] Reescrever R2/R14.** Deriva verificada: develop = 687 commits desde 2026-09-30; arquivos de que o fork depende são os mais quentes (`query.ts` 26, `ports.ts` 23, `kernel.ts` 17, `process.ts` 12, `compose.ts` 10 commits). "Fork aditivo, poucos toques" é verdade para o diff textual, FALSO para o acoplamento de API: o fork importa 4 assinaturas internas sem tag (`compose`, `registerQueryTools`, `execute`, `ToolDeps/CallContext`) e declara `ToolCall`/`ToolConfig` INLINE (Step 1, l.249), cópia de tipo interno que pode compilar limpo e comportar errado. O DIGEST protege o contrato JSON, não a API TS. O plano admite (l.85) que é "a opção E dentro de `src/omhl/`" e herda a fragilidade de R14 ("não adotada"). Exigido: reescrever R2/R14 com esses números e acrescentar um teste de contrato de API (por exemplo, `contract-sync` para as assinaturas, ou smoke test que falha ao rebase).
- **A5 [MINOR-fidelidade, Architect] `cross-process-currency` x premissa herdada.** `supersedes`/`revokes` só no mesmo processo (`src/domain/relations.ts:252`, `docs/tools.md:157`). O spec diz premissa "herdada para baixo" e "corrige-se por supersedes no mesmo target". Como cada unidade é um processo, uma filha NÃO consegue `supersede`/`revoke` premissa herdada do processo-pai. O AC-20 só testa same-process. Virar questão antes do Step 3 (com recomendação), não descoberta na implementação.
- **A6 [ANTÍTESE / realismo, Architect] A fatia NÃO é mínima** (20 ACs, 7 `plan-added`, 2 repos, 8 níveis de corte). A fatia mínima que prova a tese já está no plano: o modo `agent`-only do Q18. O subsistema de token humano (origem `UserPromptExpansion`, `create`/`annul_directive`, bypass humano, AC-3/8/20-humano) é separável e é a maior fonte de risco. Recomendação do Architect: cindir em fatia-1 (canal autenticado + open/premise(agent)/check/evidence/close/Stop/handshake) e fatia-2 (token humano). **ATENÇÃO: o spec inclui o token humano na fatia vertical ("com o hook Stop e o token humano"), então NÃO corte por conta própria.** Registre como NOVA QUESTÃO para o dono (Q20), com a recomendação do plano, a consequência de cada resposta, os ACs que viram fatia-2 e o que muda na ordem dos Steps.
- **T1' (nota de usabilidade, Architect):** é fail-closed seguro, mas dobra toda chamada de domínio e depende do modelo reemitir a chamada idêntica com o envelope do `reason`; registrar como fragilidade de usabilidade se o S1 cair.
- **Critic, MINOR:**
  - (1) `SessionStart` sem `resume` no matcher (l.117, l.315) contradiz o lembrete de resume (l.168, l.290): acrescentar `resume` ao matcher ou remover o lembrete.
  - (2) `scripts/omhl/install.ts` e `omhl setup` registram ambos o MCP, e o exemplo não leva `--scope user` (o padrão é `local`, conferido em `claude mcp add --help`; o `scripts/install.ts` do upstream usa user): um dono só e `--scope user`.
  - (3) Step 3 "em paralelo com 2a e 2b" (l.211) x gate "Q1 e Q2 já fecharam no Step 2" (l.299); a Q4 tem gate no Step 3 mas é implementada nos hooks (2b/4).
  - (4) `scripts/sync-contract.sh` fica em `scripts/` do fork, fora da lista de toques e do filtro do diff; `cspell` e `markdownlint` (`**/*.md`) varrem a cópia `contract/*.md`.
  - (5) Cabeçalho l.4 diz "sem ADR", e a seção 9 tem um ADR.
  - (6) A frase "user-typed command" e o "typing /skillname directly bypasses PreToolUse" ESTÃO na doc de hooks (conferido), então F13, A17 e a l.718 estão desatualizados; `disable-model-invocation` está confirmado na doc de skills.
  - (7) A l.250 diz que o `knip.json` lista "só" `src/server.ts` e `scripts/*.ts`, mas ele também lista `hook/bash-guard.ts`, `scripts/flow/*`, `.claude/hooks/*` e fixtures.
  - (8) O S1 contra o Hexlog de stock não diz qual campo reescrever para observar o efeito, e o servidor não loga a entrada.
  - (9) O entry do fork precisa de `isLegacy` em `ToolDeps`, e o plano não cita.
- **Critic, fidelidade ao spec (item 4, FALHA):** saem sem linha na 3.1 a projeção da anulação (spec l.75-76), o bypass estreitado (spec l.77, l.129), "versões melhores das genéricas" (spec l.45; 0 ocorrências no plano) e os checks embutidos lint, "artefato existe" e "comentário existe" (spec l.62 x l.277). Cobrir ou adiar cada um na 3.1 com justificativa.
- **Critic, escopo:** a parte repo/headRepository/baseRefName do AC-14, o `--rotate` e o aval em dois passos podem ser adiados (o R20 já admite exposição zero na v1).
- **Critic, fatos:** (a) o "só" do `knip.json` está errado (acima); (b) a frase "user-typed não aparece na doc" está errada (acima); (c) sem fonte primária: `CLAUDE_CODE_DISABLE_CRON` "remove CronCreate e /loop por inteiro" (o changelog diz "stop scheduled cron jobs mid-session", o que pesa na Q19) e o teto 8 / `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP` (já marcado R-). Corrigir a Q19 e a F13/A17.
- Verificado pelo Architect: a key de idempotência é por processo (`state.ts:308`); o replay da asserção é barrado pelo binding de `process`, não pela unicidade da key, então a defesa está certa por outro mecanismo que o plano já descreve.

## Causa raiz comum aos bloqueantes (Architect)
O plano anuncia como garantia algo condicional a (a) sandbox ativo, silenciosamente revogável, e (b) testes não escritos pelo agente, estruturalmente falso no fluxo normal. O canal está certo dado um agente sem segredo e sem controle do que o hook observa; as duas condições que sustentam esse "dado" não se sustentam sozinhas. A tabela de garantias e o Princípio 1 precisam dizer isso com todas as letras.
