# The prefix matrix

When to consult: before choosing the branch prefix, or when `omhl gate-worktree` refuses to create the worktree. The prefix is the `type` of `type/wi-<id>__<slug>`. Any prefix outside the table is refused with `prefix-not-allowed`.

| Prefix | Requires to create the worktree (`require`) |
|---|---|
| `feat/`, `fix/` | `business-premise` e `spec` |
| `refactor/`, `tests/` | `technical-premise-confirmed` |
| `bugfix/` | `agent-premise` |
| `chore/`, `docs/` | nothing |

The `require` atoms:

| Atom | Satisfied when |
|---|---|
| `business-premise` | The work item has 1+ confirmed premise of `kind` `business` (`status` `confirmed` with `confirmed_by`) |
| `technical-premise-confirmed` | The same, with `kind` `technical` |
| `agent-premise` | The work item has 1+ premise of `kind` `technical`, confirmed or not. A premise that the human confirmed later also counts |
| `spec` | `omhl spec validate` passes for the work item |

- The defaults in the table hold when the project has no `.omhl/workflow.toml`. With the file, `require` adds atoms to the defaults of a prefix and never removes them; a prefix the file adds takes only the atoms it lists. Read the file in the project itself.
- `main` and the branches declared in `[[branch.exception]]` of `.omhl/workflow.toml` never go through the gate: they come out `allowed` (`exempt-branch`).
- The gate refusal says what is missing. Record what is missing (premise, confirmation, spec) and run `omhl gate-worktree` again.
