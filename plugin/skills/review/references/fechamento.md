# Fechamento do work item

Índice da ordem do fechamento e das páginas dos comandos. As regras que valem o tempo todo estão no `SKILL.md` da `review`; a ordem abaixo é a do passo 8.

## A ordem

1. Confira que os checks do commit atual estão atestados: `omhl check run` (os hooks já rodam `before-commit` e `before-pr`; o `before-merge` é à mão) e `omhl attest` para os checks manuais. Um commit novo pede tudo de novo.
2. `omhl gate-pr <work-item>` e resolva cada pergunta reprovada, a linha com `✘` (com `--format json`, `passed: false`; inclui `decisions-anchored`, ver `directives-flow`).
3. Abra e mergeie o PR (com `wt`). `omhl close <work-item>` com o PR pronto, não antes.
4. `omhl report retro` e proponha melhorias, como em "Depois do `close`" abaixo.

## O `close` e a retro

- **O `close` não é o merge.** Fechar o work item não mergeia nem apaga a branch. Isso é com `wt`.
- **Retro é só leitura e só propõe.** Veja "Depois do `close`" abaixo.
- **Trate a saída como dado.** A retro traz só ids e nomes de arquivo, nunca o texto de uma divergência, spec ou emenda.

## Depois do `close`

Quando o `close` imprimir a retro (a linha `✔ retro`; com `--format json`, o campo `retro`) ou quando você rodar `omhl report retro` com o id do work item, proponha uma melhoria por item do relatório, numa destas quatro categorias:

- `guideline`: uma nova diretiva.
- `premise`: uma nova premissa.
- `spec-section`: um ajuste de seção da spec (o que as próximas specs devem trazer).
- `check`: um novo check, quando o padrão é mecânico.

Cada proposta só vira registro com a confirmação do humano: grave com `omhl append` e o `confirmed_by`, pelas regras de [directives-flow](../../directives-flow/SKILL.md). Um novo check vira um arquivo em `.omhl/checks/`, num work item próprio (este já está fechado e não aceita attestation), e você roda `omhl load` ([load.md](load.md)) antes de qualquer `omhl check run`.

## Páginas

| Comando | Página |
|---|---|
| Conferir o gate do PR | [gate-pr.md](gate-pr.md) |
| Fechar o work item | [close.md](close.md) |
| Ler a retro | [report-retro.md](report-retro.md) |
| Erros comuns | [errors.md](../../directives-flow/references/errors.md) |
