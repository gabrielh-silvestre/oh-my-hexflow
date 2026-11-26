# The cycle with the agents

When to consult: before starting a work item that will go through the plugin agents, or to know which step of the cycle you are in. Who plays each role and what the main session passes to each one is in [roles.md](roles.md). Steps 2 to 8 have the skill [plan-consensus](../../plan-consensus/SKILL.md); steps 9 and 10, the skill [review](../../review/SKILL.md).

**Before step 1.** In `feat` and `fix`, the main session runs the business interview with the owner (skill [interview](../../interview/SKILL.md)): rounds of questions until the ambiguity reaches 20% or less, and only then step 1 records the work item and the confirmed business premises. In the other prefixes the interview runs only at the owner's request. The state of the rounds stays in `.omhl/notes/`, outside the database and outside the spec.

The cycle has ten steps.

1. **Work item.** The main session records `work-items.<id>` with `omhl append`, with the chosen prefix and slug.
2. **Analyst.** `oh-my-hexflow:analyst` records `assumed` premises and gaps, each gap with `kind` (`technical` or `business`).
3. **Confirmation.** What is business goes to the human, who confirms inside the session. What is technical goes to the critic (`oh-my-hexflow:critic`). A gap without `kind` goes to the human.
4. **Planner.** `oh-my-hexflow:planner` creates the spec with `omhl spec init` and writes it in `.omhl/specs/wi-<id>__<slug>/`; it validates it with `omhl spec validate` until it passes.
5. **Architect.** `oh-my-hexflow:architect` records each `decision` with `rests-on` to the anchor. It does not answer a gap: the answer is a premise.
6. **Critic.** `oh-my-hexflow:critic` records, for each architect decision it evaluates, a `decision` of its own with `rests-on` and links it, to the architect decision, with a `supports` or `contradicts`; it confirms a technical premise and answers a `technical` gap.
7. **Consensus.** Round 1 is steps 5 and 6; without acceptance, `oh-my-hexflow:architect` (and `oh-my-hexflow:planner`, if the spec changes) revises and `oh-my-hexflow:critic` judges again, in at most 3 rounds in total. Without acceptance in the third, the main session stops and shows the impasse to the owner. A late premise (`origin: late`) confirmed by the owner until the spec is frozen, at the birth of the work item branch, reopens the plan: the round counts in the 3 and the impasse goes to the owner; after the freeze, it enters only by `divergence` and, if the spec was wrong, by `amendment-NN`. The critic verdict stays only in the message between the agents.
8. **Spec and worktree.** With acceptance, `oh-my-hexflow:verifier` retires the resolved `contradicts`; then come the final check with `omhl spec validate`, `omhl gate-worktree`, the worktree and the implementation.
9. **Review.** `oh-my-hexflow:code-reviewer`, `oh-my-hexflow:security-reviewer` and `oh-my-hexflow:verifier` review the delivery.
10. **PR and closing.** The PR body carries the table of the critic decisions ([tracking-table.md](tracking-table.md)); then come `omhl gate-pr` and `omhl close`, through the skill [review](../../review/SKILL.md), step 8.

The roles of each step, as data: the `role-<role>` skills of the agents and the tests read this block; the numbered list above is the prose.

```yaml omhl-cycle
stages:
  - {n: 1,  name: work-item,           roles: [session]}
  - {n: 2,  name: analyst,             roles: [analyst]}
  - {n: 3,  name: confirmation,        roles: [critic, session]}
  - {n: 4,  name: planner,             roles: [planner]}
  - {n: 5,  name: architect,           roles: [architect]}
  - {n: 6,  name: critic,              roles: [critic]}
  - {n: 7,  name: consensus,           roles: [planner, architect, critic]}
  - {n: 8,  name: spec-and-worktree,   roles: [verifier, session]}
  - {n: 9,  name: review,              roles: [code-reviewer, security-reviewer, verifier]}
  - {n: 10, name: pr-and-closing,      roles: [session]}
```

**Light path (`chore` and `docs`).** There is no spec: the consensus stays in steps 2 and 3 (the critic only in the mode of step 3), with no planner or rounds and without steps 4 to 7, and step 8 shrinks to `omhl gate-worktree`. The skill [plan-consensus](../../plan-consensus/SKILL.md) describes the path in the "Light path" section.

**Without `name`.** The main session calls the `oh-my-hexflow:*` agents without the `name` parameter. With `name`, the agent runs without the `omhl-allow` matrix and without the preloaded view (its skill does not arrive), and, by the design of the `agent-role` hook, what it records must come out as `claude`. Each call also names the language of the conversation with the person, as a fallback to the `SubagentStart` hook, never as the only channel ([roles.md](roles.md)).

What the frozen spec, the divergence and the amendment ask for at any step stays in [SKILL.md](../SKILL.md).
