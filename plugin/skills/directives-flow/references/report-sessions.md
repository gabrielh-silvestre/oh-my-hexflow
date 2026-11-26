# omhl report sessions

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

## When to use

To list the real Claude Code sessions that recorded in the project database, for example to choose the sessions of a mining that looks at several work items. Each `omhl` record keeps the id of the session that made it, and this is the only command that shows it. It is read-only.

## Inputs

- `--work-item <id>`: restricts to the sessions that recorded at least one record of the work item, like `work-items.w1`. It is repeatable: with more than one, the union applies, without repeating a session. Each work item must exist (open or closed). The line of a filtered session still shows the totals and the work items of the whole session. Without the option, the command lists all sessions.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database, outside the project folder; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ report sessions · N sessions` and one line per session, in the order of the first record of each. With no real session at all it prints `✔ report sessions · none`. Records made outside Claude Code (marker `outside-claude-code`) and old records, without a session, have no line and add to none. With `--format json` it is a JSON line, `{"sessions": [...]}`, and each session carries:

- `id`: the session id. In the text it comes after the word `session`, on a single line, without control characters; the JSON carries the exact value, which is what a program should consume.
- `first` and `last`: the date of the first and of the last record the session recorded, in the `at` format of `omhl get`.
- `nodes` and `relations`: how many node and relation records the session recorded. Every record counts, revisions and retirements included.
- `work_items`: the work items the session touched (in the text, `work items ...` or `no work items`). That of a node is the `work_item` of the data; that of a work item or pull request node is its own id; guideline, check and native node have none. A retirement inherits that of the previous record of the same node or relation. A relation belongs to the work items that its two ends had when it was recorded; its retirement does not reread the ends, even if they changed work item later.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `work-item-not-found` (exit 1): a `--work-item` does not exist. The pointer is `/work_item`. Check the id with `omhl get <id>`.
- `INVALID_INPUT` (exit 1): a `--work-item` has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

In a database recorded only outside Claude Code, the list is empty:

```omhl-example fixture=project
$ omhl report sessions
✔ report sessions · none
$ omhl report sessions --work-item work-items.w1 --format json
{"sessions":[]}
```

Refusal: the work item does not exist:

```omhl-example fixture=project
$ omhl report sessions --work-item work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```

Two sessions: the first recorded a decision and the relation that links it to the premise of `work-items.w1`, the second recorded a gap of the same work item. The decision has no `work_item`; that of the first session comes from the end of the relation. The work item and the premise were recorded outside Claude Code and have no line:

```omhl-example fixture=sessions
$ omhl report sessions
✔ report sessions · 2 sessions
session 5b1c0f3e-77a2-4c1e-9d0b-2f6a8e41c9aa · first 2026-11-24T22:32:22.926Z · last 2026-11-24T22:32:22.926Z · 1 node · 1 relation · work items work-items.w1
session 0e4d2a96-3c58-4f0b-8a17-d6b9c1e75f02 · first 2026-11-24T22:32:23.114Z · last 2026-11-24T22:32:23.114Z · 1 node · 0 relations · work items work-items.w1
$ omhl report sessions --work-item work-items.w1 --format json
{"sessions":[{"first":"2026-11-24T22:32:22.926Z","id":"5b1c0f3e-77a2-4c1e-9d0b-2f6a8e41c9aa","last":"2026-11-24T22:32:22.926Z","nodes":1,"relations":1,"work_items":["work-items.w1"]},{"first":"2026-11-24T22:32:23.114Z","id":"0e4d2a96-3c58-4f0b-8a17-d6b9c1e75f02","last":"2026-11-24T22:32:23.114Z","nodes":1,"relations":0,"work_items":["work-items.w1"]}]}
```
