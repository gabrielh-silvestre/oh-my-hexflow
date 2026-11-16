# omhl hooks install

## Quando usar

Uma vez por clone, para o git chamar os checks sozinho: grava os quatro scripts (`pre-commit`, `pre-merge-commit`, `pre-push`, `post-commit`) numa pasta única do usuário e aponta o `core.hooksPath` do clone para ela. Sem isso o clone não tem hook, e o gate de pull request cobra a falta depois. São hooks do **git**, sem relação com o hook `omhl hook session-start` do Claude Code.

## Entradas

- `--force`: segue mesmo que o clone já tenha hooks próprios ou outro `core.hooksPath`. Você perde esses hooks (git-lfs e `commit-msg`, por exemplo); encadeá-los não é suportado.
- Precisa de um repositório git, versão 2.31 ou mais nova.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.

## Saída

Uma linha JSON no stdout, saída 0: `hooks_path` (a pasta dos scripts, em `$XDG_DATA_HOME/omhl/hooks/`), `omhl` (o caminho do binário gravado nos scripts), `scripts` e `warnings`. A pasta é única por usuário: instalar num clone reescreve os scripts de todos os outros, e o binário gravado é o da última instalação. Se o `omhl` que roda a instalação está numa pasta `target/debug` ou `target/release` (`binary-in-build-dir`), ou dentro de uma worktree ligada (`binary-in-worktree`), a instalação grava mesmo assim e o aviso diz o caminho. Esse binário some com a worktree: instale um binário fora de qualquer worktree (`cargo install` ou uma cópia em `~/.local/bin`) e rode o comando de novo. Reinstalar com a mesma pasta é idempotente.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código.

- `hooks-existing` (saída 1): `core.hooksPath` não está definido e `.git/hooks` tem algum arquivo que não seja `*.sample`. Mova os hooks ou use `--force`.
- `hooks-path-conflict` (saída 1): `core.hooksPath` já aponta para outra pasta, local ou global. Use `--force` ou aponte-o para a pasta do usuário.
- `hooks-path-unreachable` (saída 1): o git não resolve a pasta de hooks para a pasta instalada. Confira o `core.hooksPath`.
- `hooks-dir-unwritable` (saída 1): a pasta do usuário não pode ser criada nem escrita. Corrija a permissão.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório, ou git anterior à 2.31). Instale um git mais novo ou rode dentro de um clone.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Instala os hooks. O aviso aparece porque o binário do exemplo está em `target/debug`; repetir o comando é idempotente:

```omhl-example fixture=hooks
$ omhl hooks install
{"hooks_path":"<xdg>/omhl/hooks","omhl":"<omhl>","scripts":["pre-commit","pre-merge-commit","pre-push","post-commit"],"warnings":[{"code":"binary-in-build-dir","hint":"install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks","path":"<omhl>"}]}
$ omhl hooks install
{"hooks_path":"<xdg>/omhl/hooks","omhl":"<omhl>","scripts":["pre-commit","pre-merge-commit","pre-push","post-commit"],"warnings":[{"code":"binary-in-build-dir","hint":"install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks","path":"<omhl>"}]}
```

Recusa: `--project-dir` que não existe:

```omhl-example fixture=hooks
$ omhl hooks install --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
