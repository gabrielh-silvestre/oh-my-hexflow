---
name: directives-flow
description: Protocolo do fluxo por diretivas e premissas do omhl. Use antes de começar um work item novo, ao registrar premissa, diretiva, gap ou divergência, ao escrever ou corrigir uma spec em .omhl/specs/, ao criar uma worktree com wt switch -c, e depois de um close que imprimiu a retro.
---

# Fluxo por diretivas e premissas

O humano define diretivas (`guideline`) e premissas (`premise`). Você escolhe os comportamentos e escreve a spec a partir delas. A referência completa, com todos os códigos de erro, está em `docs/directives-flow.md` do repositório do oh-my-hexflow.

## Regras que valem o tempo todo

- **Grave pelas tools MCP, nunca pela CLI.** Use a tool `append` do servidor `omhl`. Nunca rode `omhl append` no Bash: a tool grava `agent: "claude"`, e a CLI pode perder esse rótulo. Nenhum canal prova a confirmação: o `confirmed_by` vale pelo que você grava, então grave-o só com a resposta real do humano. Isso vale para work item, premissa, gap, divergência e relation.
- **Só o humano confirma.** Uma diretiva ou uma premissa de negócio sugerida por você fica pendente e não vincula. Para confirmar, pergunte ao humano dentro da sessão e grave o campo `confirmed_by` com `by` no formato `person:<nome>`, em que `<nome>` identifica o humano que confirmou na sessão. Se não souber o nome, pergunte ao humano; não invente. Grave também a pergunta e a resposta literais, copiadas exatamente como foram ditas. Nunca resuma, traduza nem corrija o texto. Nunca invente uma confirmação.
- **Premissa técnica pode ser autônoma.** Registre-a com `status` `assumed`, sem `confirmed_by`. O humano a confirma depois, se quiser.
- **Reaproveite o texto de uma premissa.** Antes de registrar uma premissa, rode `omhl report recurrences --min 2` (ou a tool `report_recurrences`) e, se uma premissa anterior significa a mesma coisa, copie o texto exato dela. Assim a promoção na 3ª ocorrência agrupa as duas.

## A ordem do protocolo

1. Escolha o prefixo (`feat`, `fix`, `refactor`, `tests`, `bugfix`, `chore` ou `docs`), o id e o slug. A branch será `tipo/wi-<id>__<slug>`.
2. Grave o work item com `append`: `work-items.<id>`, tipo `work-item`, `status` `open`. A guarda recusa premissa sem um work item aberto.
3. Sugira as premissas (`kind` `business` ou `technical`, com o `work_item`).
4. Peça a confirmação do humano e grave a premissa `confirmed` com `confirmed_by`.
5. Para `feat/` e `fix/`, rode `omhl spec init work-items.<id> <slug>`, escreva a spec e rode `omhl spec validate work-items.<id>` até passar.
6. Confira o gate com a tool `gate_worktree` (ou `omhl gate-worktree --branch <nome>`). Só então rode `wt switch -c <branch>`.

## Se o `wt switch` falhar com `Cannot prompt for approval`

O `wt` guarda a aprovação do hook de projeto pelo texto do comando. Sem aprovação salva e sem terminal, ele aborta todo `wt switch`, inclusive para a `develop`, com a mensagem `Cannot prompt for approval` (seguida de `in non-interactive environment`).

Pare. Peça ao dono que rode `wt config approvals add`. **Nunca** tente `--no-hooks` para contornar: ele pula o gate de criação, e esse bypass é decisão do dono, não sua.

## Dúvida durante a execução

- Uma dúvida técnica vira um `gap` (`question` e `context`). Cite-o na spec como `[PENDENTE: gaps.<nome>]`. Responda gravando uma premissa e a relation `answers` da premissa para o gap.
- Se a sua resposta contraria uma diretiva confirmada, não a imponha: o gap fica em `needs-human` até o humano decidir.

## Corrigir a spec depois da aprovação

A spec congelada nunca é editada. Não existe comando nem marca para descongelar, e você nunca deve tentar editá-la nem procurar como descongelá-la. O caminho é a emenda:

1. Registre uma `divergence` com `resolution` `open` (tool `append`), com `source_ref`, `kind`, `text`, `evidence` e `work_item`.
2. Pergunte ao humano se a spec estava errada. Se não estava, não há emenda.
3. Se estava, crie `emenda-NN.md` na pasta da spec do work item, com `Write` de um arquivo novo. O hook libera só o próximo número. A emenda tem as seções `Divergência` (o id `divergences.<nome>`), `Correções` (itens `- EMD-001 CMP-003: <novo valor>`) e `Motivo`.
4. Valide com `omhl spec validate` (`omhl spec validate work-items.<id>`).
5. Se a emenda saiu inválida, abandone-a e crie a seguinte (`emenda-(NN+1).md`): ela também fica congelada assim que existe. Os `EMD-###` da emenda abandonada continuam contados, então continue a numeração (`EMD-002` depois de `EMD-001`).
6. Mostre a emenda ao humano e peça a aprovação, copiando a pergunta e a resposta literais.
7. Grave uma nova revisão da mesma `divergence` (com `base`) com `resolution` `spec-stale`, `amendment` (`emenda-NN`, sem `.md`) e `confirmed_by`. O `append` recusa a revisão se faltar um dos dois campos ou se a emenda não valida.

## Depois do `close`

Quando o `close` imprimir o campo `retro` (ou quando você rodar a tool `report_retro`), proponha uma melhoria por item do relatório, numa destas quatro categorias:

- `guideline`: uma nova diretiva.
- `premise`: uma nova premissa.
- `spec-section`: um ajuste de seção da spec (o que as próximas specs devem trazer).
- `check`: um novo check, quando o padrão é mecânico.

Cada proposta só vira registro com a confirmação do humano, com o `confirmed_by` de sempre. Um novo check vira um arquivo em `.omhl/checks/`, num work item próprio, e você roda `omhl load` antes de qualquer `check run`.
