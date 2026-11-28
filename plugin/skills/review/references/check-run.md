# omhl check run

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review rules | `omhl check run` |
| session | review 8 | `omhl check run --event before-merge` |

## When to use

To run the mechanical checks (those with `run`) of an event, or a single check, and record the attestation of each one for the work item of the current branch. The git hooks call the command on their own (`--event before-commit` in `pre-commit`, `--event before-push --pre-push` in `pre-push`). By hand, use `--event before-merge` to record the CI status, or `--check <id>` to repeat a check. It only runs on Linux. If you touched a check, run `omhl load` first.

## Inputs

- Exactly one of `--event` (`before-commit`, `before-push` or `before-merge`) and `--check <id>` (for example `checks.ok`). `--check` must point to a check with `run`.
- `--format <agent|human|json>`: global option, valid before or after the subcommand; the output format, `agent` by default (today `human` prints the same text). Any other value, including `text`, is a clap usage error, with exit 2. The git hooks run without `--format` and show the `agent` text; to read the output from code, pass `--format json`.
- `--pre-push`: mode of the `pre-push` hook, which reads git's lines on stdin. It only holds with `--event before-push`.
- The work item comes from the branch name: `type/wi-<id>__<slug>`. On `develop` nothing is recorded (`recorded` is `false`); on `main` the command refuses.
- It needs a git repository. In `before-push` and `before-merge` the tree must be clean.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.

## Output

The default is the `agent` text, in English, with no color or ANSI sequence. With `--format json` the output is the one-line JSON, described further below. The exit code is the same in both formats.

On success, the text goes to stdout, with exit 0 when no blocking check failed:

- The first line carries the event, the work item and whether the attestation was recorded: `✔ before-push · work-items.w1 · recorded (git-commit 1a2b3c4)`. The part in parentheses is the snapshot type and the first 7 characters of the id (`git-tree` in `before-commit`, `git-commit` in the others); it disappears when there is no snapshot. On `develop` the line says `not recorded`.
- Each check has one line, in the order of the JSON: the icon (`✔` passed, `⚠` passed with warning, `✘` failed), the id, `concern/variant` when the check declares them, the outcome and the duration (`12ms`). Notes in parentheses mark `non-blocking`, `exit N`, `timed out` and `truncated`.
- Each warning has a line of its own with the action to take: `no-checks-for-event` (no check planned for the event, which is not a pass), `check-orphaned` (the check file was deleted, so it did not run) and `output-stored` (a `passed_with_warning` check recorded: the warning carries `check`, `work_item` and the command `omhl byproduct <check> <work-item> --tail 40` that reads the stored output; it does not come out on `passed`, on `failed` or on `develop`, where nothing is recorded).
- The last line says nothing was blocked and the next step: `Nothing blocked. Next: go on with the push.` (or the commit, or the merge).
- On a `--pre-push` with nothing to attest (the push only deletes refs or only sends tags), a single line comes out: `✔ before-push · nothing to attest in this push; no check ran`.

On the `checks-failed` failure, the text goes to stderr, with exit 1:

- The first line says the event is blocked and how many checks failed, separating the blocking from the non-blocking: `✘ BLOCKED · before-push · work-items.w1 · 1 blocking failed, 1 non-blocking failed, 1 passed, 1 not run`. The `non-blocking failed` part disappears when it is zero. On `develop` the work item becomes `no work item`.
- Each failed check carries a line with the id, `concern/variant` (if declared), `blocking` or `non-blocking`, `timeout Ns` (the check's limit), `exit N` or `timed out`, the duration and `truncated` (if the output was cut). The duration comes out as `NNNms` below 1 second and as `N.Ns` from there. Below come the `command:` (the check's `run`) and the `how to read the output:` with the exact `omhl byproduct`. On `develop` there is no `byproduct`: the text shows the tail of the output, and only it.
- When it shows check output, the text warns that it may hold secrets and must not be copied or repeated to the user. It closes with `Next:`.

The other errors go to stderr, also as text, with exit 1 (or today's exit): `✘ <code> · <message>`, one line per detail (`- <code> at <path>`) and, when the JSON carries them, the `kind`, the check, the cause, the `hint` and the next step.

With `--format json`, the output is one line on stdout: `event`, `work_item`, `recorded`, `snapshot` (`git-tree` in `before-commit`, `git-commit` in the others), `results` and `blocked`. Each item of `results` carries `check`, `blocking`, `outcome` (`passed`, `passed_with_warning` or `failed`), `ok`, `exit_code`, `duration_ms`, `timed_out` and `truncated`, plus `concern` and `variant` when the check declares them. The command's full output is stored; read it with `omhl byproduct`. At the start of each run the command also deletes, silently and at most once a day, the stored outputs older than 7 days that no live attestation uses ([byproduct-prune.md](byproduct-prune.md)). An event with no check in the database exits 0 with `checks_planned: 0` and the warning `no-checks-for-event`. A check whose file was deleted from the tree becomes a `check-orphaned` warning and does not run. The outcome of a snapshot is immutable: repeating the command on the same tree returns the recorded outcome.

## Errors

An error goes to stderr with exit 1: text by default, a JSON line with `--format json`. The codes below hold in both formats. An unknown option or a missing argument is a clap usage error, with exit 2 and no code. That includes missing both `--event` and `--check`, and using `--pre-push` without `--event before-push`. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `checks-failed` (exit 1): a blocking check failed. Each `details` with `failed` says how to read the output (`omhl byproduct <check> <work-item> --tail 40`). On `develop` there is no `byproduct`: the tail comes in `context.output_tail`. In the JSON, `error.context` carries `event`, `work_item` (absent on `develop`) and `summary` (`planned`, `passed`, `failed`, `blocking_failed` and `not_run`; `failed` counts every failure, and `blocking_failed` only the blocking ones), and the `context` of each `details` with `failed` carries `blocking`, `concern` and `variant` (if declared), `run`, `timeout_secs`, `exit_code` (absent with no code), `duration_ms`, `timed_out` and `truncated`. Fix the code and repeat. The output (including `context.output_tail`) may hold secrets: do not copy it to a PR, issue, commit, `--note` or `omhl append`, and do not repeat it to the user (rule in [byproduct.md](byproduct.md)).
- `check-not-run` (exit 1): a problem prevented the verdict, and nothing was recorded for the check. Each `details` carries the reason and a `context.kind`: `spawn-failed` (the `run` binary does not exist in git's `PATH`: `context.kind` `spawn-not-found`; no permission to execute it: `spawn-permission-denied`), `spec-stale` (the TOML diverges from the database: run `omhl load`), `spec-invalid`, `command-changed-tree` (the check changed the tree: keep it read-only) or `not-run` (the remaining check did not run). Follow the context's `hint` and repeat.
- `attestation-refused` (exit 1): the recording was refused, by the guard or by the environment's session. The detail `outcome-immutable` says the same snapshot already has another outcome: change the tree (a new commit) for a new verdict. Other reasons: `snapshot-tree-mismatch` and `snapshot-dirty` (do not stage during the hook), `work-item-closed`, `probe-failed`.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the marker `outside-claude-code`; nothing is recorded and the error does not show the value. It only appears as a detail of `attestation-refused`: the command of the first planned check already ran, and the others do not run. Remove the variable or run `omhl` in a Claude Code session.
- `attestation-not-recorded` (exit 1): the database was locked or failed to record after the command. The verdict was computed and not recorded: repeat the same command.
- `check-not-found` (exit 1): the `--check` does not exist in the database. Check the id and run `omhl load`; for a native disabled in both declarations the error carries a `next_step` to enable it in `[native] checks` of `.omhl/workflow.toml`.
- `check-not-mechanical` (exit 1): the `--check` points to a manual check, without `run`. Whoever records its outcome is `omhl attest`.
- `unsupported-platform`: the system is not Linux. Run on Linux or WSL.
- `probe-failed`: git did not answer (outside a repository, git older than 2.31). Run `git status` and fix the state.
- `branch-name-invalid`: the branch does not follow `type/wi-<id>__<slug>`. Rename with `git branch -m`.
- `branch-protected`: the branch is `main`, or the push has `main` as its destination. Create a work item branch and open a pull request.
- `head-ambiguous`: `HEAD` is detached and zero or several branches contain it. Go back to your branch with `git switch`.
- `work-item-not-found` (exit 1): the branch's work item does not exist. Record it first.
- `work-item-closed` (exit 1): the work item was already closed. Use another branch.
- `develop-mark-missing`: `before-commit` on `develop` without the mark. Create the mark with the command `/oh-my-hexflow:omhl-develop-mark`.
- `worktree-differs-from-index`: in `before-commit`, the tree differs from the index. Stage, ignore or delete the file.
- `worktree-dirty`: in `before-push` or `before-merge`, the tree is dirty. Commit, ignore or delete the file.
- `push-ref-not-checked-out` and `push-input-invalid`: `--pre-push` problems. The first appears when the `lsha` of a ref is not the checkout's `HEAD`, the second when a git line does not have four fields. Push only the checkout's branch.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Runs the `before-push` event (one check passes), then `before-merge` (no mechanical check: the `no-checks-for-event` warning) and, finally, a check by id. Without `--format`, the output is the `agent` text:

```omhl-example fixture=checks
$ omhl check run --event before-push
✔ before-push · work-items.w1 · recorded (git-commit <snapshot>)
  ✔ checks.ok passed 12ms
Nothing blocked. Next: go on with the push.
$ omhl check run --event before-merge
✔ before-merge · work-items.w1 · recorded
⚠ no-checks-for-event: no check is planned for before-merge, so nothing ran and this is not a pass. Next: add a mechanical check for this event if one is expected.
Nothing blocked. Next: go on with the merge.
$ omhl check run --check checks.ok
✔ before-push · work-items.w1 · recorded (git-commit <snapshot>)
  ✔ checks.ok passed 12ms
Nothing blocked. Next: go on with the push.
```

The same three commands with `--format json`:

```omhl-example fixture=checks
$ omhl check run --event before-push --format json
{"blocked":[],"event":"before-push","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":12,"exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl check run --event before-merge --format json
{"blocked":[],"checks_planned":0,"event":"before-merge","recorded":true,"results":[],"snapshot":null,"warnings":[{"code":"no-checks-for-event"}],"work_item":"work-items.w1"}
$ omhl check run --check checks.ok --format json
{"blocked":[],"event":"before-push","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":12,"exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
```

Refusals: a check that does not exist and a manual check (without `run`):

```omhl-example fixture=checks
$ omhl check run --check checks.nope --format json
2> {"error":{"code":"check-not-found","details":[{"code":"check-not-found","message":"check-not-found","path":"/check"}],"message":"check-not-found at /check"}}
# exit 1
$ omhl check run --check checks.review --format json
2> {"error":{"code":"check-not-mechanical","details":[{"code":"check-not-mechanical","message":"the check has no command","path":"/check"}],"message":"the check has no command"}}
# exit 1
```

A blocking check that fails: empty stdout, exit 1 and the exact command to read the cause. This fixture's check prints `first` and `second` on stdout, `boom` on stderr and exits 1. The text goes to stderr:

```omhl-example fixture=failing-check
$ omhl check run --event before-push
2> ✘ BLOCKED · before-push · work-items.w1 · 1 blocking failed, 0 passed, 0 not run
2>   ✘ checks.bad · blocking · timeout 60s · exit 1 · 12ms
2>     command: sh -c echo first; echo second; echo boom >&2; exit 1
2>     how to read the output: omhl byproduct checks.bad work-items.w1 --tail 40
2> The output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.
2> Next: read the output with the command above, fix the cause, then run the same command again.
# exit 1
```

The same error with `--format json`, with the summary in `error.context` and the check's facts in the detail's `context`:

```omhl-example fixture=failing-check
$ omhl check run --event before-push --format json
2> {"error":{"code":"checks-failed","context":{"event":"before-push","summary":{"blocking_failed":1,"failed":1,"not_run":0,"passed":0,"planned":1},"work_item":"work-items.w1"},"details":[{"code":"failed","context":{"blocking":true,"duration_ms":12,"exit_code":1,"run":["sh","-c","echo first; echo second; echo boom >&2; exit 1"],"timed_out":false,"timeout_secs":60,"truncated":false},"message":"omhl byproduct checks.bad work-items.w1 --tail 40","path":"checks.bad"}],"message":"a blocking check failed"}}
# exit 1
```

A **non-blocking** check that fails does **not** fail the command: it exits 0, and only the `ok: false` of the `results` item shows the failure. Read `results`, not just the exit code:

```omhl-example fixture=soft-check
$ omhl check run --event before-push --format json
{"blocked":[],"event":"before-push","recorded":true,"results":[{"blocking":false,"check":"checks.soft","duration_ms":12,"exit_code":1,"ok":false,"outcome":"failed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
```

Refusal: a dirty tree in `before-push` (the fixture leaves the file `notes.txt` uncommitted). First the text, then the JSON:

```omhl-example fixture=dirty-checks
$ omhl check run --event before-push
2> ✘ worktree-dirty · the working tree has uncommitted changes
2>   - worktree-dirty
2>     the working tree has uncommitted changes
2> Next: follow the hint above if there is one, fix the cause, then run the same command again.
# exit 1
```

```omhl-example fixture=dirty-checks
$ omhl check run --event before-push --format json
2> {"error":{"code":"worktree-dirty","details":[{"code":"worktree-dirty","context":{"branch":"feat/wi-w1__x","event":"before-push","kind":"worktree-dirty","work_item":"work-items.w1"},"message":"the working tree has uncommitted changes","path":""}],"message":"the working tree has uncommitted changes"}}
# exit 1
```

Refusal: the `run` binary does not exist, so there is no verdict and nothing is recorded (`check-not-run`). First the text, then the JSON:

```omhl-example fixture=missing-binary
$ omhl check run --event before-push
2> ✘ check-not-run · a check could not run to a verdict
2>   - spawn-failed at checks.ghost
2>     the command could not run
2>     kind: spawn-not-found
2>     check: checks.ghost
2>     cause: omhl-ghost-binary did not start: No such file or directory (os error 2)
2>     hint: `omhl-ghost-binary` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again
2> Next: follow the hint above if there is one, fix the cause, then run the same command again.
# exit 1
```

```omhl-example fixture=missing-binary
$ omhl check run --event before-push --format json
2> {"error":{"code":"check-not-run","details":[{"code":"spawn-failed","context":{"argv":["omhl-ghost-binary"],"branch":"feat/wi-w1__x","check":"checks.ghost","cwd":"<repo>","event":"before-push","hint":"`omhl-ghost-binary` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again","kind":"spawn-not-found","os_error":"omhl-ghost-binary did not start: No such file or directory (os error 2)","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"message":"the command could not run","path":"checks.ghost"}],"message":"a check could not run to a verdict"}}
# exit 1
```

A `--pre-push` that only sends a tag has nothing to attest: it exits 0, no check runs and nothing is recorded. The stdin is what git hands the hook, one line per ref (local ref, local sha, remote ref, remote sha):

```omhl-example fixture=checks
$ omhl check run --event before-push --pre-push <<'EOF'
refs/tags/v1 1111111111111111111111111111111111111111 refs/tags/v1 0000000000000000000000000000000000000000
EOF
✔ before-push · nothing to attest in this push; no check ran
```

The native `checks.run-tests` can carry a command declared by the project in two `workflow.toml` files, the one in force and the one evaluated. `check run` reads the evaluated one from the index, runs both when they differ (one attestation per digest) and records the one of the evaluated declaration under the id `attestations.evaluated.<head>.<tail>.<check>.<work_item>`, written exactly like that; the gate questions of that declaration carry the `@head` label. The table, the warnings and the rules of the union are in [native-checks.md](../../directives-flow/references/native-checks.md); the refusal of a hand attestation is in [attest.md](attest.md); the output of the command is read with [byproduct.md](byproduct.md).

A workflow.toml that is not valid UTF-8 adds no requirement and warns `evaluated-workflow-unreadable` (`from`, `reason`: `not-utf8`); `from` is `index`.
