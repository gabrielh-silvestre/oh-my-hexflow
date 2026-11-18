---
name: plan-consensus
description: Protocolo do consenso do plano com os agentes do plugin. Use depois de gravar o work item e antes de validar a spec, para chamar o analyst, o planner, o architect e o critic em sequência, limitar as voltas, escalar o impasse ao dono e retirar os contradicts resolvidos antes do omhl spec validate.
---

# Consenso do plano

Esta skill exige a `directives-flow`. Se não estiver carregada, carregue-a. Ela traz o ciclo ([cycle.md](../directives-flow/references/cycle.md)), os papéis e o que passar a cada agente ([roles.md](../directives-flow/references/roles.md)) e as regras de gravação. Aqui ficam só os passos do consenso, do passo 2 ao 8 do ciclo.

Você é a sessão principal: chama cada agente como `oh-my-hexflow:<papel>`, um por vez, e é você que roda o que está fora da matriz dele.

## Antes de começar

- O work item está gravado e aberto (`work-items.<id>`), com o prefixo e o slug escolhidos.
- Cada chamada leva o id do work item. Quem lê a spec leva também a pasta dela, `.omhl/specs/wi-<id>__<slug>/`.

## Os passos

1. Chame `oh-my-hexflow:analyst` com o id. Ele grava premissas `assumed` e gaps com `kind`.
2. Leve ao humano o que for de negócio (premissa e gap `business`, ou gap sem `kind`) e grave a confirmação com as regras da base. O que for técnico fica para o critic, no passo 5.
3. Chame `oh-my-hexflow:planner` com o id e o slug. Ele cria e escreve a spec; a validação fica para o passo 8.
4. Chame `oh-my-hexflow:architect` com o id. Ele grava cada `decision` com o `rests-on` para a âncora.
5. Chame `oh-my-hexflow:critic` com o id e a pasta da spec. Ele confirma as premissas técnicas, responde os gaps `technical` e grava `supports` ou `contradicts` nas decisões. O veredito circula só na mensagem: não o grave.
6. Se o critic não aceitar, abra outra volta: devolva as objeções ao architect (e ao planner, se a spec muda) e chame o critic de novo. **São no máximo 3 voltas.** Na terceira sem aceite, pare: mostre ao dono o impasse (o veredito do critic e os `contradicts` vigentes) e espere a decisão dele.
7. Com o aceite do critic, chame `oh-my-hexflow:verifier` em contexto isolado (passe só o id, a pasta da spec e o par `from`/`to` de cada `contradicts` de voltas anteriores) para conferir que cada `decision` revisada do architect responde ao `contradicts` e retirar os `contradicts` resolvidos. A relação aponta para o id do node, não para a revisão: sem a retirada, ela segue vigente depois da revisão e trava o gate do PR.
8. Só então rode `omhl spec validate work-items.<id>`.
9. Com a spec válida, siga o passo 8 do ciclo: `omhl gate-worktree` e a worktree.

## O que fazer com a resposta de cada agente

Toda resposta termina com dois campos fixos:

- **Next role:** o papel a chamar em seguida (ou `none`). Chame-o se concordar; senão siga o passo seguinte da lista.
- **Open questions for the main session:** perguntas que o agente não pode gravar. Grave cada uma como `gap` pelo `omhl append`, com o `kind` sugerido. Sem `kind`, o gap vai ao humano.

Se um agente devolver `role-record-forbidden` ou parecido, ele tentou gravar fora da matriz: leia [roles.md](../directives-flow/references/roles.md) e grave você mesmo o que couber à sessão principal.
