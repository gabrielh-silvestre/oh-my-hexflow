# omhl close

- [Quem usa](#quem-usa-em-qual-etapa-e-com-qual-filtro)
- [Quando usar](#quando-usar)
- [Entradas](#entradas)
- [Saída](#saída)
- [Erros](#erros)
- [Exemplos](#exemplos)

## Quem usa, em qual etapa e com qual filtro

| Papel | Etapa | Comando |
|---|---|---|
| sessao | review 8, review regras | `omhl close` |

## Quando usar

Quando o trabalho do work item terminou e o gate de pull request já foi conferido com `omhl gate-pr`. O `close` roda o gate de novo, grava o fechamento no work item e devolve o resultado do gate junto com a retro. Rode-o com o PR pronto, não antes: o fechamento é gravado num banco append-only.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir e estar aberto.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.
- Precisa de um repositório git com a árvore limpa: o fechamento prende o resultado do gate ao `HEAD`.

## Saída

Texto no stdout, saída 0, em até três blocos, nesta ordem:

- o gate: o cabeçalho `close · pull-request · passed|not passed · N questions` (`✔` com o gate aprovado, `⚠` com ele reprovado) e uma linha `✔` ou `✘` por pergunta, as mesmas do [gate-pr.md](gate-pr.md). O `next_step` de cada pergunta que reprova fica no campo abaixo da linha dela;
- o que foi gravado: `✔ closed · N records` e uma linha `- <id> · seq N` por record, como no `append`;
- a retro: `✔ retro · <contagens>` e uma linha por item, como no `omhl report retro`. O bloco só aparece quando há retro, e o `Next:` (propor uma melhoria por item ao dono) só quando a retro traz algum item.

Com `--format json` é uma linha JSON com três campos:

- `outcome`: o que foi gravado (`records`, `chain_head`, `replayed`). A gravação é uma revisão do work item com `status: closed` e, em `gate`, o nome do gate, `passed`, o `chain_head_seq` e o snapshot do commit.
- `report`: o mesmo relatório do `omhl gate-pr` (`gate`, `passed`, `questions`, `chain_head`).
- `retro`: `null` quando não há divergência nem gap respondido sem critério; senão, o relatório do `omhl report retro`. A retro lê os fatos antes de gravar, então o `close` nunca devolve erro depois de gravar.

**O `close` fecha mesmo com o gate reprovado** (`report.passed: false` no JSON, `⚠` no texto): o resultado fica gravado no work item. Depois do fechamento, a premissa do work item sai do contexto de sessão (`omhl render`) e continua na projeção. Quando `retro` vem preenchido, proponha uma melhoria por item, como em [fechamento.md](fechamento.md).

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). A falta do `<WORK_ITEM>` é erro de uso do clap, com saída 2 e sem código. Quando há retro e a leitura dos fatos falha, o `close` recusa sem gravar nada. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

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
⚠ close · pull-request · not passed · 7 questions · 4 failed
✘ checks.run-tests · occurred · matched 0
✘ checks.ci-green-before-merge · occurred · matched 0
✘ checks.ok · occurred · matched 0
✘ spec-digest-match:checks.ok · occurred · matched 0
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ closed · 1 record
- <record-id> · seq 5
$ omhl close work-items.w1
2> ✘ invalid-transition · invalid-transition at /work_item
2>   - /work_item · invalid-transition
# exit 1
```

O mesmo fechamento, em JSON:

```omhl-example fixture=checks
$ omhl close work-items.w1 --format json
{"outcome":{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false},"report":{"chain_head":{"hash":"<hash>","seq":4},"gate":"pull-request","passed":false,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]},"retro":null}
```

Com o gate verde (os dois nativos atestados e o check mecânico rodado), o `report.passed` é `true`:

```omhl-example fixture=checks
$ omhl attest checks.run-tests work-items.w1 --outcome passed
✔ attest · checks.run-tests · work-items.w1 · passed · snapshot <snapshot>
$ omhl attest checks.ci-green-before-merge work-items.w1 --outcome passed
✔ attest · checks.ci-green-before-merge · work-items.w1 · passed · snapshot <snapshot>
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":"<ms>","exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl close work-items.w1
✔ close · pull-request · passed · 7 questions
✔ checks.run-tests · occurred · matched 1
✔ checks.ci-green-before-merge · occurred · matched 1
✔ checks.ok · occurred · matched 1
✔ spec-digest-match:checks.ok · occurred · matched 1
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ closed · 1 record
- <record-id> · seq 8
```

Com uma divergência aberta, a retro vem preenchida (só ids, nunca o texto da divergência):

```omhl-example fixture=checks
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"open","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 5
$ omhl close work-items.w1
⚠ close · pull-request · not passed · 7 questions · 4 failed
✘ checks.run-tests · occurred · matched 0
✘ checks.ci-green-before-merge · occurred · matched 0
✘ checks.ok · occurred · matched 0
✘ spec-digest-match:checks.ok · occurred · matched 0
✔ no-open-contradiction · matched 0
✔ decisions-anchored · anchored · matched 0
✔ anchors-confirmed · anchored · matched 0
✔ closed · 1 record
- <record-id> · seq 6
✔ retro · 1 divergence
- divergence · missing · open · divergences.limit
Next: propose one improvement per item to the owner and change nothing without the answer.
```

Recusa: o work item não existe, o id está mal formado e o id é de uma premissa:

```omhl-example fixture=checks
$ omhl close work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl close Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
$ omhl close premises.p1
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```

Recusa: fora de um repositório git (a fixture `project` não tem git):

```omhl-example fixture=project
$ omhl close work-items.w1
2> ✘ probe-failed · probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git
2>   - /snapshot · probe-failed
# exit 1
```
