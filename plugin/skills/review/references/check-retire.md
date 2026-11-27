# omhl check retire

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

## When to use

Retiring a check is the owner's task: the agent never runs it. To discard for good a check whose file was deleted from the tree and whose deletion was already merged. Without this, `omhl load` only lists the id in `orphans`, `omhl check run` warns `check-orphaned` on every run and the pull request gate keeps asking for an attestation nobody records anymore. **The retirement is final** and holds for all clones and branches: the id never comes back. If your branch predates the check (the file did not exist in it yet), do not retire: rebase onto `main` or restore the file.

## Inputs

- `<CHECK>`: the check id, like `checks.ok`. It must be a live check of the project, with the file `.omhl/checks/<slug>.toml` absent from the tree.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--owner-override`: belongs to the owner, outside Claude Code; an agent never passes it ([config-loosening-review.md](../../review/references/config-loosening-review.md)).
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ check retire · retired <id>`. With `--format json` it is a JSON line, `{"retired":"<id>"}`. The database is append-only: the command deletes nothing, it records a retirement record (author `loader`) and the chain stays valid. Afterwards, the check disappears from `check run`, from the warning and from the pull request gate. An `omhl load` with the file back reports the id in `skipped` and does not recreate it; to bring the check back, use another file name.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `check-not-orphaned` (exit 1): the check file still exists in the tree. Delete it (and merge the removal) first, or give up the retirement.
- `check-not-found` (exit 1): the id is not a live check of the project (this includes one already retired). Check the id.
- `INVALID_INPUT` (exit 1): the id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the marker `outside-claude-code`; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Retiring a check loosens the gate, so an agent run refuses with `check-loosening-refused`. The owner retires it with `--owner-override`. The next call refuses, because the check is no longer live, and an id that never existed refuses the same way:

```omhl-example fixture=retire-check
$ omhl check retire checks.ok
2> ✘ check-loosening-refused · the revised check turns blocking off, removes its run or moves a blocking check out of the required events, which loosens a check already stored
2>   - /check
2> Next: keep the check blocking, with its run and on a required event, or retire the check and declare a new one.
# exit 1
$ omhl check retire checks.ok --owner-override
✔ check retire · retired checks.ok
$ omhl check retire checks.ok
2> ✘ check-not-found · check-not-found at /check
2>   - /check · check-not-found
# exit 1
$ omhl check retire checks.nope
2> ✘ check-not-found · check-not-found at /check
2>   - /check · check-not-found
# exit 1
```

With `--format json`, the retirement comes out for machines:

```omhl-example fixture=retire-check
$ omhl check retire checks.ok --owner-override --format json
{"retired":"checks.ok"}
```

Refusal: the check file still exists in the tree:

```omhl-example fixture=checks
$ omhl check retire checks.ok
2> ✘ check-not-orphaned · the check file still exists in the working tree
2>   - /check
# exit 1
```

Refusal: an id with an invalid form:

```omhl-example fixture=checks
$ omhl check retire BAD
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /check · semantic-id-format
# exit 1
```
