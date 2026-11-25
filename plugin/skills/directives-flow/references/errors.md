# Erros comuns a todos os comandos

Vale para todo comando do `omhl`. A página de cada comando lista só os erros dele.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`); com `--format json`, numa linha JSON. Os comandos lidos por máquina (`hook session-start`, `hook spec-lock`, `hook agent-role`, `hooks post-commit` e `gate-worktree`) só imprimem JSON, também no erro. Um erro de uso do clap (opção desconhecida, argumento que falta) sai com saída 2 e sem código.

A ordem das checagens é: uso do clap; argumentos e entrada do comando; `OMHL_ROLE`; existência do `--project-dir`; marcador; `--db`. O primeiro que falha é o que você vê.

- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.
- `role-unknown` (saída 1): o `OMHL_ROLE` tem um valor fora dos sete papéis, ou foi definido fora do Claude Code. Corrija ou remova a variável.
- `invalid-session-id` (saída 1): `CLAUDE_CODE_SESSION_ID` está definida mas vazia, não é UTF-8, passa de 100 unidades UTF-16 ou é igual ao marcador `outside-claude-code`; nada é gravado e o erro não mostra o valor (`path` vazio). Remova a variável ou rode o `omhl` numa sessão do Claude Code.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.

## Exemplos

Recusa: `--project-dir` que não existe:

```omhl-example fixture=project
$ omhl append --project-dir /nonexistent <<'EOF'
{"items":[]}
EOF
2> ✘ invalid-project-dir · the project directory does not exist
# exit 1
```
