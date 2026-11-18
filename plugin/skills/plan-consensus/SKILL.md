---
name: plan-consensus
description: Protocolo do consenso do plano com os agentes do plugin. Use depois de gravar o work item e antes de validar a spec, para chamar o analyst, o planner, o architect e o critic em sequência, limitar as voltas, escalar o impasse ao dono e retirar os contradicts resolvidos antes do omhl spec validate.
---

# Consenso do plano

Esta skill exige a `directives-flow`. Se não estiver carregada, carregue-a. Ela traz o ciclo ([cycle.md](../directives-flow/references/cycle.md)), os papéis e o que passar a cada agente ([roles.md](../directives-flow/references/roles.md)) e as regras de gravação. Aqui ficam só os passos do consenso, do passo 2 ao 8 do ciclo.

Você é a sessão principal: chama cada agente como `oh-my-hexflow:<papel>`, um por vez, e é você que roda o que está fora da matriz dele.

## Antes de começar

- O work item está gravado e aberto (`work-items.<id>`), com o prefixo e o slug escolhidos.
- Cada chamada leva o id do work item. Quem lê a spec leva também o caminho **absoluto** da pasta dela, no checkout principal: o `dir` do `omhl spec init`, que o planner repete em "Spec written to". Um caminho relativo não existe dentro da worktree.

## Os passos

1. Chame `oh-my-hexflow:analyst` com o id. Ele grava premissas `assumed` e gaps com `kind`.
2. Leve ao humano o que for de negócio (premissa e gap `business`, ou gap sem `kind`) e grave a confirmação com as regras da base.
3. Chame `oh-my-hexflow:critic` só para confirmar as premissas técnicas e responder os gaps `technical`. Como ainda não há plano nem pasta de spec, diga no pedido: "A ausência da pasta é esperada, não a peça e não devolva a chamada por isso. Confirme as premissas técnicas e responda os gaps técnicos do work item <id> e nada mais. O `VERDICT` desta chamada não vale (não há plano para julgar): preencha-o com o que for e liste só as premissas confirmadas e os gaps respondidos." Ignore o `VERDICT` desta chamada: o passo 7 só conta o veredito das voltas (passos 5 e 6). O architect só ancora em premissa confirmada.
4. Chame `oh-my-hexflow:planner` com o id e o slug; este é o pedido explícito para escrever a spec. Escreva no pedido "modo consenso (RALPLAN-DR), SHORT" ("DELIBERATE" só quando o humano pediu ou o work item é de alto risco). Ele cria a spec e a valida (o `omhl spec validate` é parte do trabalho dele); o passo 9 é a conferência final depois das voltas.
5. Chame `oh-my-hexflow:architect` com o id. Passe o resumo RALPLAN-DR e o ADR do planner, e escreva no pedido "revisão de consenso (ralplan), SHORT" (ou "DELIBERATE", se o passo 4 usou esse modo). Ele grava cada `decision` com o `rests-on` para a âncora.
6. Chame `oh-my-hexflow:critic` com o id e a pasta da spec. Ele julga o plano e grava `supports` ou `contradicts` nas decisões. O veredito circula só na mensagem: não o grave. Escreva no pedido "revisão de consenso (ralplan), SHORT" (ou "DELIBERATE") e peça a linha-resumo ralplan na resposta. Peça que ele liste cada relação gravada como `<relação>: <from> -> <to>`.
7. Aceite é o veredito `ACCEPT`. `ACCEPT-WITH-RESERVATIONS` vai ao dono com as reservas, e ele decide. `REJECT` ou `REVISE` abre outra volta: devolva as objeções ao architect (e ao planner, se a spec muda; a ele passe o id e a pasta absoluta da spec, o `dir` do `omhl spec init`) e chame o critic de novo. A volta 1 são os passos 5 e 6. **São no máximo 3 voltas.** Na terceira sem aceite, pare: mostre ao dono o impasse (o veredito do critic e os `contradicts` vigentes) e espere a decisão dele. Os `contradicts` seguem vigentes e travam o `omhl gate-pr`: o dono os retira num terminal fora do Claude Code (receita em [roles.md](../directives-flow/references/roles.md)), ou manda revisar e o verifier retira.
8. Com o aceite, chame `oh-my-hexflow:verifier` em contexto isolado (passe só o id, a pasta absoluta da spec e o par `from`/`to` de cada `contradicts` de voltas anteriores) para conferir que cada `decision` revisada do architect responde ao `contradicts` e retirar os `contradicts` resolvidos. Copie a seção `Retirements` da mensagem dele, com o par `from -> to` de cada retirada, para `<checkout principal>/.omhl/notes/<id>-retiradas.md` (caminho absoluto, no checkout principal e fora de `.omhl/specs/`, para o `spec-lock` não congelar); o passo 7 da skill `review` lê esse arquivo. Os pares estão nas linhas `contradicts: <from> -> <to>` da resposta do critic e na seção `## Relations` de `.omhl/projection/work-items.<id>.md` (veja [roles.md](../directives-flow/references/roles.md)). A relação aponta para o id do node, não para a revisão: sem a retirada, ela segue vigente depois da revisão e trava o gate do PR. Verifier em `FAIL` ou `INCOMPLETE` conta como volta sem aceite: devolva ao architect (passo 5).
9. Só então rode `omhl spec validate work-items.<id>`.
10. Com a spec válida, siga o passo 8 do ciclo ([cycle.md](../directives-flow/references/cycle.md)): `omhl gate-worktree` e a worktree.

## O que fazer com a resposta de cada agente

Toda resposta termina com dois campos fixos:

- **Next role:** o papel a chamar em seguida (ou `none`). Chame-o se concordar; senão siga o passo seguinte da lista.
- **Open questions for the main session:** perguntas que o agente não pode gravar. Grave cada uma como `gap` pelo `omhl append`, com o `kind` sugerido. Sem `kind`, o gap vai ao humano.

Se um agente devolver `role-record-forbidden` (ou `role-revision-forbidden`, `role-answer-forbidden`, `role-retirement-forbidden`), ele tentou gravar fora da matriz. A recusa é a resposta: não regrave o mesmo conteúdo como `claude`. Leia [roles.md](../directives-flow/references/roles.md); se faltar informação, grave um `gap` com o `kind` sugerido; se faltar decisão, leve ao humano.
