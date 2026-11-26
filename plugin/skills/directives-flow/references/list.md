# omhl list

- [Who uses it](#who-uses-it-at-which-stage-and-with-which-filter)
- [When to use](#when-to-use)
- [Inputs](#inputs)
- [Output](#output)
- [Errors](#errors)
- [Examples](#examples)

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| analyst | 2 | `omhl list --type gap --where work_item=work-items.<id>` |
| analyst | 2 | `omhl list --type guideline` |
| analyst | 2 | `omhl list --type premise --where work_item=work-items.<id>` |
| architect | 5, 7 | `omhl list --type decision --where work_item=work-items.<id>` |
| architect | 5, 7 | `omhl list --type guideline` |
| architect | 5, 7 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| code-reviewer | 9 | `omhl list --type decision --where work_item=work-items.<id>` |
| code-reviewer | 9 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| code-reviewer | 9 | `omhl list --type guideline` |
| code-reviewer | 9 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| critic | 6, 7 | `omhl list --type decision --where work_item=work-items.<id>` |
| critic | 3 | `omhl list --type gap --where work_item=work-items.<id> --where kind=technical` |
| critic | 6, 7 | `omhl list --type guideline` |
| critic | 3 | `omhl list --type premise --where work_item=work-items.<id> --where kind=technical --where status=assumed` |
| critic | 6, 7 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| planner | 4 | `omhl list --type gap --where work_item=work-items.<id>` |
| planner | 4 | `omhl list --type guideline` |
| planner | 4 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| planner | 4 | `omhl list --type work-item` |
| security-reviewer | 9 | `omhl list --type decision --where work_item=work-items.<id>` |
| security-reviewer | 9 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| security-reviewer | 9 | `omhl list --type guideline` |
| security-reviewer | 9 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| session | review 2 | `omhl list` |
| session | plan-consensus 5, review 6 | `omhl list --type decision --where work_item=work-items.<id>` |
| session | review 6 | `omhl list --type decision --where work_item=work-items.<id> --format json` |
| session | review 1, review 2, review 3, review 5 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| session | plan-consensus 1 | `omhl list --type gap --where work_item=work-items.<id>` |
| session | review 6 | `omhl list --type premise --where work_item=work-items.<id> --format json` |
| session | interview 3, plan-consensus 2, review 4 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| verifier | 9 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| verifier | 8, 9 | `omhl list --type guideline` |
| verifier | 9 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |

## When to use

To find nodes in the database when you do not know the id: the premises of a work item, the open divergences, the checks, the guidelines. `list` returns the current revision of each node, in recording order, page by page. If you already know the id, use `omhl get`. It is read-only.

`list` **honors the request or refuses it**: a filter that would never match anything (plural type, a field that does not exist, a value outside the allowed ones) comes back as an error with the rule, never as `✔ 0 nodes`. So a `✔ 0 nodes` (`{"items":[]}` in `json`) is a true empty answer: no node matches. To query the open divergences of a work item:

```omhl-example fixture=project
$ omhl list --type divergence --where work_item=work-items.w1 --where resolution=open
✔ 0 nodes
```

## Inputs

All options are optional. With none, it lists the nodes of all types.

- `--type <type>`: a type of the catalog, **in the singular**: `check`, `guideline`, `premise`, `decision`, `gap`, `divergence`, `attestation`, `work-item` or `pull-request`. A type that does not exist in the catalog is refused (`premises` in the plural is refused).
- `--id-prefix <prefix>`: filters by the start of the id. Rules:
  - it is a sequence of names separated by `.`; each name has 1 to 63 characters, only lowercase letters, digits and hyphens, and does not start with a hyphen; the whole prefix has up to 200 characters;
  - **matches by whole segment**: `premises.p1` matches `premises.p1`, but `premises.p` does not match `premises.p1` (and `premises.cli1` does not match `premises.cli1-b1`);
  - **starts with a type namespace**: `premises`, `guidelines`, `checks`, `decisions`, `gaps`, `divergences`, `attestations` or `work-items` (`premise` in the singular is refused). With `--type`, the namespace is that of the type (`work-items` serves `work-item` and `pull-request`).
- `--where <FIELD=VALUE>`: filters by equality of a field, repeatable; all pairs must match. Rules:
  - the command splits at the **first** `=`, so the value may contain `=`; a pair without `=` or with an empty field is refused;
  - the field must be **text**: the field schema has `type: "string"`, or has no `type` and every `enum` value is a string (the case of `status`, `kind`, `resolution`, `concern` and `outcome`). A `boolean`, `integer`, `array` or `object` field does not qualify: `--where blocking=true` is refused;
  - if the field has `enum`, the value must be one of the allowed ones (`status=confimed` is refused);
  - **one value per field**: the same field with different values is refused, and this check comes before the `enum` one: `--where status=confimed --where status=open` refuses the second pair for repetition, not the first for a value outside the `enum`; the same pair repeated is still accepted;
  - the field must exist in the type (with `--type`) or in some candidate type (see below);
  - at most 8 pairs, and the value has up to 256 characters.
- **Candidate types (without `--type`).** The candidates are the types that the namespace of `--id-prefix` admits, or the nine of the catalog if there is no prefix. The filter holds if **some candidate** declares all the `--where` fields as text and accepts the values; otherwise it is refused at `/where/<index>`. That is why `--where status=open` works without `--type`, and `--where title=x --where kind=missing` is refused (no type declares both). Since no type has more than six text fields, more than six distinct fields never pass.
- `--limit <n>`: the page size, from **1 to 200**; without the option, 50. `0` and above 200 are refused.
- `--after-seq <n>`: the page cursor: returns the nodes with `seq` greater than `n`. Use the `next_after_seq` of the previous page. A value beyond the database head returns the empty page, which is a true answer.
- `--db <path>`: the database file. A new path **creates the database**; the path must be outside the project folder.
- `--project-dir <dir>`: where to look for the `.omhl.toml` marker. The folder must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, exit 0. The first line summarizes the page: `✔ N nodes · seq A–B` (the `seq` range of the page; `✔ 0 nodes` when no node matches). Then comes one line per node, with the whole id (never shortened), the type, the `seq` and the fields of the type:

- `check`: the event, `blocking` or `non-blocking` and `concern/variant`;
- `attestation`: the outcome, the check and the work item;
- `premise`: the `status`, the `kind`, the work item and the text;
- `work-item` and `pull-request`: the `status` and the title;
- `guideline`: the `kind` and the text;
- `divergence`: the `resolution` and the text;
- `decision`: the text; `gap`: the question.

A node overridden by a native check with the same id carries the word `shadowed` after the `seq`. The rest of `data` (evidence, `confirmed_by`, `run`, `expect`) and the author, the `at` and the `record_id` stay out: read the whole node with `omhl get <id>`. When there are more nodes after the page, the last line is `Next: omhl list --after-seq N with the same filters for more. One node in full: omhl get <id>.`; repeat the command with `--after-seq N` **and the same filters**. Without `Next:`, the page is the last. With `--format human` the same facts come out in blocks, one per node: the id on one line and, below, `type`, `seq`, the fields of the type and `of` (the work item) with aligned labels.

With `--format json` the output is one line: `{"items": [...]}`. Each item carries `id`, `type`, `record_id`, `seq`, `at`, `data` (the node fields), `author` (`agent` and `client`) and `shadowed` (`true` when a native check with the same id overrides it), plus `snapshot` and `external_id` when the node has them. When there are more nodes after the page, the answer carries `next_after_seq`; repeat the command with `--after-seq` equal to that value. Without `next_after_seq`, the page is the last.

## Errors

An error goes to stderr, as text (`✘ code · message`; with `--format json`, as a JSON line), with exit 1. An unknown option or a value that clap cannot read (`--limit abc`) is a clap usage error, with exit 2 and no code. The message of each refusal states the rule and does not repeat the received value. The errors common to all commands are in [errors.md](errors.md).

- `invalid-arguments` at `/limit` (exit 1): `--limit` is `0` or exceeds 200. Use a value from 1 to 200.
- `invalid-arguments` at `/type` (exit 1): `--type` does not exist in the catalog. Use the singular name, from the list above.
- `invalid-arguments` at `/id_prefix` (exit 1): the prefix has an empty segment, a character outside lowercase letter, digit and hyphen, starting with `-` or above 63 characters, exceeds 200 characters in total, does not start with a type namespace, or is outside the namespace of `--type`. Fix the prefix.
- `invalid-arguments` at `/where/<index>` (exit 1): the `--where` pair at position `<index>` breaks a rule: no `=` or an empty field; a field the type does not declare; a field that is not text; a value outside the `enum`; a field repeated with different values (the pointer is that of the second pair); or, without `--type`, no candidate type satisfies it. Fix the pair.
- `where-eq-too-many` (exit 1): `--where` has more than 8 pairs (only identical repeated pairs pass, since a type does not declare that many text fields). Reduce the pairs.
- `where-eq-value-too-long` (exit 1): the value of a `--where` exceeds 256 characters. Use a smaller value.
- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Lists everything. The fixture has a work item and a premise; with `--format json` the same answer comes out for machines:

```omhl-example fixture=project
$ omhl list
✔ 2 nodes · seq 1–2
  work-items.w1 · work-item · seq 1 · open · First item
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --format json
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"status":"open","title":"First item"},"id":"work-items.w1","record_id":"<record-id>","seq":1,"shadowed":false,"type":"work-item"},{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
```

Filters by type and by a text field, and filters by a field alone without `--type`:

```omhl-example fixture=project
$ omhl list --type premise --where status=confirmed
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --where status=open
✔ 1 node · seq 1
  work-items.w1 · work-item · seq 1 · open · First item
```

Paginates: the first page carries `next_after_seq`; the second does not. An `--after-seq` beyond the head is the true empty page:

```omhl-example fixture=project
$ omhl list --limit 1
✔ 1 node · seq 1
  work-items.w1 · work-item · seq 1 · open · First item
Next: omhl list --after-seq 1 with the same filters for more. One node in full: omhl get <id>.
$ omhl list --limit 1 --after-seq 1
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --after-seq 99
✔ 0 nodes
```

With `--format human` the same facts come out in blocks, one per node:

```omhl-example fixture=project
$ omhl list --format human
✔ 2 nodes · seq 1–2

  work-items.w1
    type:   work-item
    seq:    1
    status: open
    title:  First item

  premises.p1
    type:   premise
    seq:    2
    status: confirmed
    kind:   business
    of:     work-items.w1
    text:   Money is stored in cents.
```

The prefix matches by whole segment: `premises.p1` matches, `premises.p` does not:

```omhl-example fixture=project
$ omhl list --id-prefix premises.p1
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --id-prefix premises.p
✔ 0 nodes
```

Refusal: `--limit` outside 1 to 200:

```omhl-example fixture=project
$ omhl list --limit 0
2> ✘ invalid-arguments · limit must be between 1 and 200
2>   - /limit
# exit 1
$ omhl list --limit 201
2> ✘ invalid-arguments · limit must be between 1 and 200
2>   - /limit
# exit 1
```

Refusal: a `--type` that does not exist in the catalog (the plural is refused):

```omhl-example fixture=project
$ omhl list --type premises
2> ✘ invalid-arguments · type is not defined in the catalog
2>   - /type
# exit 1
```

Refusal: an `--id-prefix` that does not start with a type namespace, or that leaves the namespace of `--type`:

```omhl-example fixture=project
$ omhl list --id-prefix premise
2> ✘ invalid-arguments · id prefix must start with the namespace of a type
2>   - /id_prefix
# exit 1
$ omhl list --type premise --id-prefix work-items
2> ✘ invalid-arguments · id prefix is outside the namespace of the type
2>   - /id_prefix
# exit 1
```

Refusal: an `--id-prefix` with an invalid segment (empty, starting with `-`, above 63 characters):

```omhl-example fixture=project
$ omhl list --id-prefix premises..x
2> ✘ invalid-arguments · id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total
2>   - /id_prefix
# exit 1
$ omhl list --id-prefix premises.-x
2> ✘ invalid-arguments · id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total
2>   - /id_prefix
# exit 1
$ omhl list --id-prefix premises.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
2> ✘ invalid-arguments · id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total
2>   - /id_prefix
# exit 1
```

Refusal: with `--type`, a field the type does not declare, a field that is not text and a value outside the `enum`:

```omhl-example fixture=project
$ omhl list --type premise --where blocking=true
2> ✘ invalid-arguments · field is not declared by the type
2>   - /where/0
# exit 1
$ omhl list --type check --where blocking=true
2> ✘ invalid-arguments · field is not a text field; where compares text only
2>   - /where/0
# exit 1
$ omhl list --type premise --where status=confimed
2> ✘ invalid-arguments · value is not one of the values the field allows
2>   - /where/0
# exit 1
```

Refusal: the same field with different values. The same pair repeated is still accepted:

```omhl-example fixture=project
$ omhl list --type premise --where status=confirmed --where status=assumed
2> ✘ invalid-arguments · field repeated in where with different values
2>   - /where/1
# exit 1
$ omhl list --type premise --where status=confirmed --where status=confirmed
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
```

Refusal: without `--type`, no type declares all the `--where` fields:

```omhl-example fixture=project
$ omhl list --where title=x --where kind=missing
2> ✘ invalid-arguments · no type declares every where field
2>   - /where/1
# exit 1
```

Refusal: a malformed `--where` (without `=` or with an empty field):

```omhl-example fixture=project
$ omhl list --where foo
2> ✘ invalid-arguments · where must be FIELD=VALUE with a non-empty FIELD
2>   - /where/0
# exit 1
$ omhl list --where =x
2> ✘ invalid-arguments · where must be FIELD=VALUE with a non-empty FIELD
2>   - /where/0
# exit 1
```

Refusal: `--db` inside the project folder:

```omhl-example fixture=project
$ omhl list --db novo.db
2> ✘ db-inside-project · the database folder is inside the project; its blobs/ folder would dirty the attested tree
# exit 1
```
