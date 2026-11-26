# The divergence and its outcomes

When to consult: when recording a `divergence` (the agent strayed from the frozen spec) or when closing it. The `data` fields are in the table of [append.md](append.md); here are the values and what each outcome asks for.

| Field | Value |
|---|---|
| `source_ref` | The spec item (`CMP-###` or `CA-###`) or the id of the guideline or premise |
| `kind` | `missing`, `partial`, `contradicts` or `unrequested` |
| `text` | What was expected, what was done and the reason |
| `evidence` | 1 to 20 items `{source, summary}` |
| `resolution` | `open`, `spec-stale`, `code-fixed` or `accepted` |
| `work_item` | The work item of the divergence |
| `confirmed_by`, `amendment` | Only in `spec-stale` |

Every divergence is born `open`. The three outcomes are new revisions of the same `divergence` (`omhl append` with `base`, the `record_id` from `omhl get divergences.<name>`):

- `spec-stale`: the spec was wrong and the human approved the amendment. The revision carries `amendment` (`amendment-NN` or `emenda-NN`, without `.md`) and `confirmed_by`; the step by step is in `SKILL.md`, under "Correcting the spec after approval".
- `code-fixed`: the spec was right and you fixed the code to follow it.
- `accepted`: the spec was right and the human accepts the deviation. Record it only with their answer; `accepted` keeps no proof, because `confirmed_by` is forbidden in it.

In `code-fixed` and `accepted`, do not carry `confirmed_by` or `amendment`: the guard refuses with `confirmation-invalid`. A divergence that already left `open` accepts no amendment afterwards. While it stays `open`, it appears in every retro (`omhl report retro`).
