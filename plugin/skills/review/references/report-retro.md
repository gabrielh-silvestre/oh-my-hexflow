# omhl report retro

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review 8 | `omhl report retro` |

## When to use

To read the retro of a work item, open or closed, without closing it: the same report that `omhl close` prints in the `retro` field. Use it before `close` to see what the retro will carry, or after it to propose improvements. It is read-only.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist; it may be open or closed.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- It does not require a clean tree. Without a retro it does not require git either; with a retro, the command reads the spec folder through the project's git repository and refuses with `probe-failed` outside one.

## Output

Text on stdout, exit 0. Without a retro it prints `✔ report retro · none`: the work item has no divergence or gap answered without a criterion (an open gap alone, or an amendment file alone, does not produce a retro). With a retro, the header counts the items (`✔ report retro · 1 divergence · 1 no-criterion gap`), each item gets a `- divergence`, `- amendment`, `- pending amendment`, `- no criterion` or `- open gap` line with the whole ids, and the `Next:` says to propose one improvement per item to the owner. With `--format json` it is a JSON line, `{"retro": ...}`, with `null` when there is no retro. With a retro, the object carries:

- `divergences`: the ids grouped by `kind` and `resolution`.
- `amendments`: `approved` (amendments cited by a `spec-stale` divergence) and `pending` (amendment files that no divergence cites).
- `no_criterion`: the gaps answered without a guideline backing the answer (`gap` and `answered_by`).
- `open_gaps`: the `open` and `needs-human` gaps.

The retro carries only ids and file names, never the text of a divergence, spec or amendment. Propose the improvements as described in [fechamento.md](fechamento.md).

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing `<WORK_ITEM>` is a clap usage error, with exit 2 and no code. The spec folder and `workflow.toml` are only read when there is a retro. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `work-item-not-found` (exit 1): the work item does not exist. The pointer is `/work_item`. Check the id.
- `INVALID_INPUT` (exit 1): the id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `spec-store-unusable` (exit 1): there is a retro and the work item's spec folder could not be read, or there is more than one `wi-<id>__*` folder. Fix the folder in the main directory.
- `invalid-workflow-config` (exit 1): there is a retro and `.omhl/workflow.toml` is invalid. Fix the file.
- `probe-failed` (exit 1): there is a retro and git did not answer (outside a repository, for example). Run inside a checkout. Without a retro the command does not read git.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

With no divergence or answered gap, there is no retro:

```omhl-example fixture=project
$ omhl report retro work-items.w1
✔ report retro · none
$ omhl report retro work-items.w1 --format json
{"retro":null}
```

With an open divergence and a gap answered by a premise without a guideline backing it, the retro lists both (the gap without a criterion carries the premise that answered it):

```omhl-example fixture=git
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 3 records
- <record-id> · seq 3
- <record-id> · seq 4
- <record-id> · seq 5
$ omhl report retro work-items.w1
✔ report retro · 1 divergence · 1 no-criterion gap
- divergence · missing · open · divergences.limit
- no criterion · gaps.limit · answered by premises.p1
Next: propose one improvement per item to the owner and change nothing without the answer.
$ omhl report retro work-items.w1 --format json
{"retro":{"amendments":{"approved":[],"pending":[]},"divergences":[{"ids":["divergences.limit"],"kind":"missing","resolution":"open"}],"no_criterion":[{"answered_by":"premises.p1","gap":"gaps.limit"}],"open_gaps":[]}}
```

With a retro, outside a git repository, the command refuses:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 3 records
- <record-id> · seq 3
- <record-id> · seq 4
- <record-id> · seq 5
$ omhl report retro work-items.w1
2> ✘ probe-failed · probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git
2>   - /snapshot · probe-failed
# exit 1
```

Refusal: the work item does not exist and the id is malformed:

```omhl-example fixture=project
$ omhl report retro work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl report retro Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
```
