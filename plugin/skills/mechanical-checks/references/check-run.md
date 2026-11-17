# omhl check run

## Quando usar

Para rodar os checks mecânicos (os que têm `run`) de um evento, ou um check só, e gravar a attestation de cada um para o work item da branch atual. Os hooks do git chamam o comando sozinhos (`--event before-commit` no `pre-commit`, `--event before-pr --pre-push` no `pre-push`). À mão, use `--event before-merge` para registrar o status do CI, ou `--check <id>` para repetir um check. Só roda no Linux. Se mexeu num check, rode `omhl load` antes.

## Entradas

- Exatamente um de `--event` (`before-commit`, `before-pr` ou `before-merge`) e `--check <id>` (por exemplo `checks.ok`). O `--check` precisa apontar para um check com `run`.
- `--pre-push`: modo do hook `pre-push`, que lê as linhas do git no stdin. Só vale com `--event before-pr`.
- O work item vem do nome da branch: `tipo/wi-<id>__<slug>`. Na `develop` nada é gravado (`recorded` é `false`); na `main` o comando recusa.
- Precisa de um repositório git. No `before-pr` e no `before-merge` a árvore tem de estar limpa.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.

## Saída

Uma linha JSON no stdout, saída 0 quando nenhum check bloqueante reprovou: `event`, `work_item`, `recorded`, `snapshot` (`git-tree` no `before-commit`, `git-commit` nos demais), `results` e `blocked`. Cada item de `results` traz `check`, `blocking`, `outcome` (`passed`, `passed_with_warning` ou `failed`), `ok`, `exit_code`, `duration_ms`, `timed_out` e `truncated`. A saída completa do comando fica guardada; leia-a com `omhl byproduct`. Um evento sem check no banco sai 0 com `checks_planned: 0` e o aviso `no-checks-for-event`. Um check cujo arquivo foi apagado da árvore vira aviso `check-orphaned` e não roda. O desfecho de um snapshot é imutável: repetir o comando na mesma árvore devolve o desfecho gravado.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Isso inclui faltar `--event` e `--check`, e usar `--pre-push` sem `--event before-pr`. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `checks-failed`: um check bloqueante reprovou. Cada `details` com `failed` diz como ler a saída (`omhl byproduct <check> <work-item> --tail 40`). Na `develop` não há `byproduct`: a cauda vem em `context.output_tail`. Corrija o código e repita. A saída (inclusive `context.output_tail`) pode ter segredos: não a copie para PR, issue, commit, `--note` nem `omhl append`, e não a repita ao usuário (regra em [byproduct.md](byproduct.md)).
- `check-not-run`: um problema impediu o veredito, e nada foi gravado para o check. Cada `details` traz o motivo e um `context.kind`: `spawn-failed` (o binário do `run` não existe no `PATH` do git: `context.kind` `spawn-not-found`; sem permissão para executá-lo: `spawn-permission-denied`), `spec-stale` (o TOML diverge do banco: rode `omhl load`), `spec-invalid`, `command-changed-tree` (o check alterou a árvore: mantenha-o somente leitura) ou `not-run` (o check restante não rodou). Siga o `hint` do contexto e repita.
- `attestation-refused`: a guarda recusou a gravação. O detalhe `outcome-immutable` diz que o mesmo snapshot já tem outro desfecho: mude a árvore (um novo commit) para um veredito novo. Outros motivos: `snapshot-tree-mismatch` e `snapshot-dirty` (não faça stage durante o hook), `work-item-closed`, `probe-failed`.
- `attestation-not-recorded`: o banco ficou travado ou falhou ao gravar depois do comando. O veredito foi calculado e não gravado: repita o mesmo comando.
- `check-not-found` (saída 1): o `--check` não existe no banco. Confira o id e rode `omhl load`.
- `check-not-mechanical` (saída 1): o `--check` aponta para um check manual, sem `run`. Quem grava o desfecho dele é `omhl attest`.
- `unsupported-platform`: o sistema não é Linux. Rode no Linux ou no WSL.
- `probe-failed`: o git não respondeu (fora de um repositório, git anterior à 2.31). Rode `git status` e corrija o estado.
- `branch-name-invalid`: a branch não segue `tipo/wi-<id>__<slug>`. Renomeie com `git branch -m`.
- `branch-protected`: a branch é a `main`, ou o push tem a `main` como destino. Crie uma branch de work item e abra um pull request.
- `head-ambiguous`: o `HEAD` está solto e zero ou várias branches o contêm. Volte para a sua branch com `git switch`.
- `work-item-not-found` (saída 1): o work item da branch não existe. Grave-o antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Use outra branch.
- `develop-mark-missing`: `before-commit` na `develop` sem a marca. Crie a marca com o command `/oh-my-hexflow:omhl-develop-mark`.
- `worktree-differs-from-index`: no `before-commit`, a árvore difere do índice. Faça o stage, ignore ou apague o arquivo.
- `worktree-dirty`: no `before-pr` ou no `before-merge`, a árvore está suja. Faça commit, ignore ou apague o arquivo.
- `push-ref-not-checked-out` e `push-input-invalid`: problemas do `--pre-push`. O primeiro aparece quando o `lsha` de uma ref não é o `HEAD` do checkout, o segundo quando uma linha do git não tem quatro campos. Empurre só a branch do checkout.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Roda o evento `before-pr` (um check passa), depois o `before-merge` (sem check mecânico: o aviso `no-checks-for-event`) e, por fim, um check pelo id:

```omhl-example fixture=checks
$ omhl check run --event before-pr
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":12,"exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl check run --event before-merge
{"blocked":[],"checks_planned":0,"event":"before-merge","recorded":true,"results":[],"snapshot":null,"warnings":[{"code":"no-checks-for-event"}],"work_item":"work-items.w1"}
$ omhl check run --check checks.ok
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":12,"exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
```

Recusas: check que não existe e check manual (sem `run`):

```omhl-example fixture=checks
$ omhl check run --check checks.nope
2> {"error":{"code":"check-not-found","details":[{"code":"check-not-found","message":"check-not-found","path":"/check"}],"message":"check-not-found at /check"}}
# exit 1
$ omhl check run --check checks.review
2> {"error":{"code":"check-not-mechanical","details":[{"code":"check-not-mechanical","message":"the check has no command","path":"/check"}],"message":"the check has no command"}}
# exit 1
```

Um check bloqueante que reprova: stdout vazio, saída 1 e o comando exato para ler a causa. O check desta fixture imprime `first` e `second` no stdout, `boom` no stderr e sai 1:

```omhl-example fixture=failing-check
$ omhl check run --event before-pr
2> {"error":{"code":"checks-failed","details":[{"code":"failed","message":"omhl byproduct checks.bad work-items.w1 --tail 40","path":"checks.bad"}],"message":"a blocking check failed"}}
# exit 1
```

Um check **não bloqueante** que reprova **não** falha o comando: sai 0, e só o `ok: false` do item de `results` mostra a reprovação. Leia `results`, não só o código de saída:

```omhl-example fixture=soft-check
$ omhl check run --event before-pr
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":false,"check":"checks.soft","duration_ms":12,"exit_code":1,"ok":false,"outcome":"failed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
```

Recusa: árvore suja no `before-pr` (a fixture deixa o arquivo `notes.txt` sem commit):

```omhl-example fixture=dirty-checks
$ omhl check run --event before-pr
2> {"error":{"code":"worktree-dirty","details":[{"code":"worktree-dirty","context":{"branch":"feat/wi-w1__x","event":"before-pr","kind":"worktree-dirty","work_item":"work-items.w1"},"message":"the working tree has uncommitted changes","path":""}],"message":"the working tree has uncommitted changes"}}
# exit 1
```

Recusa: o binário do `run` não existe, então não há veredito e nada é gravado (`check-not-run`):

```omhl-example fixture=missing-binary
$ omhl check run --event before-pr
2> {"error":{"code":"check-not-run","details":[{"code":"spawn-failed","context":{"argv":["omhl-ghost-binary"],"branch":"feat/wi-w1__x","check":"checks.ghost","cwd":"<repo>","event":"before-pr","hint":"`omhl-ghost-binary` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again","kind":"spawn-not-found","os_error":"omhl-ghost-binary did not start: No such file or directory (os error 2)","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"message":"the command could not run","path":"checks.ghost"}],"message":"a check could not run to a verdict"}}
# exit 1
```
