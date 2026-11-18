# G1: resultado (2026-11-18, Claude Code 2.1.327)

Parte A: headless, haiku, 6 execuções (`run-a.sh`). Parte B: sessão interativa `experimental` (Herdr, pane w9:p1), iniciada com `--plugin-dir plugin/ --plugin-dir .omc/research/omc-port-g1/probe-plugin`; 6 chamadas pelo prompt (`out/b.analysis.txt`).

| Chamada | `agent_type` no hook | `meta.json` | Skill pré-carregada? |
|---|---|---|---|
| A1 bare, A2 qualificada, A6 qualificada com `Skill` liberada (sem name) | `omhlprobe:probe-*` | `agentType` = tipo | sim (marcador no registro 3, antes de qualquer tool; controles A3 e A4 = NONE) |
| A5 qualificada, com name (headless) | `omhlprobe:probe-qual` | `agentType` = tipo, `shape` foreground | sim |
| B1 qualificada, sem name (interativa) | `omhlprobe:probe-qual` | `agentType` = tipo | **sim** (controle B3a = NONE) |
| B2 qualificada, com name `g1-probe-named` (interativa) | **`g1-probe-named`** | `taskKind` = `in_process_teammate`, `agentType` = nome, `customAgentType` = `omhlprobe:probe-qual` | **não** (`SKILLCANARY=NONE`; controle B3b = NONE) |
| B4 `oh-my-hexflow:planner`, sem name | `oh-my-hexflow:planner` | `agentType` = tipo | n/a |
| B5 `oh-my-hexflow:planner`, com name `g1-planner-named` | **`g1-planner-named`** | `in_process_teammate`, `customAgentType` = `oh-my-hexflow:planner` | n/a |

Leitura (critério da seção 5 do plano):
- Sem name, o canal `skills:` está provado nas duas grafias, no caminho interativo (B1 + controle B3a). `SKILLS_SPELLING` pode ser a qualificada (D12).
- Com name, o hook `PreToolUse` vê o nome gerado, e o `skills:` NÃO é pré-carregado (B2). O G1 confirma a pergunta 4: a condição da opção (a') (B1 provada e B4 com o prefixo) está satisfeita.
- O papel real só aparece no `meta.json` do subagente (`customAgentType`), como previa a opção (b).
