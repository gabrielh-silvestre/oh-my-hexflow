# omhl hooks install

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review rules | `omhl hooks install` |

## When to use

Once per clone, so git calls the checks on its own: it writes the four scripts (`pre-commit`, `pre-merge-commit`, `pre-push`, `post-commit`) to a single user folder and points the clone's `core.hooksPath` at it. Without this the clone has no hook, and the pull request gate charges for the lack later. These are **git** hooks, unrelated to the Claude Code `omhl hook session-start` hook.

## Inputs

- `--force`: proceeds even if the clone already has hooks of its own or another `core.hooksPath`. **Only run it with the human's explicit approval**, given after you show what will be overwritten (the step is in "Errors"). The command makes no backup: the previous `core.hooksPath` is lost and the hooks in `.git/hooks` stay on disk, but git stops running them (git-lfs and `commit-msg`, for example). Chaining them is not supported.
- It needs a git repository, version 2.31 or newer.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0: `✔ hooks install · 4 scripts`, the lines `hooks path:` (the scripts folder, in `$XDG_DATA_HOME/omhl/hooks/`), `omhl:` (the path of the binary written into the scripts) and `scripts:`, and one `- warning · <code> · <path>` line per warning. With a warning the symbol becomes `⚠` and the `Next:` carries the warning's hint. With `--format json` it is a JSON line: `hooks_path`, `omhl`, `scripts` and `warnings` (each with `code`, `path` and `hint`). The folder is unique per user: installing in one clone rewrites the scripts of all the others, and the binary written is that of the last installation. If the `omhl` running the installation is in a `target/debug` or `target/release` folder (`binary-in-build-dir`), or inside a linked worktree (`binary-in-worktree`), the installation records it anyway and the warning says the path. That binary disappears with the worktree: install a binary outside any worktree (`cargo install` or a copy in `~/.local/bin`) and run the command again. Reinstalling with the same folder is idempotent.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option or a missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `hooks-existing` (exit 1): `core.hooksPath` is not set and `.git/hooks` has some file that is not `*.sample`. **Stop.** Do not use `--force` or move the hooks on your own: ask the human and show them what exists (`ls -la "$(git rev-parse --git-path hooks)"`, ignoring the `*.sample`, and the content of each file that is not obvious). Say that those hooks stop running, that there is no backup and that the user's hooks folder holds for all clones. Only run `omhl hooks install --force` after the human authorizes it in that conversation.
- `hooks-path-conflict` (exit 1): `core.hooksPath` already points to another folder, local or global. **Stop.** Run `git config --show-origin --get-all core.hooksPath` and show the human the value, the scope (local or global) and what the folder contains. Say that `--force` writes a local `core.hooksPath` and discards the previous value without a record, and that the hooks in that folder stop running in this clone. Only run `omhl hooks install --force` after the human authorizes it in that conversation.
- `hooks-path-unreachable` (exit 1): git does not resolve the hooks folder to the installed folder. Check `core.hooksPath`.
- `hooks-dir-unwritable` (exit 1): the user folder cannot be created or written. Fix the permission.
- `probe-failed` (exit 1): git did not answer (outside a repository, or git older than 2.31). Install a newer git or run inside a clone.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Installs the hooks. The warning appears because the example's binary is in `target/debug`; repeating the command is idempotent:

```omhl-example fixture=git
$ omhl hooks install
⚠ hooks install · 4 scripts · 1 warning
  hooks path: <xdg>/omhl/hooks
  omhl: <omhl>
  scripts: pre-commit, pre-merge-commit, pre-push, post-commit
- warning · binary-in-build-dir · <omhl>
Next: install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks.
$ omhl hooks install
⚠ hooks install · 4 scripts · 1 warning
  hooks path: <xdg>/omhl/hooks
  omhl: <omhl>
  scripts: pre-commit, pre-merge-commit, pre-push, post-commit
- warning · binary-in-build-dir · <omhl>
Next: install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks.
```

With `--format json` the result comes out for machines:

```omhl-example fixture=git
$ omhl hooks install --format json
{"hooks_path":"<xdg>/omhl/hooks","omhl":"<omhl>","scripts":["pre-commit","pre-merge-commit","pre-push","post-commit"],"warnings":[{"code":"binary-in-build-dir","hint":"install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks","path":"<omhl>"}]}
```
