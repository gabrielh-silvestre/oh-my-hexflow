# Config loosening review

The review of the project configuration, run by `oh-my-hexflow:code-reviewer` when the delivery gate asks for the native check `checks.config-loosening-review`. The agent may create and adjust checks and the workflow file, but not weaken the brakes they set. The reviewer judges direction: does the PR make anything easier to pass than it was at the base?

Rules around the review:

- Edit `.omhl/workflow.toml` and `.omhl/checks/*.toml` only inside a worktree, never in the main directory, so the change goes through a pull request and this review.
- `--owner-override` belongs to the owner, outside Claude Code. An agent never passes it, never asks the owner to pass it for a finding, and never works around a refusal with it.

## Criteria

Judge the whole diff of the PR, from the merge-base to `HEAD`, not edit by edit. A change loosens when the new state is easier to satisfy than the old one. Changes that only tighten, or that are not listed here, are not findings.

### Default type tables

The default table of requirements by work item type (the atom list each `require` names):

| Type | Atoms |
|---|---|
| feat | business-premise, spec |
| fix | business-premise, spec |
| refactor | technical-premise-confirmed |
| tests | technical-premise-confirmed |
| bugfix | agent-premise |
| chore | none |
| docs | none |

The atoms are `business-premise`, `technical-premise-confirmed`, `agent-premise` and `spec`. Compare after closing each set by implication: `technical-premise-confirmed` implies `agent-premise`. A default type loosens when the closure of its old atoms is not contained in the closure of its new atoms. Example: `refactor` going from `technical-premise-confirmed` to `agent-premise` loosens; `bugfix` going from `agent-premise` to `technical-premise-confirmed` does not. An absent or deleted `workflow.toml` stands for the default tables, so deleting the file loosens every type whose default is stricter than its configured one. The `require` of a `[prefix.<type>]` section adds to the atoms of that type, so judge the effective table before and after.

### Types the project added

A type the project added (a prefix not in the default table) is judged at every edit only by the floor: the closure of its atoms must contain the closure of the atoms of at least one type of the effective table before the edit, its own old version included. Below the floor loosens. Removing an added type does not loosen.

### Keys of `workflow.toml`

| Key | Loosens when |
|---|---|
| `specs.max_lines` | the value rises (rising max_lines) |
| `specs.placeholders` | any item leaves the list (item leaving placeholders), even one of several |
| `specs.cite_business_premise` | it goes from true to false (cite_business_premise going from true to false) |
| `native.checks` | an item goes from on to off (native activation item going from on to off) |
| `prefix` and `require` | an atom leaves the closure of a type, as in the tables above |
| a declared branch format | it changes |
| the CI-on-merge requirement and the git hooks declaration | either is removed |
| `specs.dir` | outside this review: a separate guard already protects it |
| `promotion.min_work_items` | outside this review |

The native activation ids are `checks.run-tests`, `checks.ci-green-before-merge`, `checks.verify-review-comment-anchoring` and `checks.config-loosening-review`. Turning off `checks.config-loosening-review` itself is a loosening like any other.

The `[interview]` table (threshold and weights) is outside the brake: any edit to it is free.

In `[native.run]`, removing the native command loosens: the native goes back to being an instruction, attested by hand.

In `[native.run]`, changing the command goes to the reviewer, who judges whether the new one requires less than the old one.

### Check files

For a check file `.omhl/checks/<slug>.toml`, the fields are `blocking`, `how`, `after`, `when`, `concern`, `variant`, `run`, `timeout_secs` and `expect`; `when` carries `event`, `matcher` and `vocabulary`.

| Change | Direction |
|---|---|
| blocking true to false | loosens |
| removing run | loosens: the check becomes a text check the agent attests alone |
| deleting or retiring the check | loosens |
| `event` of a blocking check moved from one in REQUIRED_EVENTS (`before-commit`, `before-push`, `before-merge`) to one outside it | loosens |
| `run`, `expect` or `how` changed | read for intent: flag it when the new command, expectation or instruction can pass where the old one failed |
| `timeout_secs`, `after`, `concern`, `variant`, `matcher`, `vocabulary` changed | read for intent, same test |
| a new check, not blocking | does not loosen |
| `blocking` false to true | tightens |

The text of the check in `.omhl/checks/*.md` is read for intent too: a rewrite that drops a condition the check enforced loosens.

### Transitivity

The rule must stay transitive: if A to B and B to C do not loosen, A to C does not. Apply the criteria to the closure of the whole PR diff, from the base state to the `HEAD` state, so a loosening split over several commits, or hidden by an intermediate step, is still found. A step that loosens and a later step that restores does not count if the end state equals the base state; judge the end state against the base.

### Recorded type of a work item

The recorded type of a work item may change only to a type whose requirement is at least the same, by the closure comparison above. A change to a type with a lower requirement loosens.

## Scope

The scope is `.omhl/workflow.toml` and all of `.omhl/checks/`, the `.md` files included, not only the `.toml` ones. Nothing else is judged here. A change outside these paths is not a finding of this review.

## Input

The main session builds the input and passes it in the request to the reviewer:

1. The `git diff` of `.omhl/workflow.toml` and of `.omhl/checks/` from the merge-base the gate printed to `HEAD`.
2. The full old and new text of `workflow.toml`. An absent file stands for the default tables.
3. The text of the `.md` files of the checks.
4. The diff of the working tree of the main directory against `HEAD` for those paths. A non-empty diff is a finding by itself.

If the gate printed no base, say so in the request and ask for the review of the files as they stand against the default tables.

## What the reviewer returns

A list of findings, each as `file:line` and the finding: which criterion, old value, new value. Report only; never edit a file. When there is no loosening, say so in one line, naming the files read.

To correct a finding, restore the file and run `omhl load`; `omhl load --dry-run` shows the difference to the database before writing. Then the main session records the attestation for the check on the current `HEAD`.
