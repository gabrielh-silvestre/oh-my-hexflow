---
name: directives-flow
description: Protocolo do fluxo por diretivas e premissas do omhl em conversas Cowork no Windows. Use antes de começar um work item novo, ao registrar premissa, diretiva, gap ou divergência, ao escrever ou corrigir uma spec em .omhl/specs/, ao criar a branch de um work item, antes de abrir PR e depois de um close que imprimiu a retro.
---

# Fluxo por diretivas e premissas (Cowork, Windows)

O humano define diretivas (`guideline`) e premissas (`premise`). Você escolhe os comportamentos e escreve a spec a partir delas. A referência completa, com todos os códigos de erro, está em `docs/directives-flow.md` do repositório do oh-my-hexflow.

## Onde cada coisa roda nesta conversa

- O `omhl` roda no Windows do Gabriel como servidor MCP local, chamado `omhl`. Ele atende um único projeto, o do `--project-dir` do plugin. Nesta conversa, as tools dele podem aparecer com prefixo, por exemplo `mcp__remote-devices__plugin_oh-my-hexflow-cowork_omhl__render`. Se não aparecerem, procure por "omhl".
- O seu shell **não** alcança o Windows. Nunca rode `omhl` nem `git` no seu Bash para mexer no projeto:
  - comandos que antes eram da CLI passam pela tool `cli`, por exemplo `cli` com `args: ["spec","validate","work-items.42"]`;
  - git passa pela tool `git`, com `op` igual a `status`, `log`, `diff`, `branch_create`, `switch` ou `commit`.
- Os arquivos do projeto são editados pelas tools de arquivo do dispositivo, numa pasta conectada que aponta para o mesmo diretório do projeto.
- Esta conversa **não tem hooks**. Nada injeta o contexto e nada trava a spec por você. Por isso existem as duas regras abaixo.

## Regras que valem o tempo todo

- **Comece pelo `render`.** Antes de qualquer trabalho no projeto, chame a tool `render` e leia o contexto que ela devolve: decisões, checks e work items. É o que o hook `SessionStart` faria.
- **Nunca edite uma spec aprovada.** Uma spec fica congelada quando a branch do work item existe; confira com `git` `op: "log"` ou `status`. Não há hook que impeça a edição, então a disciplina é sua: para corrigir, use a emenda (seção abaixo).
- **Grave pelas tools MCP, nunca pela CLI.** Use a tool `append`. A tool `cli` recusa `append`. Nenhum canal prova a confirmação: o `confirmed_by` vale pelo que você grava, então grave-o só com a resposta real do humano. Isso vale para work item, premissa, gap, divergência e relation.
- **Só o humano confirma.** Uma diretiva ou uma premissa de negócio sugerida por você fica pendente e não vincula. Para confirmar:
  1. Pergunte ao humano dentro da conversa.
  2. Grave o campo `confirmed_by` com `by` no formato `person:<nome>`. Se não souber o nome, pergunte; não invente.
  3. Grave também a pergunta e a resposta literais, copiadas exatamente como foram ditas. Nunca resuma, traduza nem corrija o texto. Nunca invente uma confirmação.
- **Premissa técnica pode ser autônoma.** Registre-a com `status` `assumed`, sem `confirmed_by`. O humano a confirma depois, se quiser.
- **Reaproveite o texto de uma premissa.** Antes de registrar uma premissa, chame `report_recurrences`. Se uma premissa anterior significa a mesma coisa, copie o texto exato dela, para que a promoção na 3ª ocorrência agrupe as duas.

## A ordem do protocolo

1. Escolha o prefixo (`feat`, `fix`, `refactor`, `tests`, `bugfix`, `chore` ou `docs`), o id e o slug. A branch será `tipo/wi-<id>__<slug>`.
2. Grave o work item com `append`: `work-items.<id>`, tipo `work-item`, `status` `open`. A guarda recusa premissa sem um work item aberto.
3. Sugira as premissas, com `kind` `business` ou `technical` e o `work_item`.
4. Peça a confirmação do humano e grave a premissa `confirmed` com `confirmed_by`.
5. Para `feat/` e `fix/`:
   - chame `cli` com `["spec","init","work-items.<id>","<slug>"]`;
   - escreva a spec;
   - chame `cli` com `["spec","validate","work-items.<id>"]` até passar.
6. Confira o gate com a tool `gate_worktree`. Só então crie a branch com `git` `op: "branch_create"` e `name: "<branch>"`. Neste protótipo não há worktree: o trabalho segue na branch, no mesmo clone.

## Antes de abrir o PR

1. Faça os commits com `git` `op: "commit"`. Ele roda `add -A` e depois `commit`.
2. Rode os checks no host com `cli` `["check","run","--event","before-commit"]`, com a árvore limpa, e depois `["check","run","--event","before-pr"]`. Se algum check em `.omhl/checks/` mudou, rode antes `cli` `["load"]`.
3. Confira com a tool `gate_pr`. O push é do Gabriel: diga a ele que a branch está pronta.

## Dúvida durante a execução

- Uma dúvida técnica vira um `gap`, com `question` e `context`. Cite-o na spec como `[PENDENTE: gaps.<nome>]`. Responda gravando uma premissa e a relation `answers` da premissa para o gap.
- Se a sua resposta contraria uma diretiva confirmada, não a imponha: o gap fica em `needs-human` até o humano decidir.

## Corrigir a spec depois da aprovação

A spec congelada nunca é editada. O caminho é a emenda:

1. Registre uma `divergence` com `resolution` `open`, pela tool `append`, com `source_ref`, `kind`, `text`, `evidence` e `work_item`.
2. Pergunte ao humano se a spec estava errada. Se não estava, não há emenda.
3. Se estava, crie `emenda-NN.md` na pasta da spec do work item, como um arquivo **novo**, com o próximo número. A emenda tem três seções:
   - `Divergência`: o id `divergences.<nome>`;
   - `Correções`: itens `- EMD-001 CMP-003: <novo valor>`;
   - `Motivo`.
4. Valide com `cli` `["spec","validate","work-items.<id>"]`.
5. Se a emenda saiu inválida, abandone-a e crie a seguinte (`emenda-(NN+1).md`). Os `EMD-###` da emenda abandonada continuam contados, então siga a numeração (`EMD-002` depois de `EMD-001`).
6. Mostre a emenda ao humano e peça a aprovação, copiando a pergunta e a resposta literais.
7. Grave uma nova revisão da mesma `divergence` (com `base`), com `resolution` `spec-stale`, `amendment` (`emenda-NN`, sem `.md`) e `confirmed_by`.

## Depois do `close`

Quando o `close` devolver o campo `retro` (ou quando você chamar `report_retro`), proponha uma melhoria por item do relatório, numa destas quatro categorias:

- `guideline`: uma nova diretiva;
- `premise`: uma nova premissa;
- `spec-section`: um ajuste de seção da spec;
- `check`: um novo check, quando o padrão é mecânico.

Cada proposta só vira registro com a confirmação do humano, com o `confirmed_by` de sempre. Um novo check vira um arquivo em `.omhl/checks/`, num work item próprio, e você chama `cli` `["load"]` antes de qualquer `check run`.
