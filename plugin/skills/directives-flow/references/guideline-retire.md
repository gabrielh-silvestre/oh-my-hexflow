# omhl guideline retire

## When to use

To retire a guideline (`guideline`) whose file `.omhl/guidelines/<name>.toml` was deleted from the working tree. It leaves the session context and the next `omhl load` does not recreate it. The retirement is definitive for all clones and branches, because the database belongs to the user and is append-only: retire only after the file removal is merged, and ask the human first. On a branch older than the guideline, rebase instead of retiring.

## Inputs

- `<ID>`: the guideline id, like `guidelines.brief`. It must be a current guideline of the repository, not native, with the file already deleted.
- `--by <id>`: the guideline that replaces the retired one. Optional; with it the command records in the same request the `supersedes` relation (from the new to the old) and the retirement. The successor must be current and different from the retired one.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ guideline retire · retired <id>`, with `· superseded by <id>` when there is `--by`. With `--format json` it is a JSON line: `retired` (the retired id) and `superseded_by` (the id of `--by`, or `null`). Decisions that rested only on that guideline start failing `decisions-anchored` until they gain another anchor.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). Each refusal carries the cause in `message` and the next step in `next_step` (in the text, the `Next:` line), without repeating the input value. A missing `<ID>` is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `guideline-not-found` (exit 1): the id is not a current guideline of the repository (it does not exist, was already retired, is native or is of another type). Check the id.
- `guideline-not-orphaned` (exit 1): the guideline file still exists in the tree. Delete the file, merge the removal and repeat.
- `successor-not-current` (exit 1): the id of `--by` is not a current guideline. Load the successor with `omhl load` first, or retire without `--by`.
- `successor-is-self` (exit 1): `--by` is the retired guideline itself. Pass another, or retire without `--by`.
- `supersedes-cycle` (exit 1): the successor is already superseded by the retired one, directly or through others. Choose another successor.
- `guideline-retire-by-command` (exit 1): the retirement came through `omhl append`. Only this command records it.
- `INVALID_INPUT` (exit 1): an id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the `outside-claude-code` marker; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Retires the orphan guideline. Repeating the retirement fails, because it is no longer current:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief
✔ guideline retire · retired guidelines.brief
$ omhl guideline retire guidelines.brief
2> ✘ guideline-not-found · the id is not a current guideline stored in the repository
2>   - /id
2> Next: pass the id of a current guideline that is not native.
# exit 1
```

With `--format json` the successor appears as `null` when there is no `--by`:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --format json
{"retired":"guidelines.brief","superseded_by":null}
```

Refusal: the successor is the guideline itself:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --by guidelines.brief
2> ✘ successor-is-self · a guideline cannot be its own successor
2>   - /by
2> Next: pass a different guideline in --by, or retire without --by.
# exit 1
```

Refusal: the successor does not exist:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --by guidelines.new
2> ✘ successor-not-current · the successor is not a current guideline
2>   - /by
2> Next: pass the id of a current guideline in --by, or retire without --by.
# exit 1
```

Refusal: the guideline does not exist:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.nope
2> ✘ guideline-not-found · the id is not a current guideline stored in the repository
2>   - /id
2> Next: pass the id of a current guideline that is not native.
# exit 1
```
