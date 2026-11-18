# O ciclo com os agentes

Quando consultar: antes de começar um work item que vai passar pelos agentes do plugin, ou para saber em que passo do ciclo você está. Quem faz cada papel e o que a sessão principal passa a cada um está em [roles.md](roles.md). Os passos 2 a 8 têm a skill [plan-consensus](../../plan-consensus/SKILL.md); os passos 9 e 10, a skill [review](../../review/SKILL.md).

O ciclo tem dez passos. A sessão principal chama cada agente como `oh-my-hexflow:<papel>`, um por vez.

1. **Work item.** A sessão principal grava `work-items.<id>` com `omhl append`, com o prefixo e o slug escolhidos.
2. **Analyst.** O `oh-my-hexflow:analyst` grava premissas `assumed` e gaps, cada gap com `kind` (`technical` ou `business`).
3. **Confirmação.** O que é de negócio vai ao humano, que confirma dentro da sessão. O que é técnico vai ao critic (`oh-my-hexflow:critic`). Gap sem `kind` vai ao humano.
4. **Planner.** O `oh-my-hexflow:planner` cria a spec com `omhl spec init` e a escreve em `.omhl/specs/wi-<id>__<slug>/`.
5. **Architect.** O `oh-my-hexflow:architect` grava cada `decision` com `rests-on` para a âncora. Ele não responde gap: a resposta é uma premissa.
6. **Critic.** O `oh-my-hexflow:critic` grava `decision` com `supports` ou `contradicts`, confirma premissa técnica e responde gap `technical`.
7. **Consenso.** Architect e critic repetem em no máximo 3 voltas. Sem aceite na terceira, a sessão principal para e mostra o impasse ao dono. O veredito do critic fica só na mensagem entre os agentes.
8. **Spec e worktree.** Com o aceite, o verifier retira os `contradicts` resolvidos; então vêm `omhl spec validate`, `omhl gate-worktree`, a worktree e a implementação.
9. **Revisão.** O `oh-my-hexflow:code-reviewer`, o `oh-my-hexflow:security-reviewer` e o `oh-my-hexflow:verifier` revisam a entrega.
10. **PR e fechamento.** O PR leva no corpo a tabela das decisões do critic ([tracking-table.md](tracking-table.md)); depois vêm `omhl gate-pr` e `omhl close`, pela skill [work-item-closing](../../work-item-closing/SKILL.md).

## O que cada passo grava

- Os agentes gravam com o próprio papel no campo `agent` (`omhl:<papel>`). A sessão principal grava `claude`; o dono, fora do Claude Code, grava `user`.
- Um agente nunca grava o que está fora da matriz do papel dele (veja [roles.md](roles.md)). O que ele não pode gravar volta na mensagem final, e a sessão principal decide o que fazer.
- O que a spec congelada, a divergência e a emenda pedem em qualquer passo continua no [SKILL.md](../SKILL.md).
