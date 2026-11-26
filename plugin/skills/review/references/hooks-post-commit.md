# omhl hooks post-commit

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

Whoever calls it is the clone's git hook, not a stage of the cycle.

## When to use

Whoever calls it is the `post-commit` hook that `omhl hooks install` writes; you rarely run it by hand. On the `develop` branch, after a commit, it consumes the `develop` mark (the `omhl-develop-mark` file in the git folder), so the next direct commit requires a new mark. On another branch it does nothing.

## Inputs

- No positional arguments. It uses the branch and the git folder of the current directory.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format json`: global option, the only format of this command, which only prints JSON. `agent` and `human` exit with a usage error (exit 2), before the command runs.

## Output

A JSON line on stdout, exit 0: `consumed`, which is `true` when the mark existed on `develop` and was deleted, and `false` when there was no mark or the branch is not `develop`. Calling again after it was consumed returns `false`: the command is idempotent. `omhl check run --event before-commit` and an aborted commit never spend the mark.

## Errors

An error goes to stderr, as a JSON line, with exit 1. An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `probe-failed` (exit 1): git did not answer (outside a repository). Run inside a clone.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Consumes the mark after a commit on `develop`. The second call no longer finds the mark:

```omhl-example fixture=develop-mark
$ omhl hooks post-commit
{"consumed":true}
$ omhl hooks post-commit
{"consumed":false}
```
