# omhl hooks post-merge

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

Whoever calls it is the clone's git hook, not a stage of the cycle.

## When to use

Whoever calls it is the `post-merge` hook that `omhl hooks install` writes, with the flag git passes; you rarely run it by hand. On a branch that `.omhl/workflow.toml` declares as a `dry-run-with-mark` exception, after a merge without conflict, a fast-forward merge or a `git pull`, it consumes the commit mark (the `omhl-commit-mark` file in the git folder), as `omhl hooks post-commit` does after a commit, so the next direct commit requires a new mark. With the squash flag `1` (`git merge --squash`) it consumes nothing, because the commit that closes the squash still needs the mark. On another branch it does nothing.

## Inputs

- `<squash_flag>`: the flag git passes to the `post-merge` hook: `1` for a squash merge, `0` otherwise. Any value other than `1` consumes the mark.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format json`: global option, the only format of this command, which only prints JSON. `agent` and `human` exit with a usage error (exit 2), before the command runs.

## Output

A JSON line on stdout, exit 0: `consumed`, which is `true` when the mark existed on a `dry-run-with-mark` exception branch and was deleted, and `false` when there was no mark, the branch is not such an exception, or the flag is `1`. Calling again after it was consumed returns `false`: the command is idempotent.

## Errors

An error goes to stderr, as a JSON line, with exit 1. An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `probe-failed` (exit 1): git did not answer (outside a repository). Run inside a clone.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

The squash flag keeps the mark; a plain merge consumes it, and the next call no longer finds it:

```omhl-example fixture=commit-mark
$ omhl hooks post-merge 1
{"consumed":false}
$ omhl hooks post-merge 0
{"consumed":true}
$ omhl hooks post-merge 0
{"consumed":false}
```
