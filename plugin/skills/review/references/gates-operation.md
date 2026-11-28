# What to do when the check or the hook refuses

When to consult: `omhl check run` or a git hook (`pre-commit`, `pre-push`) exited with an error and you need to know what to try. The errors of each command are on its page; here are the refusals that prevent the verdict.

## Failure without a verdict

An error without a verdict is not a `failed`: `omhl` cannot say whether the check passed and records no attestation. Each item of `details` names the check (`path`), the reason (`code`) and, in most cases, a `context` with `kind` and `hint`. Nothing from the `context` goes to the database.

| `code` of the detail | What to do |
|---|---|
| `spawn-failed` | The `run` binary does not exist in git's `PATH`. Install it or put its folder in the hooks' `PATH`, and repeat the same command |
| `outcome-immutable` | The same snapshot already has another outcome. Change the tree (a new commit) for a new verdict |
| `command-changed-tree`, `command-changed-worktree` | The check changed the tree. Keep it read-only; put what it generates in `.gitignore` or restore the tracked files with `git checkout -- <path>` |
| `spec-stale` | The check's TOML diverges from the database. Fix the file, run `omhl load` and repeat |
| `spec-invalid` | The check's `data` in the database does not decode. Fix the TOML and run `omhl load` |
| `snapshot-tree-mismatch`, `snapshot-dirty` | The index changed during the hook, or the tree is dirty. Do not stage during the hook and run again |
| `work-item-closed` | The work item was closed during the run. Check with `omhl get <work-item>` |
| `probe-failed` | `omhl` could not read the tree or `HEAD`. Run `git status` and fix the repository state |
| `attestation-not-recorded` | The database was locked after the command ran. The verdict was computed and not recorded: repeat the same command |

When there are only failed blocking checks, the error is `checks-failed`: read the cause with `omhl byproduct` ([byproduct.md](byproduct.md)). A problem without a verdict takes precedence over `checks-failed`.

## Branch and work item

`omhl check run` finds the work item by the branch name `type/wi-<id>__<slug>`. On `main` it answers `branch-protected` and runs nothing; outside the pattern, `branch-name-invalid`. On a branch declared in `[[branch.exception]]` it runs without recording anything (`skip` runs nothing), and `before-commit` on a `dry-run-with-mark` exception asks for the commit mark.

## Push

`pre-push` only attests the checkout's branch. A `git push` of another ref, without switching checkout, is blocked with `push-ref-not-checked-out`. A push with `main` as the destination is blocked with `branch-protected`.

## Bypass is the owner's

Never use `--no-verify` or the `--force` of `omhl hooks install` on your own: getting around a hook is the owner's decision, not yours.

## Check deadline

Each check has a `timeout_secs` in the TOML. A check that overruns the deadline records `failed`, immutable for that tree. Readjusting the deadline is the owner's decision: the value is measured with a clean `target/` (slowest run times 1.3, floor of 10 seconds) and recorded in the TOML, followed by `omhl load`.
