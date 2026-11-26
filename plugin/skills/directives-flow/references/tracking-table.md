# Template: tracking table

When to consult: when building the PR table with the critic decisions and the reviewer decisions that still contradict something (skill [review](../../review/SKILL.md)) or any overview of items and the state of each one.

## Which template applies

First read `.omhl/templates/tracking-table.md` in the project. If the file exists, it applies in place of this one. If it is absent, the plugin template below applies. The plugin does not fetch the override by itself: you read it, by this instruction.

## The default template

The overview of each item and where it stands. It closes a report made of cards and is updated at each decision.

```markdown
## Tracking

**Legend:** ✅ kept · ✏️ change · 🔍 under analysis · ⏳ pending · ❓ under review · 🗑️ removed

| ID | Item | State | Change in one line |
|---|---|:-:|---|
| **1** | **<Group>** | | |
| [1.1](#11) | <Item> | ✏️ | <What changes, in one line.> |

**Totals (<n> rows):** ✅ <n> · ✏️ <n> · 🔍 <n> · ⏳ <n> · ❓ <n> · 🗑️ <n>

**To revisit at the end**

- <ID>: <what to reopen and why.>
```

Rules:

- **One row per item card, with the same ID.** A group is a bold row, with no state.
- **The ID cell is a link to its card:** `[M1](#m1)`, the ID lowercase and without punctuation (`1.1` becomes `#11`). The card is above, under a heading with only the ID. Group rows carry no link. In a table without cards, like the PR one, the ID is plain text.
- **The last column is one line.** The detail stays in the card.
- **Recount the totals row at each change.** It counts only item rows, not group rows.
- **`To revisit at the end`** lists only the decisions made provisionally. Remove the entry when it is resolved.
- **Legend:** keep the six states above; a new state is the owner's decision, not the writer's.

## The PR table

Without cards, the PR table uses the header `| ID | Item | State | Evidence |`:

- **ID:** the node id in plain text (`decisions.<name>`, `premises.<name>`).
- **Item:** the `data.text` of the node, in one line, with `|` escaped as `\|`. It is agent text that read untrusted code: no HTML, image or link. In a `decision` of `omhl:code-reviewer` or `omhl:security-reviewer` with a current `contradicts`, mark the author at the start of the text.
- **State:** one of the six in the legend. The meaning of each one in the PR table is not fixed yet: when in doubt, ask the owner, do not invent.
- **Evidence:** the reason for the retirement in one line, taken from the verifier message kept in `<id>-retiradas.md` (skill [review](../../review/SKILL.md), step 4); never the whole message nor command output (rule of [byproduct.md](../../review/references/byproduct.md)); otherwise, empty.
