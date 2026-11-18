---
name: directives-flow
description: Protocolo do fluxo por diretivas e premissas do omhl. Use antes de começar um work item novo, ao registrar premissa, diretiva, gap ou divergência, ao escrever ou corrigir uma spec em .omhl/specs/, ao criar uma worktree com wt switch -c.
---

# Fluxo por diretivas e premissas

O humano define diretivas (`guideline`) e premissas (`premise`). Você escolhe os comportamentos e escreve a spec a partir delas. A referência de cada comando e os detalhes do fluxo estão em `references/`: a matriz de prefixos em [matrix.md](references/matrix.md) e a divergência e seus desfechos em [divergence.md](references/divergence.md).

## Regras que valem o tempo todo

- **Grave pela CLI.** Rode `omhl append` no Bash, com o JSON no stdin, e [`omhl attest`](../mechanical-checks/references/attest.md) para a attestation. Sob o Claude Code a CLI grava `agent: "claude"` na sessão principal e, com `OMHL_ROLE=omhl:<papel>`, o papel. Nenhum canal prova a confirmação: o `confirmed_by` vale pelo que você grava, então grave-o só com a resposta real do humano. Isso vale para work item, premissa, gap, divergência e relation.
- **Só o humano confirma.** Uma diretiva ou uma premissa de negócio sugerida por você fica pendente e não vincula. Para confirmar, pergunte ao humano dentro da sessão e grave o campo `confirmed_by` com `by` no formato `person:<nome>`, em que `<nome>` identifica o humano que confirmou na sessão. Se não souber o nome, pergunte ao humano; não invente. Grave também a pergunta e a resposta literais, copiadas exatamente como foram ditas. Nunca resuma, traduza nem corrija o texto. Nunca invente uma confirmação.
- **Premissa técnica pode ser autônoma.** Registre-a com `status` `assumed`, sem `confirmed_by`. O humano a confirma depois, se quiser.
- **Decisão técnica se apoia numa âncora.** Registre-a como `decision` (com `work_item`, `text` e `evidence`) e ligue-a por `rests-on` a uma diretiva ou premissa confirmada. Sem isso o `omhl gate-pr` reprova `decisions-anchored`.
- **Reaproveite o texto de uma premissa.** Antes de registrar uma premissa, rode [omhl report recurrences](../query-and-integrity/references/report-recurrences.md) `--min 2` e, se uma premissa anterior significa a mesma coisa, copie o texto exato dela. Assim a promoção na 3ª ocorrência agrupa as duas.

## A ordem do protocolo

1. Escolha o prefixo (`feat`, `fix`, `refactor`, `tests`, `bugfix`, `chore` ou `docs`), o id e o slug. A branch será `tipo/wi-<id>__<slug>`.
2. Grave o work item com `omhl append`: `work-items.<id>`, tipo `work-item`, `title` e `status` `open`. A guarda recusa premissa sem um work item aberto.
3. Sugira as premissas (`kind` `business` ou `technical`, com o `work_item`).
4. Peça a confirmação do humano e grave a premissa `confirmed` com `confirmed_by`.
5. Para `feat/` e `fix/`, rode `omhl spec init work-items.<id> <slug>`, escreva a spec e rode `omhl spec validate work-items.<id>` até passar.
6. Confira o gate com `omhl gate-worktree --branch <nome>`. Só então rode `wt switch -c <branch>`.
7. Com a worktree criada, o commit e o push rodam os checks (skill `mechanical-checks`). Ao terminar, confira e feche com a skill `work-item-closing`.

## Se o `wt switch` falhar com `Cannot prompt for approval`

O `wt` guarda a aprovação do hook de projeto pelo texto do comando. Sem aprovação salva e sem terminal, ele aborta todo `wt switch`, inclusive para a `develop`, com a mensagem `Cannot prompt for approval` (seguida de `in non-interactive environment`).

Pare. Peça ao dono que rode `wt config approvals add`. **Nunca** tente `--no-hooks` para contornar: ele pula o gate de criação, e esse bypass é decisão do dono, não sua.

## Dúvida durante a execução

- Uma dúvida técnica vira um `gap` (`question` e `context`). Cite-o na spec como `[PENDENTE: gaps.<nome>]`. Responda gravando uma premissa e a relation `answers` da premissa para o gap.
- Se a sua resposta contraria uma diretiva confirmada, não a imponha: o gap fica em `needs-human` até o humano decidir.

## Corrigir a spec depois da aprovação

A spec congelada nunca é editada. Não existe comando nem marca para descongelar, e você nunca deve tentar editá-la nem procurar como descongelá-la. O caminho é a emenda:

1. Registre uma `divergence` com `resolution` `open` (`omhl append`), com `source_ref`, `kind` (`missing`, `partial`, `contradicts` ou `unrequested`), `text`, `evidence` (lista de `{source, summary}`) e `work_item`. Os demais campos e erros estão em [references/append.md](references/append.md).
2. Pergunte ao humano se a spec estava errada. Se não estava, não há emenda: feche a divergência com uma nova revisão dela (com `base`, o `node.record_id` de `omhl get divergences.<nome>`) em `resolution` `code-fixed` (você corrigiu o código para seguir a spec) ou `accepted` (o humano aceita o desvio). Pergunte ao humano qual dos dois e grave `accepted` só com a resposta dele. Não leve `confirmed_by` nem `amendment`: a guarda os recusa nesses dois desfechos (`confirmation-invalid`). Os passos 3 a 7 não se aplicam.
3. Se estava, crie `emenda-NN.md` na pasta da spec do work item, com `Write` de um arquivo novo. O hook libera só o próximo número. A emenda tem as seções `Divergência` (o id `divergences.<nome>`), `Correções` (itens `- EMD-001 CMP-003: <novo valor>`) e `Motivo`.
4. Valide com `omhl spec validate work-items.<id>`.
5. Se a emenda saiu inválida, abandone-a e crie a seguinte (`emenda-(NN+1).md`): ela também fica congelada assim que existe. Os `EMD-###` da emenda abandonada continuam contados, então continue a numeração (`EMD-002` depois de `EMD-001`).
6. Mostre a emenda ao humano e peça a aprovação, copiando a pergunta e a resposta literais.
7. Grave uma nova revisão da mesma `divergence` (com `base`) com `resolution` `spec-stale`, `amendment` (`emenda-NN`, sem `.md`) e `confirmed_by`. O `append` recusa a revisão se faltar um dos dois campos ou se a emenda não valida.

## Depois do `close`

A retro (`omhl report retro`) e a proposta de melhorias são da [work-item-closing](../work-item-closing/SKILL.md): as categorias são `guideline`, `premise`, `spec-section` e `check`. Cada proposta vira registro pelas regras de cima (`omhl append`, `confirmed_by`); um novo check vira um arquivo em `.omhl/checks/`, num work item próprio, e você roda `omhl load` antes de qualquer `omhl check run`.

## Comando para referência

Abra a referência do comando antes de usá-lo: ela traz as entradas, a saída, os erros e exemplos reais.

| Comando | Referência |
|---|---|
| `omhl append` | [references/append.md](references/append.md) |
| `omhl spec init` | [references/spec-init.md](references/spec-init.md) |
| `omhl spec validate` | [references/spec-validate.md](references/spec-validate.md) |
| `omhl gate-worktree` | [references/gate-worktree.md](references/gate-worktree.md) |
| `omhl guideline retire` | [references/guideline-retire.md](references/guideline-retire.md) |
| Erros comuns | [references/errors.md](references/errors.md) |
