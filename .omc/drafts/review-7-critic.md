# Consenso, rodada final: parecer do Critic (sonnet)

Plano avaliado (iteração 14): [omhl-v1-vertical-slice.md](../plans/omhl-v1-vertical-slice.md), [omhl-v1-marcos.md](../plans/omhl-v1-marcos.md), [omhl-v1-changelog.md](../plans/omhl-v1-changelog.md). Spec atualizado: [deep-interview-oh-my-hexflow-design.md](../specs/deep-interview-oh-my-hexflow-design.md). Pareceres anteriores: [review-6.md](review-6.md).

- **Critic** (`mandy-critic-omhl-final-round`): **ITERATE**, 2 bloqueantes de texto/ordem, 2 majors, sem redesenho. 44 citações conferidas, 44 corretas. ACs concretos: 21 de 23 (91%); com o AC-E2E, 22 de 24. Não executou nada nem editou arquivos.

## Bloqueiam a aprovação

### B1. A verificação do Step 3 cobre `create_directive` e `annul_directive`, que só são criadas no Step 5
- `omhl-v1-vertical-slice.md:316` diz "cobre AC-3, AC-4, AC-6, AC-7, AC-8 (servidor)..."; o §6 repete AC-8 em `:400`. O "Criar" do Step 3 (`:309-313`) lista só register-premise, record-evidence, applicable-directives e close-unit. `create-directive.ts` e `annul-directive.ts` aparecem em `:339` (Step 5). Os negativos AC-3(c) e AC-3(h) (`:140`) usam `annul_directive` e `create_directive`, então também não rodam no Step 3.
- Exigido: mover o lado servidor das duas tools para o Step 3, com envelope fixture (o Step 5 fica com aval em dois passos, comandos e projeção). Alternativa: tirar AC-8 e AC-3(c)/(h) do Step 3 e de `:400` e movê-los para o Step 5. O Critic prefere a primeira.

### B2. O marco fatia-1 é verificado por um e2e que só nasce no Step 5, depois do marco
- `:162` diz "Ao fim do Step 3 e da parte agent-only do Step 4... o e2e roda com premissa agent". A ordem em `:198` põe o marco antes do 2b-humano e do Step 5. `scripts/e2e.sh`, `tests/slice.rs` e `docs/e2e-runbook.md` só são criados em `:339`. Step 3 e Step 4 não os criam nem rodam. `:322` manda "o runbook registra isso" no Step 4, mas o runbook só existe no Step 5.
- Exigido: criar a variante agent-only de `tests/slice.rs` e `scripts/e2e.sh` e um esqueleto do runbook no Step 4. Pôr `cargo test --test slice` na Verificação do Step 4 como porta do marco. O Step 5 estende para o caminho humano.

## Majors (corrigir antes de executar)

### M1. A correção de premissa herdada (`--in`, Q21) está dentro e fora da fatia ao mesmo tempo
- `:186` (tabela "Fora da fatia (adiado)") diz "Corrigir premissa herdada do pai | Depende da Q21". `:107` descreve a exceção `--in <ancestral>` como desenho vigente. `:307` e `:512` dizem Q21 decidida. A gramática congelada no Step 1 (`:232`) lista só `--supersede` e `--revoke`. O AC-20 (`:157`) não testa a correção no processo do ancestral. É o único caso em que `supersedes` cruza unidade, e o Hexlog proíbe cruzar processo (`src/domain/relations.ts:252`).
- Exigido: decidir se entra. Se entra: tirar a linha de `:186`, pôr `--in` na gramática de `commands.md` e acrescentar ao AC-20 testes do ancestral aberto e do ancestral fora de `ancestors`. Se não entra: remover a exceção de `:107` e a Q21.

### M2. Tipos de registro usados no Step 3 e no Step 7 não estão no conjunto semente do `bootstrap_project`
- O AC-1 (`:138`) lista unit, premise, evidence, closure, directive, annulment, granularity. O AC-7 (`:144`) testa "registro not-applicable" com alvo `na.<id>`; o Step 7 grava `judge-verdict` com alvo `jv.<id>` (`omhl-v1-marcos.md:63,66`). Nenhum está na lista. `create_process` fixa para sempre os tipos vigentes (hexlog `docs/tools.md:113`) e `register` só aceita tipo fixado no processo; um tipo acrescentado depois não existe nas unidades abertas antes dele.
- Exigido: dizer se `not-applicable` é `kind` de `premise` ou tipo próprio. Incluir `judge-verdict` (e `not-applicable`, se for tipo) no conjunto semente congelado no Step 1.
- Atenuante: o Step 7 é pós-fatia e depende do S9; o trecho do AC-7 que usa N/A já está na fatia.

## Melhorias opcionais
1. Q resolvida ainda tratada como aberta: `:162` termina com "a Q20 pergunta ao dono" e `:589` diz "vira a Q20", mas a Q20 está decidida (OK). `:198` diz "começa quando Q1, Q2 e Q21 estiverem fechadas" e `:186` diz "Depende da Q21". O changelog da iteração 12 (`omhl-v1-changelog.md:334`) afirma que a varredura não deixou nenhuma.
2. `setup.rs` do Step 2a renderiza o `plugin/` do Step 4 (`:270`, `:325`) e a Verificação 2a (`:277`) não cobre o setup. Separar: o 2a entrega segredo, digest, marcador, deny e `mcp add`; a renderização entra no Step 4.
3. O Step 0 não diz quem executa as partes interativas (P0, S1, S4, S7(i) exigem comando digitado em sessão interativa: dono, tmux ou `-p`?). Além disso `! grep -q 'a verificar' docs/spikes.md` (`:221`) conflita com S6, S8 e S9, que são opcionais ou de marco e podem ficar com "a verificar".
4. A16 aparece como alvo do Step 0 (`:202`, "A16 a A19") embora esteja rebaixada (`:472`).
5. O plano exige que cspell e markdownlint aceitem `contract/` (`:245`), mas nem o Step 1 nem o §6 rodam `npm run spell` ou `npm run lint:md` (os dois scripts existem).
6. Resíduos do spec que a 8.1 não cobre: `:85` e `:155` ainda listam comandos humanos de "não se aplica" (pela Q2 é skill do agente); `:128` mantém `checkedAgainst` na ontologia; `:82` diz "envelope em toda tool de escrita" enquanto o AC-13 exige em toda tool de domínio exceto `version`. Os adiamentos de 3.1 (reabertura por órfão, grounded/executed, `ancestors`, N/A no corpo do PR, checks lint/artefato/comentário) divergem do spec sem constar na 8.1; vale uma linha na 8.1 dizendo que 3.1 são adiamentos de fatia, não emendas. (Números de linha do spec referem-se à versão antes das minhas edições de hoje.)
7. O fim do AC-22 (`:158`) perdeu a pontuação e duplica sentença ("O R11 continua: o agente escreve os testes O nome do gate promete..."). Plano e ADR trazem a data 2026-10-30 mas citam decisões de 2026-10-31 (`:16`, `:417`, `:574(j)`).
8. `range-diff ... <tag-anterior>` em `:407` não é executável na 1ª tag (já anotado "a partir da 2ª tag").
9. R23 não cita fonte inline para a precedência deny > defer > ask > allow (correta, conferida na hooks-guide).

## Contagens
- ACs concretos: 21 de 23 (91%). Vagos: AC-18 (prova é "resultado da P0 gravado em docs/spikes.md", sem teste nomeado) e AC-23 parte B (depende do Haiku real, não determinística; a parte A é concreta).
- Citações: 44 conferidas, 44 corretas. Não conferidas: números de issue do claude-code (#80700, #89251, #85581, #95465, #39530), o 30,4% do METR e os fatos de máquina do F15.

## Consistência
- Resíduo de sandbox: S6, A16 e nível 1 só como medição opcional futura; `attested.sandbox` e `SANDBOX_NOT_STRICT` não existem no plano.
- Links relativos dos cinco arquivos resolvem; todo código de erro usado consta em `errors.md`; nenhum AC/Q/R/A/F/S citado sem definição.
- Plano x spec: modo self, N/A via juiz, Stop como freio, bypass e outbox/proposals batem com a 8.1, com os resíduos da melhoria 6.
- Fronteira planejamento/execução respeitada.

## Perguntas abertas
- `UserPromptExpansion` entrega `command_name` namespaced (`omhl:open`) ou simples? É medição do S4; a regra "conjunto de comandos do plugin" depende da resposta.
- Depois de `omhl setup --refresh-digest`, a janela em que o hook nega tudo exige bypass? Conflita com "sem bypass antes do marco" (`:322`).

## RALPLAN
Princípio/Opção: Pass. Alternativas: Pass. Riscos/Verificação: Pass com ressalva (B1, B2). ADR: Pass.
