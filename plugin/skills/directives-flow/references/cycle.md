# O ciclo com os agentes

Quando consultar: antes de começar um work item que vai passar pelos agentes do plugin, ou para saber em que passo do ciclo você está. Quem faz cada papel e o que a sessão principal passa a cada um está em [roles.md](roles.md). Os passos 2 a 8 têm a skill [plan-consensus](../../plan-consensus/SKILL.md); os passos 9 e 10, a skill [review](../../review/SKILL.md).

**Antes do passo 1.** Em `feat` e `fix`, a sessão principal conduz a entrevista de negócio com o dono (skill [interview](../../interview/SKILL.md)): rodadas de perguntas até a ambiguidade chegar a 20% ou menos, e só então o passo 1 grava o work item e as premissas de negócio confirmadas. Nos outros prefixos a entrevista só roda a pedido do dono. O estado das rodadas fica em `.omhl/notes/`, fora do banco e fora da spec.

O ciclo tem dez passos.

1. **Work item.** A sessão principal grava `work-items.<id>` com `omhl append`, com o prefixo e o slug escolhidos.
2. **Analyst.** O `oh-my-hexflow:analyst` grava premissas `assumed` e gaps, cada gap com `kind` (`technical` ou `business`).
3. **Confirmação.** O que é de negócio vai ao humano, que confirma dentro da sessão. O que é técnico vai ao critic (`oh-my-hexflow:critic`). Gap sem `kind` vai ao humano.
4. **Planner.** O `oh-my-hexflow:planner` cria a spec com `omhl spec init` e a escreve em `.omhl/specs/wi-<id>__<slug>/`; ele a valida com `omhl spec validate` até passar.
5. **Architect.** O `oh-my-hexflow:architect` grava cada `decision` com `rests-on` para a âncora. Ele não responde gap: a resposta é uma premissa.
6. **Critic.** O `oh-my-hexflow:critic` grava `decision` com `supports` ou `contradicts`, confirma premissa técnica e responde gap `technical`.
7. **Consenso.** O `oh-my-hexflow:planner` (quando a spec muda), o `oh-my-hexflow:architect` e o `oh-my-hexflow:critic` repetem em no máximo 3 voltas. Sem aceite na terceira, a sessão principal para e mostra o impasse ao dono. O veredito do critic fica só na mensagem entre os agentes.
8. **Spec e worktree.** Com o aceite, o `oh-my-hexflow:verifier` retira os `contradicts` resolvidos; então vêm a conferência final com `omhl spec validate`, `omhl gate-worktree`, a worktree e a implementação.
9. **Revisão.** O `oh-my-hexflow:code-reviewer`, o `oh-my-hexflow:security-reviewer` e o `oh-my-hexflow:verifier` revisam a entrega.
10. **PR e fechamento.** O PR leva no corpo a tabela das decisões do critic ([tracking-table.md](tracking-table.md)); depois vêm `omhl gate-pr` e `omhl close`, pela skill [work-item-closing](../../work-item-closing/SKILL.md).

**Caminho leve (`chore` e `docs`).** Não há spec: o consenso fica nos passos 2 e 3 (o critic só no modo do passo 3), sem planner nem voltas e sem os passos 4 a 7, e o passo 8 se reduz ao `omhl gate-worktree`. A skill [plan-consensus](../../plan-consensus/SKILL.md) descreve o caminho na seção "Caminho leve".

**Sem `name`.** A sessão principal chama os agentes `oh-my-hexflow:*` sem o parâmetro `name`. Com `name`, o agente roda sem a matriz `omhl-allow` e sem a vista pré-carregada (a skill dele não chega), e o que ele gravar sai como `claude`.

O que a spec congelada, a divergência e a emenda pedem em qualquer passo continua no [SKILL.md](../SKILL.md).
