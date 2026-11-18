# omhl check retire

## Quando usar

Para descartar de vez um check cujo arquivo foi apagado da árvore e cujo apagamento já foi mergeado. Sem isso o `omhl load` só lista o id em `orphans`, o `omhl check run` avisa `check-orphaned` a cada execução e o gate de pull request continua cobrando uma attestation que ninguém grava mais. **A retirada é definitiva** e vale para todos os clones e branches: o id nunca volta. Se a sua branch é anterior ao check (o arquivo ainda não existia nela), não aposente: rebaseie sobre a `main` ou restaure o arquivo.

## Entradas

- `<CHECK>`: o id do check, como `checks.ok`. Tem de ser um check vigente do projeto, com o arquivo `.omhl/checks/<slug>.toml` ausente da árvore.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ check retire · retired <id>`. Com `--format json` é uma linha JSON, `{"retired":"<id>"}`. O banco é append-only: o comando não apaga nada, grava um registro de retirada (autor `loader`) e a cadeia segue válida. Depois, o check some do `check run`, do aviso e do gate de pull request. Um `omhl load` com o arquivo de volta reporta o id em `skipped` e não o recria; para trazer o check de volta, use outro nome de arquivo.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `check-not-orphaned` (saída 1): o arquivo do check ainda existe na árvore. Apague-o (e mergeie a remoção) antes, ou desista da retirada.
- `check-not-found` (saída 1): o id não é um check vigente do projeto (inclui o que já foi aposentado). Confira o id.
- `INVALID_INPUT` (saída 1): o id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Aposenta um check cujo arquivo foi apagado. A segunda chamada recusa, porque o check já não é vigente, e um id que nunca existiu recusa do mesmo jeito:

```omhl-example fixture=retire-check
$ omhl check retire checks.ok
✔ check retire · retired checks.ok
$ omhl check retire checks.ok
2> ✘ check-not-found · check-not-found at /check
2>   - /check · check-not-found
# exit 1
$ omhl check retire checks.nope
2> ✘ check-not-found · check-not-found at /check
2>   - /check · check-not-found
# exit 1
```

Com `--format json`, a retirada sai para máquinas:

```omhl-example fixture=retire-check
$ omhl check retire checks.ok --format json
{"retired":"checks.ok"}
```

Recusa: o arquivo do check ainda existe na árvore:

```omhl-example fixture=checks
$ omhl check retire checks.ok
2> ✘ check-not-orphaned · the check file still exists in the working tree
2>   - /check
# exit 1
```

Recusa: id com forma inválida:

```omhl-example fixture=checks
$ omhl check retire BAD
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /check · semantic-id-format
# exit 1
```
