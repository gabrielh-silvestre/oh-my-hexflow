# omhl report retro

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review 8 | `omhl report retro` |

## Quando usar

Para ler a retro de um work item, aberto ou fechado, sem fechá-lo: o mesmo relatório que o `omhl close` imprime no campo `retro`. Use antes do `close` para ver o que a retro vai trazer, ou depois dele para propor melhorias. É somente leitura.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir; pode estar aberto ou fechado.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.
- Não exige árvore limpa. Sem retro também não exige git; com retro, o comando lê a pasta da spec pelo repositório git do projeto e recusa com `probe-failed` fora de um.

## Saída

Texto no stdout, saída 0. Sem retro sai `✔ report retro · none`: o work item não tem divergência nem gap respondido sem critério (um gap aberto sozinho, ou um arquivo de emenda sozinho, não gera retro). Com retro, o cabeçalho conta os itens (`✔ report retro · 1 divergence · 1 no-criterion gap`), cada item ganha uma linha `- divergence`, `- amendment`, `- pending amendment`, `- no criterion` ou `- open gap` com os ids inteiros, e o `Next:` manda propor uma melhoria por item ao dono. Com `--format json` é uma linha JSON, `{"retro": ...}`, com `null` quando não há retro. Com retro, o objeto traz:

- `divergences`: os ids agrupados por `kind` e `resolution`.
- `amendments`: `approved` (emendas citadas por uma divergência `spec-stale`) e `pending` (arquivos de emenda que nenhuma divergência cita).
- `no_criterion`: os gaps respondidos sem uma diretiva que sustente a resposta (`gap` e `answered_by`).
- `open_gaps`: os gaps `open` e `needs-human`.

A retro traz só ids e nomes de arquivo, nunca o texto de uma divergência, spec ou emenda. Proponha as melhorias como descrito em [fechamento.md](fechamento.md).

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). A falta do `<WORK_ITEM>` é erro de uso do clap, com saída 2 e sem código. A pasta da spec e o `workflow.toml` só são lidos quando há retro. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `work-item-not-found` (saída 1): o work item não existe. O ponteiro é `/work_item`. Confira o id.
- `INVALID_INPUT` (saída 1): o id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `spec-store-unusable` (saída 1): há retro e a pasta da spec do work item não pôde ser lida, ou há mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `invalid-workflow-config` (saída 1): há retro e o `.omhl/workflow.toml` é inválido. Corrija o arquivo.
- `probe-failed` (saída 1): há retro e o git não respondeu (fora de um repositório, por exemplo). Rode dentro de um checkout. Sem retro o comando não lê o git.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Sem divergência nem gap respondido, não há retro:

```omhl-example fixture=project
$ omhl report retro work-items.w1
✔ report retro · none
$ omhl report retro work-items.w1 --format json
{"retro":null}
```

Com uma divergência aberta e um gap respondido por uma premissa sem diretiva que a sustente, a retro lista os dois (o gap sem critério traz a premissa que o respondeu):

```omhl-example fixture=git
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 3 records
- <record-id> · seq 3
- <record-id> · seq 4
- <record-id> · seq 5
$ omhl report retro work-items.w1
✔ report retro · 1 divergence · 1 no-criterion gap
- divergence · missing · open · divergences.limit
- no criterion · gaps.limit · answered by premises.p1
Next: propose one improvement per item to the owner and change nothing without the answer.
$ omhl report retro work-items.w1 --format json
{"retro":{"amendments":{"approved":[],"pending":[]},"divergences":[{"ids":["divergences.limit"],"kind":"missing","resolution":"open"}],"no_criterion":[{"answered_by":"premises.p1","gap":"gaps.limit"}],"open_gaps":[]}}
```

Com retro, fora de um repositório git, o comando recusa:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 3 records
- <record-id> · seq 3
- <record-id> · seq 4
- <record-id> · seq 5
$ omhl report retro work-items.w1
2> ✘ probe-failed · probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git
2>   - /snapshot · probe-failed
# exit 1
```

Recusa: o work item não existe e o id está mal formado:

```omhl-example fixture=project
$ omhl report retro work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl report retro Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
```
