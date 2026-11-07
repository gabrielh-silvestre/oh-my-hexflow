# Consenso, iteração 15: parecer do Critic (sonnet)

Plano avaliado: [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md) (657 linhas), [omhl-v1-marcos.md](../plans/omhl-v1-marcos.md), [omhl-v1-changelog.md](../plans/omhl-v1-changelog.md). Parecer anterior: [review-7-critic.md](review-7-critic.md). Par: [review-8-architect.md](review-8-architect.md).

- **Critic** (`benson-critic-omhl-iter15`): **ITERATE**, 2 bloqueantes leves e 1 major, todos em peças novas da iteração 15; correções de texto, sem redesenho. B1, B2, M1, M2 da iteração 14 e as melhorias 1 a 8 resolvidos. Avaliou de forma independente. ACs concretos: 22 de 24 (91,7%); citações: 19 de 19 corretas. Linhas referem-se ao plano principal, salvo indicação.

## Resolvidos
- B1: Step 3 cria `create-directive.ts`/`annul-directive.ts` (`:342`), verificação cobre AC-3(c)(h) e AC-8 servidor (`:350`), AC-8 dividido em servidor/hook (`:158`), Step 5 ficou com aval, comandos e projeção (`:375`).
- B2 (no desenho; ressalva no bloqueante 1): Step 4 cria `tests/slice.rs`, `scripts/e2e.sh`, runbook (`:364`); porta `cargo test --test slice` (`:369`, `:426`).
- M1: `--in`/`inProcess?` no schema e na gramática (`:248-249`), códigos em `errors.md` (`:250`), AC-20 (`:170`), linha de 3.1 removida (`:179`).
- M2: 8 tipos, 6 relações, 0 gates (`:266-281`); consistente com AC-1 (`:151`), `bootstrap_project` (`:290`), R9 (`:399`), ADR (`:620`, `:638-639`) e MARCOS:56,63,66. O Hexlog suporta relações entre processos; só `supersedes`/`revokes` ficam no mesmo processo (`relations.ts:262-264`, `docs/tools.md:158`).
- Melhorias 1-8 aplicadas; a 9 ficou de fora, aceito.

## Bloqueantes

### 1. A porta do marco fatia-1 não é executável como escrita
- `:369` e `:426` mandam `cargo test --test slice`, sem `--features fake-github`; só o `scripts/check.sh` usa a feature (`:221`). `:173` exige provedor falso de GitHub no e2e; `:306` diz que ele só existe atrás da feature e proíbe seleção por variável de ambiente ou arquivo, sem canal para o subprocesso `omhl check ci-status` receber a fixture. Sem a feature, `ci-status` devolve `error: no-github-remote` e o `close_unit` recusa; se o teste for gated por `cfg(feature)`, roda 0 casos e passa sem provar nada. Nenhum passo diz quem gera o bundle do fork nem onde o `slice.rs` o encontra (`/home/gabriel/personal/hexlog-omhl`).
- Exigido: `cargo test --features fake-github --test slice` em `:369`, `:426` e no AC-E2E; declarar que no build com a feature o provedor falso é fixo e determinístico (a proibição de seleção por ambiente vale só para o build de produção) e como o negativo "CI vermelho" é injetado; listar o `npm run build` do fork e o caminho/variável do bundle como pré-requisito do `slice.rs`.

### 2. A "segunda asserção" do `annul_directive` com `scope = project` não cabe no contrato congelado
- `:121`, `:158`, `:347` exigem a "segunda asserção com `scope: project`"; o envelope tem um único `assertions.human` (`:248`); o ciclo de vida admite uma asserção pendente por tool e o novo comando substitui a anterior (`:112`); o lote usa uma só `key = human:<nonce>`; o Step 1 congela o schema.
- Exigido, escolher: (a) definir a "confirmação extra" do spec como segundo passo do fluxo de proposta e aval (armar e aprovar com `scope=project` no hash completo, reusando `:125`), gerando uma única asserção sem schema novo; ou (b) transformar `human` em lista no schema congelado e abrir exceção no ciclo de vida. **Recomenda (a).** Corrigir os três pontos e o teste do AC-8.

## Major

### M1. `JUDGE_RULING_FINAL` por "checagem de existência" não é atômica
- `:281` e MARCOS:64 dizem que `jr.<directiveId>` "aceita um registro só"; o Hexlog não impõe unicidade de target (`relations.ts:192-203`; `docs/tools.md`: id errado "bifurca a linhagem"); o plano rejeita "verificar dentro do lock" (`:112`). O teto de 2 invocações permite dois juízes em paralelo gravando dois `judge-ruling` no mesmo alvo.
- Exigido: gravar com `key = "judge:<unit>:<directive>"` e mapear `IDEMPOTENCY_CONFLICT` para `JUDGE_RULING_FINAL`, o padrão de `:112`; acrescentar ao Step 3 e ao AC-23 teste de duas gravações concorrentes.

## Melhorias opcionais
1. Step 4 "parte humana" sem pacote de trabalho (`:213`, `:356`, `:360`, `:375`): `/directive`, `/annul`, `confirm-check-edit` aparecem no Step 4 e são criados no Step 5. Remover "4 (parte humana)" da ordem ou listar o que ele entrega.
2. Dono do `attested.checkFiles`, `BASE_UNRESOLVED` e `directivesRead` na fase agent-only: o lado hook do AC-22 só é verificado no 2b-humano (`:334`); o lado hook do AC-4 não aparece em verificação; o servidor só exige `treeClean`, `headSha`, `headRef` (`:346`), então `checkFiles` ausente vale como lista vazia (falha aberta). O hook emite `checkFiles: []` no 2b-agent e o servidor recusa campo ausente.
3. Faltam códigos de erro no AC-8 (`:158`): diretiva já anulada no mesmo escopo; `scope = unit` com `closure`.
4. `contract/definitions.json` "congelado antes dos demais schemas" (`:248`) é circular (`evidence` embute o recibo, `directive` embute o `criteria`): congelar os três juntos.
5. O AC-1 compara `describe_type` com `definitions.json`; `describe_type` omite `pattern`/`patternProperties` e `register` os ignora (`docs/tools.md:24-27`): comparar módulo esses campos e não depender de `pattern`.
6. O Step 3 precisa de assinador de envelope em TypeScript; o Step 1 só cria a verificação (`:256`) e os vetores de envelope não constam em `:247`.
7. Nomes no juiz: ramo `kind: "not-applicable"` aceita `applicable`/`inconclusive` (`:343`), renomear; `changedFiles` "a do AC-22" é ambíguo (todos os alterados ou só os de check); nomes de arquivo são canal controlado pelo agente para dentro do prompt do juiz.
8. A recomendação do Architect M1 (S9(i)-(iv) no Step 0) só existe no changelog, enquanto o Step 1 diz "nenhum aberto" (`:244`) e o §7 diz tudo decidido: criar a Q24 no §7 ou aplicar a recomendação.

## Ordem executável (Steps 0-5)
Nenhum passo depende de artefato posterior. Falta o assinador TS (melhoria 6). A ressalva do bundle e da feature está no bloqueante 1. Sem regressão de sandbox; 0 links quebrados nos 4 arquivos; não resta `judge-verdict`, `jv.`, `JUDGE_VERDICT` nem "Q21 aberta"; todo código de erro nomeado consta em `errors.md`, exceto os da melhoria 3.

## Contagens
- ACs: 22 de 24 concretos (AC-1 a AC-23 mais AC-E2E). Vagos: AC-18 (prova é o resultado da P0, sem teste nomeado) e AC-23 parte B (depende do Haiku real); a parte A é concreta. Fatia: 21 ACs, 8 `plan-added` (AC-13 a AC-19 e AC-22), 8 tipos, 6 relações; consistente entre `:147`, `:408`, MARCOS:8, `:268-281`.
- Citações: 19 de 19 corretas (Hexlog: `relations.ts:252,262-264`, `chain.ts:10`, `kernel.ts:193,250`, `query.ts:354`, `attachment.ts:39,59`, `mcp/server.ts:14-21`, `directory.ts:11-16`, `build.ts:19-26`, `installation.ts:50,123`, `definitions.ts:62-68`, `manifest.ts:9-22`, `no-flow-terms.spec.ts`, `eslint.boundaries.js:27-30,117-135`, `commands/process.ts:54-69`, `docs/tools.md:113-134`, `invariantes.md:22`, `gate.ts`). Não conferiu URLs da doc do Claude Code, números de issue, METR 30,4%, fatos de máquina do F15.

## Perguntas abertas
- A anulação e a premissa humana do mesmo lote não têm relação nomeada entre si (vínculo só por `nonce` e lote). Aceitável; confirmar antes de congelar as 6 relações.
- Dois registros no mesmo `premise.<hash>` (mesmo texto) no processo de diretivas: o Hexlog aceita; o plano não diz o que `applicable_directives` faz.

## RALPLAN
Princípio/Opção: Pass. Alternativas: Pass. Riscos/Verificação: Pass com ressalva (bloqueante 1, M1). ADR: Pass.
