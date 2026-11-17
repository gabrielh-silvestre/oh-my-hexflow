# omhl spec init

## Quando usar

Para criar a pasta da spec de um work item `feat/` ou `fix/`, com a primeira parte (`parte-01.md`) já com as cinco seções vazias. Rode antes de escrever a spec e antes de `omhl gate-worktree`: o gate desses prefixos exige uma spec válida. A pasta nasce no diretório principal do repositório, mesmo que você esteja numa worktree.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir e estar aberto.
- `<SLUG>`: o nome da pasta depois do id, de 1 a 64 caracteres de `a-z`, `0-9` e `-`, sem começar por `-`. A pasta será `wi-<id>__<slug>`.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- Precisa de um repositório git (o comando acha o diretório principal por ele).

## Saída

Uma linha JSON no stdout, saída 0: `dir` (a pasta criada), `path` (o `parte-01.md`), `slug` e `work_item`. O comando também escreve um `.gitignore` com `*` na pasta de specs: a spec não vai para o git.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta de um argumento é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `spec-slug-invalid` (saída 1): o slug não segue a regra. O ponteiro é `/slug`. Corrija o nome.
- `spec-exists` (saída 1): o work item já tem uma pasta de spec, de qualquer slug. Edite a existente; se a spec já foi aprovada, a correção é uma emenda.
- `work-item-not-found` (saída 1): o work item não existe. Grave-o com `omhl append` antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Use um work item aberto.
- `spec-store-unusable` (saída 1): a pasta de specs não pode ser lida ou escrita, passou de 10 000 entradas, ou o work item tem mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório). Rode dentro de um checkout.
- `invalid-workflow-config` (saída 1): o `.omhl/workflow.toml` é inválido. A mensagem traz o arquivo e a posição; corrija-o.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Cria a spec do work item `w1` (`<repo>` é a raiz do repositório):

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports
{"dir":"<repo>/.omhl/specs/wi-w1__export-reports","path":"<repo>/.omhl/specs/wi-w1__export-reports/parte-01.md","slug":"export-reports","work_item":"work-items.w1"}
```

Recusa: o work item já tem spec, mesmo com outro slug:

```omhl-example fixture=git
$ omhl spec init work-items.w1 export-reports
{"dir":"<repo>/.omhl/specs/wi-w1__export-reports","path":"<repo>/.omhl/specs/wi-w1__export-reports/parte-01.md","slug":"export-reports","work_item":"work-items.w1"}
$ omhl spec init work-items.w1 other
2> {"error":{"code":"spec-exists","details":[{"code":"spec-exists","message":"a spec folder of this work item already exists","path":"/work_item"}],"message":"a spec folder of this work item already exists"}}
# exit 1
```

Recusa: slug fora da regra:

```omhl-example fixture=git
$ omhl spec init work-items.w1 Bad_Slug
2> {"error":{"code":"spec-slug-invalid","details":[{"code":"spec-slug-invalid","message":"the slug must have 1 to 64 characters of a-z, 0-9 and -, and not start with -","path":"/slug"}],"message":"the slug must have 1 to 64 characters of a-z, 0-9 and -, and not start with -"}}
# exit 1
```

Recusa: work item que não existe:

```omhl-example fixture=git
$ omhl spec init work-items.zz x
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
```
