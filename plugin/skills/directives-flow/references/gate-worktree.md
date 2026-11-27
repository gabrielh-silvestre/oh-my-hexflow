# omhl gate-worktree

## When to use

Before `wt switch -c <branch>`, to check whether the worktree of the branch can be born. `wt` runs the same command in a `pre-switch` hook; running it first shows the refusal and what is missing without creating anything. Only after it exits 0 run `wt switch -c`. Never get around a refusal with `--no-hooks`: the bypass is the owner's decision.

## Inputs

- `--branch <name>`: the branch name, in the format `type/wi-<id>__<slug>` (`feat`, `fix`, `refactor`, `tests`, `bugfix`, `chore` or `docs`). Required; accepts a value with a leading hyphen and the form `--branch=<value>`.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. `--project-dir` must exist.
- `--format json`: global option, the only format of this command, which prints only JSON. `agent` and `human` exit with a usage error (exit 2), before the command runs.
- What each prefix requires is the default of the matrix plus what `.omhl/workflow.toml` adds with `require` (the matrix is in [matrix.md](matrix.md)): `feat` and `fix` ask for a confirmed business premise and a valid spec; `refactor` and `tests` ask for a confirmed technical premise; `bugfix` asks for an agent premise; `chore` and `docs` ask for nothing.

## Output

Without the `.omhl.toml` marker above the directory, the CLI exits 0 and prints nothing (the hook runs in any repository). With the marker, a JSON line on stdout, exit 0: `allowed: true`, `branch` and `reason`, which is `exempt-branch` (`develop` and `main`), `worktree-exists` (the local branch already has a worktree) or `matrix-satisfied` (the prefix was satisfied).

## Errors

An error goes to stderr, as a JSON line, with exit 1. A missing `--branch` is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `worktree-gate-refused` (exit 1): the matrix of the prefix was not satisfied. The `context` carries `missing` (the atoms that are missing: `business-premise`, `spec`, `technical-premise-confirmed`, `agent-premise`), `prefix`, `work_item` and `main_dir`. Record and confirm what is missing, or write and validate the spec, and repeat the gate. The spec violation does not come out here: run `omhl spec validate`.
- `work-item-not-found` (exit 1): the prefix opens the database and the work item of the branch does not exist. The pointer is `/branch`. Record the work item with `omhl append`, or fix the branch id.
- `work-item-closed` (exit 1): the work item of the branch is already closed. Use an open work item.
- `prefix-not-allowed` (exit 1): the name does not follow `type/wi-<id>__<slug>`, or the `type` is not in the prefix table. `pr:N` and `dependabot/...` fall here. Fix the name.
- `main-dir-unknown` (exit 1): git did not find the main directory (a bare repository, for example). Run in a regular checkout.
- `spec-store-unusable` (exit 1): the prefix asks for a spec and the spec folder of the work item cannot be read, or there is more than one `wi-<id>__*` folder. Fix the folder in the main directory.
- `invalid-workflow-config` (exit 1): `.omhl/workflow.toml` is invalid. Fix the file.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

`develop` and `main` pass without checking anything else:

```omhl-example fixture=git
$ omhl gate-worktree --branch develop
{"allowed":true,"branch":"develop","reason":"exempt-branch"}
```

A branch that already has a worktree (the fixture is on `feat/wi-w1__x`):

```omhl-example fixture=git
$ omhl gate-worktree --branch feat/wi-w1__x
{"allowed":true,"branch":"feat/wi-w1__x","reason":"worktree-exists"}
```

A prefix with no requirement (`chore/`) does not open the database and passes:

```omhl-example fixture=git
$ omhl gate-worktree --branch chore/wi-w1__tidy
{"allowed":true,"branch":"chore/wi-w1__tidy","reason":"matrix-satisfied"}
```

With the spec created and valid, the new `feat/` branch passes:

```omhl-example fixture=spec
$ omhl gate-worktree --branch feat/wi-w1__y
{"allowed":true,"branch":"feat/wi-w1__y","reason":"matrix-satisfied"}
```

Refusal: `feat/` without a spec (the business premise of the fixture is confirmed, only the spec is missing):

```omhl-example fixture=git
$ omhl gate-worktree --branch feat/wi-w1__y
2> {"error":{"code":"worktree-gate-refused","details":[{"code":"worktree-gate-refused","context":{"branch":"feat/wi-w1__y","kind":"worktree-gate-refused","main_dir":"<repo>","missing":["spec"],"prefix":"feat","work_item":"work-items.w1"},"message":"the premises the branch prefix requires are missing","path":"/branch"}],"message":"the premises the branch prefix requires are missing"}}
# exit 1
```

Refusal: `refactor/` without a confirmed technical premise:

```omhl-example fixture=git
$ omhl gate-worktree --branch refactor/wi-w1__z
2> {"error":{"code":"worktree-gate-refused","details":[{"code":"worktree-gate-refused","context":{"branch":"refactor/wi-w1__z","kind":"worktree-gate-refused","main_dir":"<repo>","missing":["technical-premise-confirmed"],"prefix":"refactor","work_item":"work-items.w1"},"message":"the premises the branch prefix requires are missing","path":"/branch"}],"message":"the premises the branch prefix requires are missing"}}
# exit 1
```

Refusal: the work item of the branch does not exist:

```omhl-example fixture=git
$ omhl gate-worktree --branch feat/wi-zz__y
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/branch"}],"message":"work-item-not-found at /branch"}}
# exit 1
```

Refusal: name outside the format:

```omhl-example fixture=git
$ omhl gate-worktree --branch pr:12
2> {"error":{"code":"prefix-not-allowed","details":[{"code":"prefix-not-allowed","message":"the branch name must be <type>/wi-<id>__<slug> with a type listed in the workflow prefixes","path":"/branch"}],"message":"the branch name must be <type>/wi-<id>__<slug> with a type listed in the workflow prefixes"}}
# exit 1
```

Refusal: a `--project-dir` that does not exist exits 1, unlike a missing marker, which exits 0 silently:

```omhl-example fixture=git
$ omhl gate-worktree --branch develop --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
