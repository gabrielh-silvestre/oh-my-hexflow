# omhl project

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | plan-consensus 6, plan-consensus 7, plan-consensus 8, review 4, review 7 | `omhl project` |

## When to use

To regenerate the project projection: one Markdown file per work item in `.omhl/projection/`, with everything that is current for it (decisions, premises, attestations, relations, and the gaps and divergences sections). Use it when you need to read the state of a work item as a file, or after recording and wanting the projection up to date. It records nothing in the database; it only writes files, and only inside `.omhl/projection/`.

## Inputs

- No arguments.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database, outside the project folder; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ project · <dir> · N written · N unchanged · N removed`, and a `- written · <files>` line and a `- removed · <files>` line only when the list is not empty. The `unchanged` names are only counted in the text. With `--format json` it is a JSON line: `dir` (the projection folder, `.omhl/projection`) and three lists of file names: `written` (created or changed), `unchanged` (already equal) and `removed` (of work items that no longer exist). The same database state generates the same bytes, so running twice in a row returns everything in `unchanged`.

The files are read-only and start with the header `omhl: generated file, do not edit`. The folder carries its own `.gitignore` and is not versioned, so the projection does not dirty the git tree. Do not edit these files.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `projection-failed` (exit 1): the projection could not be written. The detail carries the path and the code of the cause: `not-generated` (the folder already has a file without the generated header, which the command never overwrites: move it) or `too-many-work-items` (there are too many work items for the projection).
- `no-project-root` (exit 1): there is no project root to write the folder. Run inside the project.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Generates the projection and repeats: the second time nothing changes:

```omhl-example fixture=project
$ omhl project
✔ project · .omhl/projection · 1 written · 0 unchanged · 0 removed
- written · work-items.w1.md
$ omhl project
✔ project · .omhl/projection · 0 written · 1 unchanged · 0 removed
```

A new work item gets its own file:

```omhl-example fixture=project
$ omhl project
✔ project · .omhl/projection · 1 written · 0 unchanged · 0 removed
- written · work-items.w1.md
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w2","type":"work-item","data":{"title":"Second item","status":"open"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl project
✔ project · .omhl/projection · 1 written · 1 unchanged · 0 removed
- written · work-items.w2.md
$ omhl project --format json
{"dir":".omhl/projection","removed":[],"unchanged":["work-items.w1.md","work-items.w2.md"],"written":[]}
```
