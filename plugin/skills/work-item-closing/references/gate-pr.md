# omhl gate-pr

## Quando usar

Antes de abrir ou mergear o pull request de um work item, para saber se tudo o que o gate cobra já está registrado. É somente leitura: não grava nada e vale para work item aberto ou fechado. O gate cobra o **commit atual** (`HEAD`): depois de qualquer commit novo, os checks precisam ser atestados de novo. O `omhl close` roda esse mesmo gate e grava o resultado.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).
- Precisa de um repositório git com a árvore limpa: o gate lê o `HEAD` e a árvore dele.

## Saída

Uma linha JSON no stdout, saída 0, **mesmo com o gate reprovado**: leia o campo `passed`. Os campos são `gate` (`pull-request`), `passed`, `questions` e `chain_head`. Cada pergunta traz `label`, `kind`, `passed`, `matched` (quantos registros a respondem), `failing` (os ids que reprovam), `omitted` (quantos o relatório deixou de listar) e, quando reprova uma pergunta de âncora, `next_step`. O gate passa quando todas as perguntas passam, na ordem:

- uma pergunta `occurred` por check obrigatório: precisa existir uma attestation com `ok: true` para o par check e work item, presa ao `HEAD` (à árvore do `HEAD`, para um check com `snapshot` de árvore). Os obrigatórios são os checks nativos `checks.run-tests` (`before-commit`) e `checks.ci-green-before-merge` (`before-merge`), mais os checks do projeto com `blocking = true` e evento `before-commit`, `before-pr` ou `before-merge`. Um check mecânico do projeto no mesmo evento substitui o nativo daquele evento;
- para um check com `run`, a pergunta `spec-digest-match:<check>`: a attestation precisa ter sido gravada com a definição atual do check (se você editou o TOML depois do `check run`, rode `omhl load` e o check de novo);
- `no-open-contradiction`: nenhuma decisão do work item contradiz outra;
- `decisions-anchored` e `anchors-confirmed`: toda decisão do work item se apoia (`rests-on`) em uma diretiva ou premissa confirmada.

Um check que reprovou no `check run` não conta: a pergunta dele sai com `matched: 0`, como se nunca tivesse rodado; a causa está em `omhl byproduct`. Um check **manual** (sem `run`) recebe o desfecho de `omhl attest`; um check com `run`, de `omhl check run`. Um check de `blocking = false` não entra no gate.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta do `<WORK_ITEM>` é erro de uso do clap, com saída 2 e sem código. Um gate reprovado **não** é erro. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `work-item-not-found` (saída 1): o work item não existe (ou o id é de outro tipo de node). O ponteiro é `/work_item`. Confira o id.
- `INVALID_INPUT` (saída 1): o id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `snapshot-dirty` (saída 1): a árvore tem mudança não commitada. Commite e repita.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório, por exemplo). Rode dentro de um checkout.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Sem nenhuma attestation, o gate reprova. Cada check obrigatório aparece com `matched: 0`:

```omhl-example fixture=checks
$ omhl gate-pr work-items.w1
{"chain_head":{"hash":"<hash>","seq":4},"gate":"pull-request","passed":false,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":0,"omitted":0,"passed":false},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]}
```

Com os dois checks nativos atestados à mão e o check mecânico rodado, no mesmo commit, o gate passa:

```omhl-example fixture=checks
$ omhl attest checks.run-tests work-items.w1 --outcome passed
✔ attest · checks.run-tests · work-items.w1 · passed · snapshot <snapshot>
$ omhl attest checks.ci-green-before-merge work-items.w1 --outcome passed
✔ attest · checks.ci-green-before-merge · work-items.w1 · passed · snapshot <snapshot>
$ omhl check run --event before-pr --format json
{"blocked":[],"event":"before-pr","recorded":true,"results":[{"blocking":true,"check":"checks.ok","duration_ms":"<ms>","exit_code":0,"ok":true,"outcome":"passed","timed_out":false,"truncated":false}],"snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"}
$ omhl gate-pr work-items.w1
{"chain_head":{"hash":"<hash>","seq":7},"gate":"pull-request","passed":true,"questions":[{"failing":[],"kind":"occurred","label":"checks.run-tests","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"occurred","label":"checks.ci-green-before-merge","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"occurred","label":"checks.ok","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"occurred","label":"spec-digest-match:checks.ok","matched":1,"omitted":0,"passed":true},{"failing":[],"kind":"no-open-contradiction","label":"no-open-contradiction","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"decisions-anchored","matched":0,"omitted":0,"passed":true},{"failing":[],"kind":"anchored","label":"anchors-confirmed","matched":0,"omitted":0,"passed":true}]}
```

Recusa: o work item não existe e o id tem forma inválida:

```omhl-example fixture=checks
$ omhl gate-pr work-items.zz
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
$ omhl gate-pr Bad_Id
2> {"error":{"code":"INVALID_INPUT","details":[{"code":"semantic-id-format","message":"invalid semantic id","path":"/work_item"}],"message":"invalid semantic id"}}
# exit 1
```

Recusa: árvore com mudança não commitada (a fixture deixa o arquivo `notes.txt` sem commit):

```omhl-example fixture=dirty-checks
$ omhl gate-pr work-items.w1
2> {"error":{"code":"snapshot-dirty","details":[{"code":"snapshot-dirty","message":"snapshot-dirty","path":"/snapshot"}],"message":"snapshot-dirty at /snapshot"}}
# exit 1
```

Recusa: fora de um repositório git (a fixture `project` não tem git):

```omhl-example fixture=project
$ omhl gate-pr work-items.w1
2> {"error":{"code":"probe-failed","details":[{"code":"probe-failed","message":"probe-failed","path":"/snapshot"}],"message":"probe-failed at /snapshot: fatal: not a git repository (or any of the parent directories): .git"}}
# exit 1
```
