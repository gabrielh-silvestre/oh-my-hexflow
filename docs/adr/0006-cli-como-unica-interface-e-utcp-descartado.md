---
status: accepted
---

# CLI como única interface do agente e UTCP descartado

## Contexto

O `omhl` tinha duas portas para o agente: a CLI e o adapter MCP (`omhl mcp`, `adapters/mcp.rs`). O plugin já não inicia o MCP (`plugin.json` sem `mcpServers`, teste em `crates/omhl/tests/plugin.rs`), e o README dizia que nenhum fluxo depende dele. Mesmo sem uso, o adapter custava 403 linhas, 14 schemas de ferramenta e uma terceira porta a tocar a cada ação nova, além de vocabulário de protocolo dentro de `app` (`parse_tool_call`, `Caller::mcp`, `AppError::unknown_tool`).

**Nota de 2026-11-18 (work item `perf3`).** O teste do plugin agora é `crates/omhl/tests/integration/plugin.rs`. O layout de `tests/` mudou; ver [Testes unitários no gate, integração no CI](../mechanical-gates.md#testes-unitários-no-gate-integração-no-ci).

## Decisão

O `omhl` tem uma única interface para o agente: a CLI. O adapter MCP é removido junto com o que só ele usava (`parse_tool_call`, `Caller::mcp`, `AppError::unknown_tool`). **CLI direto, UTCP descartado.**

O Claude Code usa a CLI por quatro skills do plugin, com instruções em `references/` verificadas por um teste de cobertura (`skill_coverage`) e por um teste que executa os exemplos (`skill_examples`), ambos num binário de teste (`tests/skills.rs`) rodado pelo `checks.test-suite` (até o work item `perf2`; ver Consequências). As skills e os testes chegaram no PR #16, do mesmo work item; o primeiro PR deste ADR registrou só a decisão. O hook `session-start` e o `spec-lock` ficam fora das skills.

**Nota de 2026-11-18 (work item `perf3`).** Os testes `skill_coverage` e `skill_examples` agora ficam em `crates/omhl/tests/e2e/skills/` (`coverage.rs` e `examples.rs`), no binário e2e, e não mais em `tests/skills.rs`. O layout de `tests/` mudou; ver [Testes unitários no gate, integração no CI](../mechanical-gates.md#testes-unitários-no-gate-integração-no-ci).

## Drivers

- A CLI é a única interface do agente (premissa B1).
- A instrução do agente é verificada por código, não por revisão (B2).
- Remover é subtrair: no primeiro PR, a saída da CLI e o comportamento dos comandos não mudam (T1).
- O vocabulário do MCP sai da camada `app` (T3).
- Custo de execução sequencial: cada porta a mais é código e teste a manter, e a máquina do dono roda uma coisa por vez.
- Efeito do `omhl load` no tenant: qualquer solução que exija um check novo carrega esse efeito para as branches abertas.

## Alternativas consideradas

- **Manter o MCP.** Descartada: o plugin já não o inicia e nenhum fluxo depende dele; o custo é o descrito no contexto.
- **UTCP.** Descartada pelo spec do work item, pelos motivos abaixo. A pesquisa que os sustenta não leu o código dos SDKs TypeScript e Go, e a latência medida no site do projeto não tem metodologia.
  - O Claude Code não o consome nativamente (inferência da pesquisa, não confirmada). Só entra por uma ponte `@utcp/mcp-bridge`, que volta ao MCP.
  - O template de CLI devolve o `stdout` se o código de saída é 0 e o `stderr` se não, e perde o código de saída.
  - Foram 8 avisos de segurança em 2026, 2 deles altos no plugin de CLI.
  - O cliente Rust (`rs-utcp`) segue o contrato antigo.
  - Prazo fixo de 120 s, sem streaming nem estado.
- **CLI direto, com skills** (escolhida).
- **Agents em vez de skills.** Descartada: nenhum dos quatro fluxos pede contexto isolado.
- **Check mecânico dedicado para a cobertura das skills.** Descartada pelo dono: o `checks.test-suite` já roda o teste e reprova; um check novo exigiria `omhl load`, que é definitivo e abre janela de órfão para as branches abertas.

## Por que esta

Nenhuma superfície de protocolo a manter, a saída da CLI preservada no primeiro PR, instrução que um teste prova e nenhum efeito no banco do dono.

## Consequências

- Sem o MCP, o autor gravado pelo agente é `claude/cli` só quando `CLAUDECODE=1`; sem a variável, a CLI grava `user/cli` (ver [directives-flow.md](../directives-flow.md)). O MCP gravava `claude` sem depender do ambiente; agora o rótulo do agente depende da variável e é só rótulo de auditoria, não prova de autoria. Records antigos com `client: "mcp"` continuam na chain (append-only), e nenhuma leitura os trata de forma especial: a guarda não distingue o autor `mcp`.
- `omhl --help` perde a linha `mcp`.
- Um subcomando novo exige citação nas skills e `references/` com os cinco títulos e um exemplo; o `skill_coverage` e o `skill_examples` reprovam sem eles.
- **A T3 fica cumprida só na parte do MCP.** `app` ainda carrega `Caller::{cli, cli_under_claude_code, hook}` e o payload do `PreToolUse` (débito 5 de [architecture.md](../architecture.md)). O que as duas portas usavam em comum já mora em `app`.
- O `check sweep` do `skill_examples` varre qualquer raiz da máquina (efeito documentado em [architecture.md](../architecture.md), seção 4).
- O ADR 0004 foi editado no lugar para tirar o MCP da lista de clientes da guarda.
- Um segundo cliente de agente exigirá nova decisão.
- **Nota de 2026-11-17 (work item `perf2`).** O `checks.test-suite` local passou a rodar só `cargo test --workspace --lib --locked`, e `tests/skills.rs` saiu do pre-push: roda no CI (`.github/workflows/ci.yml`). A razão da alternativa descartada ("o `checks.test-suite` já roda o teste e reprova") passa a valer no merge, pelo CI e pelo ruleset da `main`, não no push. Follow-up: um check mecânico `before-merge` que consulte o `gh` foi considerado no `perf2` para registrar o CI e ficou fora.
- **Nota de 2026-11-18 (work item `perf3`).** O `tests/skills.rs` da nota acima agora é `crates/omhl/tests/e2e/skills/`. O layout de `tests/` mudou; ver [Testes unitários no gate, integração no CI](../mechanical-gates.md#testes-unitários-no-gate-integração-no-ci).

## Entrega

- As quatro skills e os testes `skill_coverage` e `skill_examples` chegaram no PR #16.
- O teto de `SKILL.md` ficou como constante do teste, `MAX_SKILL_LINES = 80` (`crates/omhl/tests/skills/coverage.rs`), e não como configuração.
- **Nota de 2026-11-18 (work item `perf3`).** O teto agora mora em `crates/omhl/tests/e2e/skills/coverage.rs`. O layout de `tests/` mudou; ver [Testes unitários no gate, integração no CI](../mechanical-gates.md#testes-unitários-no-gate-integração-no-ci).

## Follow-ups

- Cortar o débito 5: o adapter traduz o payload e passa o `Caller` pronto.
- Reavaliar a igualdade exata dos exemplos se a manutenção pesar.
- Avisar o work item de performance de que a linha de base dele deve ser tirada depois do merge do PR das skills: o work item troca um binário de teste por outro e soma a execução dos exemplos.
- Apagar `blobs/` antigos continua manual.
