# omhl byproduct prune

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review rules | `omhl byproduct prune` |

The roles' matrix does not have it: no role agent runs it, and the `agent-role` hook denies it with `command-outside-matrix`. Only the main session and the agents without a ported role use it.

## When to use

To delete right away the stored outputs that outlived the retention, without waiting for the next `omhl check run`. `check run` already does the same pruning on its own, silently, at the start of each run (any event), at most **once a day per (database, project)**: a `.pruned-at` stamp in the (database, project) folder holds back the others. This command ignores the stamp and renews it.

The rule is the same in both: it deletes the output file (and the `.tmp-*` temporary left from an interrupted write) older than **7 days** by modification date, **except** that of a live attestation. Live is the attestation with the highest `seq` of each (check, work item) pair of the project: the output of the current verdict never disappears, whatever its age, so `omhl byproduct <check> <work-item>` does not return `byproduct-missing` because of the retention. The output of an earlier tree of the same pair disappears after 7 days. The pull request gate reads only the outcome and does not change; it is the output of the verdict it accepted that may already be gone.

What the pruning sees is only the folder `blobs/by-db/<database file>/<project>` next to the database: another project or another database in the same folder is never touched. The files of the old layout (`blobs/<xx>/<hash>`, from before the folder per database and project) remain readable, but are **never pruned**: cleaning them is manual, once, after the delivery.

## Inputs

- No positional arguments and no options of its own: the retention is fixed, 7 days.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0, also when there is nothing to delete: `✔ byproduct prune · N removed · N kept live · N bytes freed`. With `--format json` it is a JSON line: `removed` (files deleted), `kept_live` (files older than 7 days kept because they belong to a live attestation) and `freed_bytes` (the size of those deleted). The numbers depend on what is on disk at that moment.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `byproduct-prune-failed` (exit 1): the database or the outputs folder could not be read, or an expired file could not be deleted (no write permission on the folder, for example). Check the permissions of `blobs/by-db/` next to the database and run the command again. `omhl check run` does not show this error: its pruning has no error channel, and this command is what reveals it.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

With nothing to delete (the numbers change with what is on disk):

```omhl-example fixture=checks
$ omhl byproduct prune
✔ byproduct prune · 0 removed · 0 kept live · 0 bytes freed
```

The same pruning for machines:

```omhl-example fixture=checks
$ omhl byproduct prune --format json
{"freed_bytes":0,"kept_live":0,"removed":0}
```

`omhl byproduct` with no arguments is still a usage error (exit 2), and `omhl byproduct <check> <work-item>` still reads the output ([byproduct.md](byproduct.md)).
