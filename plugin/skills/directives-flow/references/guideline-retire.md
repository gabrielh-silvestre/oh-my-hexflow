# omhl guideline retire

## Quando usar

Para retirar uma diretiva (`guideline`) cujo arquivo `.omhl/guidelines/<nome>.toml` foi apagado da árvore de trabalho. Ela sai do contexto da sessão e o próximo `omhl load` não a recria. A retirada é definitiva para todos os clones e branches, porque o banco é do usuário e é append-only: retire só depois de a remoção do arquivo estar mergeada, e peça ao humano antes. Numa branch anterior à diretiva, faça rebase em vez de retirar.

## Entradas

- `<ID>`: o id da diretiva, como `guidelines.brief`. Precisa ser uma diretiva vigente do repositório, não nativa, com o arquivo já apagado.
- `--by <id>`: a diretiva que substitui a retirada. Opcional; com ela o comando grava no mesmo pedido a relation `supersedes` (da nova para a antiga) e a retirada. A sucessora precisa ser vigente e diferente da retirada.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.

## Saída

Uma linha JSON no stdout, saída 0: `retired` (o id retirado) e `superseded_by` (o id de `--by`, ou `null`). Decisões que só se apoiavam nessa diretiva passam a reprovar `decisions-anchored` até ganharem outro apoio.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Cada recusa traz a causa em `message` e o próximo passo em `next_step`, sem repetir o valor de entrada. A falta do `<ID>` é erro de uso do clap, com saída 2 e sem código.

- `guideline-not-found` (saída 1): o id não é uma diretiva vigente do repositório (não existe, já foi retirada, é nativa ou é de outro tipo). Confira o id.
- `guideline-not-orphaned` (saída 1): o arquivo da diretiva ainda existe na árvore. Apague o arquivo, mergeie a remoção e repita.
- `successor-not-current` (saída 1): o id de `--by` não é uma diretiva vigente. Carregue a sucessora com `omhl load` antes, ou retire sem `--by`.
- `successor-is-self` (saída 1): o `--by` é a própria diretiva retirada. Passe outra, ou retire sem `--by`.
- `supersedes-cycle` (saída 1): a sucessora já é superada pela retirada, direto ou por outras. Escolha outra sucessora.
- `guideline-retire-by-command` (saída 1): a retirada veio pelo `omhl append`. Só este comando a grava.
- `INVALID_INPUT` (saída 1): um id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Retira a diretiva órfã. Repetir a retirada falha, porque ela já não é vigente:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief
{"retired":"guidelines.brief","superseded_by":null}
$ omhl guideline retire guidelines.brief
2> {"error":{"code":"guideline-not-found","details":[{"code":"guideline-not-found","message":"the id is not a current guideline stored in the repository","next_step":"pass the id of a current guideline that is not native","path":"/id"}],"message":"the id is not a current guideline stored in the repository"}}
# exit 1
```

Recusa: a sucessora é a própria diretiva:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --by guidelines.brief
2> {"error":{"code":"successor-is-self","details":[{"code":"successor-is-self","message":"a guideline cannot be its own successor","next_step":"pass a different guideline in --by, or retire without --by","path":"/by"}],"message":"a guideline cannot be its own successor"}}
# exit 1
```

Recusa: a sucessora não existe:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --by guidelines.new
2> {"error":{"code":"successor-not-current","details":[{"code":"successor-not-current","message":"the successor is not a current guideline","next_step":"pass the id of a current guideline in --by, or retire without --by","path":"/by"}],"message":"the successor is not a current guideline"}}
# exit 1
```

Recusa: a diretiva não existe:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.nope
2> {"error":{"code":"guideline-not-found","details":[{"code":"guideline-not-found","message":"the id is not a current guideline stored in the repository","next_step":"pass the id of a current guideline that is not native","path":"/id"}],"message":"the id is not a current guideline stored in the repository"}}
# exit 1
```

Recusa: `--project-dir` que não existe:

```omhl-example fixture=retire-guideline
$ omhl guideline retire guidelines.brief --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
