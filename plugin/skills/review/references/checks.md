# Mechanical checks

A check with the `run` field is mechanical: `omhl check run` runs the command, compares the result with `expect`, records an attestation (`passed`, `passed_with_warning` or `failed`) and stores the command's output. The delivery gate (`omhl gate-delivery`) reads only the outcome. What to do when the check or the hook refuses is in [gates-operation.md](gates-operation.md). The rules that always apply are in the `SKILL.md` of `review`.

## Check rules

- **One command per check.** Do not join commands in a `run` (`sh -c "a && b"`, `;`, `2>&1`). Order several checks with the `after` field.
- **The outcome is immutable.** The same snapshot, check and work item do not change outcome. Only a new tree (a new commit) releases a new verdict.
- **A read-only check.** If the command changes the tree, `check run` stops with `check-not-run`. Put what it generates in `.gitignore`.
- **`main` receives no direct commit or push.** The working branch follows `type/wi-<id>__<slug>`.

## The cycle

1. `omhl hooks install`, once per clone: it wires `pre-commit` (event `before-commit`) and `pre-push` (event `before-push`).
2. `omhl load` for each file created, edited or deleted in `.omhl/`.
3. `git commit` and `git push` run `omhl check run` on their own and block on failure. `before-merge` has no hook: run `omhl check run --event before-merge` by hand. A manual check (without `run`) belongs to `omhl attest`, after the human reviews.
4. A failed check: read `omhl byproduct <check> <work-item> --tail 40`, fix it and make a new commit (the outcome of the same snapshot does not change).
5. The global sweep (`check sweep`) only after an interrupted `omhl`, with the human's approval. Retiring a check is the owner's task ([check-retire.md](check-retire.md)).

## Pages

| Command | Page |
|---|---|
| Load the checks into the database | [load.md](load.md) |
| Run the checks | [check-run.md](check-run.md) |
| Sweep runaway processes | [check-sweep.md](check-sweep.md) |
| Retire a check | [check-retire.md](check-retire.md) |
| Read a check's output | [byproduct.md](byproduct.md) |
| Delete the expired stored outputs | [byproduct-prune.md](byproduct-prune.md) |
| Install the git hooks | [hooks-install.md](hooks-install.md) |
| `post-commit` hook | [hooks-post-commit.md](hooks-post-commit.md) |
| Attest a manual check | [attest.md](attest.md) |
| When the check or the hook refuses | [gates-operation.md](gates-operation.md) |
| Common errors | [errors.md](../../directives-flow/references/errors.md) |
