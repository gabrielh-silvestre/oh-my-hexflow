# omhl attest

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| session | review 8, review rules | `omhl attest` |

## Quando usar

Para gravar à mão o desfecho de um check **manual** (sem o campo `run`) sobre um work item, depois de o humano revisá-lo. O registro é do par check e work item, preso ao commit atual. Um check com `run` é mecânico: quem grava o desfecho dele é o `omhl check run`, nunca você (a recusa é `check-run-author`).

## Entradas

- `<CHECK>`: o id do check, como `checks.review`. O check precisa existir (rode `omhl load` depois de criar o arquivo em `.omhl/checks/`).
- `<WORK_ITEM>`: o id do work item, como `work-items.w1`. Precisa existir e estar aberto.
- `--outcome`: `passed`, `passed_with_warning` ou `failed`. Obrigatório.
- `--note <texto>`: observação livre, opcional.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.
- Precisa de um repositório git com a árvore limpa: o registro prende o desfecho ao `HEAD`.

## Saída

Texto no stdout, saída 0: `✔ attest · <check> · <work item> · <outcome> · snapshot <7 caracteres do commit>`, com `⚠` no lugar de `✔` quando o desfecho não é `passed`, e uma linha `note: ...` quando há `--note`. Com `--format json` é uma linha JSON: `attestation` (o `id` derivado do commit, o check, o work item, `outcome`, `ok` e `snapshot`, mais `note` quando houver), `chain_head` e `records`; o `id` da attestation só sai no JSON. Repetir o mesmo desfecho para o mesmo commit grava uma revisão e sai 0. Um desfecho diferente para o mesmo commit é recusado: o primeiro desfecho gravado é imutável.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). A falta de `--outcome` ou de um argumento é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `invalid-arguments` (saída 1): o `--outcome` não é um dos três valores. O ponteiro `/outcome` e a mensagem dizem a regra. Corrija o valor.
- `check-run-author` (saída 1): o check tem `run`. Só o `omhl check run` grava o desfecho dele. Use um check manual ou rode o check.
- `check-not-found` (saída 1): o check não existe no banco. Confira o id e rode `omhl load`.
- `work-item-not-found` (saída 1): o work item não existe. Grave-o antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Ateste num work item aberto.
- `outcome-immutable` (saída 1): o commit já tem outro desfecho gravado para o par. Faça um commit novo, ou aceite o desfecho gravado.
- `snapshot-dirty` (saída 1): a árvore tem mudança não commitada. Commite e repita.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório, por exemplo). Rode dentro de um checkout.
- `invalid-session-id` (saída 1): `CLAUDE_CODE_SESSION_ID` está definida mas vazia, não é UTF-8, passa de 100 unidades UTF-16 ou é igual ao marcador `outside-claude-code`; nada é gravado e o erro não mostra o valor (`path` vazio). Remova a variável ou rode o `omhl` numa sessão do Claude Code.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.
- `INVALID_INPUT` (saída 1): um id tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.

## Exemplos

Atesta um check manual. A segunda chamada tenta gravar outro desfecho para o mesmo commit e é recusada:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome passed --note 'Reviewed by hand.'
✔ attest · checks.review · work-items.w1 · passed · snapshot <snapshot>
  note: Reviewed by hand.
$ omhl attest checks.review work-items.w1 --outcome failed
2> ✘ outcome-immutable · outcome-immutable at /outcome
2>   - /outcome · outcome-immutable
# exit 1
```

Com `--format json` a resposta traz o `id` da attestation, o `snapshot` inteiro e a cabeça da cadeia:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome passed --format json
{"attestation":{"check":"checks.review","id":"attestations.2dbf9e30bc28b8e377c513c63a9f8cae.a3ca65f3.checks.review.work-items.w1","ok":true,"outcome":"passed","snapshot":{"id":"<commit>","kind":"git-commit"},"work_item":"work-items.w1"},"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":5}],"replayed":false}
```

Recusa: o check tem `run`, então é mecânico:

```omhl-example fixture=checks
$ omhl attest checks.ok work-items.w1 --outcome passed
2> ✘ check-run-author · check-run-author at /check
2>   - /check · check-run-author
# exit 1
```

Recusa: `--outcome` fora dos três valores:

```omhl-example fixture=checks
$ omhl attest checks.review work-items.w1 --outcome maybe
2> ✘ invalid-arguments · outcome must be passed, passed_with_warning or failed
2>   - /outcome
# exit 1
```

Recusa: check e work item que não existem:

```omhl-example fixture=checks
$ omhl attest checks.nope work-items.w1 --outcome passed
2> ✘ check-not-found · check-not-found at /check
2>   - /check · check-not-found
# exit 1
$ omhl attest checks.review work-items.zz --outcome passed
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```
