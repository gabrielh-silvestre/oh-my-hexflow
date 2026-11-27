# omhl load

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review rules | `omhl load` |

## When to use

After creating, editing or deleting a file in `.omhl/` (check or guideline), so the database reflects the files. Run it before any `omhl check run` that comes after a change to a check: `check run` refuses with `spec-stale` if the TOML and the database diverge. `--dry-run` shows the plan without recording anything.

Edit `.omhl/workflow.toml` and `.omhl/checks/` only inside a worktree, never in the main directory.

## Inputs

- No positional arguments. It reads the project files in `.omhl/`.
- `--dry-run`: reports what it would do, without recording.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--owner-override`: belongs to the owner, outside Claude Code; an agent never passes it ([config-loosening-review.md](../../review/references/config-loosening-review.md)).
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ loaded` (or `✔ dry run`, which records nothing) with the counts of `created`, `revised` and `unchanged`, and those of `skipped`, relations, orphans and `warning` when they exist; then one line per non-empty group with the ids (`created`, `revised`, `orphan`) or one line per item (`skipped`, `relation`, `orphan relation`, `warning` with path, code and message). With `warning` the header becomes `⚠`. The `unchanged` ids only enter the count. With `--format json` it is a JSON line: `tenant`, `dry_run`, and the id lists `created`, `revised`, `unchanged`, `skipped`, `orphans`, `orphan_relations`, `relations_created`, plus `warnings`. A file deleted from the tree does not vanish from the database: `load` only lists the id in `orphans` (`orphan` in the text). To discard it for good, use `omhl check retire` (check) or `omhl guideline retire` (guideline). An id already retired comes back in `skipped` and is not recreated. The command is idempotent: with no change to the files, everything comes out in `unchanged`.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `extensions-invalid` (exit 1): some file in `.omhl/` is invalid. The `details` carry the `path` and the `code` of the problem (for example `event-unsupported`, for an event outside `before-commit`, `before-pr` and `before-merge`, or `rule-two-streams`, for a rule with a pattern in `stdout` and in `stderr`). Fix the TOML and run again.
- `check-loosening-refused` (exit 1): the revised check turns blocking off, removes its run or moves a blocking check out of the required events. Keep it blocking, with its run and on a required event, or ask the owner to retire it and declare a new one.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the marker `outside-claude-code`; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Loads the project files. With no change at all, the checks come out in `unchanged`; `--dry-run` only reports:

```omhl-example fixture=checks
$ omhl load
✔ loaded · 0 created · 0 revised · 2 unchanged
$ omhl load --dry-run
✔ dry run · 0 created · 0 revised · 2 unchanged
```

With `--format json` the report comes out for machines, with the id lists in full:

```omhl-example fixture=checks
$ omhl load --format json
{"created":[],"dry_run":false,"orphan_relations":[],"orphans":[],"relations_created":[],"revised":[],"skipped":[],"tenant":"acme","unchanged":["checks.ok","checks.review"],"warnings":[]}
```
