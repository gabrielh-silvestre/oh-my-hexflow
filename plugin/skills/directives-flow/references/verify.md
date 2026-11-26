# omhl verify

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

Reading the `verify` symbol (it exits 0 with a broken chain) is a base rule: [directives-flow](../SKILL.md), Querying section.

## When to use

To prove that the project database was not tampered with: `verify` walks the hash chain of all the project records and says whether it closes. Use it when you suspect corruption, after restoring a database from backup, or before trusting an attestation. It is read-only.

## Inputs

- No arguments.
- `--db <path>` and `--project-dir <dir>`: as in the other commands. A `--db` with a new path creates the database, outside the project folder; `--project-dir` must exist.
- `--format <agent|human|json>`: global option, valid before or after the subcommand. The default is `agent`; `human` shows the same facts in blocks; `json` is the output for machines.

## Output

Text on stdout, **exit 0 even with a broken chain**: read the symbol of the first line. Closed chain: `✔ verify · chain ok · N records`. Broken chain: `✘ verify · chain broken · N records · M breaks`, one `- reason · seq S` line per break found (the first 100) and, only then, `Next:` with what to do. With `--format json` it is a JSON line, whose fields are:

- `ok`: `true` when the chain closes.
- `total_records`: how many records were walked.
- `chain_head`: the hash of the chain head.
- `breaks`: the breaks found, each with `reason` and, when it applies, `seq`. The reasons are `diverging-seq`, `hash-mismatch`, `invalid-body`, `column-mismatch`, `batch-index-mismatch`, `anchor-mismatch` e `chain-head-not-found`.
- `total_breaks`: the total of breaks.

With `✘ verify · chain broken` (in JSON, `ok: false`), record nothing new in the database and do not trust the attestations: warn the owner, with the `reason` and the `seq` of the first break.

## Errors

An error goes to stderr, with exit 1, as text (`✘ code · message`; with `--format json`, as a JSON line). An unknown option is a clap usage error, with exit 2 and no code. A broken chain is **not** an error: it exits 0 with `✘ verify · chain broken` (in JSON, `ok: false`). The errors common to all commands are in [errors.md](errors.md).

- `no-marker` (exit 1): there is no `.omhl.toml` in the folder or above it. Run inside the project; on a branch older than the marker, rebase onto `main`.
- `db-inside-project` (exit 1): `--db` points inside the project. Use a path outside it.
- `invalid-project-dir` (exit 1): `--project-dir` does not exist. The CLI does not climb to a project in a folder above: fix the path.

## Examples

Verifies a database with two records (the work item and the premise):

```omhl-example fixture=project
$ omhl verify
✔ verify · chain ok · 2 records
```

The chain grows with each record, and `verify` follows it; with `--format json` the same answer comes out for machines:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl verify --format json
{"breaks":[],"chain_head":"<hash>","ok":true,"total_breaks":0,"total_records":3}
```
