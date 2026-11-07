# Consenso, iteração 16: parecer do Critic (sonnet)

Plano avaliado: [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md) (670 linhas), [omhl-v1-marcos.md](../plans/omhl-v1-marcos.md), [omhl-v1-changelog.md](../plans/omhl-v1-changelog.md). Parecer anterior: [review-8-critic.md](review-8-critic.md). Par: [review-9-architect.md](review-9-architect.md).

- **Critic** (`raven-critic-omhl-iter16`): **APPROVE com ressalvas**, sem bloqueante. 2 correções de texto fortemente recomendadas (M-a, M-b), de uma linha cada. Avaliou de forma independente. ACs concretos: 22 de 24 (91,7%; 23 de 24 se o AC-18 contar por :327); citações: 25 de 25 corretas. Linhas do plano principal.

## Resolvidos
- **Bloqueante 1 (porta do marco):** `cargo test --features fake-github --test slice` em :174, :176, :365, :370, :427; provedor falso determinístico no build de teste e ausente do de produção (:307, :311); negativo "CI vermelho" por `headRef` terminado em `-ci-red`; pré-requisitos do `slice.rs` em :174. Testou `required-features` em crate no scratchpad: `cargo test --test slice` sem a feature dá erro; `cargo test` simples pula o alvo ("0 tests"); com a feature roda.
- **Bloqueante 2 (anulação de projeto):** "segunda asserção" só como alternativa rejeitada (:650); dois passos do aval gerando uma só asserção com `proposalId` (:112, :122, :126, :159, :204, :249, :376); spec:59 bate.
- **M1 (JUDGE_RULING_FINAL):** key `jr:<directiveId>` (:113, :119, :282, :344); teste de duas gravações concorrentes (`Promise.all`) em :351 e MARCOS:77; prova de escopo conferida (`process.ts` 55-70, `register/AGENTS.md:15`, `chain.ts:59`, `chain.ts:10`).
- **Melhorias 1-8:** aplicadas (:214, :357; :172, :322, :347; :159; :249; :152; :248, :349; MARCOS:59; Q24 em :566).

## Achados não bloqueantes (aplicar na mesma passada)

### M-a. `scripts/check.sh` contradiz a porta e o `slice.rs` (efeito colateral da correção do bloqueante 1)
- :222 define `check.sh` como `cargo test --features fake-github`; com a feature ligada, o `required-features` do slice é satisfeito e esse comando também roda `tests/slice.rs`. :174 diz que o `cargo test` sozinho não constrói o bundle e que bundle ausente falha o teste; :370 e :427 tratam o slice como execução separada. `check.sh` é a verificação de cada passo (:335, §6): do Step 4 em diante fica vermelho sem bundle, ou verde com bundle velho; nada reconstrói o bundle quando o fork muda, então o marco pode passar com bundle desatualizado.
- Exigido (uma linha), escolher: `check.sh` chama o mesmo passo de build do fork que o `e2e.sh` antes do `cargo test`; ou `check.sh` roda `cargo test --features fake-github --lib --bins` mais os alvos nomeados, deixando o slice só para `e2e.sh` e para a porta. Em qualquer caso, `slice.rs` falha se o bundle for mais antigo que o HEAD do fork, ou o `e2e.sh` é a porta oficial.

### M-b. Prazo da Q24 incoerente com o passo que ela altera
- Q24 não bloqueia o início do Step 1 e a resposta é exigida antes de congelar `attested` (:245, :566), coerente com o congelamento no Step 1. Mas :245 diz que, com "sim", o S9(i)-(iv) roda "no Step 0"; pela ordem, o Step 0 já terminou quando o Step 1 chega ao congelamento (fim dele). Um "sim" obriga a reabrir o Step 0, e a medição rodando depois do congelamento chega tarde.
- Exigido: reescrever o prazo como "resposta até o fim do Step 0 (no mais tardar, com tempo de rodar o S9(i)-(iv) antes de congelar o `attested`)". Acrescentar ao Step 1 um ponto de verificação explícito "perguntar ao dono a Q24 antes de congelar".

## Melhorias opcionais
1. `proposalId` sem checagem de conteúdo (:122; resíduo em changelog:488, não na tabela de riscos): o servidor pode recomputar `SHA-256(JCS{directiveId, scope, reason})` e comparar; dá ao AC-8 um negativo ("`proposalId` de outro conteúdo": `HUMAN_ASSERTION_MISMATCH`).
2. Estado "armado" sem tipo definido: não consta nos tipos de `src/state/` (:302: session, unit, receipt, bypass, stop, assertion); acrescentar `armed`/`proposal`. Dizer se o 1º comando (que arma e não cunha) substitui uma asserção pendente da mesma tool (:113).
3. `hooks.json` no 2b-humano: :357 dá a ele `UserPromptSubmit` e `SessionEnd`, mas o "Criar" do 2b-humano (:332-333) não lista a edição.
4. `init_idempotent` (:152, :364): `/omhl-init` é uma skill (instrução ao modelo) e a CLI listada (:253) não tem subcomando de init; definir `omhl init` (parte determinística) ou dizer o que o teste executa.
5. O prefixo `bootstrap:` mapeia para `ASSERTION_INVALID` (:113), o mesmo código de `seedHash` errado (:152): uma frase em `errors.md`.
6. Detalhes do e2e: o 4º negativo precisa de um branch `*-ci-red` no repositório temporário; o negativo "premissa revogada" precisa de uma decision com `restsOn` para gerar órfão (a sequência de :174 não a cria); o `! grep -qa 'fake/repo' target/release/omhl` (:311) precisa de `cargo build --release` antes.
7. Replay do juiz (:282): "lote idêntico" é mesmo ruling, motivo e `branchRead`, mas o registro também inclui `judge{agentId,...}` no fingerprint; um 2º juiz com o mesmo parecer dá `JUDGE_RULING_FINAL`, não `replayed`. Uma frase.

## Contagens e consistência
- ACs concretos: 22 de 24 (AC-1 a AC-23 mais AC-E2E). Vagos: AC-18 (prova é o resultado da P0, :169) e AC-23 parte B (Haiku real). Fatia: 21 ACs, 8 `plan-added`; bate com :148, :409, MARCOS:3.
- Citações: 25 de 25 corretas no Hexlog (`process.ts` bloco do register em 55-70, plano cita 54-69: desvio de 1 linha; `chain.ts:10,59`; `relations.ts:252`; `kernel.ts:193,250`; `query.ts:354`; `mcp/server.ts:14-21`; `definitions.ts:62-68`; `directory.ts:11-16`; `build.ts:19-26`; `eslint.boundaries.js:27-30` e 119-135; `installation.ts:50,123`; `manifest.ts:9`; `no-flow-terms.spec.ts:27-37`; `skill-coherence.spec.ts:85`; `eslint.config.js:67`; `ports.ts:87`; `attachment.ts:39,59`; `mcp/tools/process.ts:13-20`; `invariantes.md:22`; `docs/tools.md`; `definition.ts:53`; `formats.ts`).
- Links: 0 quebrados. Sem `judge-verdict`/`JUDGE_VERDICT`/"Q21 aberta"; `scriptDigest`/`maxAge` só históricos; sandbox só "sem sandbox", S6 opcional ou nível futuro. Todo código de erro nomeado consta em `errors.md` (:251). Sem contradição plano x spec x marcos. Ordem executável (0, 1, 2a, 2b-agent, 3, 4, marco, 2b-humano, 5) sem dependência posterior.
- Observação: o changelog é cronológico e a iteração 16 está no fim (changelog:438); não é defeito.

## Justificativa
Os dois bloqueantes e o M1 do parecer anterior estão resolvidos; sem achado CRITICAL. M-a e M-b são MAJOR-leve; M-a é erro introduzido pela correção do bloqueante 1, então recomenda aplicar M-a e M-b antes de entregar ao dono.
