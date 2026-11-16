# omhl check retire

## Quando usar

Para descartar de vez um check cujo arquivo foi apagado da árvore e cujo apagamento já foi mergeado. Sem isso o `omhl load` só lista o id em `orphans`, o `omhl check run` avisa `check-orphaned` a cada execução e o gate de pull request continua cobrando uma attestation que ninguém grava mais. **A retirada é definitiva** e vale para todos os clones e branches: o id nunca volta. Se a sua branch é anterior ao check (o arquivo ainda não existia nela), não aposente: rebaseie sobre a `main` ou restaure o arquivo.

## Entradas

- `<CHECK>`: o id do check, como `checks.ok`. Tem de ser um check vigente do projeto, com o arquivo `.omhl/checks/<slug>.toml` ausente da árvore.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.

## Saída

Uma linha JSON no stdout, saída 0: `{"retired":"<id>"}`. O banco é append-only: o comando não apaga nada, grava um registro de retirada (autor `loader`) e a cadeia segue válida. Depois, o check some do `check run`, do aviso e do gate de pull request. Um `omhl load` com o arquivo de volta reporta o id em `skipped` e não o recria; para trazer o check de volta, use outro nome de arquivo.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código.

- `check-not-orphaned` (saída 1): o arquivo do check ainda existe na árvore. Apague-o (e mergeie a remoção) antes, ou desista da retirada.
- `check-not-found` (saída 1): o id não é um check vigente do projeto (inclui o que já foi aposentado). Confira o id.
- `INVALID_INPUT` (saída 1): o id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Aposenta um check cujo arquivo foi apagado. A segunda chamada recusa, porque o check já não é vigente, e um id que nunca existiu recusa do mesmo jeito:

```omhl-example fixture=retire-check
$ omhl check retire checks.ok
{"retired":"checks.ok"}
$ omhl check retire checks.ok
2> {"error":{"code":"check-not-found","details":[{"code":"check-not-found","message":"check-not-found","path":"/check"}],"message":"check-not-found at /check"}}
# exit 1
$ omhl check retire checks.nope
2> {"error":{"code":"check-not-found","details":[{"code":"check-not-found","message":"check-not-found","path":"/check"}],"message":"check-not-found at /check"}}
# exit 1
```

Recusa: o arquivo do check ainda existe na árvore:

```omhl-example fixture=checks
$ omhl check retire checks.ok
2> {"error":{"code":"check-not-orphaned","details":[{"code":"check-not-orphaned","message":"the check file still exists in the working tree","path":"/check"}],"message":"the check file still exists in the working tree"}}
# exit 1
```

Recusa: id com forma inválida:

```omhl-example fixture=checks
$ omhl check retire BAD
2> {"error":{"code":"INVALID_INPUT","details":[{"code":"semantic-id-format","message":"invalid semantic id","path":"/check"}],"message":"invalid semantic id"}}
# exit 1
```
