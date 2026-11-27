# omhl attest

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review 8, review rules | `omhl attest` |

## When to use

To record by hand the outcome of a **manual** check (without the `run` field) on a work item, after the human reviews it. The record belongs to the check and work item pair, tied to the current commit. A check with `run` is mechanical: whoever records its outcome is `omhl check run`, never you (the refusal is `check-run-author`).

**Native with a command.** `attest` reads the evaluated declaration from `HEAD`, like the gate. A requirement of the native that has no command stays an instruction, attested by hand; in a repair PR, where the main-directory file is invalid, no command is in force and `attest` accepts the hand attestation of the instruction requirement. The output of a command is read with [byproduct.md](byproduct.md).

## Inputs

- `<CHECK>`: the check id, like `checks.review`. The check must exist (run `omhl load` after creating the file in `.omhl/checks/`).
- `<WORK_ITEM>`: the work item id, like `work-items.w1`. It must exist and be open.
- `--outcome`: `passed`, `passed_with_warning` or `failed`. Required.
- `--note <text>`: free note, optional.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- It needs a git repository with a clean tree: the record ties the outcome to `HEAD`.

## Output

Text on stdout, exit 0: `✔ attest · <check> · <work item> · <outcome> · snapshot <7 characters of the commit>`, with `⚠` in place of `✔` when the outcome is not `passed`, and a `note: ...` line when there is `--note`. With `--format json` it is a JSON line: `attestation` (the `id` derived from the commit, the check, the work item, `outcome`, `ok` and `snapshot`, plus `note` when present), `chain_head` and `records`; the attestation `id` only comes out in the JSON. Repeating the same outcome for the same commit records a revision and exits 0. A different outcome for the same commit is refused: the first recorded outcome is immutable.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). A missing `--outcome` or argument is a clap usage error, with exit 2 and no code. The errors common to all commands are in [errors.md](../../directives-flow/references/errors.md).

- `invalid-arguments` (exit 1): `--outcome` is not one of the three values. The `/outcome` pointer and the message state the rule. Fix the value.
- `check-run-author` (exit 1): the check has `run`, or it is a native whose every requirement has a command (see [native-checks.md](../../directives-flow/references/native-checks.md) and [check-run.md](check-run.md)). Only `omhl check run` records its outcome. Use a manual check or run the check.
- `check-not-found` (exit 1): the check does not exist in the database. Check the id and run `omhl load`.
- `work-item-not-found` (exit 1): the work item does not exist. Record it first.
- `work-item-closed` (exit 1): the work item was already closed. Attest on an open work item.
- `outcome-immutable` (exit 1): the commit already has another outcome recorded for the pair. Make a new commit, or accept the recorded outcome.
- `snapshot-dirty` (exit 1): the tree has an uncommitted change. Commit and repeat.
- `probe-failed` (exit 1): git did not answer (outside a repository, for example). Run inside a checkout.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the marker `outside-claude-code`; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.
- `INVALID_INPUT` (exit 1): an id has an invalid form (`semantic-id-format`). The `path` of the detail says which argument.

## Examples

Attests a manual check. The second call tries to record another outcome for the same commit and is refused:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome passed --note 'Reviewed by hand.'
✔ attest · checks.review · work-items.w1 · passed · snapshot <snapshot>
  note: Reviewed by hand.
$ omhl attest checks.review work-items.w1 --outcome failed
2> ✘ outcome-immutable · outcome-immutable at /outcome
2>   - /outcome · outcome-immutable
# exit 1
```

With `--format json` the answer carries the attestation `id`, the whole `snapshot` and the chain head:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome passed --format json
{"attestation":{"check":"checks.review","id":"attestations.5c008b85e4c4c8cb1c31b8ab9f9fb681.c2445976.checks.review.work-items.w1","ok":true,"outcome":"passed","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false}
```

Refusal: the check has `run`, so it is mechanical:

```omhl-example fixture=checks
$ omhl attest checks.ok work-items.w1 --outcome passed
2> ✘ check-run-author · check-run-author at /check
2>   - /check · check-run-author
# exit 1
```

Refusal: `--outcome` outside the three values:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome maybe
2> ✘ invalid-arguments · outcome must be passed, passed_with_warning or failed
2>   - /outcome
# exit 1
```

Refusal: a check and a work item that do not exist:

```omhl-example fixture=checks
$ omhl attest checks.nope work-items.w1 --outcome passed
2> ✘ check-not-found · check-not-found at /check
2>   - /check · check-not-found
# exit 1
$ omhl attest checks.review work-items.zz --outcome passed
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```
