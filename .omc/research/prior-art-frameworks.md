# Prior art: frameworks de workflow com agentes

Fonte: relatório do agente `gandalf-document-specialist-prior-art-frameworks`, verificado em 2026-10-30 (API do GitHub e docs oficiais). O que ele não conseguiu confirmar está marcado "não verificado". Spec de origem: [deep-interview-oh-my-hexflow-design.md](../specs/deep-interview-oh-my-hexflow-design.md).

## Maturidade (verificada em 2026-10-30)
| Framework | Estrelas / licença / último release | Enforcement real? |
|---|---|---|
| OMC (Yeachan-Heo/oh-my-claudecode) | 40.433, MIT, v5.7.22 (2026-10-30; cache local 5.7.19) | Sim: hook Stop (`persistent-mode.mjs`, 1878 linhas) |
| Spec Kit (github/spec-kit) | 144.120, MIT, v1.2.5 (2026-10-28) | Quase não: gates são prosa de prompt |
| Superpowers (obra/superpowers) | 309.386, MIT, v7.0.1 (2026-10-21) | Não: só hook SessionStart |
| OpenSpec (Fission-AI/OpenSpec) | 73.468, MIT, v1.15.0 (2026-10-24) | Não (CLI `validate`/`status --json`) |
| BMAD-METHOD | 54.792, MIT (API mostra NOASSERTION; há TRADEMARK.md), v6.12.4 (2026-10-14) | Parcial: plano com `blocked_at`, teto de 5 iterações de repair |
| Kiro | IDE fechada; repo kirodotdev/Kiro sem licença nem release | Steering é guia, não gate |

Outros citados, mecânica de gates **não verificada**: gsd-core, Archon, moai-adk, cc-sdd, claude-task-master.

## OMC (cache local 5.7.19)
- Plugin: `.claude-plugin/plugin.json`, `hooks/hooks.json`, `agents/*.md`, `commands/*.md`. Cada hook roda `node "${CLAUDE_PLUGIN_ROOT}"/scripts/run.cjs <script>`, timeout <= 5 s.
- Stop: 5 hooks encadeados (context-guard-stop, workflow-drift-guard, persistent-mode, budget-guard, code-simplifier).
- `persistent-mode.mjs` libera em: `stop_hook_active`, limite de contexto, contexto >= 95%, abort do usuário, erro de auth, wakeup agendado, trabalho assíncrono pendente. Estado velho após 2 h. Teto duro 500 iterações (200 em strict); o teto soft do ralph se estende +10.
- Estado em `.omc/state/sessions/{sessionId}/ralph-state.json`. O bloqueio olha `state.active`, que o agente zera com `/cancel`.
- `verify-epic-3698-closure.mjs`: exit 0 pass, 1 fail, 2 pending (pré-requisito temporal). Análogo ao nosso closure.
- `omc ralph verify --write-baseline` guarda falhas pré-existentes como baseline.

## Claude Code (hooks)
- Stop/SubagentStop bloqueiam com `decision:"block"` + `reason`; `stop_hook_active=true` quando já continua por hook.
- Handler `mcp_tool` existe (hook chama tool de servidor MCP), mas falha vira erro **não bloqueante** (fail-open); em SessionStart no launch os servidores MCP ainda não estão disponíveis.
- PreToolUse suporta `updatedInput` e o campo `if`. Exec form (`command` + `args`, sem shell) é suportado.
- Issues relevantes: anthropics/claude-code #95465 e #83229 (resposta bloqueada reaparece a cada retry), #39530 (guard de session_id falhava em sessões paralelas), #60490.

## Spec Kit
- Constitution em `.specify/memory/constitution.md`, com versão SemVer e datas; docs falam em "immutable principles". Plan template tem "Constitution Check: GATE". Tudo é instrução de prompt, sem hook que bloqueie.
- Workflow engine YAML com `type: gate` (aprovação humana), estado em `.specify/workflows/runs/<id>/state.json`, `specify workflow resume`.
- Emenda da constitution é edição com bump SemVer + Sync Impact Report. Nosso modelo (só criar e anular) é mais estrito.

## Superpowers
- `hooks/hooks.json` só tem SessionStart (matcher `startup|clear|compact`) reinjetando a skill de entrada.
- Disciplina 100% prosa: verification-before-completion (Iron Law, tabelas de racionalização). Issue obra/superpowers#2286: evidência sem proveniência (agente rodou o probe à mão e leu a saída como se o sistema tivesse agido). #715: não há como forçar o workflow.
- `.version-bump.json` e `scripts/bump-version.sh` sincronizam versões entre manifests (conteúdo não verificado).

## OpenSpec / BMAD / Kiro
- OpenSpec: `changes/<nome>/` com proposal/design/tasks e delta specs; `archive` aplica o delta e move para `changes/archive/AAAA-MM-DD-nome/`. `initiatives/<id>/` com `linked_changes` e `work-items/` é o mais próximo de unit recursiva.
- BMAD v6: worker não assistido com status terminal e halts nomeados (ex.: "review repair loop exceeded 5 iterations (non-convergence)"); plano com `status: blocked`, `blocked_at`, `blocked_reason`.
- Kiro: steering com frontmatter `inclusion: always|fileMatch|manual|auto`; tasks em waves por grafo de dependência.

## Reaproveitar
1. Estrutura de plugin (`.claude-plugin/plugin.json`, `hooks/hooks.json`, `${CLAUDE_PLUGIN_ROOT}`); para o binário Rust, exec form `command` + `args`, sem wrapper node.
2. Lista de saídas seguras do Stop do `persistent-mode` (contexto cheio, abort, auth, `stop_hook_active`, estado velho, escopo por `session_id`).
3. Tri-estado pass/fail/pending do verificador de closure do OMC: avaliar `pending` ao lado de `error` no contrato JSON dos checks.
4. Baseline de falhas pré-existentes (`omc ralph verify --write-baseline`).
5. Reinjeção em SessionStart com matcher `startup|clear|compact` (regras e handshake).
6. Tabelas de racionalização e "Iron Law" nas skills como camada de prosa sobre o gate duro.
7. Status terminais e halts nomeados com teto de não convergência (BMAD, 5 iterações) para o teto do Stop.
8. Gate com estado em disco e resume (Spec Kit) como modelo para token humano e bypass.
9. Frontmatter de steering (Kiro) como ideia para `docs/directives/*.md`.
10. Archive com prefixo de data e initiatives/work-items (OpenSpec) para o histórico do epic.
11. Template por artefato com `instruction` (OpenSpec) para `docs/templates/epic-report.md`.

## Evitar
1. Bloquear por flag que o agente limpa (`state.active` do ralph).
2. Stop hook monolítico (1878 linhas, 5 hooks encadeados): manter o Stop pequeno e tabelado.
3. Estender o teto automaticamente (+10 do ralph): teto do `omhl` fixo.
4. Liberar em `stop_hook_active` como o OMC (reduz o gate a uma tentativa): o `omhl` precisa de contador próprio em `.omhl/state/` (inferência, não testada).
5. Gates só de prompt (Spec Kit, Superpowers).
6. Evidência sem proveniência (Superpowers #2286): reforça o recibo conferido pelo hook.
7. Specs mutáveis por delta (OpenSpec MODIFIED, edição da constitution): conflita com "diretiva só criar e anular".
8. Confiar em `mcp_tool` para fail-closed: falha vira não bloqueante.
9. Guard fraco de `session_id` (#39530): relevante para "uma unidade ativa por sessão" e por worktree.
10. UX do bloqueio: `reason` curto e constante, para não repetir a resposta a cada retry (#95465).

## Já resolvido por terceiros vs. a construir
- Resolvido: mecanismo de bloqueio do Stop (nativo + padrão do OMC); estrutura de plugin; loop "continue até acabar" (`/goal`, ralph), mas sem prova reproduzível.
- Hook `mcp_tool` poderia dispensar o `omhl` de falar com o Hexlog, mas é fail-open; a decisão "o `omhl` nunca fala com o Hexlog" continua válida.
- **Construir:** constitution imutável COM enforcement; recibo conferido de evidência; relatório final do epic por template regenerado e comparado byte a byte; token humano preso a hash de conteúdo; hierarquia de unidades com gate por nível (OpenSpec tem só parcial).

## Não verificado
Semântica de `project_memory_add_directive` do OMC; conteúdo de `.version-bump.json`; campo `requires` no schema do OpenSpec; mecânica de gates do moai-adk, Archon e smart-ralph; uso de EARS no Kiro.
