# omhl verify

## Quando usar

Para provar que o banco do projeto não foi adulterado: o `verify` percorre a cadeia de hashes de todos os registros do projeto e diz se ela fecha. Use quando suspeitar de corrupção, depois de restaurar um banco de backup, ou antes de confiar numa attestation. É somente leitura.

## Entradas

- Nenhum argumento.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir.

## Saída

Uma linha JSON no stdout, **saída 0 mesmo com a cadeia quebrada**: leia o campo `ok`. Os campos são:

- `ok`: `true` quando a cadeia fecha.
- `total_records`: quantos registros foram percorridos.
- `chain_head`: o hash da cabeça da cadeia.
- `breaks`: as quebras achadas, cada uma com `reason` e, quando se aplica, `seq`. Os motivos são `diverging-seq`, `hash-mismatch`, `invalid-body`, `column-mismatch`, `batch-index-mismatch`, `anchor-mismatch` e `chain-head-not-found`.
- `total_breaks`: o total de quebras.

Com `ok: false`, não grave nada novo no banco nem confie nas attestations: avise o dono, com o `reason` e o `seq` da primeira quebra.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida é erro de uso do clap, com saída 2 e sem código. Uma cadeia quebrada **não** é erro: sai 0 com `ok: false`. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Verifica um banco com dois registros (o work item e a premissa):

```omhl-example fixture=project
$ omhl verify
{"breaks":[],"chain_head":"<hash>","ok":true,"total_breaks":0,"total_records":2}
```

A cadeia cresce a cada gravação, e o `verify` a acompanha:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":3},"records":[{"id":"<record-id>","seq":3}],"replayed":false}
$ omhl verify
{"breaks":[],"chain_head":"<hash>","ok":true,"total_breaks":0,"total_records":3}
```
