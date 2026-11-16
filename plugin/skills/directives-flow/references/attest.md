# omhl attest

## Quando usar

Para gravar à mão o desfecho de um check **manual** (sem o campo `run`) sobre um work item, depois de o humano revisá-lo. É a mesma folha para a attestation de work item e de check: o registro é do par check e work item, preso ao commit atual. Um check com `run` é mecânico: quem grava o desfecho dele é o `omhl check run`, nunca você (a recusa é `check-run-author`).

## Entradas

- `<CHECK>`: o id do check, como `checks.review`. O check precisa existir (rode `omhl load` depois de criar o arquivo em `.omhl/checks/`).
- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir e estar aberto.
- `--outcome`: `passed`, `passed_with_warning` ou `failed`. Obrigatório.
- `--note <texto>`: observação livre, opcional.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- Precisa de um repositório git com a árvore limpa: o registro prende o desfecho ao `HEAD`.

## Saída

Uma linha JSON no stdout, saída 0: `attestation` (o `id` derivado do commit, o check, o work item, `outcome`, `ok` e `snapshot`, mais `note` quando houver), `chain_head` e `records`. Repetir o mesmo desfecho para o mesmo commit grava uma revisão e sai 0. Um desfecho diferente para o mesmo commit é recusado: o primeiro desfecho gravado é imutável.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta de `--outcome` ou de um argumento é erro de uso do clap, com saída 2 e sem código.

- `invalid-arguments` (saída 1): o `--outcome` não é um dos três valores. O ponteiro `/outcome` e a mensagem dizem a regra. Corrija o valor.
- `check-run-author` (saída 1): o check tem `run`. Só o `omhl check run` grava o desfecho dele. Use um check manual ou rode o check.
- `check-not-found` (saída 1): o check não existe no banco. Confira o id e rode `omhl load`.
- `work-item-not-found` (saída 1): o work item não existe. Grave-o antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Ateste num work item aberto.
- `outcome-immutable` (saída 1): o commit já tem outro desfecho gravado para o par. Faça um commit novo, ou aceite o desfecho gravado.
- `snapshot-dirty` (saída 1): a árvore tem mudança não commitada. Commite e repita.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório, por exemplo). Rode dentro de um checkout.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.
- `INVALID_INPUT` (saída 1): um id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.

## Exemplos

Atesta um check manual. A segunda chamada tenta gravar outro desfecho para o mesmo commit e é recusada:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome passed --note 'Reviewed by hand.'
{"attestation":{"check":"checks.review","id":"attestations.2dbf9e30bc28b8e377c513c63a9f8cae.a3ca65f3.checks.review.work-items.w1","note":"Reviewed by hand.","ok":true,"outcome":"passed","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false}
$ omhl attest checks.review work-items.w1 --outcome failed
2> {"error":{"code":"outcome-immutable","details":[{"code":"outcome-immutable","message":"outcome-immutable","path":"/outcome"}],"message":"outcome-immutable at /outcome"}}
# exit 1
```

Recusa: o check tem `run`, então é mecânico:

```omhl-example fixture=checks
$ omhl attest checks.ok work-items.w1 --outcome passed
2> {"error":{"code":"check-run-author","details":[{"code":"check-run-author","message":"check-run-author","path":"/check"}],"message":"check-run-author at /check"}}
# exit 1
```

Recusa: `--outcome` fora dos três valores (B3: a opção é recusada com a regra, não ajustada):

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome maybe
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"outcome must be passed, passed_with_warning or failed","path":"/outcome"}],"message":"outcome must be passed, passed_with_warning or failed"}}
# exit 1
```

Recusa: check e work item que não existem:

```omhl-example fixture=checks
$ omhl attest checks.nope work-items.w1 --outcome passed
2> {"error":{"code":"check-not-found","details":[{"code":"check-not-found","message":"check-not-found","path":"/check"}],"message":"check-not-found at /check"}}
# exit 1
$ omhl attest checks.review work-items.zz --outcome passed
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
```

Recusa: `--project-dir` que não existe:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome passed --project-dir /nonexistent
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
