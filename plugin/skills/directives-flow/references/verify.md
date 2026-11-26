# omhl verify

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

A leitura do símbolo do `verify` (sai 0 com a cadeia quebrada) é regra da base: [directives-flow](../SKILL.md), seção Consulta.

## Quando usar

Para provar que o banco do projeto não foi adulterado: o `verify` percorre a cadeia de hashes de todos os registros do projeto e diz se ela fecha. Use quando suspeitar de corrupção, depois de restaurar um banco de backup, ou antes de confiar numa attestation. É somente leitura.

## Entradas

- Nenhum argumento.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, **saída 0 mesmo com a cadeia quebrada**: leia o símbolo da primeira linha. Cadeia fechada: `✔ verify · chain ok · N records`. Cadeia quebrada: `✘ verify · chain broken · N records · M breaks`, uma linha `- reason · seq S` por quebra achada (as primeiras 100) e, só então, `Next:` com o que fazer. Com `--format json` é uma linha JSON, cujos campos são:

- `ok`: `true` quando a cadeia fecha.
- `total_records`: quantos registros foram percorridos.
- `chain_head`: o hash da cabeça da cadeia.
- `breaks`: as quebras achadas, cada uma com `reason` e, quando se aplica, `seq`. Os motivos são `diverging-seq`, `hash-mismatch`, `invalid-body`, `column-mismatch`, `batch-index-mismatch`, `anchor-mismatch` e `chain-head-not-found`.
- `total_breaks`: o total de quebras.

Com `✘ verify · chain broken` (em JSON, `ok: false`), não grave nada novo no banco nem confie nas attestations: avise o dono, com o `reason` e o `seq` da primeira quebra.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida é erro de uso do clap, com saída 2 e sem código. Uma cadeia quebrada **não** é erro: sai 0 com `✘ verify · chain broken` (em JSON, `ok: false`). Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Verifica um banco com dois registros (o work item e a premissa):

```omhl-example fixture=project
$ omhl verify
✔ verify · chain ok · 2 records
```

A cadeia cresce a cada gravação, e o `verify` a acompanha; com `--format json` a mesma resposta sai para máquinas:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl verify --format json
{"breaks":[],"chain_head":"<hash>","ok":true,"total_breaks":0,"total_records":3}
```
