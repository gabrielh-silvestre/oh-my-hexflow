# Consenso, iteração 15: parecer do Architect (sonnet)

Plano avaliado: [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md) (657 linhas), [omhl-v1-marcos.md](../plans/omhl-v1-marcos.md), [omhl-v1-changelog.md](../plans/omhl-v1-changelog.md). Parecer anterior: [review-7-architect.md](review-7-architect.md).

- **Architect** (`walter-white-architect-omhl-iter15`): **SOLIDO COM RESSALVAS**. B1 a B4 e M2 a M5 resolvidos. As decisões novas do Planner não quebram regra do Hexlog nem contradizem AC-1, AC-7, AC-20 ou o spec. 1 bloqueante e 5 ressalvas, todos corrigíveis só com texto. Não executou nada nem leu o parecer do Critic.

## Itens anteriores (resolvidos)
- B1: SLICE:151 (AC-1), :248 (definitions.json), :265-281; 8 tipos, 6 relações, 0 gates; R9 (:399) e 3.1 (:201) registram que definição acrescentada depois só vale para processos novos.
- B2: sem `verdict`/`judge-verdict`/`jv.`; só restam FLOW_TERMS (:259) e o grep do §6 (:430, :441). Nenhum nome novo está em FLOW_TERMS (judge-ruling, ruling, jr, ruled-on, annuls, anchored-in, rests-on, evidence-of, inProcess, PROCESS_NOT_ANCESTOR, ANCESTOR_NOT_OPEN, JUDGE_RULING_FINAL).
- B3: cabeçalho da 8.1 (:589) e linha "Escopo da v1" (:603); `checkedAgainst` virou nota histórica no spec (:148). Resta R3.
- B4: SLICE:226, :236, :155. Residual: `branchRead` do juiz, que o marco 2 já falha fechado.
- M2/M3 (MARCOS:59-60, SLICE:359), M4 (tabela SLICE:113-124), M5 (A16 fora do Step 0, tabela do §7 com S3-S5).

## Decisões novas do Planner (compatíveis)
(a) `judge-ruling` único com `ruling=not-applicable`, alvo `jr.<directiveId>`: o alvo passa na regex (`ids.ts:15-18`); `derivesFrom` para diretiva em outro processo é permitido (só `supersedes`/`revokes` ficam no mesmo processo: `relations.ts`, `static.ts:119`). "Um só" por alvo é checagem do servidor (R1). (b) 8 tipos/6 relações/sem gates: `create_process` só recusa projeto totalmente vazio (`process.ts:46-48`); ressalva R4. (c) premissa humana e annulment no processo de diretivas, `scope=unit` recusando unidade com closure: coerente; faltam códigos (R2); problema B-A. (d) `--in`/`inProcess?`/PROCESS_NOT_ANCESTOR/ANCESTOR_NOT_OPEN: respeita o Hexlog (AC-20 :170, AC-3 :153, errors.md :250); menores: sem `--in`, supersede de premissa herdada cai em `cross-process-currency` (`errors.ts:9`) sem dica de `--in`; `applicableAtRegistration` de premissa gravada no ancestral não diz de qual unidade deriva. (e) `annul_directive` no Step 3: ordem correta.

## Bloqueante

### B-A. "Segunda asserção com `scope: project`" não existe no schema congelado e contradiz o ciclo de vida
- SLICE:121, :347 e AC-8 (:158) dizem que `scope=project` "exige a segunda asserção". O schema do envelope tem UM objeto `assertions.human` (:248). O ciclo de vida (:112) diz "no máximo uma asserção pendente por tool… um novo comando para a mesma tool substitui a anterior".
- Consequência: a 2ª asserção apagaria a 1ª, ou o schema ganharia campo depois do Step 1, derrotando o congelamento. Como `annul_directive` agora é testado no Step 3, o furo entra no caminho crítico.
- Exigido (sem reabrir decisão do dono; o spec só diz "confirmação extra"): escolher a representação antes do Step 1 e escrevê-la em AC-8, na tabela de :121, no Step 3, em `contract/commands.md` e no schema `human`. Opção 1 (mais barata): uma única asserção cujo `scope: project` só é cunhado por forma distinta do comando (`/annul --project <id> --confirm`). Opção 2: reusar o aval em dois passos (arma e aprova) das diretivas também para a anulação de projeto.

## Ressalvas
- **R1.** "Primeiro ruling é final" é check-then-write (SLICE:281, :343), o mesmo tipo de corrida que :112 trata para o nonce. Dois juízes paralelos podem gravar rulings diferentes no mesmo alvo. Correção: gravar o lote do juiz com `key="jr:<directiveId>"` (cabe em `BATCH_KEY_MAX=200`, `chain.ts:10`); lote diferente vira `IDEMPOTENCY_CONFLICT` mapeado para `JUDGE_RULING_FINAL`. Acrescentar `jr:` à regra de mapeamento de :112.
- **R2.** `errors.md` se diz "todos os códigos" (:250) mas o AC-8 (:158) recusa sem nomear: diretiva já anulada no mesmo escopo, `scope=unit` de unidade com closure, unidade inexistente em `scope=unit`. Nomear agora (ex.: `DIRECTIVE_ALREADY_ANNULLED`, `UNIT_CLOSED`, `UNIT_NOT_FOUND`), pois o arquivo congela no Step 1.
- **R3.** A 8.1 está incompleta: Q21/`--in` (spec :46 e :85 não o conhecem), annul `scope=unit` recusando unidade fechada, e "primeiro ruling é final" com `inconclusive` terminal não aparecem na 8.1 nem em "o que o plano acrescenta ao spec" (:583). Remover do corpo as notas históricas "Aperto em relação à iteração 14" (MARCOS:64) e "(escolha da iteração 15…)" (MARCOS:63).
- **R4.** Limites do Hexlog não mencionados: (1) `define_type` recusa `pattern`/`patternProperties` (`docs/tools.md:41-80`, `INVALID_SCHEMA` pattern-not-allowed): o `describe_type` do AC-1 deve falhar com esses termos e hashes de alvo usam `format`; (2) `data` de cada registro tem teto de 16.000 caracteres canônicos: teste com o maior `evidence` (3 tentativas) e o maior `closure`.
- **R5.** O escopo `unit-<process>` (SLICE:275) pode estourar o segmento de 63 caracteres (`NAME_SRC {0,62}`, `ids.ts:4`) quando o processo tem mais de 58: fixar teto para o nome da unidade ou usar hash. `directiveId` é "um nome válido" em SLICE:108 e um alvo com pontos em MARCOS:59: fixar como `Name` único (sem pontos).

## Melhorias opcionais
- Renomear o ramo `kind: "not-applicable"` de `register_premise` (SLICE:343, MARCOS:62) para `kind: "ruling"`, pois agora carrega também `applicable` e `inconclusive`.
- `applicableAtRegistration` na premissa humana da anulação (processo de diretivas, sem unidade) deve ser opcional no schema ou vazio por definição.
- M1 do Architect (S9(i)-(iv) para o Step 0): concorda com a recomendação do Planner; levar ao resumo do §7, onde hoje só aparece no changelog.
- Dois "primeiros" no Step 1: AC-17 é o "primeiro teste a escrever" (:247) e `definitions.json` é "congelado antes dos demais schemas" (:248). Distinguir.
