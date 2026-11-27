# omhl gate-pr

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review 8, review rules | `omhl gate-pr` |

## When to use

Before opening or merging the pull request of a work item, to know whether everything the gate asks for is already recorded. It is read-only: it records nothing and holds for an open or closed work item. The gate charges the **current commit** (`HEAD`): after any new commit, the checks need to be attested again. `omhl close` runs this same gate and records the result.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- `--base <ref>`: the base of the loosening review; the review reads the diff from the merge-base it prints to `HEAD`, and a pull request without `--base` is not recognized as a repair PR.
- It needs a git repository with a clean tree: the gate reads `HEAD` and its tree. It also reads the `.omhl/workflow.toml` of the main directory, which says which native checks are on ([native-checks.md](../../directives-flow/references/native-checks.md)).

## Output

Text on stdout, exit 0, **even with the gate failed**: the header symbol says the result (`✔ gate-pr · pull-request · passed · 7 questions` when it passes, `✘ gate-pr · pull-request · not passed · 7 questions · 4 failed` when it fails). Each question gets a `✔` or `✘` line with the `label`, the `kind` (omitted when it repeats the `label`), `matched N` (how many records answer it) and, when there are any, `failing <ids>` (the ids that fail) and `N omitted` (how many the report left unlisted). The `next_step` of the first failing question that carries one goes in the `Next:`; that of the others stays in the `next_step` field below its own line. With `--format json` it is a JSON line with `gate` (`pull-request`), `passed`, `questions` and `chain_head`; each question carries `label`, `kind`, `passed`, `matched`, `failing`, `omitted` and, when an anchor question fails, `next_step`. The gate passes when all the questions pass, in order:

- one `occurred` question per required check: there must be an attestation with `ok: true` for the check and work item pair, tied to `HEAD` (to the tree of `HEAD`, for a check with a tree `snapshot`). The required ones are the native checks `checks.run-tests` (`before-commit`) and `checks.ci-green-before-merge` (`before-merge`) that `[native] checks` turns on, plus the project checks with `blocking = true` and event `before-commit`, `before-pr` or `before-merge`. A native check that is off gets no question, and a project check never replaces a native one that is on: both are asked, each with its own attestation;
- for a check with `run`, the question `spec-digest-match:<check>`: the attestation must have been recorded with the check's current definition (if you edited the TOML after `check run`, run `omhl load` and the check again);
- `no-open-contradiction`: no decision of the work item contradicts another;
- `decisions-anchored` and `anchors-confirmed`: every decision of the work item rests (`rests-on`) on a confirmed guideline or premise.

A check that failed in `check run` does not count: its question comes out with `matched 0`, as if it had never run; the cause is in `omhl byproduct`. A **manual** check (without `run`) gets the outcome of `omhl attest`; a check with `run`, of `omhl check run`. A check with `blocking = false` does not enter the gate.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing `<WORK_ITEM>` is a clap usage error, with exit 2 and no code. A failed gate is **not** an error. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `work-item-not-found` (exit 1): the work item does not exist (or the id belongs to another node type). The pointer is `/work_item`. Check the id.
- `INVALID_INPUT` (exit 1): the id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `work-item-type-unknown` (exit 1): the work item has no type the project's matrix knows. Record the type with `omhl append` or name the branch with its prefix.
- `review-base-invalid` (exit 1): `--base` does not name a commit or is the same commit as `HEAD`. Pass the integration branch the pull request targets, such as `develop`.
- `snapshot-dirty` (exit 1): the tree has an uncommitted change. Commit and repeat.
- `probe-failed` (exit 1): git did not answer (outside a repository, or in one with no commits). Run inside a checkout with at least one commit.
- `invalid-workflow-config` (exit 1): the `.omhl/workflow.toml` of the main directory is not accepted (a bad key or value, an unsupported version, a symbolic link, a file that is not UTF-8 or one above the size limit). The gate refuses before asking anything, except for the repair pull request (`--base` naming a diff that only fixes the file), and the error never turns a native check on or off. The other exception is `close --abandon` (`--abandon`), which does not evaluate the gate. Fix the file; `omhl load --dry-run` details it.
- `main-dir-unknown` (exit 1): the main directory could not be resolved from this checkout. Check the repository.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

With no attestation at all, the gate fails. Each required check appears with `matched 0` and its class (`warn` or `refuse`; only a failed `refuse` question stops `close`). The type requirements come last as `requires:*`, here with `requires:spec` failing because the spec is not valid (the fixture turns on `checks.run-tests` and `checks.ci-green-before-merge`):

```omhl-example fixture=native-checks
$ omhl gate-pr work-items.w1
✘ gate-pr · pull-request · not passed · 9 questions · 5 failed
✘ checks.run-tests · warn · occurred · matched 0
✘ checks.ci-green-before-merge · refuse · occurred · matched 0
✘ checks.ok · refuse · occurred · matched 0
✘ spec-digest-match:checks.ok · refuse · occurred · matched 0
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ requires:business-premise · requirement · matched 1
✘ requires:spec · refuse · requirement · matched 1
Next: make the spec valid with omhl spec validate.
```

The same gate, in JSON (the `passed` and the fields of each question that the text summarizes):

```omhl-example fixture=native-checks
$ omhl gate-pr work-items.w1 --format json
{"chain_head":{"hash":"<hash>","seq":4},"gate":"pull-request","passed":false,"questions":[{"class":"warn","failing":[],"kind":"occurred","label":"checks.run-tests","matched":0,"omitted":0,"passed":false},{"class":"refuse","failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":0,"omitted":0,"passed":false},{"class":"refuse","failing":[],"kind":"occurred","label":"checks.ok","matched":0,"omitted":0,"passed":false},{"class":"refuse","failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"requirement","label":"requires:business-premise","matched":1,"omitted":0,"passed":true},{"class":"refuse","failing":[],"kind":"requirement","label":"requires:spec","matched":1,"next_step":"make the spec valid with omhl spec validate","omitted":0,"passed":false}]}
```

With the two native checks attested by hand and the mechanical check run, on the same commit, the checks pass and only the `requires:spec` requirement still fails:

```omhl-example fixture=native-checks
$ omhl attest checks.run-tests work-items.w1 --outcome passed
✔ attest · checks.run-tests · work-items.w1 · passed · snapshot <snapshot>
$ omhl attest checks.ci-green-before-merge work-items.w1 --outcome passed
✔ attest · checks.ci-green-before-merge · work-items.w1 · passed · snapshot <snapshot>
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":"<ms>","exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl gate-pr work-items.w1
✘ gate-pr · pull-request · not passed · 9 questions · 1 failed
✔ checks.run-tests · occurred · matched 1
✔ checks.ci-green-before-merge · occurred · matched 1
✔ checks.ok · occurred · matched 1
✔ spec-digest-match:checks.ok · occurred · matched 1
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ requires:business-premise · requirement · matched 1
✘ requires:spec · refuse · requirement · matched 1
Next: make the spec valid with omhl spec validate.
```

With no `[native]` table, the gate asks only about the project checks, the decisions and the type requirements:

```omhl-example fixture=checks
$ omhl gate-pr work-items.w1
✘ gate-pr · pull-request · not passed · 7 questions · 3 failed
✘ checks.ok · refuse · occurred · matched 0
✘ spec-digest-match:checks.ok · refuse · occurred · matched 0
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ requires:business-premise · requirement · matched 1
✘ requires:spec · refuse · requirement · matched 1
Next: make the spec valid with omhl spec validate.
```

Refusal: the `.omhl/workflow.toml` lists an id that is not a native check:

```omhl-example fixture=invalid-workflow
$ omhl gate-pr work-items.w1
2> ✘ invalid-workflow-config · <repo>/.omhl/workflow.toml: native.checks must list only native check ids at line 4, column 11
2>   - /workflow
2> Next: fix the workflow file named in the message; `omhl load --dry-run` details the problem.
# exit 1
```

Refusal: the work item does not exist and the id has an invalid form:

```omhl-example fixture=checks
$ omhl gate-pr work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl gate-pr Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
```

Refusal: a tree with an uncommitted change (the fixture leaves the file `notes.txt` uncommitted):

```omhl-example fixture=dirty-checks
$ omhl gate-pr work-items.w1
2> ✘ snapshot-dirty · snapshot-dirty at /snapshot
2>   - /snapshot · snapshot-dirty
# exit 1
```

Refusal: outside a git repository (the `project` fixture has no git):

```omhl-example fixture=project
$ omhl gate-pr work-items.w1
2> ✘ probe-failed · probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git
2>   - /snapshot · probe-failed
# exit 1
```
