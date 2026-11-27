# omhl append

- [Who uses it](#who-uses-it-at-which-stage-and-with-which-filter)
- [When to use](#when-to-use)
- [Inputs](#inputs)
- [Output](#output)
- [Errors](#errors)
- [Examples](#examples)

## When to use

To record in the project database: work item, premise, gap, divergence, guideline and relation. It is the only generic write command; the records that have their own command (`omhl attest`, `omhl guideline retire`) do not go through here. Under Claude Code the CLI records `agent: "claude"`. The guard refuses the whole request if an item breaks a rule: nothing is recorded halfway.

## Inputs

- The request is a JSON on stdin: `{"key": "<text>", "items": [...]}`. `items` has one to fifty items. Each item has `body` and, optionally, `base` (the `record_id` it revises), `snapshot` and `external_id`. Any other key at the top of the request is `append-invalid`.
- `body` is `{"kind":"node","id":"<namespace>.<name>","type":"<type>","data":{...}}`, `{"kind":"relation","relation_type":"<name>","from":"<id>","to":"<id>"}` or `{"kind":"retirement","lineage":{"kind":"relation","relation_type":"contradicts","from":"<id>","to":"<id>"}}`, which retires a current relation. The retirement requires the `base` of the relation (the current `record_id`) and no `omhl` command reads it: send the request without `base`, the answer is `FORK_REJECTED` with `not-current` and `details[0].current` is the `base`; repeat the request with that value in `base` on the item. Another error on the first call means the relation does not exist, was already retired or is yours: do not repeat. Only the verifier and the owner retire.
- A `confirmed` premise carries `confirmed_by` with `by` (`person:<name>`), and verbatim `question` and `answer`, copied from the human's real answer.
- A new or revised guideline carries `kind`: `business` or `architecture`.
- Options: `--db <path>` (a new path creates the database) and `--project-dir <dir>` (the folder where the search for the `.omhl.toml` marker starts; it must exist).
- `--owner-override`: belongs to the owner, outside Claude Code; an agent never passes it ([config-loosening-review.md](../../review/references/config-loosening-review.md)).
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.
- `key` (optional, at the top of the JSON, next to `items`; there is no `--key` flag): a text of 1 to 200 characters that you choose to identify this request, for example `gaps-limit-1`. It exists for the retry. If `append` fails without you knowing whether it recorded (`BUSY`, `STORE_ERROR`, timeout), repeat the identical request with the same `key`:
  - If the first had not recorded, the second records and returns `replayed: false`.
  - If the first had already recorded, nothing is recorded again and the output carries the same `records` with `replayed: true`. In both cases the request is done: move on.
  - The same `key` with different `items` is `IDEMPOTENCY_CONFLICT` (`path` `/key`): the `key` already belongs to another request. Use a new `key` if the content changed on purpose.
  - Only `items` counts in the comparison: the author does not. An empty `key` or one with more than 200 characters is `INVALID_INPUT`.
  - The omhl guard runs before the `key`: if the state changed and the guard started refusing the request (for example, `work-item-closed`), the retry comes out with the guard error, not with `replayed: true`.
  - Without `key`, the retry is a new request. A revision (with `base`) that had already recorded comes back as `FORK_REJECTED` (`not-current`) instead of `replayed`. So: a request you may have to repeat carries `key`; a quick request with no retry can go without.

### `data` fields by type

An extra field is refused. Texts have 1 to 2000 characters (`title`, 1 to 200).

| Type | Required | Optional |
|---|---|---|
| `work-item` | `title`, `status` (`open`; `closed` only through `omhl close`) | `type` |
| `premise` | `text`, `status` (`assumed` or `confirmed`) | `kind` (`technical`, `business`), `origin` (`late`, only in `business`), `work_item`, `confirmed_by` |
| `guideline` | `text` | `kind` (`business`, `architecture`), `confirmed_by` |
| `gap` | `question`, `context` | `kind` (`technical`, `business`), `work_item` |
| `divergence` | `source_ref`, `kind` (`missing`, `partial`, `contradicts`, `unrequested`), `text`, `evidence`, `resolution` (`open`, `spec-stale`, `code-fixed`, `accepted`), `work_item` | `confirmed_by`, `amendment` |
| `decision` | `text`, `evidence` | `work_item` |

Record the `type` when creating the work item, in `data.type`; it picks what the prefix requires.

`evidence` is a list of 1 to 20 items `{"source": "...", "summary": "..."}`. `confirmed_by` is `{"by": "person:<name>", "question": "...", "answer": "..."}`.

## Output

Text on stdout, exit 0: `✔ append · N records` (`1 record` in the singular) and one `- <record-id> · seq S` line per recorded item, in the order of the request. The whole `<record-id>` is the `base` of a future revision. When the `key` of the request had already recorded exactly these `items`, the header gains `· replayed` (see the Inputs section). The chain head does not appear in the text. With `--format json` it is a JSON line: `chain_head` (`hash` and `seq` of the chain head), `records` (the `id` and `seq` of each recorded item) and `replayed` (`true` or `false`). An error goes to stderr, with exit 1, as text (`✘ code · message`, one `- path · code · message` line per detail and `Next:` when there is `next_step`; with `--format json`, as a JSON line).

## Errors

Each error carries the code (`error.code` in the JSON) and, in the details, the `path` of the field (never the typed content). A clap usage error (unknown option) exits with 2, with no code. The errors common to all commands are in [errors.md](errors.md).

- `input-invalid-json` (exit 1): stdin is not valid JSON. The message says the line and the column. Fix the JSON.
- `input-not-utf8` (exit 1): stdin is not UTF-8. Resend in UTF-8.
- `input-too-large` (exit 1): the request exceeded the size ceiling. Split into smaller requests.
- `input-too-deep` (exit 1): the JSON has too much nesting. Flatten the structure.
- `append-invalid` (exit 1): an unknown key or a field with the wrong shape. The `path` points at the field (for example, `base` lives on the item, not in the `body`).
- `confirmation-required` (exit 1): a `confirmed` premise, guideline or `spec-stale` divergence without `confirmed_by`. Ask the human and record the verbatim question and answer.
- `confirmation-invalid` (exit 1): the `by` does not start with `person:` or the name has fewer than 3 visible characters (except `"by": "critic"` recorded by `omhl:critic` on a `technical` premise), or the `confirmed_by` came on a record that does not admit it (for example, `confirmed_by` or `amendment` on a divergence that is not in `spec-stale`). Fix the field.
- `kind-missing` (exit 1): a new or revised guideline without `kind`. Add `business` or `architecture`.
- `kind-invalid` (exit 1): a guideline `kind` with another value. Use `business` or `architecture`.
- `origin-invalid` (exit 1): `origin` on a premise that is not a business one (technical or without `kind`). Remove the `origin`, or mark the premise as `business`.
- `origin-immutable` (exit 1): the revision adds, changes or removes the `origin` of the premise. Keep what it already has, or record a new premise.
- `origin-required` (exit 1): the work item already has a `decision` and the new business premise does not carry `origin: late`. On creation, add `origin: late` and record again; on a revision that makes the premise a business one, record a new premise with `origin: late`, because adding `origin` in a revision gives `origin-immutable`.
- `work-item-not-found` (exit 1): the `work_item` of the premise, gap or divergence does not exist. Record the work item first.
- `work-item-closed` (exit 1): the work item is already closed. Record in an open work item.
- `use-close-work-item` (exit 1): the `status` of the work item was changed to `closed` through here. Close with `omhl close`.
- `rests-on-architecture` (exit 1): a guideline rests on a guideline of `kind` `architecture`. Rest it on a business one.
- `rests-on-target-type` (exit 1): a guideline rests on a premise. The `rests-on` relation of a guideline accepts only a guideline.
- `guideline-retire-by-command` (exit 1): the guideline retirement came through `append`. Use `omhl guideline retire`.
- `role-unknown` (exit 1): `OMHL_ROLE` has a value outside the seven roles, or was set outside Claude Code. Fix or remove the variable.
- `invalid-session-id` (exit 1): `CLAUDE_CODE_SESSION_ID` is set but empty, is not UTF-8, exceeds 100 UTF-16 units or equals the `outside-claude-code` marker; nothing is recorded and the error does not show the value (empty `path`). Remove the variable or run `omhl` in a Claude Code session.
- `role-record-forbidden` (exit 1): the role records a node type or relation that is not its own, a divergence created outside `open`, or a relation from a node of another role or a nonexistent one. Record only what the role line allows.
- `role-revision-forbidden` (exit 1): the role revises a node of another, an already confirmed premise or an already resolved divergence, changes the `kind` of a premise or gap, or moves a divergence out of `open` outside the verifier closing. Record a node of your own or ask the owner.
- `role-answer-forbidden` (exit 1): the critic links `answers` to a target that is not a `technical` gap. Leave the gap to the owner.
- `role-retirement-forbidden` (exit 1): the caller cannot retire this node or relation, or the `contradicts` is its own. Ask the verifier or the owner.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.
- `INVALID_INPUT` (exit 1): a value of the request has an invalid form for the database (for example, a malformed semantic id, `batch-size` with an empty batch or one of more than 50 items, or `idempotency-key-format`). The `path` and the `code` of the detail say which.
- `INVALID_RECORD` (exit 1): the `data` does not match the schema of the table above, or the relation points to a nonexistent node. Each violation comes in `details`, with `code` (`required`, `enum`, `additional-properties`, `min-length`, `endpoint-not-found`, among others) and the `path` of the field; the `message` does not carry the valid values, look at the table. Fix all the listed fields at once.
- `FORK_REJECTED` (exit 1): `not-current` means the `base` is not the current record of the id; use the `current` of the detail as the new `base` (if `retired` is `true`, the record was retired: stop and warn). `lineage-in-batch`: two items of the same id in the batch; merge them.
- `IDEMPOTENCY_CONFLICT` (exit 1): the `key` already recorded a different request. The detail is `key-reused`, at `path` `/key`. Repeat the same request with the same `key`, or use a new `key` for a new request.
- `CYCLE_REJECTED` (exit 1): the relation would close a cycle (`rests-on`, `supersedes`). The `next_step` of the detail says what to do.
- `BUSY` (exit 1): another `omhl` is recording. `omhl` already tried 3 times and nothing was recorded: repeat the same command.
- `STORE_ERROR` (exit 1): database failure; the result is uncertain. Repeat the request with the same `key`: if it had already recorded, `replayed: true` comes back.
- `TYPE_NOT_FOUND` (exit 1): the `type` does not exist. Use one from the table above.
- `RELATION_TYPE_NOT_FOUND` (exit 1): the `relation_type` does not exist (`answers`, `rests-on`, `supports`, `contradicts`, `supersedes`).
- `RESERVED_ID` (exit 1): the id belongs to a native node. Choose another id.

## Examples

Records a gap and the `answers` relation from a premise to it, in the same request:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 2 records
- <record-id> · seq 3
- <record-id> · seq 4
```

With `--format json` the answer also carries the chain head:

```omhl-example fixture=project
$ omhl append --format json <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w8","type":"work-item","data":{"title":"Export PDF","status":"open"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":3},"records":[{"id":"<record-id>","seq":3}],"replayed":false}
```

Creates the work item (step 2 of the protocol):

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w9","type":"work-item","data":{"title":"Export CSV","status":"open"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
```

Refusal by the schema: the same work item without `title`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w9","type":"work-item","data":{"status":"open"}}}]}
EOF
2> ✘ INVALID_RECORD · record content is invalid
2>   - /items/0/body/data/title · required · required
# exit 1
```

The same request, with the same `key`, twice: the second returns `replayed: true` and does not record again:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record · replayed
- <record-id> · seq 3
```

The same `key` with other content is refused with `IDEMPOTENCY_CONFLICT`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"Excel export.","work_item":"work-items.w1"}}}]}
EOF
2> ✘ IDEMPOTENCY_CONFLICT · idempotency key was used with different items
2>   - /key · key-reused
# exit 1
```

Refusal: `confirmed_by` on a divergence that is not `spec-stale` (when closing as `code-fixed` or `accepted`, omit `confirmed_by` and `amendment`):

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"code-fixed","work_item":"work-items.w1","confirmed_by":{"by":"person:ana","question":"q","answer":"a"}}}}]}
EOF
2> ✘ confirmation-invalid · confirmation-invalid at /items/0/body/data/confirmed_by
2>   - /items/0/body/data/confirmed_by · confirmation-invalid
# exit 1
```

Refusal: a `confirmed` premise without `confirmed_by`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p2","type":"premise","data":{"text":"Dates are UTC.","status":"confirmed","kind":"business","work_item":"work-items.w1"}}}]}
EOF
2> ✘ confirmation-required · confirmation-required at /items/0/body/data/confirmed_by
2>   - /items/0/body/data/confirmed_by · confirmation-required
# exit 1
```

Refusal: the `by` without the `person:` prefix:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p4","type":"premise","data":{"text":"Dates are UTC.","status":"confirmed","kind":"business","work_item":"work-items.w1","confirmed_by":{"by":"ana","question":"q","answer":"a"}}}}]}
EOF
2> ✘ confirmation-invalid · confirmation-invalid at /items/0/body/data/confirmed_by/by
2>   - /items/0/body/data/confirmed_by/by · confirmation-invalid
# exit 1
```

Refusal: a premise of a work item that does not exist:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p3","type":"premise","data":{"text":"Dates are UTC.","status":"assumed","kind":"technical","work_item":"work-items.zz"}}}]}
EOF
2> ✘ work-item-not-found · work-item-not-found at /items/0/body/data/work_item
2>   - /items/0/body/data/work_item · work-item-not-found
# exit 1
```

Refusal: a guideline without `kind`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"guidelines.g1","type":"guideline","data":{"text":"Be brief."}}}]}
EOF
2> ✘ kind-missing · the guideline has no kind
2>   - /items/0/body/data/kind
2> Next: set kind to business or architecture in the item data and append again.
# exit 1
```

Refusal: stdin is not JSON:

```omhl-example fixture=project
$ omhl append <<'EOF'
not json
EOF
2> ✘ input-invalid-json · input is not valid JSON at line 1 column 2
# exit 1
```
