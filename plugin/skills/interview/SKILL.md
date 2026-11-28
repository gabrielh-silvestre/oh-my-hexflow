---
name: interview
description: Protocol of the business interview with the owner, before the work item is recorded. Use when starting a `feat` or `fix` work item (mandatory) and, at the owner's request, in the `refactor`, `tests`, `bugfix`, `chore` and `docs` prefixes, to clarify the request in rounds of questions, measure the ambiguity and record the work item and the confirmed business premises.
---

# Business interview

This skill requires `directives-flow`. If it is not loaded, load it. It carries the recording rules and the cycle ([cycle.md](../directives-flow/references/cycle.md)). The rounds of questions come before step 1 of the cycle: each business premise is born in a round, drafted by you and confirmed by the owner on the spot, and only at the end are the work item and the premises recorded; [plan-consensus](../plan-consensus/SKILL.md) covers only the technical side.

You are the main session and you run the interview alone. You do not write the spec: the spec belongs to the planner (step 4 of the cycle).

## When to use

- **Mandatory** in `feat` and `fix`: the interview comes before the work item, and nothing is recorded in the database before it ends.
- **Optional** in `refactor`, `tests`, `bugfix`, `chore` and `docs`: it runs only at the owner's request. Without the request, go straight through `directives-flow`.

## The rounds of questions

- **Round 0.** Before the first question, list the parts of the owner's goal and ask them to confirm, merge, split or postpone each one. The list goes to the `Parts` section of the note and stays locked: rewriting the note at each round does not change it. A postponed part leaves the coverage.
- **Owner's name.** Ask for their name (`person:<name>`) in Round 0 or in round 1, before the first premise assembles the `confirmed_by`.
- **Scoreboard.** At each answer, give each dimension a score from 0 to 1 and compute `ambiguity = 1 - (goal × 0.35 + constraints × 0.25 + success criterion × 0.25 + context × 0.15)`. The weights and their `[interview]` keys, with the defaults: goal `objective` 0.35, constraints `constraints` 0.25, success criterion `success` 0.25, context `context` 0.15. Context is how much is understood about the existing system to change it safely. Show the owner the short table (dimension, score, gap) and the ambiguity in %.
- **Stop.** The interview ends with ambiguity of 20% or less (20% by default; the `[interview]` table of the main checkout `.omhl/workflow.toml` changes it) and with each active part of Round 0 covered by at least one confirmed business premise. From round 3 on the owner may end early: state the remaining ambiguity and the parts with no premise. At round 10, ask whether to continue; at round 20, stop with whatever clarity there is.
- **Threshold.** The threshold is 20% by default; the `[interview]` table of the main checkout `.omhl/workflow.toml` sets it.
- **Coverage.** At each round, record in the note which active part already has a confirmed premise and which does not; resuming after compaction reads from there.
- **Question.** About the weakest dimension (say which and why, in one sentence), with the recommended one first, so that "yes" accepts it. Ask about the owner's assumptions, not a list of features.
- **Round premise.** With the answer, draft the business premise it supports, show it in the language of the conversation and ask the owner to confirm or correct it in the same round. The confirmed premise waits in the note until the end: nothing goes to the database before then.
- **Proof of confirmation.** The premise's `confirmed_by` carries in `question` the premise as you drafted it and in `answer` the owner's literal confirmation or correction, each text up to 2000 characters (the schema rejects more). The round's question and answer, in full, stay only in the note.
- **Code facts.** Read them yourself, before asking, and cite the file or the symbol that motivated the question. There is no search agent in the plugin.
- **State.** It lives in `.omhl/notes/interview-<slug>.md`, in the main checkout, rewritten at each round with the `Write` tool (not `Bash`: the hook does not see what `Bash` writes, and `Write` is where the folder gets its `.gitignore`): the round's literal question and answer, in full, the scoreboard, the weakest dimension, the drafted premise and the coverage; the `Parts` section does not change. If the session dies, resume from there. The state does not become a record: no round, note or scoreboard goes to the database, and the folder stays outside `.omhl/specs/`, so `spec-lock` does not freeze it. The `<slug>` is provisional until step 1; if the work item's differs, rename the file.

## Challenge modes (only when the owner asks)

Each mode changes the angle of the question and runs at most once.

- **Contrarian:** "what if the opposite were true?". Tests whether the framing belongs to the project or is just habit.
- **Simplifier:** "what is the simplest version that still has value?". Separates the necessary constraint from the supposed one.
- **Ontologist:** "what is this, really?". Asks which entity is the core and which are support.

## The steps

1. Show the owner the summary of what the interview clarified (goal, constraints, success criterion, context) and propose the prefix, the id and the slug; the branch will be `type/wi-<id>__<slug>`. The session checks with `omhl get work-items.<id>` (the id is free).
2. Record the work item (batch 1, as `claude`, with `omhl append` and an idempotency `key`; the format is in [gravar.md](references/gravar.md)): `work-items.<id>`, `title` and `status` `open`. The session checks with `omhl get work-items.<id>` (it exists and is `open`).
3. Record the premises already confirmed in the rounds (batch 2) in one `omhl append`, with an idempotency `key` and **without** a new batch confirmation round: each one carries `status` `confirmed`, `kind` `business`, the `work_item` and the round's `confirmed_by`, and the `text` in English. If batch 2 fails, fix what the refusal pointed at and record only it again (same `key` if the content did not change, new `key` if it did); batch 1 is already recorded. Then call [plan-consensus](../plan-consensus/SKILL.md). The session checks with `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed`.
