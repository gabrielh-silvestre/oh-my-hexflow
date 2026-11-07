# Verificação de caixas preta e cinza e de design patterns: briefing comum

Plano a verificar (final do consenso, `pending approval`): [omhl-v2-core-step3.md](../plans/omhl-v2-core-step3.md). Contexto: plano mestre [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md) (decisões 1 a 33, módulos na seção 3, decisão 8 sobre caixas cinzas), relatório do Hexlog atual [hexlog-current-model.md](../research/hexlog-current-model.md) e o consenso em [round2-reviews.md](round2-reviews.md).

## Pedido do dono

Depois do plano pronto, passar verificadores para ver **quais partes aceitam o princípio da caixa preta e da caixa cinza**, e **fazer as abstrações com design patterns**. O dono quer caixas cinzas aninháveis: um módulo profundo de interface pequena, dentro do qual podem existir outras caixas, de modo a garantir abstração no código e no contexto de quem depende dela.

## Definições de trabalho (do dono e de Matt Pocock; entendidas por resumos secundários, não pela fonte primária)

- **Módulo profundo:** interface pequena, muita implementação escondida.
- **Caixa preta:** interface estável e implementação trocável; quem depende dela nunca precisa olhar dentro e a troca da implementação não muda os dependentes (por exemplo, SQLite, JCS e SHA-256).
- **Caixa cinza:** módulo cujo interior é delegado (a um agente ou a quem implementa) e verificado só pelos testes na interface; o interior pode ser lido e mudado, mas o contrato e o contexto de quem depende dela ficam limitados à interface.
- **Aninhamento:** uma caixa cinza pode conter caixas cinzas; cada nível expõe só uma interface pequena para o nível acima.
- Critério prático para o dono: o que depende da caixa deve caber no contexto sem ler o interior.

## Regras

- Todos em sonnet. Somente leitura: não escreva arquivos. Nenhum código de produto. Não altere o plano nem o plano mestre.
- As 33 decisões do dono não são rediscutidas; se uma classificação as contradiz, sinalize com o número.
- Cite o arquivo e a seção do plano (`omhl-v2-core-step3.md §n` ou linha) em cada classificação.
- Texto em pt-BR; identificadores e nomes de padrão em inglês. Até 700 palavras. A mensagem final de conclusão é o próprio resultado.
- Você não vê o resultado dos outros verificadores.

## Formato de saída comum

Tabela com uma linha por módulo ou fronteira: `módulo | classificação (preta, cinza, aberta) | interface em uma linha | padrão sugerido | por quê | referência no plano`. Depois: **desacordos prováveis** (onde um revisor razoável classificaria diferente) e **riscos** (onde a abstração vaza).

## Lentes (uma por verificador)

1. **Caixas pretas:** o que tem interface estável com implementação trocável e nunca precisa ser lido por quem depende (armazenamento SQLite, canonicalização JCS, SHA-256, validação de JSON Schema, relógio, geração de ids técnicos). Diga o que o dependente pode assumir e o que não pode, e o teste de contrato que prova a troca.
2. **Caixas cinzas:** o que é módulo profundo cujo interior pode ser delegado e verificado só pela interface (engine, catalog, verify, Facade, cada crate). Para cada um, a interface pública mínima, o que fica escondido, o que o contexto de quem depende precisa carregar e os testes de interface que o protegem.
3. **Aninhamento:** onde uma caixa cinza cresce e precisa de caixas dentro (a de `engine`, a de `store`, a futura de `graphs` e de `render`/adapters da decisão 8 e da tabela da seção 3 do plano mestre). Proponha a árvore de caixas com dois níveis, a interface de cada nível e o limiar para dividir uma caixa (tamanho, número de métodos públicos, motivo de mudança).
4. **Design patterns:** para cada fronteira do plano, o padrão que a sustenta e a alternativa recusada, com a justificativa (por exemplo, Facade, Adapter, Repository, Strategy, Port and Adapter, Builder, Unit of Work, Newtype, State). Aponte padrão usado onde não se paga (abstração prematura) e fronteira que precisa de padrão e não tem.
