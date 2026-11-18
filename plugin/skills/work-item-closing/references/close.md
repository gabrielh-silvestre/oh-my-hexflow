# omhl close

- [Quando usar](#quando-usar)
- [Entradas](#entradas)
- [Saída](#saída)
- [Erros](#erros)
- [Exemplos](#exemplos)

## Quando usar

Quando o trabalho do work item terminou e o gate de pull request já foi conferido com `omhl gate-pr`. O `close` roda o gate de novo, grava o fechamento no work item e devolve o resultado do gate junto com a retro. Rode-o com o PR pronto, não antes: o fechamento é gravado num banco append-only.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir e estar aberto.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).
- Precisa de um repositório git com a árvore limpa: o fechamento prende o resultado do gate ao `HEAD`.

## Saída

Uma linha JSON no stdout, saída 0, com três campos:

- `outcome`: o que foi gravado (`records`, `chain_head`, `replayed`). A gravação é uma revisão do work item com `status: closed` e, em `gate`, o nome do gate, `passed`, o `chain_head_seq` e o snapshot do commit.
- `report`: o mesmo relatório do `omhl gate-pr` (`gate`, `passed`, `questions`, `chain_head`).
- `retro`: `null` quando não há divergência nem gap respondido sem critério; senão, o relatório do `omhl report retro`. A retro lê os fatos antes de gravar, então o `close` nunca devolve erro depois de gravar.

**O `close` fecha mesmo com o gate reprovado** (`report.passed: false`): o resultado fica gravado no work item. Depois do fechamento, a premissa do work item sai do contexto de sessão (`omhl render`) e continua na projeção. Quando `retro` vem preenchido, proponha uma melhoria por item, como em [SKILL.md](../SKILL.md).

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta do `<WORK_ITEM>` é erro de uso do clap, com saída 2 e sem código. Quando há retro e a leitura dos fatos falha, o `close` recusa sem gravar nada. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `work-item-not-found` (saída 1): o work item não existe. O ponteiro é `/work_item`. Confira o id.
- `invalid-transition` (saída 1): o work item já está fechado. Um work item fechado não fecha de novo.
- `INVALID_INPUT` (saída 1): o id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `snapshot-dirty` (saída 1): a árvore tem mudança não commitada. Commite e repita.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório, por exemplo). Rode dentro de um checkout.
- `spec-store-unusable` (saída 1): há retro e a pasta da spec do work item não pôde ser lida, ou há mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `invalid-workflow-config` (saída 1): há retro e o `.omhl/workflow.toml` é inválido. Corrija o arquivo. Sem retro, o `close` grava mesmo com o arquivo inválido.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Fecha o work item e, em seguida, tenta fechá-lo de novo. O gate reprova (faltam as attestations), e mesmo assim o fechamento é gravado; a retro é `null`:

```omhl-example fixture=checks
$ omhl close work-items.w1
{"outcome":{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false},"report":{"chain_head":{"hash":"<hash>","seq":4},"gate":"pull-request","passed":false,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]},"retro":null}
$ omhl close work-items.w1
2> {"error":{"code":"invalid-transition","details":[{"code":"invalid-transition","message":"invalid-transition","path":"/work_item"}],"message":"invalid-transition at /work_item"}}
# exit 1
```

Com o gate verde (os dois nativos atestados e o check mecânico rodado), o `report.passed` é `true`:

```omhl-example fixture=checks
$ omhl attest checks.run-tests work-items.w1 --outcome passed
{"attestation":{"check":"checks.run-tests","id":"attestations.2dbf9e30bc28b8e377c513c63a9f8cae.a3ca65f3.checks.run-tests.work-items.w1","ok":true,"outcome":"passed","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false}
$ omhl attest checks.ci-green-before-merge work-items.w1 --outcome passed
{"attestation":{"check":"checks.ci-green-before-merge","id":"attestations.2dbf9e30bc28b8e377c513c63a9f8cae.a3ca65f3.checks.ci-green-before-merge.work-items.w1","ok":true,"outcome":"passed","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"chain_head":{"hash":"<hash>","seq":6},"records":[{"id":"<record-id>","seq":6}],"replayed":false}
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":"<ms>","exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl close work-items.w1
{"outcome":{"chain_head":{"hash":"<hash>","seq":8},"records":[{"id":"<record-id>","seq":8}],"replayed":false},"report":{"chain_head":{"hash":"<hash>","seq":7},"gate":"pull-request","passed":true,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"occurred","label":"checks.ok","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]},"retro":null}
```

Com uma divergência aberta, a retro vem preenchida (só ids, nunca o texto da divergência):

```omhl-example fixture=checks
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false}
$ omhl close work-items.w1
{"outcome":{"chain_head":{"hash":"<hash>","seq":6},"records":[{"id":"<record-id>","seq":6}],"replayed":false},"report":{"chain_head":{"hash":"<hash>","seq":5},"gate":"pull-request","passed":false,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]},"retro":{"amendments":{"approved":[],"pending":[]},"divergences":[{"ids":["divergences.limit"],"kind":"missing","resolution":"open"}],"no_criterion":[],"open_gaps":[]}}
```

Recusa: o work item não existe, o id está mal formado e o id é de uma premissa:

```omhl-example fixture=checks
$ omhl close work-items.zz
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
$ omhl close Bad_Id
2> {"error":{"code":"INVALID_INPUT","details":[{"code":"semantic-id-format","message":"invalid semantic id","path":"/work_item"}],"message":"invalid semantic id"}}
# exit 1
$ omhl close premises.p1
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
```

Recusa: fora de um repositório git (a fixture `project` não tem git):

```omhl-example fixture=project
$ omhl close work-items.w1
2> {"error":{"code":"probe-failed","details":[{"code":"probe-failed","message":"probe-failed","path":"/snapshot"}],"message":"probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git"}}
# exit 1
```
