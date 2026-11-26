# omhl report answers

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| planner | 4 | `omhl report answers --work-item work-items.<id>` |
| session | plan-consensus 3 | `omhl report answers --work-item work-items.<id>` |

## When to use

To see the state of each gap (a technical doubt opened during execution): whether it is open, who answered it and whether a confirmed guideline supports the answer. Use it before writing the spec or closing the work item, to know which doubts still await an answer and which answers the human has not seen yet. It is read-only.

## Inputs

- `--work-item <id>`: restricts to the gaps of a work item, like `work-items.w1`. The work item must exist (open or closed). Without the option, the report covers all gaps.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database, outside the project folder; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ report answers · N gaps` and one line per live gap, in creation order, `id · state · ...`. With no gap at all (or a work item filter with no gaps) it prints `✔ report answers · none`, which here is a true answer (the work item exists, it just has no gaps). With `--format json` it is a JSON line, `{"gaps": [...]}`. Each gap carries `id`, `work_item` (when the gap has one; in the text, `of <work item>`) and `state`:

- `open`: no premise answers it. Only these three fields.
- `answered`: there is an answer. The gap gains `answered_by` (the premise; in the text, `by <premise>`), `answerer` (in the text, `<answerer> answer`), `author` (who recorded the answer; JSON only) and `no_criterion` (in the text, `no criterion` or `criterion found`).
- `needs-human`: the answer contradicts a confirmed guideline and has no `confirmed_by`. The gap keeps counting as open in the validator and in the retro, until the human decides.

`answerer` is `human` when the answering premise is confirmed (`confirmed` with `confirmed_by`) and `autonomous` in any other case. `no_criterion` is `true` when no confirmed guideline supports the answer; these answers enter the retro. With more than one answer, the first confirmed premise counts; with none, the first in recording order.

An answer is the `answers` relation from a premise to the gap, recorded with `omhl append`. To support the answer, record the `supports` relation from a confirmed guideline to the premise.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `work-item-not-found` (exit 1): `--work-item` does not exist. The pointer is `/work_item`. Check the id with `omhl get <id>`.
- `INVALID_INPUT` (exit 1): `--work-item` has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

With no gap at all, the report is empty:

```omhl-example fixture=project
$ omhl report answers
✔ report answers · none
```

Two gaps: one answered by the confirmed premise `premises.p1` (with no guideline supporting it) and one open. The work item filter returns the same, because both are its own; with `--format json` the report comes out for machines:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.fmt","type":"gap","data":{"question":"Which date format?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 3 records
- <record-id> · seq 3
- <record-id> · seq 4
- <record-id> · seq 5
$ omhl report answers
✔ report answers · 2 gaps
gaps.limit · answered · by premises.p1 · human answer · no criterion · of work-items.w1
gaps.fmt · open · of work-items.w1
$ omhl report answers --work-item work-items.w1
✔ report answers · 2 gaps
gaps.limit · answered · by premises.p1 · human answer · no criterion · of work-items.w1
gaps.fmt · open · of work-items.w1
$ omhl report answers --format json
{"gaps":[{"answered_by":"premises.p1","answerer":"human","author":{"agent":"claude","client":"cli"},"id":"gaps.limit","no_criterion":true,"state":"answered","work_item":"work-items.w1"},{"id":"gaps.fmt","state":"open","work_item":"work-items.w1"}]}
```

Refusal: the work item does not exist and the id is malformed:

```omhl-example fixture=project
$ omhl report answers --work-item work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl report answers --work-item Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
```
