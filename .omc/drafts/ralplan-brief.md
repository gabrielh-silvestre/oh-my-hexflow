# Consenso do plano v2 (ralplan): briefing comum

Plano mestre: [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md) (33 decisões do dono, 7 pontos abertos). Apoio:

- [hexlog-current-model.md](../research/hexlog-current-model.md): relatório do Hexlog TypeScript atual (754 linhas, com `caminho:linha`)
- [jury-verdict.md](../research/vocab/jury-verdict.md): veredito sobre vocabulários
- [native-base-candidates.md](../research/native-base-candidates.md): candidatos à base nativa

## Regras do dono (valem para todos os papéis)

- Todos os subagentes em sonnet ou haiku.
- As 33 decisões do plano mestre já foram tomadas pelo dono: **não as rediscuta**. Se duas decisões se contradizem, ou uma decisão deixa lacuna, sinalize com os números das decisões e proponha como resolver; quem decide é o dono.
- Nenhum código de produto é escrito durante o consenso. Nenhum arquivo do plano mestre é alterado.
- Texto em pt-BR; identificadores, nomes de crate, de tipo e de comando em inglês.
- Máximo de 3 rodadas de revisão no consenso.

## Objetivo do consenso

1. Achar contradições e lacunas entre as 33 decisões.
2. Fechar, com proposta, os pontos abertos da seção 5 que o código resolve melhor que uma conversa; marcar os que ainda precisam do dono.
3. Entregar um plano executável do **passo 3 do plano mestre**: núcleo `engine` e `store` em Rust (workspace Cargo no monorepo `oh-my-hexflow`, crate do motor `hexlog`), com testes dos critérios de aceite 1, 3 e 7 do plano mestre.

## Papel: Planner

Escreva **um único arquivo**: `/home/gabriel/personal/oh-my-hexflow/.omc/plans/omhl-v2-core-step3.md`, marcado `pending approval`, com estas seções, nesta ordem:

1. **RALPLAN-DR:** Princípios (3 a 5), Decision Drivers (os 3 principais), Opções viáveis (pelo menos 2, com prós e contras limitados) para as escolhas técnicas do passo 3 (por exemplo: crate SQLite, canonicalização JCS, layout do workspace). Se só uma opção sobrar, a invalidação das outras.
2. **Contradições e lacunas entre as decisões:** lista numerada, cada item com os números das decisões envolvidas, por que se contradizem ou deixam lacuna, e a proposta de resolução. Pistas a verificar (não são conclusões): (a) decisão 31 (estado no núcleo, SHA do git) contra decisão 1 (motor agnóstico); (b) decisão 17 (só adiciona) contra os ids semânticos e o `externalId` reservado das decisões 6 e 21; (c) relações de um registro do projeto que apontam para nós nativos fora da cadeia (decisões 16, 20, 33): como validar a ponta nativa; (d) decisões 18, 26 e 27 (projeção só do banco para o arquivo, hook após `git pull`, um usuário por tenant) contra o banco global da decisão 9; (e) decisão 13 (relações como registros) e o que o relatório do Hexlog diz sobre as 8 espécies de relação fixas em código, contra "tipos e relações definidos por dados" do plano; (f) decisões 24, 29 e 30 (`gate` avaliável, `unit` do PR, aprovação determinística) contra a decisão 4 (v1 só texto, nada executa); (g) o que o `gate` nativo de PR avalia, já que a evidência é embutida na decisão (28).
3. **Pontos abertos:** para cada um da seção 5 do plano mestre, uma proposta ou "precisa do dono".
4. **Plano executável do passo 3:** layout do workspace e dos crates com caminhos de arquivo; esquema das tabelas SQLite; a Facade pública (assinaturas em Rust, sem implementação); cadeia de hash por tenant (sha256 sobre JCS, `seq` contíguo, `expectedHead`); validação do id semântico; lote atômico com `@alias` e idempotência por `key` mais `fingerprint`; regra de um vigente por linhagem; tarefas ordenadas, cada uma com arquivos, dependências e critério de pronto.
5. **Testes e verificação:** mapa dos critérios 1, 3 e 7 do plano mestre para testes concretos (nome do teste, o que arruma, o que afirma), mais os comandos de verificação (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`).
6. **Riscos e mitigações.**
7. **ADR** (Decisão, Drivers, Alternativas consideradas, Por que esta, Consequências, Próximos passos) — preencha ao fim.

Use o relatório do Hexlog como fonte, citando `caminho:linha` quando reaproveitar uma regra. Na mensagem final devolva: o caminho do arquivo, o número de linhas, as contradições encontradas em uma linha cada e o que ficou sem resposta.

## Papel: Architect (revisão independente)

Leia o plano em `omhl-v2-core-step3.md` (instantâneo fixo) e o plano mestre. Não escreva arquivos. Entregue: (a) a antítese mais forte (steelman) contra o desenho proposto; (b) pelo menos uma tensão real de trade-off; (c) a síntese, quando possível; (d) achados numerados com severidade (BLOQUEANTE, IMPORTANTE, MENOR), cada um com a seção do plano e uma correção concreta; (e) se há decisão do dono que o plano viola. Você não vê a revisão do Critic. Máximo de 500 palavras. A mensagem final é o próprio resultado.

## Papel: Critic (avaliação independente)

Leia o plano em `omhl-v2-core-step3.md` (instantâneo fixo) e o plano mestre. Não escreva arquivos. Avalie: consistência entre princípios e opções, alternativas justas, clareza da mitigação de riscos, critérios de aceite testáveis e passos de verificação concretos. Entregue o veredito **APPROVE**, **ITERATE** ou **REJECT**, mais achados numerados com severidade e a correção exigida para cada um. Você não vê a revisão do Architect. Máximo de 500 palavras. A mensagem final é o próprio resultado.
