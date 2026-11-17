# A matriz de prefixos

Quando consultar: antes de escolher o prefixo da branch, ou quando o `omhl gate-worktree` recusar a criação da worktree. O prefixo é o `tipo` de `tipo/wi-<id>__<slug>`. Qualquer prefixo fora da tabela é recusado com `prefix-not-allowed`.

| Prefixo | Exige para criar a worktree (`require`) |
|---|---|
| `feat/`, `fix/` | `business-premise` e `spec` |
| `refactor/`, `tests/` | `technical-premise-confirmed` |
| `bugfix/` | `agent-premise` |
| `chore/`, `docs/` | nada |

Os átomos de `require`:

| Átomo | Satisfeito quando |
|---|---|
| `business-premise` | O work item tem 1+ premissa `kind` `business` confirmada (`status` `confirmed` com `confirmed_by`) |
| `technical-premise-confirmed` | O mesmo, com `kind` `technical` |
| `agent-premise` | O work item tem 1+ premissa `kind` `technical`, confirmada ou não. Uma premissa que o humano confirmou depois também serve |
| `spec` | `omhl spec validate` passa para o work item |

- Os padrões da tabela valem quando o projeto não tem `.omhl/workflow.toml`. Com o arquivo, o que cada prefixo exige vem dele; leia-o no próprio projeto.
- A `develop` e a `main` nunca passam pelo gate: saem com `allowed` (`exempt-branch`).
- A recusa do gate diz o que falta. Grave o que falta (premissa, confirmação, spec) e rode o `omhl gate-worktree` de novo.
