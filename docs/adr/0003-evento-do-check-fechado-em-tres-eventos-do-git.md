---
status: accepted
---

# Evento do check fechado nos três eventos do git

## Contexto

Antes, o campo `when.event` de um check aceitava qualquer texto (`when.vocabulary` era `free`). Um check de texto com evento inventado era aceito e nunca era cobrado, porque nada no `omhl` dispara esse evento. O dono fixou o princípio em 2026-11-11: um evento só existe se o `omhl` o absorve de forma nativa (hook do git, interceptação ou camada fina em volta do comando). A ferramenta é deliberadamente limitada: o que ela absorve funciona perfeitamente, e não há espaço para plugar nada de forma desestruturada nem não planejada.

O nativo `checks.verify-review-comment-anchoring` usa `before-publishing-review-comments`, que nenhum hook do `omhl` absorve. Das três opções (fechar só os checks com `run`; fechar todo check carregado e manter os nativos; fechar também os nativos), o dono escolheu a segunda.

Motivo registrado pelo dono para a escolha exata de `before-commit`, `before-pr` e `before-merge`: não encontrado. O princípio acima explica o fechamento da lista, não quais eventos entram nela.

## Decisão

Todo check carregado, com ou sem `run`, tem `when.event` em `before-commit`, `before-pr` ou `before-merge` (`RUNNABLE_EVENTS`), e o catálogo exige `when.vocabulary = "closed"`. Um evento fora da lista é recusado pelo `omhl load` com o código `event-unsupported`. O nativo `checks.verify-review-comment-anchoring` fica como exceção do produto, como instrução de texto.

## Consequências

- Evento novo só entra mudando o `omhl`: um hook ou uma camada que o absorva, e o evento na lista.
- O `pr_gate` cobra exatamente esses três eventos (`REQUIRED_EVENTS`), então nenhum check carregado fica sem quem o cobre.
- O nativo de revisão não é cobrado por mecanismo nenhum. Vincular esse evento a um hook do Claude Code (`PreToolUse` nas ferramentas do GitHub que publicam review) e criar eventos customizados, definidos de forma semanticamente rica para o agente inferir quando executá-los, ficam como follow-up sem decisão. Eventos customizados dependeriam da boa vontade do agente, não de um mecanismo sistemático.
