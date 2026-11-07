# Consenso de linguagem (ralplan, até 3 rodadas): briefing comum

Plano a reescrever: [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md) (986 linhas, aprovado pelo Critic na rodada 3 do consenso anterior; histórico em [omhl-v2-core-step3.r3.md](../plans/omhl-v2-core-step3.r3.md)). Plano mestre: [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md) (decisões 1 a 35). **Autoridade de vocabulário: [GLOSSARY.md](../../GLOSSARY.md)** (32 termos, revisados por 2 critics e por uma revisão opus contra o schema.org; referência em [glossary-schema-org-review.md](../research/vocab/glossary-schema-org-review.md)). Briefing do consenso anterior: [ralplan-brief.md](ralplan-brief.md).

## Regras do dono

- Todos os subagentes em sonnet. Somente o Planner escreve, e só `omhl-v2-core-step3.md` mais a cópia `.r4.md`. Architect e Critic são somente leitura.
- Nenhum código de produto. Não altere o plano mestre nem o GLOSSARY.md; se faltar um termo no glossary, sinalize em vez de inventar.
- Os itens [DONO] e P0 pendentes continuam pendentes: não os resolva.
- As decisões 1 a 35 não são rediscutidas; só sinalize contradição com o número.
- Texto em pt-BR; identificadores, nomes de tipo, de arquivo, de coluna e de código de erro em inglês.
- Máximo de 3 rodadas de revisão.

## Objetivo

Deixar o plano do passo 3 coerente com o glossary e com as decisões 34 e 35, sem perder o que o consenso anterior aprovou (ordem T1 a T6 com P0, tabela de mutação, travas de C7, contagens dos "Pronto"). Nenhum nome antigo sobrevive no plano, exceto onde o glossary manda "evitar" e a frase explica a mudança.

## Renomeios obrigatórios (nome antigo → novo)

- Link, elo, registro → **Record** (`LinkBody`→`RecordBody`, `LinkRow`→`RecordRow`, `LinkColumns`→`RecordColumns`, `NewLink`→`NewRecord`, `AppendPlan.links`→`records`, tabela `links`→`records`, índice `links_state`→`records_snapshot`, "elo da cadeia"→record).
- State → **Snapshot** (`State`→`Snapshot`, `StateKind`→`SnapshotKind`, `requires_state`→`requires_snapshot`, colunas `state_kind`/`state_id`→`snapshot_kind`/`snapshot_id`, códigos de erro `state-required`, `state-not-allowed`, `state-format`→`snapshot-required`, `snapshot-not-allowed`, `snapshot-format`, arquivos `state.rs`→`snapshot.rs`, `state_rules.rs`→`snapshot_rules.rs`, testes e a tabela S1 a S9 e X1 a X4). `EndpointState` e `from_state`/`to_state` NÃO são snapshot: renomear para `EndpointStatus`, `from_status`, `to_status`.
- Edge → **Relation** (`EdgeView`→`RelationView`, `EdgeQuery`→`RelationQuery`, `edges()`→`relations()`, `tests/edges.rs`→`tests/relations.rs`). O tipo da ligação passa a ser **Relation type** (`RelationDef`→`RelationTypeDef`, nome da relation → relation type). Onde o plano diz "relação" para a instância, passa a ser relation.
- Tip, `TipRow`, `tip()`, "ponta" → **Latest record** (`LatestRow`, `latest_record()`). **Current revision** é só a que vale hoje.
- Head → **Chain head** (`head()`→`chain_head()`, `expected_head`→`expected_chain_head`; `ChainHead` fica).
- Subject, linhagem → **Lineage** (`Subject`→`Lineage`).
- Unit → **Work item** (`graphs/units`→`graphs/work_items`, tipo `unit`→`work_item`, estados aberto e fechado ficam, sem a palavra "state": usar "status" do work item).
- Approval → **Attestation** (com desfecho de 3 valores: passou, passou com alerta, falhou).
- Retire, aposentadoria → **Retirement** / retirement (`ItemBody::Retire`→`ItemBody::Retirement`).
- `ForeignNodes` → `NativeNodes` (coerente com `Origin::Native`, `MissingNative`).
- Hospedeiro → "o omhl" (o consumidor Rust do motor). "Host" só para o ambiente do agente.
- Exemplos de id com namespace `gates.` (decisão 6) → `checks.`.
- `AppendItem` e `ItemBody` ficam (o item de uma batch não colide mais com o glossary); registre uma nota se algum revisor achar confusão com Work item.

## Coerência com as decisões 34 e 35

- D34: os YAML em `.omhl/` são **Extension** (escritos à mão, carregados no banco); o texto longo é Markdown escrito à mão referenciado pelo YAML; **Projection** é só uma vista sob demanda do banco (primeira: o estado atual por work item). Remova do plano o que ainda trata as decisões 18, 25, 26 e 27 como vigentes (regeneração por hook após `git pull`, `.md` como projeção do texto longo). O item 6 da seção 2 do plano ("banco vazio em máquina nova") muda: um clone novo recarrega o banco a partir das extensions. O passo 3 continua sem carregar extensions nem gerar projeções (fora do escopo); apenas não pode contradizê-las.
- D35: o glossary prevalece sobre os nomes antigos.

## Mudanças além dos renomeios

1. Acrescente o teste de arquitetura de imports: `engine` não importa `store` nem `schema`; `tests/` usam só a API pública; visibilidade `pub(crate)` ou `pub(in crate::engine)` nos filhos (decisão do dono). Ajuste a contagem de testes de `architecture` e o "Pronto" da T1, e mantenha tudo coerente.
2. `TxRead`: **não divida** (o dono não decidiu). Defina-o em linguagem simples no plano ("as leituras que o engine faz dentro de uma gravação") e proponha, marcado [DONO], um nome mais claro e a divisão por uso. O dono confundiu o nome em conversa.
3. Mantenha as seções 8, 9 e 10 do plano como histórico, com uma nota de que usam os nomes antigos, e acrescente a seção "11. Rodada de linguagem" com uma linha por mudança relevante.
4. Atualize a lista de decisões citadas (o plano cita 33; o mestre tem 35) e confira as citações de linha H:n.

## Papel: Planner

Copie o plano atual para `/home/gabriel/personal/oh-my-hexflow/.omc/plans/omhl-v2-core-step3.r4.md` (histórico) e reescreva `/home/gabriel/personal/oh-my-hexflow/.omc/plans/omhl-v2-core-step3.md`, marcado `pending approval`, aplicando tudo acima. Depois de reescrever, procure por script (grep) cada nome antigo e confirme que não sobrou; confira que as contagens de testes e os "Pronto" T1 a T6 e P0 continuam batendo. Mensagem final: caminho, linhas, quais renomeios foram aplicados e quais nomes antigos ainda aparecem (e por quê), o que ficou sem resposta.

## Papel: Architect (revisão independente)

Leia o instantâneo fixo do plano, o GLOSSARY.md e o plano mestre. Somente leitura, até 500 palavras. Entregue: (a) nomes antigos que sobraram, ou renomeios aplicados errado (por exemplo `EndpointState`); (b) contradições com as decisões 34 e 35; (c) coerência técnica do plano depois dos renomeios (nada quebrou na ordem de tarefas, nas tabelas, nos nomes de teste); (d) achados numerados com severidade BLOQUEANTE, IMPORTANTE ou MENOR e a correção concreta; (e) decisões do dono violadas. Você não vê o Critic.

## Papel: Critic (avaliação independente)

Mesmos insumos. Somente leitura, até 500 palavras. Avalie se o plano continua executável e verificável depois da reescrita: critérios de aceite testáveis, contagens dos "Pronto" coerentes com as tabelas de §5, travas de C7 sem contradição com os novos nomes (o varredor de vocabulário, o schema.sql), e se o glossary é seguido em todo o plano. Veredito APPROVE, ITERATE ou REJECT, achados numerados com severidade e a correção exigida. Você não vê o Architect.
