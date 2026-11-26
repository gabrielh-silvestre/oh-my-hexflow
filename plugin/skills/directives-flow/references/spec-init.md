# omhl spec init

## When to use

To create the spec folder of a `feat/` or `fix/` work item, with the first part (`part-01.md`) already holding the five empty sections, with the English headings `Objective`, `Applicable directives and premises`, `Behaviors`, `Acceptance criteria` and `Out of scope`. Run it before writing the spec and before `omhl gate-worktree`: the gate of these prefixes requires a valid spec. The folder is born in the main directory of the repository, even if you are in a worktree. An old spec with `parte-NN.md` files and Portuguese headings stays valid, and `omhl spec validate` accepts both name families (`part-NN.md` or `parte-NN.md`, `amendment-NN.md` or `emenda-NN.md`); new files use the English names and headings.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist and be open.
- `<SLUG>`: the folder name after the id, 1 to 64 characters of `a-z`, `0-9` and `-`, not starting with `-`. The folder will be `wi-<id>__<slug>`.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- It needs a git repository (the command finds the main directory through it).

## Output

Text on stdout, exit 0: `✔ spec init · <work item> · <slug>`, the `dir:` (the created folder) and `path:` (the `part-01.md`) lines and the `Next:` with the following step (fill in the five sections and run `omhl spec validate`). With `--format json` it is a JSON line: `dir`, `path`, `slug` and `work_item`. The command also writes a `.gitignore` with `*` in the specs folder: the spec does not go to git.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `spec-slug-invalid` (exit 1): the slug does not follow the rule. The pointer is `/slug`. Fix the name.
- `spec-exists` (exit 1): the work item already has a spec folder, of any slug. Edit the existing one; if the spec was already approved, the correction is an amendment.
- `work-item-not-found` (exit 1): the work item does not exist. Record it with `omhl append` first.
- `work-item-closed` (exit 1): the work item is already closed. Use an open work item.
- `spec-store-unusable` (exit 1): the specs folder cannot be read or written, exceeded 10 000 entries, or the work item has more than one `wi-<id>__*` folder. Fix the folder in the main directory.
- `probe-failed` (exit 1): git did not respond (outside a repository). Run inside a checkout.
- `invalid-workflow-config` (exit 1): `.omhl/workflow.toml` is invalid. The message carries the file and the position; fix it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Creates the spec of work item `w1` (`<repo>` is the repository root):

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports
✔ spec init · work-items.w1 · export-reports
  dir: <repo>/.omhl/specs/wi-w1__export-reports
  path: <repo>/.omhl/specs/wi-w1__export-reports/part-01.md
Next: fill in the five sections of the part file, then run omhl spec validate work-items.w1.
```

With `--format json` the result comes out for machines:

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports --format json
{"dir":"<repo>/.omhl/specs/wi-w1__export-reports","path":"<repo>/.omhl/specs/wi-w1__export-reports/part-01.md","slug":"export-reports","work_item":"work-items.w1"}
```

Refusal: the work item already has a spec, even with another slug:

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports
✔ spec init · work-items.w1 · export-reports
  dir: <repo>/.omhl/specs/wi-w1__export-reports
  path: <repo>/.omhl/specs/wi-w1__export-reports/part-01.md
Next: fill in the five sections of the part file, then run omhl spec validate work-items.w1.
$ omhl spec init work-items.w1 other
2> ✘ spec-exists · a spec folder of this work item already exists
2>   - /work_item
# exit 1
```

Refusal: slug outside the rule:

```omhl-example fixture=git
$ omhl spec init work-items.w1 Bad_Slug
2> ✘ spec-slug-invalid · the slug must have 1 to 64 characters of a-z, 0-9 and -, and not start with -
2>   - /slug
# exit 1
```

Refusal: a work item that does not exist:

```omhl-example fixture=git
$ omhl spec init work-items.zz x
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```
