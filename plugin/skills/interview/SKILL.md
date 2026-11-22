---
name: interview
description: Protocolo da entrevista de negócio com o dono, antes de gravar o work item. Use ao começar um work item `feat` ou `fix` (obrigatória) e, a pedido do dono, nos prefixos `refactor`, `tests`, `bugfix`, `chore` e `docs`, para esclarecer o pedido em rodadas de perguntas, medir a ambiguidade e gravar o work item e as premissas de negócio confirmadas.
---

# Entrevista de negócio

Esta skill exige a `directives-flow`. Se não estiver carregada, carregue-a. Ela traz as regras de gravação e o ciclo ([cycle.md](../directives-flow/references/cycle.md)). As rodadas de perguntas vêm antes do passo 1 do ciclo: cada premissa de negócio nasce numa rodada, redigida por você e confirmada pelo dono na hora, e só no fim o work item e as premissas são gravados; a [plan-consensus](../plan-consensus/SKILL.md) cobre só o técnico.

Você é a sessão principal e conduz a entrevista sozinha. Não escreve spec: a spec é do planner (passo 4 do ciclo).

## Quando usar

- **Obrigatória** em `feat` e `fix`: a entrevista vem antes do work item, e nada é gravado no banco antes de ela terminar.
- **Opcional** em `refactor`, `tests`, `bugfix`, `chore` e `docs`: só roda a pedido do dono. Sem o pedido, siga direto pela `directives-flow`.

## As rodadas de perguntas

- **Rodada 0.** Antes da primeira pergunta, liste as partes do objetivo do dono e peça que ele confirme, junte, divida ou adie cada uma. A lista vai para a seção `Partes` da nota e fica travada: a reescrita da nota a cada rodada não a altera. Parte adiada sai da cobertura.
- **Nome do dono.** Peça o nome dele (`person:<nome>`) na Rodada 0 ou na rodada 1, antes de a primeira premissa montar o `confirmed_by`.
- **Placar.** A cada resposta, dê uma nota de 0 a 1 a cada dimensão e calcule `ambiguidade = 1 - (objetivo × 0,35 + restrições × 0,25 + critério de sucesso × 0,25 + contexto × 0,15)`. Contexto é o quanto se entende do sistema existente para mexer nele com segurança. Mostre ao dono a tabela curta (dimensão, nota, lacuna) e a ambiguidade em %.
- **Parada.** A entrevista termina só com ambiguidade de **20% ou menos** e com cada parte ativa da Rodada 0 coberta por ao menos uma premissa de negócio confirmada. O limiar é fixo, sem configuração. Da rodada 3 em diante o dono pode encerrar antes: avise a ambiguidade que sobra e as partes sem premissa. Na rodada 10, pergunte se segue; na 20, pare com a clareza que houver.
- **Cobertura.** A cada rodada, registre na nota qual parte ativa já tem premissa confirmada e qual não tem; a retomada depois de compactação lê dali.
- **Pergunta.** Uma por mensagem, sobre a dimensão mais fraca (diga qual e por quê, em uma frase), com 2 a 4 opções, a recomendada primeiro, de modo que "sim" a aceite. Pergunte as suposições do dono, não uma lista de funcionalidades.
- **Premissa da rodada.** Com a resposta, redija a premissa de negócio que ela sustenta, mostre-a em português e peça que o dono a confirme ou corrija na mesma rodada. A premissa confirmada espera na nota até o fim: nada vai ao banco antes.
- **Prova da confirmação.** O `confirmed_by` da premissa leva em `question` a premissa como você a redigiu e em `answer` a confirmação ou a correção literal do dono, cada texto com até 2000 caracteres (o schema recusa mais). A pergunta da rodada e a resposta, inteiras, ficam só na nota.
- **Fato do código.** Leia você mesma, antes de perguntar, e cite o arquivo ou o símbolo que motivou a pergunta. Não há agente de busca no plugin.
- **Estado.** Fica em `.omhl/notes/interview-<slug>.md`, no checkout principal, reescrito a cada rodada: a pergunta e a resposta literais da rodada, inteiras, o placar, a dimensão mais fraca, a premissa redigida e a cobertura; a seção `Partes` não muda. Se a sessão cair, retome dali. O estado não vira registro: nenhuma rodada, nota ou placar vai ao banco, e a pasta fica fora de `.omhl/specs/`, para o `spec-lock` não congelar. O `<slug>` é provisório até o passo 1; se o do work item diferir, renomeie o arquivo.

## Modos de desafio (só quando o dono pedir)

Cada modo muda o ângulo da pergunta e roda no máximo uma vez.

- **Contrarian:** "e se o oposto fosse verdade?". Testa se o enquadramento é do projeto ou só hábito.
- **Simplifier:** "qual é a versão mais simples que ainda tem valor?". Separa a restrição necessária da suposta.
- **Ontologist:** "o que isto é, de fato?". Pergunta qual entidade é o núcleo e quais são apoio.

## Os passos

1. Mostre ao dono o resumo do que a entrevista esclareceu (objetivo, restrições, critério de sucesso, contexto) e proponha o prefixo, o id e o slug; a branch será `tipo/wi-<id>__<slug>`. A sessão confere com `omhl get work-items.<id>` (o id está livre).
2. Grave o work item (lote 1, como `claude`, com `omhl append` e uma `key` de idempotência; o formato está em [gravar.md](references/gravar.md)): `work-items.<id>`, `title` e `status` `open`. A sessão confere com `omhl get work-items.<id>` (existe e está `open`).
3. Grave as premissas já confirmadas nas rodadas (lote 2) num `omhl append`, com `key` de idempotência e **sem** nova rodada de confirmação em lote: cada uma leva `status` `confirmed`, `kind` `business`, o `work_item` e o `confirmed_by` da rodada, e o `text` em inglês. Se o lote 2 falhar, corrija o que a recusa apontou e regrave só ele (mesma `key` se o conteúdo não mudou, `key` nova se mudou); o lote 1 já está gravado. Depois chame a [plan-consensus](../plan-consensus/SKILL.md). A sessão confere com `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed`.
