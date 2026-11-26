# omhl close

- [Who uses it](#who-uses-it-at-which-stage-and-with-which-filter)
- [When to use](#when-to-use)
- [Inputs](#inputs)
- [Output](#output)
- [Errors](#errors)
- [Examples](#examples)

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review 8, review rules | `omhl close` |

## When to use

When the work item's work is finished and the pull request gate was already checked with `omhl gate-pr`. `close` runs the gate again, records the closing on the work item and returns the gate result together with the retro. Run it with the PR ready, not before: the closing is recorded in an append-only database.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist and be open.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- It needs a git repository with a clean tree: the closing ties the gate result to `HEAD`.

## Output

Text on stdout, exit 0, in up to three blocks, in this order:

- the gate: the header `close · pull-request · passed|not passed · N questions` (`✔` with the gate passed, `⚠` with it failed) and one `✔` or `✘` line per question, the same as in [gate-pr.md](gate-pr.md). The `next_step` of each failing question stays in the field below its line;
- what was recorded: `✔ closed · N records` and one `- <id> · seq N` line per record, as in `append`;
- the retro: `✔ retro · <counts>` and one line per item, as in `omhl report retro`. The block only appears when there is a retro, and the `Next:` (propose one improvement per item to the owner) only when the retro carries some item.

With `--format json` it is a JSON line with three fields:

- `outcome`: what was recorded (`records`, `chain_head`, `replayed`). The recording is a revision of the work item with `status: closed` and, in `gate`, the gate name, `passed`, the `chain_head_seq` and the commit snapshot.
- `report`: the same report as `omhl gate-pr` (`gate`, `passed`, `questions`, `chain_head`).
- `retro`: `null` when there is no divergence or gap answered without a criterion; otherwise, the report of `omhl report retro`. The retro reads the facts before recording, so `close` never returns an error after recording.

**`close` closes even with the gate failed** (`report.passed: false` in the JSON, `⚠` in the text): the result stays recorded on the work item. After the closing, the work item's premise leaves the session context (`omhl render`) and stays in the projection. When `retro` comes filled in, propose one improvement per item, as in [fechamento.md](fechamento.md).

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing `<WORK_ITEM>` is a clap usage error, with exit 2 and no code. When there is a retro and reading the facts fails, `close` refuses without recording anything. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `work-item-not-found` (exit 1): the work item does not exist. The pointer is `/work_item`. Check the id.
- `invalid-transition` (exit 1): the work item is already closed. A closed work item does not close again.
- `INVALID_INPUT` (exit 1): the id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `snapshot-dirty` (exit 1): the tree has an uncommitted change. Commit and repeat.
- `probe-failed` (exit 1): git did not answer (outside a repository, for example). Run inside a checkout.
- `spec-store-unusable` (exit 1): there is a retro and the work item's spec folder could not be read, or there is more than one `wi-<id>__*` folder. Fix the folder in the main directory.
- `invalid-workflow-config` (exit 1): there is a retro and `.omhl/workflow.toml` is invalid. Fix the file. Without a retro, `close` records even with the file invalid.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the marker `outside-claude-code`; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Closes the work item and then tries to close it again. The gate fails (the attestations are missing), and the closing is recorded anyway; the retro is `null`:

```omhl-example fixture=checks
$ omhl close work-items.w1
⚠ close · pull-request · not passed · 7 questions · 4 failed
✘ checks.run-tests · occurred · matched 0
✘ checks.ci-green-before-merge · occurred · matched 0
✘ checks.ok · occurred · matched 0
✘ spec-digest-match:checks.ok · occurred · matched 0
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ closed · 1 record
- <record-id> · seq 5
$ omhl close work-items.w1
2> ✘ invalid-transition · invalid-transition at /work_item
2>   - /work_item · invalid-transition
# exit 1
```

The same closing, in JSON:

```omhl-example fixture=checks
$ omhl close work-items.w1 --format json
{"outcome":{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false},"report":{"chain_head":{"hash":"<hash>","seq":4},"gate":"pull-request","passed":false,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]},"retro":null}
```

With the gate green (the two native checks attested and the mechanical check run), `report.passed` is `true`:

```omhl-example fixture=checks
$ omhl attest checks.run-tests work-items.w1 --outcome passed
✔ attest · checks.run-tests · work-items.w1 · passed · snapshot <snapshot>
$ omhl attest checks.ci-green-before-merge work-items.w1 --outcome passed
✔ attest · checks.ci-green-before-merge · work-items.w1 · passed · snapshot <snapshot>
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":"<ms>","exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl close work-items.w1
✔ close · pull-request · passed · 7 questions
✔ checks.run-tests · occurred · matched 1
✔ checks.ci-green-before-merge · occurred · matched 1
✔ checks.ok · occurred · matched 1
✔ spec-digest-match:checks.ok · occurred · matched 1
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ closed · 1 record
- <record-id> · seq 8
```

With an open divergence, the retro comes filled in (only ids, never the divergence text):

```omhl-example fixture=checks
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 5
$ omhl close work-items.w1
⚠ close · pull-request · not passed · 7 questions · 4 failed
✘ checks.run-tests · occurred · matched 0
✘ checks.ci-green-before-merge · occurred · matched 0
✘ checks.ok · occurred · matched 0
✘ spec-digest-match:checks.ok · occurred · matched 0
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ closed · 1 record
- <record-id> · seq 6
✔ retro · 1 divergence
- divergence · missing · open · divergences.limit
Next: propose one improvement per item to the owner and change nothing without the answer.
```

Refusal: the work item does not exist, the id is malformed and the id belongs to a premise:

```omhl-example fixture=checks
$ omhl close work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl close Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
$ omhl close premises.p1
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```

Refusal: outside a git repository (the `project` fixture has no git):

```omhl-example fixture=project
$ omhl close work-items.w1
2> ✘ probe-failed · probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git
2>   - /snapshot · probe-failed
# exit 1
```
