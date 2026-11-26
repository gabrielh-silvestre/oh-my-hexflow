# omhl check sweep

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

The global sweep is the owner's and only runs with the human's approval: `SKILL.md` of `review`, Rules section.

## When to use

When a process was left over from a check whose `omhl` already exited (Ctrl+C, `kill`, a dropped terminal). Before running, ask for the human's approval: the command holds for **any project root on the machine**, of the same user, and may stop a server (`sccache`, the Gradle daemon) that another session still uses. For the common case, run `omhl check run`: it already sweeps, silently, the orphans of its own project at the start of each run. This command does the sweep on demand. It only runs on Linux.

## Inputs

- No positional arguments.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0, also when there is no target: `✔ check sweep · N swept · N unreadable` and one `- <comm> · pid <pid> · owner <pid>` line per process stopped. When reading `/proc` times out (at most 1 second), the header becomes `⚠ check sweep · N swept · N unreadable · scan timed out` and closes with a `Next:` telling you to repeat the command, because the sweep gave up before reading everything and the empty `swept` proves nothing. With `--format json` it is a JSON line: `swept` (a list; each item has `pid`, `comm` and `owner_pid`), `unreadable` (how many processes could not be read) and `scan_timed_out`. Because it sweeps the whole machine, **`swept`, `unreadable` and `scan_timed_out` (in the text, the counts and the pids) are not deterministic**: they depend on the processes alive at that moment. Do not read an empty `swept` as proof that nothing leaked in another project, and do not count on the same number in two calls. When it finds a target, the sweep stops the process (`TERM`, 2 seconds of grace, `KILL`) and appends a line to `$XDG_DATA_HOME/omhl/sweep.jsonl`. The file has a 1 MiB ceiling: above that it becomes `sweep.jsonl.1` (a single old generation, the previous one is replaced) and a new `sweep.jsonl` receives the line. Two simultaneous rotations may lose a generation.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

A sweep with no target. On another machine, or with other live processes, `swept`, `unreadable` and `scan_timed_out` change:

```omhl-example fixture=checks
$ omhl check sweep
✔ check sweep · 0 swept · 0 unreadable
```

The same sweep for machines:

```omhl-example fixture=checks
$ omhl check sweep --format json
{"scan_timed_out":false,"swept":[],"unreadable":0}
```
