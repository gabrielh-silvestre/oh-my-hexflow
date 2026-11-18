# omhl gate-worktree

## Quando usar

Antes de `wt switch -c <branch>`, para conferir se a worktree da branch pode nascer. O `wt` roda o mesmo comando num hook `pre-switch`; rodá-lo antes mostra a recusa e o que falta sem criar nada. Só depois de ele sair 0 rode o `wt switch -c`. Nunca contorne uma recusa com `--no-hooks`: o bypass é decisão do dono.

## Entradas

- `--branch <nome>`: o nome da branch, no formato `tipo/wi-<id>__<slug>` (`feat`, `fix`, `refactor`, `tests`, `bugfix`, `chore` ou `docs`). Obrigatório; aceita valor com hífen inicial e a forma `--branch=<valor>`.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- `--format json`: opção global, o único formato deste comando, que só imprime JSON. `agent` e `human` saem com erro de uso (saída 2), antes de o comando rodar.
- O que cada prefixo exige vem de `.omhl/workflow.toml` (a matriz está em [matrix.md](matrix.md)): `feat` e `fix` pedem uma premissa de negócio confirmada e uma spec válida; `refactor` e `tests` pedem uma premissa técnica confirmada; `bugfix` pede uma premissa do agente; `chore` e `docs` não pedem nada.

## Saída

Sem o marcador `.omhl.toml` acima do diretório, a CLI sai 0 e não imprime nada (o hook roda em qualquer repositório). Com o marcador, uma linha JSON no stdout, saída 0: `allowed: true`, `branch` e `reason`, que é `exempt-branch` (`develop` e `main`), `worktree-exists` (a branch local já tem worktree) ou `matrix-satisfied` (o prefixo foi satisfeito).

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta do `--branch` é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `worktree-gate-refused` (saída 1): a matriz do prefixo não foi satisfeita. O `context` traz `missing` (os átomos que faltam: `business-premise`, `spec`, `technical-premise-confirmed`, `agent-premise`), `prefix`, `work_item` e `main_dir`. Registre e confirme o que falta, ou escreva e valide a spec, e repita o gate. A violação da spec não sai aqui: rode `omhl spec validate`.
- `work-item-not-found` (saída 1): o prefixo abre o banco e o work item da branch não existe. O ponteiro é `/branch`. Grave o work item com `omhl append`, ou corrija o id da branch.
- `work-item-closed` (saída 1): o work item da branch já foi fechado. Use um work item aberto.
- `prefix-not-allowed` (saída 1): o nome não segue `tipo/wi-<id>__<slug>`, ou o `tipo` não está na tabela de prefixos. `pr:N` e `dependabot/...` caem aqui. Corrija o nome.
- `main-dir-unknown` (saída 1): o git não achou o diretório principal (repositório bare, por exemplo). Rode num checkout comum.
- `spec-store-unusable` (saída 1): o prefixo pede spec e a pasta da spec do work item não pode ser lida, ou há mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `invalid-workflow-config` (saída 1): o `.omhl/workflow.toml` é inválido. Corrija o arquivo.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

`develop` e `main` passam sem checar mais nada:

```omhl-example fixture=git
$ omhl gate-worktree --branch develop
{"allowed":true,"branch":"develop","reason":"exempt-branch"}
```

Uma branch que já tem worktree (a fixture está em `feat/wi-w1__x`):

```omhl-example fixture=git
$ omhl gate-worktree --branch feat/wi-w1__x
{"allowed":true,"branch":"feat/wi-w1__x","reason":"worktree-exists"}
```

Um prefixo sem exigência (`chore/`) não abre o banco e passa:

```omhl-example fixture=git
$ omhl gate-worktree --branch chore/wi-w1__tidy
{"allowed":true,"branch":"chore/wi-w1__tidy","reason":"matrix-satisfied"}
```

Com a spec criada e válida, a branch `feat/` nova passa:

```omhl-example fixture=spec
$ omhl gate-worktree --branch feat/wi-w1__y
{"allowed":true,"branch":"feat/wi-w1__y","reason":"matrix-satisfied"}
```

Recusa: `feat/` sem spec (a premissa de negócio da fixture está confirmada, falta só a spec):

```omhl-example fixture=git
$ omhl gate-worktree --branch feat/wi-w1__y
2> {"error":{"code":"worktree-gate-refused","details":[{"code":"worktree-gate-refused","context":{"branch":"feat/wi-w1__y","kind":"worktree-gate-refused","main_dir":"<repo>","missing":["spec"],"prefix":"feat","work_item":"work-items.w1"},"message":"the premises the branch prefix requires are missing","path":"/branch"}],"message":"the premises the branch prefix requires are missing"}}
# exit 1
```

Recusa: `refactor/` sem premissa técnica confirmada:

```omhl-example fixture=git
$ omhl gate-worktree --branch refactor/wi-w1__z
2> {"error":{"code":"worktree-gate-refused","details":[{"code":"worktree-gate-refused","context":{"branch":"refactor/wi-w1__z","kind":"worktree-gate-refused","main_dir":"<repo>","missing":["technical-premise-confirmed"],"prefix":"refactor","work_item":"work-items.w1"},"message":"the premises the branch prefix requires are missing","path":"/branch"}],"message":"the premises the branch prefix requires are missing"}}
# exit 1
```

Recusa: o work item da branch não existe:

```omhl-example fixture=git
$ omhl gate-worktree --branch feat/wi-zz__y
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/branch"}],"message":"work-item-not-found at /branch"}}
# exit 1
```

Recusa: nome fora do formato:

```omhl-example fixture=git
$ omhl gate-worktree --branch pr:12
2> {"error":{"code":"prefix-not-allowed","details":[{"code":"prefix-not-allowed","message":"the branch name must be <type>/wi-<id>__<slug> with a type listed in the workflow prefixes","path":"/branch"}],"message":"the branch name must be <type>/wi-<id>__<slug> with a type listed in the workflow prefixes"}}
# exit 1
```

Recusa: `--project-dir` que não existe sai 1, ao contrário da falta do marcador, que sai 0 calada:

```omhl-example fixture=git
$ omhl gate-worktree --branch develop --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
