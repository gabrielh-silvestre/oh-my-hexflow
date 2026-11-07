# Consenso, iteração 16: parecer do Architect (sonnet)

Plano avaliado: [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md) (670 linhas), [omhl-v1-marcos.md](../plans/omhl-v1-marcos.md), [omhl-v1-changelog.md](../plans/omhl-v1-changelog.md). Parecer anterior: [review-8-architect.md](review-8-architect.md).

- **Architect** (`tony-stark-architect-omhl-iter16`): **SOLIDO COM RESSALVAS**, **nenhum bloqueante**. B-A e R1 a R5 resolvidos.

## Resolução dos achados anteriores
- **B-A:** um só `assertions.human`; o 1º comando arma o hash e não cunha; o 2º cunha uma asserção presa a `{directiveId, scope, reason}` e ao `proposalId` (SLICE:112, 122, 126, 159, 249). Schema sem campo novo (`proposalId?` já existia). `PROJECT_SCOPE_NEEDS_APPROVAL` no servidor. "Segunda asserção" só como alternativa rejeitada (:650). Spec (:59), 8.1 (:612) e AC-8 coerentes.
- **R1:** a key vale por processo (hexlog `src/commands/process.ts:54-69`, nível 5, sob o lock); o fingerprint ignora key/agent/model e `BATCH_KEY_MAX` é 200 (`chain.ts`). `judge-ruling` mora no processo da unidade (MARCOS:64; SLICE:119, 282). Idêntico dá `replayed`; diferente dá `IDEMPOTENCY_CONFLICT` → `JUDGE_RULING_FINAL`; mapeamento por prefixo em :113.
- **R2:** 5 códigos em `errors.md` (:251), AC-8 (:159), tabela de processos (:122), `open_unit` (:290).
- **R3:** 8.1 com Q21, anulação e ruling final (:612-613).
- **R4:** `FORMAT_CATALOG` (`formats.ts`) só tem `git-sha`; `attachment` é tratado à parte (`definitions.ts:156`); `pattern`/`patternProperties` recusados (`schema-walk.ts:6`); o validador manda usar catálogo ou `minLength`/`maxLength` (`validator.ts:240`); `DATA_MAX_CHARS` (`record.ts:11`) e `RECORD_TYPE_MAX_CHARS` (`definitions.ts:9`) são 16.000. O Planner acertou.
- **R5:** nome da unidade <= 58 e `unit-<process>` <= 63 (:276, 290); `directiveId` sem pontos.

## Decisões novas do Planner
- **(a) Provedor falso por feature:** viável, sem brecha no build de produção. Testado em crate descartável: sem a feature, `cargo build --release` não contém `'fake/repo'` (o `grep -qa` falha, como o teste espera); com a feature, contém. Resíduo aceito: `cargo install --features fake-github` dá "CI verde" falso (mesmo nível do resíduo de Q14).
- **(b) `required-features`:** confirmado por teste: `cargo test --test slice` sem a feature FALHA ("target `slice` in package requires the features: `fake-github`"). `cargo test` sem `--test` e `clippy --all-targets` pulam o alvo em silêncio.
- **(c) `jr:<directiveId>` sob concorrência:** sustenta "primeiro ruling é final"; o segundo juiz em paralelo tem fingerprint diferente e recebe `IDEMPOTENCY_CONFLICT`.
- **(d) Premissa repetida com supersedes:** não viola a regra (`checkSuccession`, `relations.ts` ~243, exige mesmo processo/tipo/target). Com `--in`, a busca de duplicata tem de ser feita no processo do ancestral.

## Ressalvas (não bloqueantes; corrigir até o Step 2a/3)
1. **Caminho "sem remoto / sem token" sem teste.** Com a feature ligada o provedor falso ignora remoto e token (SLICE:307) e o `check.sh` sempre roda com `--features fake-github` (:222), então `no-github-remote` e `no-github-token` (R22, :44) nunca são exercitados pelo binário, e eles sustentam o "Stop chega a non-convergence". Tornar a seleção do provedor uma função pura, sem `cfg`, testada em unidade (a feature só injeta o falso depois), ou rodar também um `cargo test` sem a feature no `check.sh`.
2. **Guard do binário de produção só na Verificação 2a (:311).** Falta no §6 (:417-440) e no `check.sh`, sem `cargo build --release` explícito antes. Acrescentar `cargo build --release && ! grep -qa 'fake/repo' target/release/omhl` aos dois.
3. **`clippy --all-targets` sem a feature não compila `tests/slice.rs` (:222).** Usar `cargo clippy --all-targets --features fake-github -- -D warnings`.
4. **Teto de 16.000 do schema de TIPO** (`RECORD_TYPE_MAX_CHARS`; `evidence` embute o recibo) é prometido em :293, mas o §6 só traz o `jq` do `pattern`. Medir o JCS de cada tipo.
5. **"Premissa repetida → `created:false`" (:111) é ler e depois gravar, sem key.** Duas chamadas paralelas do agente com o mesmo texto bifurcam a linhagem; key permanente não serve (bloquearia regravar depois de um `revoke`). Mutex assíncrono por (projeto, processo) no servidor em volta de ler e gravar, ou declarar o resíduo.

## Melhorias opcionais
- O servidor pode recalcular `SHA-256(JCS{directiveId, scope, reason})` e exigir `proposalId` igual (hoje só confere presença; resíduo declarado em changelog ~486).
- O handshake poderia expor as features compiladas, para o `omhl setup` recusar um binário com `fake-github`.

## Regressões
Nenhuma referência quebrada nem contradição plano x spec x marcos; 21 ACs na fatia conferem; sandbox só como história ou nível 1 futuro.
