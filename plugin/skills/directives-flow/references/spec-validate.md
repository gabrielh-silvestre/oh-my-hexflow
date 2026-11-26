# omhl spec validate

## When to use

To check the spec of a work item after writing it, until it passes, and again after writing an amendment. It is what the worktree creation gate asks of `feat/` and `fix/`. Only read the report: the command never edits the spec.

## Inputs

- `<WORK_ITEM>`: the work item id, like `work-items.w1`.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- The spec folder (`.omhl/specs/wi-<id>__<slug>/`, in the main directory) with the parts `part-NN.md` and the amendments `amendment-NN.md`. Each part has the five `##` sections in order: `Objective`, `Applicable directives and premises`, `Behaviors` (items `- CMP-###:`), `Acceptance criteria` (items `- CA-###:`) and `Out of scope`. An amendment has `Divergence`, `Corrections` (items `- EMD-###`) and `Reason`, and a pending doubt is marked `[PENDING: gaps.<name>]`.
- An old spec stays valid: files named `parte-NN.md` and `emenda-NN.md`, with the Portuguese headings and the Portuguese pending marker. Each file uses one vocabulary whole, chosen by its first heading, and the two name families (`part-NN.md` or `parte-NN.md`, `amendment-NN.md` or `emenda-NN.md`) can coexist in a folder.
- The graph in the database: the cited `premises.<name>` and `guidelines.<name>` must exist and be confirmed, and the spec must cite a confirmed business premise of the work item.

## Output

Text on stdout, exit 0, when the spec passes: `✔ spec validate · <N> parts · <N> amendments`, one `part` line per part read and one `amendment` line per amendment, with the `state` (`approved` or `pending`). A still-pending amendment with a violation appears with a `<file>:<line> · <rule>` line below it, the header becomes `⚠` and the exit stays 0. With `--format json` it is a JSON line: `ok: true`, `parts` (the parts read) and `amendments` (each amendment with `name`, `state` and `violations`).

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](errors.md).

- `spec-invalid` (exit 1): the spec breaks rules. Each item of `details` carries the `code` of the rule (`section-empty`, `sections-invalid`, `item-id-invalid`, `placeholder-found`, `cited-id-unconfirmed`, `business-premise-missing`, `main-business-premise-missing`, `pending-gap-unresolved`...) and the `path` in the format `<file>:<line>`. Up to 20 details appear; the message counts the omitted ones. The spec text never enters the error. Fix the pointed line and run again. `business-premise-missing` says the spec cites no confirmed business premise of the work item; `main-business-premise-missing` says the cited ones are all late (`origin: late`), and the spec must cite at least one main premise.
- `spec-not-found` (exit 1): the work item has no spec folder with at least one part. Run `omhl spec init`.
- `spec-store-unusable` (exit 1): the folder cannot be read, has a file that is not regular or not UTF-8, or the work item has more than one `wi-<id>__*` folder. Fix the folder in the main directory.
- `probe-failed` (exit 1): git did not respond (outside a repository). Run inside a checkout.
- `invalid-workflow-config` (exit 1): `.omhl/workflow.toml` is invalid. Fix the file.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Valid spec, citing the confirmed business premise `premises.p1`:

```omhl-example fixture=spec
$ omhl spec validate work-items.w1
✔ spec validate · 1 part · 0 amendments
- part · part-01.md
```

With `--format json` the report comes out for machines:

```omhl-example fixture=spec
$ omhl spec validate work-items.w1 --format json
{"amendments":[],"ok":true,"parts":["part-01.md"]}
```

Refusal: the newly created part has the five sections empty and cites no business premise:

```omhl-example fixture=git
$ omhl spec init work-items.w1 x
✔ spec init · work-items.w1 · x
  dir: <repo>/.omhl/specs/wi-w1__x
  path: <repo>/.omhl/specs/wi-w1__x/part-01.md
Next: fill in the five sections of the part file, then run omhl spec validate work-items.w1.
$ omhl spec validate work-items.w1
2> ✘ spec-invalid · the spec is invalid: 6 violation(s) shown, 0 omitted
2>   - part-01.md:1 · section-empty · the spec breaks the rule section-empty
2>   - part-01.md:3 · section-empty · the spec breaks the rule section-empty
2>   - part-01.md:3 · business-premise-missing · the spec breaks the rule business-premise-missing
2>   - part-01.md:5 · section-empty · the spec breaks the rule section-empty
2>   - part-01.md:7 · section-empty · the spec breaks the rule section-empty
2>   - part-01.md:9 · section-empty · the spec breaks the rule section-empty
# exit 1
```

Refusal: the work item has no spec folder:

```omhl-example fixture=git
$ omhl spec validate work-items.w1
2> ✘ spec-not-found · the work item has no spec folder with a part
2>   - /work_item
# exit 1
```
