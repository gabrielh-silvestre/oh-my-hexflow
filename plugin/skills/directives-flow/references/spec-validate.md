# omhl spec validate

## Quando usar

Para conferir a spec de um work item depois de escrevê-la, até passar, e de novo depois de escrever uma emenda. É o que o gate de criação da worktree cobra de `feat/` e `fix/`. Só leia o relatório: o comando nunca edita a spec.

## Entradas

- `<WORK_ITEM>`: o id do work item, como `work-items.w1`.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. O `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.
- A pasta da spec (`.omhl/specs/wi-<id>__<slug>/`, no diretório principal) com as partes `parte-NN.md` e as emendas `emenda-NN.md`. Cada parte tem as cinco seções `##` na ordem: `Objetivo`, `Diretivas e premissas aplicáveis`, `Comportamentos` (itens `- CMP-###:`), `Critérios de aceitação` (itens `- CA-###:`) e `Fora de escopo`.
- O grafo no banco: os `premises.<nome>` e `guidelines.<nome>` citados precisam existir e estar confirmados, e a spec precisa citar uma premissa de negócio confirmada do work item.

## Saída

Texto no stdout, saída 0, quando a spec passa: `✔ spec validate · <N> parts · <N> amendments`, uma linha `part` por parte lida e uma linha `amendment` por emenda, com o `state` (`approved` ou `pending`). Uma emenda ainda pendente com violação aparece com uma linha `<arquivo>:<linha> · <regra>` abaixo dela, o cabeçalho passa a `⚠` e a saída segue 0. Com `--format json` é uma linha JSON: `ok: true`, `parts` (as partes lidas) e `amendments` (cada emenda com `name`, `state` e `violations`).

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). A falta do argumento é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `spec-invalid` (saída 1): a spec viola regras. Cada item de `details` traz o `code` da regra (`section-empty`, `sections-invalid`, `item-id-invalid`, `placeholder-found`, `cited-id-unconfirmed`, `business-premise-missing`, `main-business-premise-missing`, `pending-gap-unresolved`...) e o `path` no formato `<arquivo>:<linha>`. Até 20 detalhes aparecem; a mensagem conta os omitidos. O texto da spec nunca entra no erro. Corrija a linha apontada e rode de novo. `business-premise-missing` diz que a spec não cita premissa de negócio confirmada do work item; `main-business-premise-missing` diz que as citadas são todas tardias (`origin: late`), e a spec precisa citar ao menos uma principal.
- `spec-not-found` (saída 1): o work item não tem pasta de spec com ao menos uma parte. Rode `omhl spec init`.
- `spec-store-unusable` (saída 1): a pasta não pode ser lida, tem arquivo que não é regular ou não é UTF-8, ou o work item tem mais de uma pasta `wi-<id>__*`. Corrija a pasta no diretório principal.
- `probe-failed` (saída 1): o git não respondeu (fora de um repositório). Rode dentro de um checkout.
- `invalid-workflow-config` (saída 1): o `.omhl/workflow.toml` é inválido. Corrija o arquivo.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Spec válida, citando a premissa de negócio confirmada `premises.p1`:

```omhl-example fixture=spec
$ omhl spec validate work-items.w1
✔ spec validate · 1 part · 0 amendments
- part · parte-01.md
```

Com `--format json` o relatório sai para máquinas:

```omhl-example fixture=spec
$ omhl spec validate work-items.w1 --format json
{"amendments":[],"ok":true,"parts":["parte-01.md"]}
```

Recusa: a parte recém-criada tem as cinco seções vazias e não cita premissa de negócio:

```omhl-example fixture=git
$ omhl spec init work-items.w1 x
✔ spec init · work-items.w1 · x
  dir: <repo>/.omhl/specs/wi-w1__x
  path: <repo>/.omhl/specs/wi-w1__x/parte-01.md
Next: fill in the five sections of the part file, then run omhl spec validate work-items.w1.
$ omhl spec validate work-items.w1
2> ✘ spec-invalid · the spec is invalid: 6 violation(s) shown, 0 omitted
2>   - parte-01.md:1 · section-empty · the spec breaks the rule section-empty
2>   - parte-01.md:3 · section-empty · the spec breaks the rule section-empty
2>   - parte-01.md:3 · business-premise-missing · the spec breaks the rule business-premise-missing
2>   - parte-01.md:5 · section-empty · the spec breaks the rule section-empty
2>   - parte-01.md:7 · section-empty · the spec breaks the rule section-empty
2>   - parte-01.md:9 · section-empty · the spec breaks the rule section-empty
# exit 1
```

Recusa: o work item não tem pasta de spec:

```omhl-example fixture=git
$ omhl spec validate work-items.w1
2> ✘ spec-not-found · the work item has no spec folder with a part
2>   - /work_item
# exit 1
```
