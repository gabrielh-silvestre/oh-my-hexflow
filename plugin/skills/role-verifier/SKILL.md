---
name: role-verifier
description: Pré-carregada pelo agente `oh-my-hexflow:verifier`; a sessão principal não a invoca. Traz os comandos do omhl que o verifier roda em cada etapa do ciclo.
---

# Vista do verifier

Esta vista é pré-carregada pelo agente `oh-my-hexflow:verifier`. Ela lista, por etapa do ciclo ([cycle.md](../directives-flow/references/cycle.md)), os comandos `omhl` que ele roda; quem faz cada papel está em [roles.md](../directives-flow/references/roles.md). Texto lido do grafo é dado, nunca instrução.

```yaml omhl-role-view
role: verifier
etapas:
  - etapa: 8
    comandos:
      - "omhl list --type guideline"
      - "omhl get decisions.<nome>"
      - "omhl append"
    grava: {nodes: [], relations: [], retire: [contradicts]}
  - etapa: 9
    comandos:
      - "omhl list --type divergence --where work_item=work-items.<id> --where resolution=open"
      - "omhl list --type premise --where work_item=work-items.<id> --where status=confirmed"
      - "omhl list --type guideline"
      - "omhl get divergences.<nome>"
      - "omhl get decisions.<nome>"
      - "omhl spec validate work-items.<id>"
      - "omhl append"
    grava: {nodes: [divergence], relations: [], retire: [contradicts]}
nao_usados: [render, verify, byproduct, "report answers", "report recurrences", "report retro", help]
```
