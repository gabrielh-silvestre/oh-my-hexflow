# Erros comuns a todos os comandos

Vale para todo comando desta skill. A página de cada comando lista só os erros dele.

## Erros

Um erro sai no stderr com saída 1: em texto (`✘ código · mensagem`) nos comandos que falam `agent` e `human`, e numa linha JSON com `--format json` e nos cinco comandos lidos por máquina (`hook session-start`, `hook spec-lock`, `hook agent-role`, `hooks post-commit` e `gate-worktree`), que só imprimem JSON. Um erro de uso do clap (opção desconhecida, argumento que falta) sai com saída 2 e sem código.

A ordem das checagens é: uso do clap; argumentos e entrada do comando; existência do `--project-dir`; marcador; `--db`. O primeiro que falha é o que você vê.

- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.

## Exemplos

Recusa: `--project-dir` que não existe:

```omhl-example fixture=project
$ omhl list --project-dir /nonexistent
2> ✘ invalid-project-dir · the project directory does not exist
# exit 1
```
