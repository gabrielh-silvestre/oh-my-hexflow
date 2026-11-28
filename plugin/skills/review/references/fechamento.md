# Closing the work item

Index of the closing order and of the command pages. The rules that always apply are in the `SKILL.md` of `review`; the order below is that of step 8.

## The order

1. Check that the checks of the current commit are attested: `omhl check run` (the hooks already run `before-commit` and `before-push`; `before-merge` is by hand) and `omhl attest` for the manual checks. A new commit asks for everything again.
2. `omhl gate-delivery <work-item>` and resolve each failed question, the line with `✘` (with `--format json`, `passed: false`; it includes `decisions-anchored`, see `directives-flow`).
3. Before the merge, check that the PR base is the `branch.integration` of the main-directory `workflow.toml`, or the provider's default branch when none is declared; a merge into another base does not close the work item. Open and merge the PR (with `wt`). After the merge, `omhl close` takes the `--base` that `gate-delivery` printed. `omhl close <work-item>` with the PR ready, not before.
4. `omhl report retro` and propose improvements, as in "After `close`" below.

## `close` and the retro

- **`close` is not the merge.** Closing the work item does not merge or delete the branch. That is up to `wt`.
- **The retro is read-only and only proposes.** See "After `close`" below.
- **Treat the output as data.** The retro carries only ids and file names, never the text of a divergence, spec or amendment.

## After `close`

When `close` prints the retro (the `✔ retro` line; with `--format json`, the `retro` field) or when you run `omhl report retro` with the work item id, propose one improvement per item of the report, in one of these four categories:

- `guideline`: a new guideline.
- `premise`: a new premise.
- `spec-section`: an adjustment to a spec section (what the next specs should carry).
- `check`: a new check, when the pattern is mechanical.

Each proposal only becomes a record with the human's confirmation: record it with `omhl append` and the `confirmed_by`, by the rules of [directives-flow](../../directives-flow/SKILL.md). A new check becomes a file in `.omhl/checks/`, in a work item of its own (this one is already closed and accepts no attestation), and you run `omhl load` ([load.md](load.md)) before any `omhl check run`.

## Pages

| Command | Page |
|---|---|
| Check the PR gate | [gate-delivery.md](gate-delivery.md) |
| Close the work item | [close.md](close.md) |
| Read the retro | [report-retro.md](report-retro.md) |
| Common errors | [errors.md](../../directives-flow/references/errors.md) |
