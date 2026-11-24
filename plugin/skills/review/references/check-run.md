# omhl check run

## Quem usa, em qual etapa e com qual filtro

| Papel | Etapa | Comando |
|---|---|---|
| sessao | review regras | `omhl check run` |
| sessao | review 8 | `omhl check run --event before-merge` |

## Quando usar

Para rodar os checks mecânicos (os que têm `run`) de um evento, ou um check só, e gravar a attestation de cada um para o work item da branch atual. Os hooks do git chamam o comando sozinhos (`--event before-commit` no `pre-commit`, `--event before-pr --pre-push` no `pre-push`). À mão, use `--event before-merge` para registrar o status do CI, ou `--check <id>` para repetir um check. Só roda no Linux. Se mexeu num check, rode `omhl load` antes.

## Entradas

- Exatamente um de `--event` (`before-commit`, `before-pr` ou `before-merge`) e `--check <id>` (por exemplo `checks.ok`). O `--check` precisa apontar para um check com `run`.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando; o formato da saída, `agent` por padrão (hoje `human` imprime o mesmo texto). Qualquer outro valor, inclusive `text`, é erro de uso do clap, com saída 2. Os hooks do git rodam sem `--format` e mostram o texto `agent`; para ler a saída por código, passe `--format json`.
- `--pre-push`: modo do hook `pre-push`, que lê as linhas do git no stdin. Só vale com `--event before-pr`.
- O work item vem do nome da branch: `tipo/wi-<id>__<slug>`. Na `develop` nada é gravado (`recorded` é `false`); na `main` o comando recusa.
- Precisa de um repositório git. No `before-pr` e no `before-merge` a árvore tem de estar limpa.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.

## Saída

O padrão é o texto `agent`, em inglês, sem cor nem sequência ANSI. Com `--format json` a saída é o JSON de uma linha, descrito mais abaixo. O código de saída é o mesmo nos dois formatos.

No sucesso, o texto sai no stdout, com saída 0 quando nenhum check bloqueante reprovou:

- A primeira linha traz o evento, o work item e se a attestation foi gravada: `✔ before-pr · work-items.w1 · recorded (git-commit 1a2b3c4)`. O trecho entre parênteses é o tipo do snapshot e os 7 primeiros caracteres do id (`git-tree` no `before-commit`, `git-commit` nos demais); some quando não há snapshot. Na `develop` a linha diz `not recorded`.
- Cada check tem uma linha, na ordem do JSON: o ícone (`✔` passou, `⚠` passou com aviso, `✘` reprovou), o id, `concern/variant` quando o check os declara, o desfecho e a duração (`12ms`). Notas entre parênteses marcam `non-blocking`, `exit N`, `timed out` e `truncated`.
- Cada aviso tem uma linha própria com a ação a tomar: `no-checks-for-event` (nenhum check planejado para o evento, o que não é uma aprovação), `check-orphaned` (o arquivo do check foi apagado, então ele não rodou) e `output-stored` (um check `passed_with_warning` gravado: o aviso traz `check`, `work_item` e o comando `omhl byproduct <check> <work-item> --tail 40` que lê a saída guardada; não sai em `passed`, em `failed` nem na `develop`, onde nada é gravado).
- A última linha diz que nada foi bloqueado e o próximo passo: `Nothing blocked. Next: go on with the push.` (ou o commit, ou o merge).
- No `--pre-push` sem nada a atestar (o push só apaga refs ou só envia tags), sai uma linha só: `✔ before-pr · nothing to attest in this push; no check ran`.

Na falha `checks-failed`, o texto sai no stderr, com saída 1:

- A primeira linha diz que o evento está bloqueado e quantos checks reprovaram, separando os bloqueantes dos não bloqueantes: `✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 1 non-blocking failed, 1 passed, 1 not run`. O trecho `non-blocking failed` some quando é zero. Na `develop` o work item vira `no work item`.
- Cada check reprovado traz uma linha com o id, `concern/variant` (se declarados), `blocking` ou `non-blocking`, `timeout Ns` (o limite do check), `exit N` ou `timed out`, a duração e `truncated` (se a saída foi cortada). A duração sai em `NNNms` abaixo de 1 segundo e em `N.Ns` a partir dele. Abaixo vêm o `command:` (o `run` do check) e o `how to read the output:` com o `omhl byproduct` exato. Na `develop` não há `byproduct`: o texto mostra a cauda da saída, e só ela.
- Quando mostra saída de check, o texto avisa que ela pode ter segredos e não deve ser copiada nem repetida ao usuário. Fecha com `Next:`.

Os demais erros saem no stderr, também em texto, com saída 1 (ou a saída de hoje): `✘ <código> · <mensagem>`, uma linha por detalhe (`- <código> at <caminho>`) e, quando o JSON os traz, o `kind`, o check, a causa, o `hint` e o próximo passo.

Com `--format json`, a saída é uma linha no stdout: `event`, `work_item`, `recorded`, `snapshot` (`git-tree` no `before-commit`, `git-commit` nos demais), `results` e `blocked`. Cada item de `results` traz `check`, `blocking`, `outcome` (`passed`, `passed_with_warning` ou `failed`), `ok`, `exit_code`, `duration_ms`, `timed_out` e `truncated`, mais `concern` e `variant` quando o check os declara. A saída completa do comando fica guardada; leia-a com `omhl byproduct`. No começo de cada execução o comando também apaga, em silêncio e no máximo uma vez por dia, as saídas guardadas com mais de 7 dias que nenhuma attestation viva usa ([byproduct-prune.md](byproduct-prune.md)). Um evento sem check no banco sai 0 com `checks_planned: 0` e o aviso `no-checks-for-event`. Um check cujo arquivo foi apagado da árvore vira aviso `check-orphaned` e não roda. O desfecho de um snapshot é imutável: repetir o comando na mesma árvore devolve o desfecho gravado.

## Erros

Um erro sai no stderr com saída 1: texto por padrão, uma linha JSON com `--format json`. Os códigos abaixo valem nos dois formatos. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Isso inclui faltar `--event` e `--check`, e usar `--pre-push` sem `--event before-pr`. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `checks-failed` (saída 1): um check bloqueante reprovou. Cada `details` com `failed` diz como ler a saída (`omhl byproduct <check> <work-item> --tail 40`). Na `develop` não há `byproduct`: a cauda vem em `context.output_tail`. No JSON, o `error.context` traz `event`, `work_item` (ausente na `develop`) e `summary` (`planned`, `passed`, `failed`, `blocking_failed` e `not_run`; `failed` conta todo reprovado, e `blocking_failed` só os bloqueantes), e o `context` de cada `details` com `failed` traz `blocking`, `concern` e `variant` (se declarados), `run`, `timeout_secs`, `exit_code` (ausente sem código), `duration_ms`, `timed_out` e `truncated`. Corrija o código e repita. A saída (inclusive `context.output_tail`) pode ter segredos: não a copie para PR, issue, commit, `--note` nem `omhl append`, e não a repita ao usuário (regra em [byproduct.md](byproduct.md)).
- `check-not-run` (saída 1): um problema impediu o veredito, e nada foi gravado para o check. Cada `details` traz o motivo e um `context.kind`: `spawn-failed` (o binário do `run` não existe no `PATH` do git: `context.kind` `spawn-not-found`; sem permissão para executá-lo: `spawn-permission-denied`), `spec-stale` (o TOML diverge do banco: rode `omhl load`), `spec-invalid`, `command-changed-tree` (o check alterou a árvore: mantenha-o somente leitura) ou `not-run` (o check restante não rodou). Siga o `hint` do contexto e repita.
- `attestation-refused` (saída 1): a gravação foi recusada, pela guarda ou pela sessão do ambiente. O detalhe `outcome-immutable` diz que o mesmo snapshot já tem outro desfecho: mude a árvore (um novo commit) para um veredito novo. Outros motivos: `snapshot-tree-mismatch` e `snapshot-dirty` (não faça stage durante o hook), `work-item-closed`, `probe-failed`.
- `invalid-session-id` (saída 1): `CLAUDE_CODE_SESSION_ID` está definida mas vazia, não é UTF-8, passa de 100 unidades UTF-16 ou é igual ao marcador `outside-claude-code`; nada é gravado e o erro não mostra o valor. Só aparece como detalhe de `attestation-refused`: o comando do primeiro check planejado já rodou, e os demais não rodam. Remova a variável ou rode o `omhl` numa sessão do Claude Code.
- `attestation-not-recorded` (saída 1): o banco ficou travado ou falhou ao gravar depois do comando. O veredito foi calculado e não gravado: repita o mesmo comando.
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

Roda o evento `before-pr` (um check passa), depois o `before-merge` (sem check mecânico: o aviso `no-checks-for-event`) e, por fim, um check pelo id. Sem `--format`, a saída é o texto `agent`:

```omhl-example fixture=checks
$ omhl check run --event before-pr
✔ before-pr · work-items.w1 · recorded (git-commit <snapshot>)
  ✔ checks.ok passed 12ms
Nothing blocked. Next: go on with the push.
$ omhl check run --event before-merge
✔ before-merge · work-items.w1 · recorded
⚠ no-checks-for-event: no check is planned for before-merge, so nothing ran and this is not a pass. Next: add a mechanical check for this event if one is expected.
Nothing blocked. Next: go on with the merge.
$ omhl check run --check checks.ok
✔ before-pr · work-items.w1 · recorded (git-commit <snapshot>)
  ✔ checks.ok passed 12ms
Nothing blocked. Next: go on with the push.
```

Os mesmos três comandos com `--format json`:

```omhl-example fixture=checks
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":12,"exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl check run --event before-merge --format json
{"blocked":[],"checks_planned":0,"event":"before-merge","recorded":true,"results":[],"snapshot":null,"warnings":[{"code":"no-checks-for-event"}],"work_item":"work-items.w1"}
$ omhl check run --check checks.ok --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":12,"exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
```

Recusas: check que não existe e check manual (sem `run`):

```omhl-example fixture=checks
$ omhl check run --check checks.nope --format json
2> {"error":{"code":"check-not-found","details":[{"code":"check-not-found","message":"check-not-found","path":"/check"}],"message":"check-not-found at /check"}}
# exit 1
$ omhl check run --check checks.review --format json
2> {"error":{"code":"check-not-mechanical","details":[{"code":"check-not-mechanical","message":"the check has no command","path":"/check"}],"message":"the check has no command"}}
# exit 1
```

Um check bloqueante que reprova: stdout vazio, saída 1 e o comando exato para ler a causa. O check desta fixture imprime `first` e `second` no stdout, `boom` no stderr e sai 1. O texto sai no stderr:

```omhl-example fixture=failing-check
$ omhl check run --event before-pr
2> ✘ BLOCKED · before-pr · work-items.w1 · 1 blocking failed, 0 passed, 0 not run
2>   ✘ checks.bad · blocking · timeout 60s · exit 1 · 12ms
2>     command: sh -c echo first; echo second; echo boom >&2; exit 1
2>     how to read the output: omhl byproduct checks.bad work-items.w1 --tail 40
2> The output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.
2> Next: read the output with the command above, fix the cause, then run the same command again.
# exit 1
```

O mesmo erro com `--format json`, com o resumo em `error.context` e os fatos do check no `context` do detalhe:

```omhl-example fixture=failing-check
$ omhl check run --event before-pr --format json
2> {"error":{"code":"checks-failed","context":{"event":"before-pr","summary":{"blocking_failed":1,"failed":1,"not_run":0,"passed":0,"planned":1},"work_item":"work-items.w1"},"details":[{"code":"failed","context":{"blocking":true,"duration_ms":12,"exit_code":1,"run":["sh","-c","echo first; echo second; echo boom >&2; exit 1"],"timed_out":false,"timeout_secs":60,"truncated":false},"message":"omhl byproduct checks.bad work-items.w1 --tail 40","path":"checks.bad"}],"message":"a blocking check failed"}}
# exit 1
```

Um check **não bloqueante** que reprova **não** falha o comando: sai 0, e só o `ok: false` do item de `results` mostra a reprovação. Leia `results`, não só o código de saída:

```omhl-example fixture=soft-check
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":false,"check":"checks.soft","duration_ms":12,"exit_code":1,"ok":false,"outcome":"failed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
```

Recusa: árvore suja no `before-pr` (a fixture deixa o arquivo `notes.txt` sem commit). Primeiro o texto, depois o JSON:

```omhl-example fixture=dirty-checks
$ omhl check run --event before-pr
2> ✘ worktree-dirty · the working tree has uncommitted changes
2>   - worktree-dirty
2>     the working tree has uncommitted changes
2> Next: follow the hint above if there is one, fix the cause, then run the same command again.
# exit 1
```

```omhl-example fixture=dirty-checks
$ omhl check run --event before-pr --format json
2> {"error":{"code":"worktree-dirty","details":[{"code":"worktree-dirty","context":{"branch":"feat/wi-w1__x","event":"before-pr","kind":"worktree-dirty","work_item":"work-items.w1"},"message":"the working tree has uncommitted changes","path":""}],"message":"the working tree has uncommitted changes"}}
# exit 1
```

Recusa: o binário do `run` não existe, então não há veredito e nada é gravado (`check-not-run`). Primeiro o texto, depois o JSON:

```omhl-example fixture=missing-binary
$ omhl check run --event before-pr
2> ✘ check-not-run · a check could not run to a verdict
2>   - spawn-failed at checks.ghost
2>     the command could not run
2>     kind: spawn-not-found
2>     check: checks.ghost
2>     cause: omhl-ghost-binary did not start: No such file or directory (os error 2)
2>     hint: `omhl-ghost-binary` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again
2> Next: follow the hint above if there is one, fix the cause, then run the same command again.
# exit 1
```

```omhl-example fixture=missing-binary
$ omhl check run --event before-pr --format json
2> {"error":{"code":"check-not-run","details":[{"code":"spawn-failed","context":{"argv":["omhl-ghost-binary"],"branch":"feat/wi-w1__x","check":"checks.ghost","cwd":"<repo>","event":"before-pr","hint":"`omhl-ghost-binary` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again","kind":"spawn-not-found","os_error":"omhl-ghost-binary did not start: No such file or directory (os error 2)","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"message":"the command could not run","path":"checks.ghost"}],"message":"a check could not run to a verdict"}}
# exit 1
```

Um `--pre-push` que só envia uma tag não tem nada a atestar: sai 0, nenhum check roda e nada é gravado. O stdin é o que o git entrega ao hook, uma linha por ref (ref local, sha local, ref remota, sha remoto):

```omhl-example fixture=checks
$ omhl check run --event before-pr --pre-push <<'EOF'
refs/tags/v1 1111111111111111111111111111111111111111 refs/tags/v1 0000000000000000000000000000000000000000
EOF
✔ before-pr · nothing to attest in this push; no check ran
```
