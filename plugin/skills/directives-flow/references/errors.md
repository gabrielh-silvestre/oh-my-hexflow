# Errors common to all commands

This holds for every `omhl` command. The page of each command lists only its own errors.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`); with `--format json`, as a JSON line. The machine-read commands (`hook session-start`, `hook spec-lock`, `hook agent-role`, `hooks post-commit` and `gate-worktree`) print only JSON, on error too. A clap usage error (unknown option, missing argument) exits with 2 and no code.

The order of the checks is: clap usage; arguments and input of the command; `OMHL_ROLE`; existence of `--project-dir`; marker; `--db`. The first one that fails is the one you see.

- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.
- `role-unknown` (exit 1): `OMHL_ROLE` has a value outside the seven roles, or was set outside Claude Code. Fix or remove the variable.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the `outside-claude-code` marker; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.

## Examples

Refusal: a `--project-dir` that does not exist:

```omhl-example fixture=project
$ omhl append --project-dir /nonexistent <<'EOF'
{"items":[]}
EOF
2> ✘ invalid-project-dir · the project directory does not exist
# exit 1
```
