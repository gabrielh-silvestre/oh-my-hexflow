# omhl merge

- [Who uses it](#who-uses-it-at-which-stage-and-with-which-filter)
- [When to use](#when-to-use)
- [Inputs](#inputs)
- [Steps](#steps)
- [Output](#output)
- [Errors](#errors)
- [Examples](#examples)

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

Only the main session runs it, at the owner's request, after `omhl gate-delivery` passes: no role view lists it, so the `agent-role` hook denies it to a subagent of the plugin.

## When to use

With `[provider] name = "github"` in the `.omhl/workflow.toml` of the main directory, `omhl merge <work-item>` is the only way the agent merges a pull request: it finds the pull request by the branch, checks that the local `HEAD` is the commit of the pull request and that its base is the integration branch, runs the `before-merge` checks, reads the CI of the commit and attests it as `checks.ci-green-before-merge`, evaluates the delivery gate, calls `gh pr merge` pinned to that commit, reads the pull request again and, only when it is `MERGED` into the integration branch, closes the work item and records the `pull-request` node in the same write. With the integration on, the `command-guard` and `mcp-merge-guard` hooks deny `gh pr merge` and the MCP merge tool (`merge-through-omhl`), and the agent never attests `checks.ci-green-before-merge` by hand. The owner can still merge on GitHub.

Without `[provider]`, the command refuses with `integration-off`: merge the pull request on the provider and close the work item with `omhl close` ([close.md](close.md)).

The command can be repeated from any point, and each run starts again from the preconditions: a pull request already `MERGED` at the evaluated commit skips the merge call and closes; a pull request with auto-merge on, or in the merge queue, is reported as `merge-pending`, and running the command again after the merge closes the work item.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist and be open, and the current branch must be its branch.
- `--squash` or `--rebase`: the merge strategy; without either, the merge is a merge commit (`--merge`). The two together are a clap usage error, with exit 2.
- `--force --note <text>`: passes a red CI check that branch protection does not require, and warn questions of the gate, with the owner's literal phrase in `--note`. `--force` without `--note` and `--note` without `--force` are clap usage errors, with exit 2. A required red check, a running CI and a failed `refuse` question never pass with `--force`.
- `--base` `<ref>`: the base of the loosening review, with the same meaning as in [close.md](close.md) and [gate-delivery.md](gate-delivery.md).
- `--db <path>` and `--project-dir <dir>`: as in the other commands.
- `--format <agent|human|json>`: global option. The default is `agent`; `json` is the output for machines.
- It needs `gh` on the `PATH`, logged in to the provider host, and an `origin` remote on that host. Every `gh` call names the repository explicitly and runs with a time limit.

## Steps

The order is fixed, and each step refuses the command without running the next one:

1. `lookup`: the preconditions (readable `workflow.toml`, `[provider]`, open work item, branch of the work item, clean tree, `origin`, `gh` installed, logged in and recent enough) and the search of the pull request by the branch (`gh pr list --head <branch> --state all`). Fork pull requests are not supported.
2. `head`: the local `HEAD` must be the head commit of the pull request.
3. `base`: the base of the pull request must be the declared `branch.integration` or, without it, the default branch of the provider.
4. `checks`: the `before-merge` checks run in process, as in `omhl check run --event before-merge`.
5. `ci`: the CI of the commit is read and attested as `checks.ci-green-before-merge` at `HEAD` (`passed`, or `passed_with_warning` with no CI seen and `provider.require_ci` off, or with a red check passed by `--force`).
6. `gate`: the delivery gate, as in `omhl close`.
7. `merge`: `gh pr merge <n> --match-head-commit <sha>` with the strategy flag, then the pull request is read again.
8. `close`: the work item closes and the `pull-request` node (`work-items.<work-item>.github-<number>`) is recorded in the same write.

## Output

Text on stdout, exit 0, only with the work item closed: the header `merge · <work-item> · <pull request id> · <strategy>`, the pull request, one line per step with its result, the items passed by `--force` under the step that passed them (`forced` under `ci`, `overridden` under `gate`), the warnings, and a `Next:` line.

With `--format json` it is a JSON line with `work_item`, `pull_request` (`id`, `number`, `repo`, `strategy`, `merge_commit`), `steps` (each with `step` and `result`; `base` also has `expected` and `actual`, `ci` has `forced` and `gate` has `overridden`), `warnings` and `next`.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`, the hint and `Next:`; with `--format json`, as a JSON line whose `context` has `steps`, up to the step that refused, and `next`). A refusal records nothing, except the `checks.ci-green-before-merge` attestation already recorded by the `ci` step. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md), and the refusals of the gate are the ones of [close.md](close.md) (`close-refused`, `close-warnings`, `review-base-invalid`).

- `integration-off` (exit 1): the main-directory `workflow.toml` declares no `[provider]`. Merge on the provider and close with `omhl close`; `gh` is never started.
- `head-detached` (exit 1): `HEAD` is detached. Check out the branch of the work item.
- `branch-not-work-item` (exit 1): the current branch is not a branch of this work item (`main` included). Check out the branch of the work item.
- `origin-missing` (exit 1): the repository has no `origin` remote. Add it.
- `origin-host-mismatch` (exit 1): `origin` is not a repository on the declared host (`github.com` or `provider.host`). Fix `origin` or `provider.host`.
- `provider-cli-missing` (exit 1): `gh` is not on the `PATH`. Install the GitHub CLI.
- `provider-not-authenticated` (exit 1): `gh auth status` failed. Run `gh auth login --hostname <host>`.
- `provider-cli-too-old` (exit 1): `gh pr merge --help` does not list `--match-head-commit`. Update `gh`.
- `pull-request-not-found` (exit 1): GitHub has no open pull request from the branch, and no merged one at `HEAD`. Open the pull request.
- `pull-request-ambiguous` (exit 1): GitHub lists more than one candidate pull request; the error names the numbers. Close the extra ones.
- `fork-unsupported` (exit 1): the branch has only fork pull requests, or `origin` is a fork; the error names them or the parent. The merge stays on GitHub.
- `head-not-pushed` (exit 1): the local `HEAD` is not the head commit of the pull request (unpushed or rewritten commit), or it moved while the CI was read. Push and run again.
- `pull-request-base-mismatch` (exit 1): the base of the pull request is not the integration branch; `context` has `pr_base` and `expected_base`. With the pull request `OPEN`, change its base and run again; with it already `MERGED`, stop and ask the owner: the work item stays open.
- `ci-running` (exit 1): the CI of the commit is still running. Wait and run again.
- `ci-not-green` (exit 1): a CI check is red and branch protection requires it, or it could not be classified, or `--force --note` was not given for one it does not require.
- `ci-required-missing` (exit 1): `provider.require_ci` is on and no CI was seen at the commit. Wait for the CI to register.
- `merge-refused` (exit 1): GitHub did not merge (the pull request is closed, or the call failed with it still open); the message carries the `gh` stderr.
- `merge-pending` (exit 1): the merge is pending (merge queue or auto-merge); `context.state` is `open` and the work item stays open. Run the command again after the merge.
- `pull-request-record-exists` (exit 1): a node already has the computed id of the pull request record. Nothing is overwritten; ask the owner.
- `merge-through-omhl` (hook denial, not an exit code): with the integration on, the `command-guard` hook denies `gh pr merge` in Bash and the `mcp-merge-guard` hook denies the MCP merge tool. Run `omhl merge <work-item> [--squash|--rebase]`; the owner can still merge on GitHub.
- `invalid-workflow-config` (exit 1): the `.omhl/workflow.toml` of the main directory is not accepted. Fix it; `omhl load --dry-run` details it.
- `work-item-not-found` (exit 1): the work item does not exist. Check the id.
- `invalid-transition` (exit 1): the work item is already closed.
- `snapshot-dirty` (exit 1): the tree has a modified or untracked file. Commit and repeat.

## Examples

Without `[provider]` in the main-directory `workflow.toml`, the command refuses before it starts `gh` and records nothing:

```omhl-example fixture=git
$ omhl merge work-items.w1
2> ✘ integration-off · the main-directory workflow.toml declares no [provider], so omhl merge has no provider to merge through
2>   - /work_item
2>   steps[0].step: lookup
2> Next: merge the pull request on the provider, then close the work item with omhl close.
# exit 1
```

The happy path, with the CI green and the gate passed: `omhl merge work-items.w1` prints

```text
✔ merge · work-items.w1 · work-items.w1.github-42 · merge
  pull_request: #42 · acme/widgets
  - lookup · done
  - head · done
  - base · done · expected develop · actual develop
  - checks · done
  - ci · passed
  - gate · passed
  - merge · done
  - close · done
Next: the work item is closed; read the retro with omhl report retro work-items.w1.
```

A pull request that targets another base refuses before any check, CI read, gate or merge; on stderr, with exit 1:

```text
✘ pull-request-base-mismatch · the pull request targets main, but the integration branch is develop
Next: change the base of the pull request to develop on GitHub and run omhl merge again.
```
