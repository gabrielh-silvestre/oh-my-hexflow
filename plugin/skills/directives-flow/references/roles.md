# The agent roles

When to consult: before calling a plugin agent (what to pass it), when reading its answer, or when a guard refuses a record with `role-record-forbidden`, `role-revision-forbidden`, `role-answer-forbidden` or `role-retirement-forbidden`. The order in which the agents enter is in [cycle.md](cycle.md).

## The matrix

The matrix of commands, node types, relations and retirements of each role is in a single table, in the section "A matriz por papel" of [directives-flow.md](https://github.com/gabrielh-silvestre/oh-my-hexflow/blob/main/docs/directives-flow.md#a-matriz-por-papel). This page does not copy it. The matrix of the installed version is in the frontmatter (`omhl-allow`) of each agent in `plugin/agents/`, for example [critic.md](../../../agents/critic.md). Reading (`omhl list`, `omhl get` and the others) holds for the seven roles; `attest`, `load`, `close`, `gate-pr` and `gate-worktree` hold for none.

## What the main session passes to each agent

The call is `oh-my-hexflow:<role>`. Every agent receives the id of the work item (`work-items.<id>`): without it, it does not record. Every call also names the language of the conversation with the person (for example "Conversation language: pt-BR"): it is the fallback to the `SubagentStart` hook, which already announces the language to the agent, never the only channel.

| Agent | Also pass |
|---|---|
| `oh-my-hexflow:analyst` | Nothing beyond the id; the human's request, if any; what the interview already confirmed (in `feat` and `fix`), so it covers only the technical |
| `oh-my-hexflow:planner` | The spec slug in the first call, the absolute spec folder in the following ones; the consensus mode (RALPLAN-DR), SHORT (DELIBERATE only at the human's request or for a high-risk item); the open questions of the analyst |
| `oh-my-hexflow:architect` | The RALPLAN-DR summary and the ADR of the planner; the mode "consensus review (ralplan), SHORT" (or DELIBERATE); the objection of the critic, in the following rounds |
| `oh-my-hexflow:critic` | The absolute path of the spec folder; the mode "consensus review (ralplan), SHORT" (or DELIBERATE) and the request for the ralplan summary line |
| `oh-my-hexflow:code-reviewer` | The spec folder and the change to review |
| `oh-my-hexflow:security-reviewer` | The spec folder and the change to review |
| `oh-my-hexflow:verifier` | The spec folder; the id of the divergence to check; the `from`/`to` pair of each `contradicts` to retire |

The spec lives in the main checkout, outside the agent worktree. Pass the **absolute** path of the folder, in the main checkout: the `dir` of `omhl spec init`, which the planner repeats in "Spec written to". A relative path (`.omhl/specs/wi-<id>__<slug>/`) does not exist inside the worktree, and without the folder the spec readers do not find it.

The `from`/`to` pair of each `contradicts` comes from the `contradicts: <from> -> <to>` lines that the critic and the reviewers list in the answer (it is part of their contract). To check what is current, or if the answer did not bring the pair, run `omhl project` and read the `## Relations` section of `.omhl/projection/work-items.<id>.md`: the projection lists only non-retired relations. No database read command (`omhl list`, `omhl get`, `omhl render`) returns relations, and the verifier does not run `project`: without the pair in the request, it hands it back.

## What to do with the answer

Every answer ends with two fixed fields:

- **Next role:** the role to call next, or `none`. The main session decides; calling the indicated role is the default.
- **Open questions for the main session:** questions that the agent cannot record. The main session records each one as a `gap` with `omhl append`, with the suggested `kind` (`technical` or `business`). Without `kind`, the gap goes to the human.

## Who confirms what

- The critic confirms any `technical` premise, whoever the author, and only premises: it records `status` `confirmed` and `confirmed_by` with `by` equal to `critic`, plus verbatim `question` and `answer`. Business premise, guideline and `business` gap or gap without `kind` stay with the human. The critic answers (`answers`) only a `technical` gap.
- A `confirmed_by` with `by` equal to `critic` recorded by another author, or on a business premise, is `confirmation-invalid`.

## The anchor

Every `decision` (of the architect, the critic or a reviewer) carries `rests-on` to a confirmed guideline or premise, including the premise that the critic confirmed. Without the anchor, `omhl gate-pr` fails `decisions-anchored`. The reviewer who reports a violation records their `decision` with `rests-on` and links a `contradicts` to the violated target.

## The retirement

- Only `oh-my-hexflow:verifier` (isolated, without the author's context) and the owner retire a `contradicts`. Nobody retires what they recorded themselves. The main session (`claude`) does not retire: the guard refuses with `role-retirement-forbidden`.
- The verifier retires in two calls to `omhl append` and gives the reason in its message, because the graph keeps only the retirement. Do not pass it a pair it should not retire.
- The owner (`user`) is the CLI outside Claude Code, so the owner retires a `contradicts` only in an external terminal, outside Claude Code. `! omhl ...` inside Claude Code records `claude` and is refused. The recipe (`retirement` body and the `base` in two calls) is in [append.md](append.md#inputs). Since the main session does not know the ids, pass the owner the `from`/`to` pair copied from the `contradicts: <from> -> <to>` lines or from the `## Relations` section of the projection.
- Among the agents, only the verifier moves a reviewer `divergence` out of `open`, to `code-fixed`, with new evidence. `accepted` and `spec-stale` are answers of the human: the main session records them, with their answer.
