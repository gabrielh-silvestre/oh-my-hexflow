# Consenso, iteração 1: parecer do Critic

Agente: `benson-critic-omhl-plan` (sonnet). Plano avaliado: [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md). Veredito: **ITERATE** (0 CRITICAL, 7 MAJOR). Modo THOROUGH, escalado para ADVERSARIAL ao passar de 3 MAJOR.

## Critérios
1. Consistência princípio-opção: **FALHA**. O Princípio 1 ("o servidor decide; o hook é freio") contradiz o AC-6: o recibo não é assinado e o servidor só confere campos que quem forja também controla (criteriaHash, SHA, scriptDigest). O R1 item (3) ("forjar estado local sem o servidor não produz closure") é falso.
2. Alternativas: **FALHA**. D é espantalho ("não é opção viável"). B foi montada sem spikes. C é rejeitada atacando `git subtree`. T2 é invalidada em uma linha.
3. Risco: **PASSA COM RESSALVA**. Faltam: dono e cadência do `fork-drift.sh`; a pilha de hooks real do dono (RTK, OMC, context-mode); versão mínima do Claude Code (`onFailure` exige 2.1.295 ou mais).
4. Critérios de aceite: **FALHA (marginal)**, ~81% concretos (< 90%). Vagos: AC-11 (contexto cheio), AC-12 (3 combinações sem dizer quais; perna plugin sem detecção), AC-14, AC-16 (manual). O AC-1 do spec só é provado do lado servidor (sem idempotência de hooks, `.omhl/state`, `settings.json`).
5. Verificação: **FALHA**. Depende de `just`, `mise`, `hyperfine`, `cargo-nextest`, `cargo-insta`, nenhum instalado. O Step 0 não instala. `git range-diff upstream/main...HEAD` não é inventário de carry patches. `upstream/main` pressupõe branch `main`; o Hexlog está em `develop`. `claude -p "/omhl:omhl-init"` pressupõe o namespace (Q5).
6. Fatos x suposições: **PASSA COM RESSALVA**. Erros: (a) Step 4 diz que o `git remote -v` do Hexlog não listou remoto, mas existe `origin git@github.com:gabrielh-silvestre/hexlog.git` com branches `main` e `develop`; (b) F8 cita `process.ts:77-117` para `defineTool`, que vem de `kernel.ts`. A17 está subestimado: a doc diz que `/skill` digitado "bypasses PreToolUse" e que `UserPromptExpansion` "runs when a user-typed command expands".
7. Fidelidade: **FALHA ao spec, PASSA aos relatórios**. Não aparecem "grounded", "executed", "omhl detect", "reabre"; "extends" só na seção 8. O baseline de falhas pré-existentes (PA-fw) foi ignorado sem comentário.
8. Vagos e escopo: **PASSA COM RESSALVA**. "curto e constante", "Stop pequeno e tabelado", "esqueleto fino". 21 ACs contra 11 do spec; 8 spikes; Q1 a Q13. Nenhum AC é marcado como `spec` ou `plan-added`.

## Achados MAJOR
- **M-1** Integridade do recibo só do hook; R1(3) afirma o contrário. Correção: o PostToolUse grava um MAC do recibo com o segredo que o hook já usa, o PreToolUse injeta o MAC em `omhl.attested` e o servidor o verifica (`RECEIPT_UNATTESTED`, com AC); ou reescrever o Princípio 1 e o R1(3) e mover o resíduo.
- **M-2** Fato errado sobre o remoto. Ajustar o Step 4, apontar `upstream` para `origin`, fixar a branch seguida e usar o mesmo nome nos comandos.
- **M-3** Regras do spec sem AC nem adiamento: registro `bypass` no Hexlog citado no PR; estados `grounded` e `executed`; unidade fechada com órfão reabre; `extends`; `omhl detect`; processo sucessor de major (filhos abertos terminam na major antiga; como o AC-2 provoca `MAJOR_MISMATCH` se `define_type` é bloqueado pelo AC-13); versão da granularidade fixada na abertura; idempotência do `/omhl-init` no lado plugin. Cada item entra com AC ou em "Fora da fatia (adiado)" com justificativa.
- **M-4** A pilha de hooks real do dono (RTK condensa e reescreve a saída do Bash; OMC e context-mode têm hooks) pode quebrar a persistência do recibo. O PostToolUse exige o comando "exatamente" `omhl check` e parseia o stdout. Correção: P0 e S2 rodam com a pilha completa e provam que o stdout chega intacto, ou o recibo sai com delimitadores fixos.
- **M-5** Contrato de `criteria` e interface de `omhl check` ausentes (`criteria.schema.json`, `criteria.params.allow`, `scriptDigest`, retry, `criteriaHash`). O bootstrap cria as 3 diretivas mínimas no Step 1, antes de o Step 2 definir o check. Não está dito como `omhl check` recebe a diretiva, já que o `omhl` não fala com o Hexlog. Correção: `criteria.schema.json` no Step 1, assinatura da CLI e AC.
- **M-6** `bootstrap_project` cria diretivas por serviços internos, sem token, e o PostToolUse só projeta em `create_directive`/`annul_directive`. Dizer como o bootstrap projeta e que a criação inicial sem aval é exceção do `/omhl-init`.
- **M-7** A perna plugin do handshake não tem enforcement: o `omhl` injeta a própria constante e o fork compara com a dele; o contrato do `plugin.json` só aparece no SessionStart, que não bloqueia. Correção: o `omhl` lê o `plugin.json` (ou recebe o contrato por argumento no `hooks.json`) e o PreToolUse nega se divergir.

## Minor
1. Definir `allow` ou `ask` (permissionDecision) para as tools de domínio; `allow` pula o prompt (decidir no S1).
2. Como o `omhl` sabe que o projeto é gerenciado e descobre o process (aberto no spec).
3. AC-1 lista 6 tipos, mas o Step 1 cria registros de granularidade: falta o tipo `granularity` (e o de decisão/N/A).
4. `close_unit` não exige árvore limpa no fechamento.
5. `rust-version = "1.85"` copiado; confirmar o MSRV de hmac 0.13, sha2 0.11 e minijinja 3.
6. Sem versão mínima do Claude Code no handshake.
7. `fork-drift.sh` sem dono nem cadência.
8. O teto 5 do Stop não deve ser atribuído ao BMAD.
9. Rotular ACs como `spec` ou `plan-added`.

## Falta
- Gates de decisão por pergunta: só Q1 a Q4 têm prazo; Q6 é necessário no Step 1 e Q9 e Q10 antes do Step 2.
- "CI verde" real nunca é exercitado (repo só git local, provedor falso): declarar no AC-16.
- Serialização do nonce de uso único. Como o servidor MCP localiza o segredo.

## Mudanças exigidas (bloqueiam a aprovação, por gravidade)
1. M-1. 2. M-3. 3. M-5 (`criteria.schema.json` e CLI de `omhl check`, com AC). 4. M-7 (e AC-12 com as 3 combinações listadas). 5. M-6. 6. M-4. 7. M-2. 8. Verificação: instalar ou trocar `just`, `mise`, `nextest`, `insta`, `hyperfine` por comandos cargo puros no Step 0. 9. Alternativas: remover D da lista; reescrever B como "spikes primeiro, depois peça por peça" e C com uma variante real de monorepo; justificar T2 melhor. 10. Tornar concretos AC-11, AC-12, AC-14 e AC-16 para passar de 90%.

Opcionais: Minor 1 a 9, em especial 4, 1 e 9; declarar no AC-16 que o e2e usa provedor falso; gates de decisão para Q6, Q9 e Q10; A17 como fato parcial; registrar por que o baseline do OMC foi descartado.
