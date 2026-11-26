# omhl report recurrences

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| analyst | 2 | `omhl report recurrences --min 2` |

The session also runs it before recording a premise: [directives-flow](../SKILL.md), Rules section.

## When to use

At two moments. Before recording a premise, with `--min 2`, to find an earlier premise that means the same thing and copy its exact text (so the promotion groups the two). And at the end of a work item, without `--min`, to see the premises repeated across enough work items to suggest promotion to a guideline. The report only reads: suggesting the promotion is up to you, the confirmation is the human's, and only then is the `guideline` recorded, with `confirmed_by`. It is read-only.

## Inputs

- `--min <n>`: the minimum number of distinct work items for the group to enter. Without the option, `promotion.min_work_items` of `.omhl/workflow.toml` applies, and in its absence the default 3. The option beats the file. The command does not validate the value: `--min 0` and `--min 1` list all groups.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database, outside the project folder; `--project-dir` must exist. `workflow.toml` is read from the project even with `--db`.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ report recurrences · min N · G groups` (`min` is the value used) and, per group, the line `- kind · text` (the normalized text) followed by `work items:` and `premises:` with the whole ids. With `--format json` it is a JSON line: `min` and `recurrences`, the list of groups, each with `kind`, `text`, `work_items` and `premises`. The grouping is by the `kind` key and normalized text: the text goes through `trim`, lowercase, collapsed spaces and removal of trailing punctuation. The group counts the distinct work items, including those already closed. A premise without `kind` or without `work_item` does not enter, and a group disappears when a guideline with the same normalized text already exists. `✔ report recurrences · min 3 · none` (in JSON, `{"min":3,"recurrences":[]}`) is the answer when nothing repeats enough.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A `--min` that is not an unsigned integer is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `invalid-workflow-config` (exit 1): without `--min`, `.omhl/workflow.toml` is invalid. Fix the file, or pass `--min`.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

A single premise does not repeat: with the default 3, the report comes out empty. With `--min 1`, the premise group appears; with `--format json` the same report comes out for machines:

```omhl-example fixture=project
$ omhl report recurrences
✔ report recurrences · min 3 · none
$ omhl report recurrences --min 1
✔ report recurrences · min 1 · 1 group
- business · money is stored in cents
  work items: work-items.w1
  premises: premises.p1
$ omhl report recurrences --min 1 --format json
{"min":1,"recurrences":[{"kind":"business","premises":["premises.p1"],"text":"money is stored in cents","work_items":["work-items.w1"]}]}
```

Three work items with the same premise, written with different case, spaces and punctuation, form a group at the default 3. With `--min 4`, the group disappears:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w2","type":"work-item","data":{"title":"Second item","status":"open"}}},{"body":{"kind":"node","id":"work-items.w3","type":"work-item","data":{"title":"Third item","status":"open"}}}]}
EOF
✔ append · 2 records
- <record-id> · seq 3
- <record-id> · seq 4
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p2","type":"premise","data":{"text":"money is stored in cents.","status":"assumed","kind":"business","work_item":"work-items.w2"}}},{"body":{"kind":"node","id":"premises.p3","type":"premise","data":{"text":"Money  is stored in CENTS","status":"assumed","kind":"business","work_item":"work-items.w3"}}}]}
EOF
✔ append · 2 records
- <record-id> · seq 5
- <record-id> · seq 6
$ omhl report recurrences
✔ report recurrences · min 3 · 1 group
- business · money is stored in cents
  work items: work-items.w1, work-items.w2, work-items.w3
  premises: premises.p1, premises.p2, premises.p3
$ omhl report recurrences --min 4
✔ report recurrences · min 4 · none
```
