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

When the work item's work is finished and the delivery gate was already checked with `omhl gate-delivery`. `close` runs the gate again, records the closing on the work item and returns the gate result together with the retro. Run it with the PR ready, not before: the closing is recorded in an append-only database.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist and be open.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- `--base <ref>`: the base of the loosening review. Pass the merge-base that `gate-delivery` prints, as in [gate-delivery.md](gate-delivery.md); `close` does not compute it.
- `--force --note <text>`: closes with the gate failed only by warnings; `--force --note` only for `close-warnings`, and the note carries the owner's literal phrase authorizing it. `--force` does not help in `close-refused`. Before asking the owner on `close-warnings`, try to resolve each warning; ask only for what is left, citing each one.
- `--failed-delivery --note <text>`: closes a work item whose pull request was merged with the gate failed; the note carries the owner's literal phrase. The request for it names the failed `refuse` questions and says the history stays as a failed delivery.
- `--abandon --reason <text> --note <text>` (`--abandon --reason --note` together): closes a work item that will not be delivered, or whose merge `--failed-delivery` does not cover. The `--reason` says how the merge happened, when there was one. `--abandon` does not evaluate the gate. Before the merge, the request for it says what evidence, premise or spec is missing and proposes the fix first.
- It needs a git repository with a clean tree at the tip of the work item branch: the closing ties the gate result to `HEAD`. When every attestation fails at once, check the commit before any other exit: the checkout is probably not at that tip. It also reads the `.omhl/workflow.toml` of the main directory, which says which native checks the gate asks about ([native-checks.md](../../directives-flow/references/native-checks.md)).

## Output

Text on stdout, exit 0, in up to three blocks, in this order:

- the gate: the header `close · delivery · passed|not passed · N questions` (`✔` with the gate passed, `⚠` with it failed) and one `✔` or `✘` line per question, the same as in [gate-delivery.md](gate-delivery.md). The `next_step` of each failing question stays in the field below its line;
- what was recorded: `✔ closed · N records` and one `- <id> · seq N` line per record, as in `append`;
- the retro: `✔ retro · <counts>` and one line per item, as in `omhl report retro`. The block only appears when there is a retro, and the `Next:` (propose one improvement per item to the owner) only when the retro carries some item.

With `--format json` it is a JSON line with three fields:

- `outcome`: what was recorded (`records`, `chain_head`, `replayed`). The recording is a revision of the work item with `status: closed` and, in `gate`, the gate name, `passed`, the `chain_head_seq` and the commit snapshot.
- `report`: the same report as `omhl gate-delivery` (`gate`, `passed`, `questions`, `chain_head`).
- `retro`: `null` when there is no divergence or gap answered without a criterion; otherwise, the report of `omhl report retro`. The retro reads the facts before recording, so `close` never returns an error after recording.

**With the gate failed, `close` refuses** (`close-refused`, or `close-warnings` when only warnings failed) and records nothing. Closing is the last step, with no commit after the merge: a commit after it asks for the checks again. After the closing, the work item's premise leaves the session context (`omhl render`) and stays in the projection.

Closing a work item that was already merged:

- A merged work item never closes with `--abandon`, unless the merge is one `--failed-delivery` does not cover. It closes with `--failed-delivery --note`, with the owner's literal phrase in the `--note`.
- The limit: `--failed-delivery` does not cover squash or rebase merges or a project without `branch.integration`. There the owner uses `--abandon`, and the `--reason` says how the merge happened.
- The retro is required only after `--failed-delivery`: propose one improvement per item, and one improvement per failed `refuse` question saying how it got past the brake before the merge, as in [fechamento.md](fechamento.md). `--abandon` has no required retro.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing `<WORK_ITEM>` is a clap usage error, with exit 2 and no code. When there is a retro and reading the facts fails, `close` refuses without recording anything. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `work-item-not-found` (exit 1): the work item does not exist. The pointer is `/work_item`. Check the id.
- `close-refused` (exit 1): the gate failed. `--force` does not help here: fix each failed question, or the owner decides on `--abandon` or `--failed-delivery`. The request to the owner differs by error code: here, before the merge, say what is missing and propose the fix before asking for `--abandon`; after the merge, name the failed `refuse` questions.
- `close-warnings` (exit 1): the gate failed only by warnings. Try to resolve each warning first; then close with `--force --note`, only with the owner's phrase.
- `work-item-type-unknown` (exit 1): the work item has no type the project's matrix knows. Record the type with `omhl append` or name the branch with its prefix.
- `review-base-invalid` (exit 1): `--base` does not name a commit or is the same commit as `HEAD`. Pass the merge-base that `gate-delivery` prints.
- `failed-delivery-unneeded` (exit 1): no `refuse` question failed, so `--failed-delivery` has nothing to record. Close without it, or with `--force --note` when only warnings failed.
- `delivery-not-on-work-item-branch` (exit 1): the current branch is not the work item's. Check it out and run `close` again.
- `delivery-not-integrated` (exit 1): the evaluated commit is not in the local integration branch. Update the local integration branch (`branch.integration`) from the remote without a new commit, or ask the owner for `--abandon`.
- `invalid-transition` (exit 1): the work item is already closed. A closed work item does not close again.
- `INVALID_INPUT` (exit 1): the id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `snapshot-dirty` (exit 1): the tree has an uncommitted change. Commit and repeat.
- `probe-failed` (exit 1): git did not answer (outside a repository, for example). Run inside a checkout.
- `spec-store-unusable` (exit 1): there is a retro and the work item's spec folder could not be read, or there is more than one `wi-<id>__*` folder. Fix the folder in the main directory.
- `invalid-workflow-config` (exit 1): the `.omhl/workflow.toml` of the main directory is not accepted. `close` refuses before it evaluates the gate and before it records anything, with a retro or without one, and the error never turns a native check on or off. Two paths do not refuse: `--abandon`, and the repair pull request (`--base` naming a diff that only fixes the file). Fix the file; `omhl load --dry-run` details it.
- `main-dir-unknown` (exit 1): the main directory could not be resolved from this checkout. Check the repository.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the marker `outside-claude-code`; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Tries to close the work item. The gate fails (the attestations are missing and the spec is not valid; the fixture turns on `checks.run-tests` and `checks.ci-green-before-merge`). A failed `refuse` question stops `close`: it records nothing, so the work item stays open and the next call refuses the same way. The `warn` question is listed, but only the `refuse` ones block:

```omhl-example fixture=native-checks
$ omhl close work-items.w1
2> ✘ close-refused · a refuse question failed, and --force does not override it: fix its cause before the merge, use --abandon for work that will not be delivered, or --failed-delivery --note when the evaluated commit is already on the integration branch
2>   - /questions/0 · warn · the gate question did not pass
2>     label: checks.run-tests
2>   - /questions/1 · refuse · the gate question did not pass
2>     label: checks.ci-green-before-merge
2>   - /questions/2 · refuse · the gate question did not pass
2>     label: checks.ok
2>   - /questions/3 · refuse · the gate question did not pass
2>     label: spec-digest-match:checks.ok
2>   - /questions/4 · refuse · the gate question did not pass
2>     label: requires:spec
2> Next: make the spec valid with omhl spec validate.
# exit 1
```

The same refusal, in JSON:

```omhl-example fixture=native-checks
$ omhl close work-items.w1 --format json
2> {"error":{"code":"close-refused","details":[{"code":"warn","context":{"label":"checks.run-tests"},"message":"the gate question did not pass","path":"/questions/0"},{"code":"refuse","context":{"label":"checks.ci-green-before-merge"},"message":"the gate question did not pass","path":"/questions/1"},{"code":"refuse","context":{"label":"checks.ok"},"message":"the gate question did not pass","path":"/questions/2"},{"code":"refuse","context":{"label":"spec-digest-match:checks.ok"},"message":"the gate question did not pass","path":"/questions/3"},{"code":"refuse","context":{"label":"requires:spec"},"message":"the gate question did not pass","next_step":"make the spec valid with omhl spec validate","path":"/questions/4"}],"message":"a refuse question failed, and --force does not override it: fix its cause before the merge, use --abandon for work that will not be delivered, or --failed-delivery --note when the evaluated commit is already on the integration branch"}}
# exit 1
```

With the two native checks attested and the mechanical check run, only the type requirement `requires:spec` is left, and `close` still refuses until the spec is valid:

```omhl-example fixture=native-checks
$ omhl attest checks.run-tests work-items.w1 --outcome passed
✔ attest · checks.run-tests · work-items.w1 · passed · snapshot <snapshot>
$ omhl attest checks.ci-green-before-merge work-items.w1 --outcome passed
✔ attest · checks.ci-green-before-merge · work-items.w1 · passed · snapshot <snapshot>
$ omhl check run --event before-push --format json
{"blocked":[],"event":"before-push","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":"<ms>","exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl close work-items.w1
2> ✘ close-refused · a refuse question failed, and --force does not override it: fix its cause before the merge, use --abandon for work that will not be delivered, or --failed-delivery --note when the evaluated commit is already on the integration branch
2>   - /questions/0 · refuse · the gate question did not pass
2>     label: requires:spec
2> Next: make the spec valid with omhl spec validate.
# exit 1
```

With an open divergence the gate fails the same way and nothing is recorded, so there is no retro: the retro (only ids, never the divergence text) comes only with a `close` that records:

```omhl-example fixture=native-checks
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 5
$ omhl close work-items.w1
2> ✘ close-refused · a refuse question failed, and --force does not override it: fix its cause before the merge, use --abandon for work that will not be delivered, or --failed-delivery --note when the evaluated commit is already on the integration branch
2>   - /questions/0 · warn · the gate question did not pass
2>     label: checks.run-tests
2>   - /questions/1 · refuse · the gate question did not pass
2>     label: checks.ci-green-before-merge
2>   - /questions/2 · refuse · the gate question did not pass
2>     label: checks.ok
2>   - /questions/3 · refuse · the gate question did not pass
2>     label: spec-digest-match:checks.ok
2>   - /questions/4 · refuse · the gate question did not pass
2>     label: requires:spec
2> Next: make the spec valid with omhl spec validate.
# exit 1
```

With the gate failed and the work item not to be delivered, the owner closes it with `--abandon`, which does not evaluate the gate:

```omhl-example fixture=native-checks
$ omhl close work-items.w1 --abandon --reason not-delivered --note owner-authorized
✔ close · retro · 1 abandonment
- abandoned · not-delivered · owner-authorized
Next: propose one improvement per item to the owner and change nothing without the answer.
```

Refusal: the `.omhl/workflow.toml` lists an id that is not a native check. Nothing is recorded:

```omhl-example fixture=invalid-workflow
$ omhl close work-items.w1
2> ✘ invalid-workflow-config · <repo>/.omhl/workflow.toml: native.checks must list only native check ids at line 4, column 11
2>   - /workflow
2> Next: fix the workflow file named in the message; `omhl load --dry-run` details the problem.
# exit 1
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

A `[native.run."<id>"]` table of a native disabled in its own workflow.toml is ignored, with a `native-run-ignored` warning (`check`, `from`, `hint`). `from` is `main-directory` or `HEAD`.

A workflow.toml that is not valid UTF-8 adds no requirement and warns `evaluated-workflow-unreadable` (`from`, `reason`: `not-utf8`); `from` is `HEAD`.
