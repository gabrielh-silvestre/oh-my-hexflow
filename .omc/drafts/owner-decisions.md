# Decisões do dono sobre as questões abertas do plano

Data: 2026-10-31. Plano: [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md). Respostas do Gabriel às questões que pedem decisão dele.

## Decididas

- **Q9 (Stop como freio): ACEITO.** O `Stop` é um empurrão, não uma trava. A garantia fica no servidor (`close_unit`) e no `PreToolUse`.
- **Q10 (como o `omhl check` fala com o GitHub): API do GitHub por SDK/API em código Rust**, sem `gh` e sem MCP. Fica aberta a sub-questão do token (onde mora; ver abaixo).
- **Q14 (sandbox): OPÇÃO C, SEM SANDBOX**, com **observabilidade e métricas** para validar depois se faz sentido ter algo mais forte para barrar. Motivo do dono: o objetivo é disciplina do agente (como o Ralph do OMC), não barreira contra burla deliberada. Consequências no plano:
  - Sai o sandbox como pré-condição: o S6 deixa de bloquear a fatia; saem `failIfUnavailable`, `allowUnsandboxedCommands: false`, `attested.sandbox`, `SANDBOX_NOT_STRICT` e o AC-19 como pré-condição; saem `bubblewrap` e `socat` dos pré-requisitos; a Q14 e a Q18 (regra sobre `-p` aninhado e S6(d)) precisam ser reescritas; R21 e as linhas "sandbox ausente" da tabela de garantias mudam.
  - O Princípio 1 e a tabela de garantias passam a dizer abertamente: **freio, não barreira**. O envelope HMAC, o recibo com MAC e o guard continuam, como freio e como evidência de adulteração (forjar vira ato deliberado, detectável e registrado), não como barreira. O R11 ("CI verde" auto-certificado) já era assim e continua.
  - O núcleo do AC-15 vira "guard como freio com registro de toda negação", sem a barreira do sandbox.
- **Q19 (cron): MANTER ATIVO.** Nada de `CLAUDE_CODE_DISABLE_CRON` nem `permissions.deny` em `CronCreate`/`ScheduleWakeup`/`loop`. Não remover funcionalidade nativa. Se a sonda S7 mostrar que prompt agendado pode cunhar asserção humana, a regra é desligar as asserções humanas (não o cron).
- **Q20: OK** (manter o token humano na fatia, com o marco fatia-1 antes dele).
- **Q21: OK** (corrigir premissa herdada no processo do ancestral, `--in`).
- **Q22: OK** (bypass: a recomendação do plano).

## Ainda sem resposta do dono (tratar como "recomendação do plano, pendente de confirmação")
Q1 a Q8, Q11 a Q13, Q15 a Q17, Q23 (as demais 15, com recomendação), e a sub-questão do token do GitHub (Q10b).

## O que o dono pediu de novo: observabilidade e métricas
Objetivo: medir se o freio basta ou se vale algo mais forte (sandbox ou outra barreira). Requisitos do dono: simples, sem nova pilha; serve para uma decisão futura baseada em dados.

## Atualização
- **Q10b (token do GitHub): VARIÁVEL DE AMBIENTE.** O `omhl check` lê o token do ambiente (nome a definir, por exemplo `GITHUB_TOKEN`). Sem sandbox, o agente pode lê-la: registrar como resíduo declarado, e a observabilidade deve marcar tentativas de ler o ambiente (por exemplo, `env`, `printenv`, `/proc/*/environ`). Recomendar token com escopo mínimo (somente leitura de checks, PRs e reviews).
- **Produto, todas aprovadas conforme a recomendação do plano:** Q3 (3 diretivas `mechanical` de PR, nenhuma `product`), Q5 (`/omhl:...`, salvo se a sonda S4 provar o nome sem prefixo), Q6 (fork `hexlog-omhl` em `/home/gabriel/personal/hexlog-omhl`, base `main`, tag `vX.Y.Z-omhl.N`), Q12 (aprovar o ADR do fork com numeração própria), Q15 a Q17 (adiar sucessão de major, bypass registrado no Hexlog e estados `grounded`/`executed` com reabertura por órfão).
- **Técnicas (Q1, Q2, Q4, Q7, Q8, Q11, Q13, Q23):** o dono pediu explicação do impacto antes de decidir.

## Decisões técnicas do dono (2026-10-31), com ajustes
- **Q1: OK, com abstração.** `kind: decision` em `register_premise`, mas o processo deve ser abstraído para dar menos carga e responsabilidade ao agente: o agente passa só o conteúdo (texto da decisão, opcionalmente as diretivas); o servidor/ferramenta deriva `target`, relações (`anchored-in`) e `checkedAgainst` a partir de `applicable_directives`.
- **Q2: MUDANÇA DE SPEC.** O "não se aplica" é declarado **pelo agente**, mas **somente usando o Haiku como navegador/juiz do grafo** (o juiz lê o ramo de diretivas e dá o veredito). Não exige mais o token humano para o N/A. Isso traz o modo `judge` (Haiku) para a v1, ao menos para o N/A. O Planner deve propor: como o Haiku é invocado (subagente do plugin com modelo haiku e ferramentas só de leitura), como o servidor sabe que a chamada veio do juiz (o hook recebe `agent_type` em subagentes), e se o juiz entra na fatia ou como marco logo depois (com a Q3, a fatia não tem diretivas `product`, então o N/A não é exercitado no início).
- **Q4: OK.**
- **Q7:** envelope **2 min** (era 45 s); asserção humana 10 min (OK); teto do `Stop` 5 (OK); estado velho 24 h (OK); **a evidência de CI vale até o próximo commit** (sai o `maxAge` de 15 min e o `EVIDENCE_EXPIRED` por tempo; a validade é só presa ao SHA).
- **Q8: OK.**
- **Q11: OK**, e o comando humano também pode **aprovar uma premissa pré-sugerida** pelo agente (fluxo de proposta e aval, hoje só de diretivas, vale para premissas).
- **Q13: sem `scriptDigest`.** Não entrar nesse nível. Tratar com **instrução ao agente + verificação se o arquivo foi editado via git**, levando a um **gate humano** para conferir se a edição era trabalho legítimo.
- **Q23: OK** (`outcome`).
