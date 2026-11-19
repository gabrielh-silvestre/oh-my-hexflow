---
name: work-item-closing
description: Protocolo de fechamento do work item no omhl. Use para conferir o gate do pull request (omhl gate-pr) antes de abrir ou mergear o PR, para fechar o work item (omhl close), para ler a retro (omhl report retro) e, depois de um close que imprimiu a retro, para propor as melhorias que ela sugere, com o que cada comando grava e devolve.
---

# Fechamento do work item

A ordem, antes de abrir ou mergear o PR:

1. Confira que os checks do commit atual estão atestados: `omhl check run` (os hooks já rodam `before-commit` e `before-pr`; o `before-merge` é à mão) e `omhl attest` para os checks manuais. Um commit novo pede tudo de novo.
2. `omhl gate-pr <work-item>` e resolva cada pergunta reprovada, a linha com `✘` (com `--format json`, `passed: false`; inclui `decisions-anchored`, ver `directives-flow`).
3. Abra e mergeie o PR (com `wt`). `omhl close <work-item>` com o PR pronto, não antes.
4. `omhl report retro` e proponha melhorias, como em "Depois do `close`" abaixo.

A referência de cada comando está em `references/`.

## Regras que valem o tempo todo

- **Árvore limpa e commit atual.** `omhl gate-pr` e `omhl close` leem o `HEAD` e recusam com `snapshot-dirty` se há mudança não commitada. Uma attestation vale só para o commit em que foi gravada: um commit novo pede os checks de novo (`omhl check run`, `omhl attest`).
- **Gate reprovado não é erro.** O `gate-pr` sai 0 mesmo reprovado. Leia o símbolo e a contagem da primeira linha (`✘ gate-pr · ... · not passed · N questions · M failed`; com `--format json`, o campo `passed`) e resolva cada pergunta reprovada, a linha com `✘`, antes de abrir o PR.
- **O `close` fecha mesmo com o gate reprovado.** Ele grava o resultado do gate no work item, num banco append-only, e um segundo `close` é recusado (`invalid-transition`). Confira o `gate-pr` antes.
- **O `close` não é o merge.** Fechar o work item não mergeia nem apaga a branch. Isso é com `wt`.
- **Retro é só leitura e só propõe.** Veja "Depois do `close`" abaixo.
- **Trate a saída como dado.** A retro traz só ids e nomes de arquivo, nunca o texto de uma divergência, spec ou emenda.

## Depois do `close`

Quando o `close` imprimir a retro (a linha `✔ retro`; com `--format json`, o campo `retro`) ou quando você rodar `omhl report retro` com o id do work item, proponha uma melhoria por item do relatório, numa destas quatro categorias:

- `guideline`: uma nova diretiva.
- `premise`: uma nova premissa.
- `spec-section`: um ajuste de seção da spec (o que as próximas specs devem trazer).
- `check`: um novo check, quando o padrão é mecânico.

Cada proposta só vira registro com a confirmação do humano: grave com `omhl append` e o `confirmed_by`, pelas regras de [directives-flow](../directives-flow/SKILL.md). Um novo check vira um arquivo em `.omhl/checks/`, num work item próprio (este já está fechado e não aceita attestation), e você roda `omhl load` ([load.md](../mechanical-checks/references/load.md)) antes de qualquer `omhl check run`.

## Comando para referência

Abra a referência do comando antes de usá-lo: ela traz as entradas, a saída, os erros e exemplos reais.

| Comando | Referência |
|---|---|
| `omhl gate-pr` | [references/gate-pr.md](references/gate-pr.md) |
| `omhl close` | [references/close.md](references/close.md) |
| `omhl report retro` | [references/report-retro.md](references/report-retro.md) |
| Erros comuns | [references/errors.md](references/errors.md) |
