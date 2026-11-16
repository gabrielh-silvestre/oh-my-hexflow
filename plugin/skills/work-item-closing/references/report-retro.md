# omhl report retro

## Quando usar

Para ler a retro de um work item, aberto ou fechado, sem fechá-lo: o mesmo relatório que o `omhl close` imprime no campo `retro`. Use antes do `close` para ver o que a retro vai trazer, ou depois dele para propor melhorias. É somente leitura.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir; pode estar aberto ou fechado.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- Não exige git nem árvore limpa.

## Saída

Uma linha JSON no stdout, saída 0: `{"retro": ...}`. O valor é `null` quando não há retro, isto é, quando o work item não tem divergência nem gap respondido sem critério. Um gap aberto sozinho, ou um arquivo de emenda sozinho, não gera retro. Com retro, o objeto traz:

- `divergences`: os ids agrupados por `kind` e `resolution`.
- `amendments`: `approved` (emendas citadas por uma divergência `spec-stale`) e `pending` (arquivos de emenda que nenhuma divergência cita).
- `no_criterion`: os gaps respondidos sem uma diretiva que sustente a resposta (`gap` e `answered_by`).
- `open_gaps`: os gaps `open` e `needs-human`.

A retro traz só ids e nomes de arquivo, nunca o texto de uma divergência, spec ou emenda. Proponha uma melhoria por item (`guideline`, `premise`, `spec-section` ou `check`), cada uma só com a confirmação do humano.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta do `<WORK_ITEM>` é erro de uso do clap, com saída 2 e sem código. A pasta da spec e o `workflow.toml` só são lidos quando há retro.

- `work-item-not-found` (saída 1): o work item não existe. O ponteiro é `/work_item`. Confira o id.
- `INVALID_INPUT` (saída 1): o id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `spec-store-unusable` (saída 1): há retro e a pasta da spec do work item não pôde ser lida, ou há mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `invalid-workflow-config` (saída 1): há retro e o `.omhl/workflow.toml` é inválido. Corrija o arquivo.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Sem divergência nem gap respondido, a retro é `null`:

```omhl-example fixture=project
$ omhl report retro work-items.w1
{"retro":null}
```

Com uma divergência aberta e um gap respondido por uma premissa sem diretiva que a sustente, a retro lista os dois:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":3},{"id":"<record-id>","seq":4},{"id":"<record-id>","seq":5}],"replayed":false}
$ omhl report retro work-items.w1
2> {"error":{"code":"probe-failed","details":[{"code":"probe-failed","message":"probe-failed","path":"/snapshot"}],"message":"probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git"}}
# exit 1
```

Recusa: o work item não existe e o id está mal formado:

```omhl-example fixture=project
$ omhl report retro work-items.zz
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
$ omhl report retro Bad_Id
2> {"error":{"code":"INVALID_INPUT","details":[{"code":"semantic-id-format","message":"invalid semantic id","path":"/work_item"}],"message":"invalid semantic id"}}
# exit 1
```

Recusa: `--project-dir` que não existe:

```omhl-example fixture=project
$ omhl report retro work-items.w1 --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
