# Native checks

The `omhl` binary carries native checks. They come off: a project turns each one on in `.omhl/workflow.toml`, and until then nothing prints, asks or blocks for it. The two native guidelines are not in this page and stay on.

| Check | When | What it asks | Gate |
|---|---|---|---|
| `checks.run-tests` | `before-commit` | Run the project's test suite and confirm it passes before creating a commit | `gate-pr` and `close` ask for its attestation |
| `checks.ci-green-before-merge` | `before-merge` | Wait until every check run on the head commit is green; if one fails, fix it, push and run CI again | `gate-pr` and `close` ask for its attestation |
| `checks.config-loosening-review` | `before-pr` | Review the diff of `.omhl/workflow.toml` and `.omhl/checks/` for loosened brakes, as in [config-loosening-review.md](../../review/references/config-loosening-review.md) | `gate-pr` and `close` ask for it when the pull request touches configuration |
| `checks.verify-review-comment-anchoring` | `before-publishing-review-comments` | Compare path, line and side of each inline review comment with the diff of the head commit and post the approved text unchanged | Instruction only: no blocking, outside the gate |

When both `checks.run-tests` and `checks.ci-green-before-merge` are on, `omhl render` prints `after: checks.run-tests` on the second. With only the second on, the line is not printed.

## Turn them on

List the ids in `[native] checks`. A check is on only when its id is in the list; no file, no table, no key or an empty list leaves them all off. Repeating an id changes nothing.

```toml
version = 1

[native]
checks = ["checks.run-tests", "checks.ci-green-before-merge", "checks.verify-review-comment-anchoring"]
```

Keep only the ids the project wants. Any other id (an unknown one, a guideline, a project check) or any other key under `[native]` makes the whole file invalid. After editing, `omhl load --dry-run` reports the problem with the line.

The file that counts is the one in the working tree of the main directory, never the copy in a worktree or in the commit under review. A change made in a worktree takes effect once it is merged and the main directory has it.

The main directory is the parent of the git common directory. In a clone made with `--separate-git-dir`, in a submodule or in a bare repository with worktrees, that parent is not the working tree, so `omhl` finds no file and no native check is on.

## Who turns them on and off

- The agent may turn a check on, inside a worktree, as a change of the pull request.
- Turning a check off belongs to the owner.
- No hook compares the enabled checks, so nothing blocks an agent that removes an id. The agent never does it, and the reviewer confirms in the pull request that the diff of `workflow.toml` removes no id.

## What the gate asks

- `gate-pr` and `close` ask one `occurred` question for each of `checks.run-tests` and `checks.ci-green-before-merge` that is on, answered by [omhl attest](../../review/references/attest.md) tied to `HEAD`. A check that is off gets no question, and an `attest` for it is accepted without creating one.
- A native check is not configurable and a project check never replaces it: the project can only turn it on or off. A project check on the same event is asked next to it, and the attestation of one does not answer the other. To ask something else, create a check of the project.
- `checks.verify-review-comment-anchoring`, when on, only prints its block for the moment of publishing review comments. No command or hook blocks on it, and the gate does not ask about it.
- With no native check on, no project check and no decision, the gate passes with `no-open-contradiction`, `decisions-anchored` and `anchors-confirmed` over zero nodes. Nothing was verified.

## When the file is invalid

An invalid `.omhl/workflow.toml` never turns a check on or off.

- `omhl render` and the context of the session start still exit 0, print no native check block and carry one `invalid-workflow-config` warning line.
- `gate-pr` and `close` refuse with `invalid-workflow-config` before evaluating the gate, and `close` refuses before recording anything, with a retro or without one.
- The recommended fix is a repair pull request with `--base`: the fix goes through a pull request and the loosening review like any other change.
- The exception, for when the pull request cannot be used (without `--base`, a pull request is not recognized as a repair), is an edit made directly by the owner in the working tree of the main directory, committed on the branch checked out there.

## A project without git

A project with `.omhl.toml` and no git repository has no main directory to read.

- Every session start shows the `main-dir-unknown` line, and `omhl render` prints no native check block.
- `gate-pr` and `close` refuse there with `probe-failed`: the probe of `HEAD` runs before the main directory is resolved.
- `git init` takes the line out of the session. `gate-pr` and `close` also need a first commit with a clean tree, because the probe refuses a repository with no commits.
