# Recording the work item and the interview premises

When to consult: in steps 2 and 3 of the [interview](../SKILL.md) skill, when recording, at the end of the interview, the work item and the premises the rounds already confirmed. The complete reference of `omhl append`, with the inputs and the errors, is in [append.md](../../directives-flow/references/append.md).

There are two `omhl append` batches, in this order (the guard refuses a premise without an open work item):

- **Batch 1:** the work item (`work-items.<id>`, `title`, `status` `open`), recorded as `claude`.
- **Batch 2:** one business premise per item, all already confirmed in the rounds, with no new batch confirmation. Each one carries `status` `confirmed`, `kind` `business`, the `work_item` and the `confirmed_by`, with `by` = `person:<name>`, the `question` (the premise as you drafted it) and the `answer` (the owner's literal confirmation or correction, without summarizing, translating or correcting), each text up to 2000 characters. The round's full question and answer stay in the note.

Each batch carries a `key`. If batch 2 fails, record only it again; batch 1 is already in the database.

Show the premise to the owner in the language of the conversation; the recorded `text` is in English.

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.i1","type":"work-item","data":{"title":"Export CSV","status":"open"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.i1","type":"premise","data":{"text":"The export must include every column the user sees.","status":"confirmed","kind":"business","work_item":"work-items.i1","confirmed_by":{"by":"person:ana","question":"The export includes every column the user sees on the screen.","answer":"I confirm."}}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 4
```
