---
name: review
description: Protocolo da revisão da entrega com os agentes do plugin. Use depois de implementar o work item e antes de abrir o PR, para chamar o code-reviewer, o security-reviewer e o verifier, gravar os achados como divergence ou decision, montar a tabela do PR e conferir o omhl gate-pr.
---

# Revisão da entrega

Esta skill exige a `directives-flow`. Se não estiver carregada, carregue-a. Ela traz o ciclo ([cycle.md](../directives-flow/references/cycle.md)), os papéis e o que passar a cada agente ([roles.md](../directives-flow/references/roles.md)), o template da tabela ([tracking-table.md](../directives-flow/references/tracking-table.md)) e as regras de gravação. Aqui ficam só os passos da revisão, os passos 9 e 10 do ciclo.

Você é a sessão principal: chama cada agente como `oh-my-hexflow:<papel>`, um por vez.

## Os passos

1. Chame `oh-my-hexflow:code-reviewer` e depois `oh-my-hexflow:security-reviewer`. Passe a cada um o id do work item (`work-items.<id>`), a pasta da spec (`.omhl/specs/wi-<id>__<slug>/`) e a mudança a revisar.
2. Cada achado vira uma destas coisas, e nenhuma outra:
   - código que não cumpre a spec: `divergence` com `source_ref` no `CA-###` da spec e `resolution` `open`;
   - violação de diretiva ou de decisão: `decision` do revisor com `rests-on` para a âncora e `contradicts` para o alvo violado;
   - o resto (estilo, performance genérica): fica só na mensagem.
3. Corrija o código das divergências abertas. Um desvio que o dono aceita não é correção: pergunte a ele e grave `accepted` só com a resposta dele (regras em [divergence.md](../directives-flow/references/divergence.md)).
4. Antes do gate do PR, chame `oh-my-hexflow:verifier` em contexto isolado, sem o contexto do autor: passe o id, a pasta da spec, o id de cada divergência a conferir e o par `from`/`to` de cada `contradicts` a retirar. Ele confere a correção com evidência nova e retira os `contradicts` resolvidos; a razão de cada retirada vem na mensagem dele.
5. O verifier fecha a `divergence` de qualquer revisor de `open` para `code-fixed`, com a evidência nova em `evidence`, e nunca para `accepted`, que é resposta do dono.
6. Monte a tabela do PR com o template do plugin ([tracking-table.md](../directives-flow/references/tracking-table.md)); se o arquivo `.omhl/templates/tracking-table.md` existir, ele vale no lugar. As linhas vêm destes comandos, um comando por chamada, sem pipe:
   - `omhl list --type decision --where work_item=work-items.<id>`
   - `omhl list --type premise --where work_item=work-items.<id>`
7. Leia o autor em `author.agent` na saída: entram as `decision` com `omhl:critic` e as premissas com `confirmed_by.by` igual a `critic`. A coluna de evidência traz a mensagem do verifier quando houve retirada.
8. Cole a tabela no corpo do PR. Confira com `omhl gate-pr` e feche pela skill [work-item-closing](../work-item-closing/SKILL.md).

## O que fazer com a resposta de cada agente

Toda resposta termina com dois campos fixos:

- **Next role:** o papel a chamar em seguida (ou `none`).
- **Open questions for the main session:** perguntas que o agente não pode gravar. Grave cada uma como `gap` pelo `omhl append`, com o `kind` sugerido. Sem `kind`, o gap vai ao humano.
