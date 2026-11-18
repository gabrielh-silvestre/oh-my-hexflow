# omhl spec validate

## Quando usar

Para conferir a spec de um work item depois de escrevê-la, até passar, e de novo depois de escrever uma emenda. É o que o gate de criação da worktree cobra de `feat/` e `fix/`. Só leia o relatório: o comando nunca edita a spec.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).
- A pasta da spec (`.omhl/specs/wi-<id>__<slug>/`, no diretório principal) com as partes `parte-NN.md` e as emendas `emenda-NN.md`. Cada parte tem as cinco seções `##` na ordem: `Objetivo`, `Diretivas e premissas aplicáveis`, `Comportamentos` (itens `- CMP-###:`), `Critérios de aceitação` (itens `- CA-###:`) e `Fora de escopo`.
- O grafo no banco: os `premises.<nome>` e `guidelines.<nome>` citados precisam existir e estar confirmados, e a spec precisa citar uma premissa de negócio confirmada do work item.

## Saída

Uma linha JSON no stdout, saída 0, quando a spec passa: `ok: true`, `parts` (as partes lidas) e `amendments` (cada emenda com `name`, `state` e `violations`). Uma emenda ainda pendente com violação aparece em `amendments[].violations` e não derruba o `ok`.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. A falta do argumento é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `spec-invalid` (saída 1): a spec viola regras. Cada item de `details` traz o `code` da regra (`section-empty`, `sections-invalid`, `item-id-invalid`, `placeholder-found`, `cited-id-unconfirmed`, `business-premise-missing`, `pending-gap-unresolved`...) e o `path` no formato `<arquivo>:<linha>`. Até 20 detalhes aparecem; a mensagem conta os omitidos. O texto da spec nunca entra no erro. Corrija a linha apontada e rode de novo.
- `spec-not-found` (saída 1): o work item não tem pasta de spec com ao menos uma parte. Rode `omhl spec init`.
- `spec-store-unusable` (saída 1): a pasta não pode ser lida, tem arquivo que não é regular ou não é UTF-8, ou o work item tem mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório). Rode dentro de um checkout.
- `invalid-workflow-config` (saída 1): o `.omhl/workflow.toml` é inválido. Corrija o arquivo.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Spec válida, citando a premissa de negócio confirmada `premises.p1`:

```omhl-example fixture=spec
$ omhl spec validate work-items.w1
{"amendments":[],"ok":true,"parts":["parte-01.md"]}
```

Recusa: a parte recém-criada tem as cinco seções vazias e não cita premissa de negócio:

```omhl-example fixture=git
$ omhl spec init work-items.w1 x
{"dir":"<repo>/.omhl/specs/wi-w1__x","path":"<repo>/.omhl/specs/wi-w1__x/parte-01.md","slug":"x","work_item":"work-items.w1"}
$ omhl spec validate work-items.w1
2> {"error":{"code":"spec-invalid","details":[{"code":"section-empty","message":"the spec breaks the rule section-empty","path":"parte-01.md:1"},{"code":"section-empty","message":"the spec breaks the rule section-empty","path":"parte-01.md:3"},{"code":"business-premise-missing","message":"the spec breaks the rule business-premise-missing","path":"parte-01.md:3"},{"code":"section-empty","message":"the spec breaks the rule section-empty","path":"parte-01.md:5"},{"code":"section-empty","message":"the spec breaks the rule section-empty","path":"parte-01.md:7"},{"code":"section-empty","message":"the spec breaks the rule section-empty","path":"parte-01.md:9"}],"message":"the spec is invalid: 6 violation(s) shown, 0 omitted"}}
# exit 1
```

Recusa: o work item não tem pasta de spec:

```omhl-example fixture=git
$ omhl spec validate work-items.w1
2> {"error":{"code":"spec-not-found","details":[{"code":"spec-not-found","message":"the work item has no spec folder with a part","path":"/work_item"}],"message":"the work item has no spec folder with a part"}}
# exit 1
```
