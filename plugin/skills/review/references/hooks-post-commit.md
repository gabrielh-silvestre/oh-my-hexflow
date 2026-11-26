# omhl hooks post-commit

## Who uses it, at which stage and with which filter

No stage of the cycle runs it.

Quem o chama é o hook de git do clone, não uma etapa do ciclo.

## Quando usar

Quem chama é o hook `post-commit` que o `omhl hooks install` grava; você raramente o roda à mão. Na branch `develop`, depois de um commit, ele consome a marca da `develop` (o arquivo `omhl-develop-mark` na pasta do git), para o próximo commit direto exigir uma marca nova. Em outra branch ele não faz nada.

## Entradas

- Sem argumentos posicionais. Usa a branch e a pasta do git do diretório atual.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format json`: opção global, o único formato deste comando, que só imprime JSON. `agent` e `human` saem com erro de uso (saída 2), antes de o comando rodar.

## Saída

Uma linha JSON no stdout, saída 0: `consumed`, que é `true` quando a marca existia na `develop` e foi apagada, e `false` quando não havia marca ou a branch não é a `develop`. Chamar de novo depois de consumida devolve `false`: o comando é idempotente. O `omhl check run --event before-commit` e um commit abortado nunca gastam a marca.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `probe-failed` (saída 1): o git não respondeu (fora de um repositório). Rode dentro de um clone.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Consome a marca depois de um commit na `develop`. A segunda chamada já não encontra a marca:

```omhl-example fixture=develop-mark
$ omhl hooks post-commit
{"consumed":true}
$ omhl hooks post-commit
{"consumed":false}
```
