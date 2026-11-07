# Plano executável do passo 3: núcleo `engine` e `store` em Rust

- **Status:** `pending approval` (rodada de linguagem do consenso RALPLAN-DR, modo SHORT: o plano aprovado na rodada 3 foi reescrito sobre o vocabulário do [GLOSSARY.md](../../GLOSSARY.md) e sobre as decisões 34 e 35; ver as seções 11, 12 e 13). Nada aprovado, nada iniciado, nenhum código escrito.
- **Histórico:** a rodada 1 está congelada em [omhl-v2-core-step3.r1.md](omhl-v2-core-step3.r1.md), a rodada 2 em [omhl-v2-core-step3.r2.md](omhl-v2-core-step3.r2.md), a rodada 3 em [omhl-v2-core-step3.r3.md](omhl-v2-core-step3.r3.md) e a versão anterior à rodada de linguagem (aprovada pelo Critic na rodada 3) em [omhl-v2-core-step3.r4.md](omhl-v2-core-step3.r4.md), a anterior à rodada 2 de linguagem em [omhl-v2-core-step3.r5.md](omhl-v2-core-step3.r5.md) e a anterior à rodada 3 em [omhl-v2-core-step3.r6.md](omhl-v2-core-step3.r6.md); as revisões estão em [round1-reviews.md](../drafts/round1-reviews.md) e [round2-reviews.md](../drafts/round2-reviews.md). As seções 8, 9 e 10 listam o que mudou, achado por achado, e **usam os nomes anteriores à rodada de linguagem** (elo, state, edge, tip, head, unit, approval e afins); a seção 11 traz o mapa dos nomes novos.
- **Plano mestre:** [omhl-v2-graph-engine.md](omhl-v2-graph-engine.md) (35 decisões do dono, passo 3 da seção 6, critérios de aceite 1, 3 e 7 da seção 4). Este plano não altera o mestre. O texto das decisões 1 a 34 no mestre ainda usa os nomes antigos (`unit`, estado, elo e afins); pela D35 o glossary prevalece, e este plano cita cada decisão pelo número `D<n>` e a lê com os nomes do glossary.
- **Vocabulário:** [GLOSSARY.md](../../GLOSSARY.md) é a autoridade (D35). Termos do glossary ficam em inglês; identificadores, nomes de tipo, de arquivo, de coluna e de código de erro também.
- **Apoio:** [hexlog-current-model.md](../research/hexlog-current-model.md), [jury-verdict.md](../research/vocab/jury-verdict.md), [native-base-candidates.md](../research/native-base-candidates.md). Briefing do consenso: [ralplan-brief.md](../drafts/ralplan-brief.md); briefing da rodada de linguagem: [ralplan-brief-language.md](../drafts/ralplan-brief-language.md); revisões das rodadas 1 e 2 de linguagem: [round1-language-reviews.md](../drafts/round1-language-reviews.md) e [round2-language-reviews.md](../drafts/round2-language-reviews.md).
- **Convenções de citação:** `H:n` é a linha `n` de `hexlog-current-model.md`; o `caminho:linha` ao lado é o do repositório do Hexlog TypeScript, como o relatório o cita. `D<n>` é a decisão `n` do plano mestre; `C<n>` é o critério de aceite `n`; `P<n>` é o ponto aberto `n` da seção 5 do mestre, na ordem em que ela os lista.
- **Modo SHORT:** não há segredo, pagamento nem migração de dado (D12: recomeça do zero). O custo de errar o formato da cadeia é real, mas é barato até o primeiro tenant de verdade existir (passo 4); por isso o formato do record é congelado no portão P0, **antes** de qualquer código que o grave (seção 4.6).

## 1. RALPLAN-DR

### Princípios

1. **O motor conhece estrutura, não significado.** Node types, relation types e regras de domínio chegam como dado (D1, D2, D13). O que o motor conhece de fábrica é pouco: revision, retirement, relation, snapshot tipado (D31), chain. Fonte: separação genérico/embutido em H:577-602. Esse "pouco" é vocabulário estrutural, e o dono o confirma no portão P0 (item 19 da seção 2).
2. **A cadeia é a prova; as colunas do SQLite são índice.** O hash cobre o JSON canônico do record. Toda coluna usada para filtrar é derivada desse JSON e conferida pelo `verify_chain`. Nenhum dado de payload é lido de coluna.
3. **Nada é reescrito.** Anexar-só é imposto em três camadas: a API não tem atualização, o banco recusa `UPDATE`/`DELETE` e índices únicos impedem bifurcação, e o hash detecta o que escapar das duas primeiras (invariante em H:457).
4. **Um só caminho de escrita.** `decide` é função pura chamada dentro de uma transação `BEGIN IMMEDIATE`; a Facade é a única entrada (D7, H:80, H:655).
5. **Portar regras, não mecanismos.** Levamos as regras do Hexlog que a tabela de H:626-661 manda portar (cadeia, lote, `key`, um vigente) e deixamos para trás lock por diretório, linhas rasgadas, escrita atômica manual e teto de 64 MiB (H:630, H:649-651).

### Decision Drivers (os 3 principais)

1. **Integridade verificável sem arrependimento de formato** (C3). O que entra no corpo hasheado do record fica para sempre; por isso o formato é decidido antes de ser gravado.
2. **Neutralidade do motor demonstrável por teste** (C7, D1, D31): sem vocabulário de domínio e sem dependência de git ou de rede, e sem mudar o `engine` para um grafo novo.
3. **Executável por agentes sonnet/haiku com verificação mecânica:** API síncrona, poucas peças móveis, testes determinísticos (relógio e gerador de id injetados, como em H:78).

### Opções viáveis por escolha técnica

**E1. Crate SQLite**

| Opção | Prós | Contras |
|---|---|---|
| **A. `rusqlite` com feature `bundled` (recomendada)** | Síncrona, que é o desenho da Facade. SQLite embutido dá versão fixa com JSON1 e FTS5 (D9, plano seção 3) e `STRICT`. Controle direto de `BEGIN IMMEDIATE`, triggers e PRAGMAs. | Compila C (precisa de `cc`; `gcc 13.3` está na máquina). Binário maior. `Connection` é `Send` e não `Sync`: a Facade serializa com `Mutex` (seção 4.4). |
| B. `sqlx` (sqlite) | Verificação de SQL em tempo de compilação. | Async sem necessidade; exige runtime; macros pedem banco em tempo de build; mais peças para o executor errar. |
| C. `diesel` / `sea-orm` | Modelo tipado. | ORM sobre um esquema de 3 tabelas com triggers e índices parciais atrapalha mais do que ajuda. |
| D. SQLite do sistema (sem `bundled`) | Binário menor. | Versão varia por máquina; sem garantia de `STRICT`, JSON1 e FTS5; testes deixam de ser reproduzíveis. |

**E2. Canonicalização JCS (RFC 8785)**

| Opção | Prós | Contras |
|---|---|---|
| **A. `serde_json_canonicalizer` + vetores do RFC + hash dourado calculado pelo `canonicalize` do Hexlog TS + recusar número que não seja inteiro seguro (recomendada)** | Mantém o JCS que o plano manda portar (H:628). O guarda de inteiros remove a classe de divergência mais comum (formatação de ponto flutuante). O valor dourado vem de uma implementação independente. | Crate e versão não verificados contra o RFC por este plano; a T1 faz o spike. Proibir float limita campos futuros (por exemplo, nota 0.5). |
| B. `serde_jcs` | Mais antigo. | Mesma verificação pendente; menos ativo nas buscas de hoje (0.2.0). |
| C. Canonicalizador próprio sobre `serde_json` | Controle total. | Reimplementar RFC 8785 é risco sem ganho; só se A e B reprovarem nos vetores. |

**E3. Layout do workspace (D11 manda dois crates: `hexlog` e `omhl`)**

| Opção | Prós | Contras |
|---|---|---|
| **A. Dois crates; `hexlog` com módulos `engine`, `schema`, `store` (recomendada)** | Segue D11 literalmente e a tabela da seção 3 do mestre. Fronteira imposta por teste de arquitetura. | Rust não proíbe `use` entre módulos do mesmo crate; a trava é um teste que tokeniza `src/` e confere `cargo metadata`. |
| B. Crates separados (`hexlog-engine`, `hexlog-store`, ...) | O compilador impõe a fronteira. | Contradiz D11 (dois crates) e multiplica manifestos antes de haver segundo consumidor. |
| C. Um crate só | Mais simples. | Perde a separação motor/plugin de D11 e a prova de que o motor não depende do plugin. |

**E4. Modelo de revisão (D6, D21)**

| Opção | Prós | Contras |
|---|---|---|
| **A. Revisão por `base` explícito + índices únicos parciais (recomendada)** | Preserva `FORK_REJECTED` (H:636, H:157-159) com ids semânticos: gravar sobre revisão que não é o latest record é recusado. Bifurcação fica fisicamente impossível. | O agente precisa conhecer o latest record (a resposta do `get` e do `relations` traz o id técnico). |
| B. Revisão implícita, última escrita vence | Mais simples para o agente. | Perde a regra de um vigente por lineage que o mestre manda portar (mestre seção 3); duas escritas concorrentes se atropelam. |
| C. `supersedes` explícito como no Hexlog | Fiel ao original. | Transforma uma propriedade do motor em relation type do usuário, contra D13/D21; e `supersedes` não cruzava processos (H:635), o que não casa com a sobreposição futura. |

**E5. Onde moram as definições de node type e relation type**

| Opção | Prós | Contras |
|---|---|---|
| **A. `Catalog` injetado pelo `omhl` e visto pelo `engine` só pela porta `Definitions` (item 22 da seção 2); cada record grava o `def_hash` que o validou (recomendada)** | Sem manifesto imutável por processo (H:642 aponta esse problema); a cadeia prova com que definição cada record foi validado; grafos novos são só dado (C7). | Mudança de definição não é detectada como quebra; ficamos sem a classificação compatível/quebra do Hexlog (H:289-293, que o relatório marca como "mudar"). Invariantes entre records (por exemplo, transição aberto→fechado, D32) não cabem no schema por record: ficam no adaptador na v1 (ADR). |
| B. Definições como records na cadeia | Auditável. | Bootstrap circular (tipo do tipo) e peso desnecessário na v1. |
| C. Manifesto fixado por processo como no Hexlog | Fiel ao original. | Tenant fica imutável a mudanças de tipo; conflita com o native versionado com o plugin (D16). |

**E6. Política de verificação da cadeia — [DONO]**

| Opção | Prós | Contras |
|---|---|---|
| **A. A escrita confere só a cauda; verificação completa em `verify_chain` (recomendada)** | Escrita O(1). Leituras indexadas do SQLite ficam baratas. | **Desvio de H:243**: o Hexlog "falha fechado" em toda leitura. Com `Tail`, adulteração no meio da cadeia só aparece no `verify_chain` (teste documenta o limite). |
| B. Verificação completa em toda leitura e escrita | Falha fechado, como o original. | O(n) por chamada; o Hexlog já media orçamento de 500 ms para 5.000 records (H:556). |

O dono decide se aceita o desvio. Enquanto não decidir, `Config.verify_policy` aceita `Tail` e `Full`, o padrão de produção fica `Tail` por recomendação e os testes usam `Full`. Reavaliar com medição (R4).

**Escolhas sem alternativa material:** `sha2` para sha256, `uuid` (v7) para id técnico, `thiserror` para o catálogo de erros, `proptest` para propriedades (H:568), `tempfile` para bancos de teste, `proc-macro2` (só em teste) para tokenizar fontes na trava de vocabulário. Versões vistas no índice de crates hoje (`cargo search`): `rusqlite 0.41.1`, `serde_json_canonicalizer 0.4.1`, `sha2 0.11.1`, `jsonschema 0.60.2`, `uuid 1.29.1`, `thiserror 2.0.21`. Fixar as versões exatas só depois do spike da T1; a documentação dessas crates não foi consultada neste plano.

## 2. Contradições e lacunas entre as decisões

Marcação: **[DONO]** = só o dono decide; o plano traz proposta e não a executa como decisão. **[CÓDIGO]** = resolvida pelo desenho do passo 3. Os itens 1, 5, 10, 16 e 19, mais a tabela de regras de `snapshot` (item 11), são o conteúdo do portão P0 (seção 4.6): a T2 só começa depois de respondidos.

1. **[DONO] Snapshot no núcleo contra motor agnóstico (D31 × D1).** D31 põe o SHA do git no núcleo; D1 diz que o motor não conhece domínio. **Esta proposta é uma releitura de D31, não a letra dela:** o núcleo conhece só o tipo `Snapshot { kind: SnapshotKind, id: String }`, com `SnapshotKind` enum fechado (hoje `GitCommit`) e validação de formato; não invoca git nem importa crate de git. A sonda que lê `HEAD` e checa árvore limpa é uma porta (`SnapshotProbe`) implementada no crate `omhl` (passos 4 e 5). Consequência que o dono precisa aceitar: o núcleo confia no `Snapshot` que o chamador entrega e não prova que o SHA pertence ao repositório nem que a árvore estava limpa; a regra de D31 de que um snapshot com alterações não commitadas não pode ser atestado (D31 diz "estado" e "aprovado"; pela D35 leem-se snapshot e attestation) é imposta pelo adaptador, não pelo motor. O teste de arquitetura permite a palavra `git` somente em `src/engine/snapshot.rs`. A formatação de `GitCommit` é hexadecimal minúsculo de **40 ou 64** caracteres, mais estrita que o `git-sha` do Hexlog TS (7 a 40, sem normalizar; `src/domain/formats.ts:3-4`), porque abreviação não é "tão reproduzível quanto o git" (D31). Se o dono quiser a letra de D31 (o núcleo chamar o git), a mudança é uma porta a menos e uma dependência proibida a menos na trava; o formato do record não muda.
2. **[DONO] "Só adiciona" contra semantic ids e external id (D17 × D6 × D21).** Quatro lacunas.
   - O motor não conhece o native (D16, fora da cadeia), mas precisa recusar gravação com id semântico nativo (C2). Proposta [CÓDIGO]: porta `NativeNodes` injetada na Facade; id reservado é o que `lookup` reconhece (casamento exato).
   - Com revisão por id não há o que bifurcar sem um ponto de comparação: o mestre manda portar `FORK_REJECTED` (mestre seção 3) mas não diz como. Proposta [CÓDIGO]: campo `base` (E4-A).
   - O external id é "reservado" sem definição (D17). Proposta [CÓDIGO]: coluna opcional `external_id`, texto de 1 a 200 caracteres, sem unicidade, entra no hash quando presente e é omitido quando ausente (assim acrescentar o uso depois não muda o hash dos records antigos). H:634 diz que o análogo atual é a raiz da lineage.
   - Atualização do plugin pode introduzir um id nativo que já existe no tenant, ou remover um id nativo ao qual relations do projeto apontam. Proposta [DONO]: o nativo prevalece e a leitura marca o nó do projeto como `shadowed` (campo de `NodeView`); nativos nunca saem dentro de uma versão maior. Fora do passo 3: aqui só se entrega a porta e o teste com fake.
3. **[DONO] Exemplo de sobreposição por id contra "substituição fica para depois" (D16 × D17, candidato 5).** [native-base-candidates.md](../research/native-base-candidates.md) (linha 24) traz a política de commit como "exemplo de sobreposição por id": o projeto sobrepõe o padrão conservador. D17 proíbe alterar ou substituir nativo na v1. Proposta: o candidato 5 entra no native da v1 só como diretriz nativa fixa, e o projeto acrescenta uma diretriz própria com outro id; o `render` mostra as duas e o agente resolve a precedência por texto. Alternativa: retirar o candidato 5 da v1. Escolha do dono.
4. **[CÓDIGO] Relations do projeto para nós nativos (D16 × D20 × D33).** O destino nativo está fora da cadeia. Proposta: `NativeNodes::lookup` devolve existência e tipo; a escrita valida existência e os tipos permitidos da `RelationTypeDef` (como em H:144 e H:187-208); o record grava `from_origin`/`to_origin` (`tenant` ou `native`) e a versão do catálogo nativo no momento da escrita (`native_catalog`), informativa e dentro do hash. A leitura devolve `EndpointStatus` por extremo (`Current`, `Retired`, `MissingNative`) em `RelationView` e nunca falha por nativo ausente; `verify_chain` ignora conteúdo nativo. Relation entre dois nativos é recusada. A detecção de ciclo inclui as relations internas do native via `NativeNodes::native_relations(relation_type)`.
5. **[DONO] "Antes de um nativo" exige relation com origem nativa (D17 × D20 × D33).** D33 só tem `after`; "P antes de N" é `N after P`, relation cuja origem é o nativo N. É alterar N? Proposta: a relation é record do projeto (D13), não altera o nativo; permitida com pelo menos um extremo no tenant. Se o dono recusar, a restrição é uma linha em `engine/rules.rs` e um teste; `from_origin` continua no corpo do record (a coluna existe por simetria e por `to_origin`). Resposta necessária no P0.
6. **[DONO] Banco global por máquina contra extensions no repositório (D9 × D34).** O banco é global do usuário e não está no repositório. Pela D34 os YAML de `.omhl/` são extensions, escritas à mão e carregadas no banco: um clone novo ou uma segunda máquina recarrega o banco a partir delas, e a regeneração por `post-merge` das D26 e D27 deixou de valer para os YAML. O que essa recarga não traz é o que só existe no banco: nodes gravados em uso (decisions, premises, attestations). Esse conteúdo continua preso ao banco da máquina; o plano não decide se isso é aceitável nem se uma projeção o cobre, porque a D34 deixou em aberto o formato e o comando da projeção. Sem efeito no passo 3: o motor expõe `chain_head(tenant)` e `verify_chain` com `expected_chain_head`, que servem ao `omhl` para comparar o banco com qualquer cópia do histórico. O dono decide, antes do passo 5, se o carregamento das extensions precisa ser idempotente (recarregar duas vezes não duplica nem altera node) e se o conteúdo que só vive no banco precisa de outro caminho de recuperação.
7. **[DONO] Arquivos versionados no repositório contra attestation amarrada ao snapshot (D34 × D30 × D31).** Editar uma extension em `.omhl/` e commitá-la troca o snapshot (o commit): a attestation feita no snapshot anterior continua amarrada ao anterior e não vale para o novo. D31 proíbe atestar com alterações não commitadas (item 1: a sonda recusa). Uma projeção (vista só de leitura gerada sob demanda, D34), se for gravada dentro do repositório, suja a árvore do mesmo modo. Proposta: ordem de uso editar a extension, commitar, atestar; a projeção determinística (C8: bytes idênticos para o mesmo conteúdo do banco) evita diferença espúria. Fica registrado como risco; o dono decide se commits de `.omhl/` são esperados antes da attestation e se uma projeção pode ser gravada no repositório. Nada a fazer no passo 3.
8. **[DONO] Quatro perguntas "como estão" contra herança de lineage descartada (D24 × H:639).** As perguntas `approved` e `no_pending` do Hexlog TS (nomes do Hexlog TS; o nome de cada pergunta no `omhl` é do passo 4) têm a opção `inherit` (H:337) apoiada na herança por lineage (H:164-171), que o relatório manda descartar na v1. Proposta: portar as perguntas sem `inherit` (caminho `inherit: false`, que é o padrão). Fora do passo 3 (a avaliação mora em `graphs/*`); o passo 3 deixa as consultas genéricas que elas precisam (`relations`, `list` com `snapshot` e `where_eq`).
9. **[CÓDIGO] Relations como records contra oito espécies em código (D13 × plano seção 2 × H:129-146, H:637).** Resolução: nenhuma "espécie" fica em código. O motor entende só revisão (`base`), retirement (item 10) e relation; a semântica de cada relation type é dado em `RelationTypeDef` (`from`, `to`, `acyclic`, `equivalent`). `supports`, `contradicts` e `answers`, de que D24 depende, passam a ser relation types definidos pelo catálogo do grafo de decisões (passo 4). `needsReview` e herança ficam fora (item 8). Identidade de uma relation: a tripla (`relation_type`, `from`, `to`); a chave de lineage da relation é `relation_type|from|to` (o separador `|` nunca ocorre em nome ou id).
10. **[DONO] Sem desfazer em um modelo "só adiciona" (D17 × D13 × D20).** Uma relation `after` errada, ou um nó do projeto que deixou de valer, não tem como sair. Revisar uma relation não resolve (a identidade é a tripla). Proposta: variante `Retirement` (uma revision sem dado, `base` obrigatória), que tira a lineage da vigência sem apagar nada (glossary: Retirement), equivalente ao `revokes` sem sucessor do Hexlog TS (H:134). Incluir já no passo 3 porque adicionar uma variante ao corpo do record depois do congelamento muda o formato. Se o dono recusar, remover a variante no P0. Resposta necessária no P0.
11. **[DONO] Regras de `snapshot` dentro do corpo hasheado (D31 × D28 × D30).** O corpo do record carrega `snapshot` e `external_id`; duas leituras de "quando o `snapshot` vale" gravam corpos diferentes e o formato congela. Proposta: a tabela da seção 4.5 (tipo × item × `snapshot`), com um teste por linha. Em resumo: o `snapshot` é obrigatório nos nós de tipo `requires_snapshot` e em cada revisão deles (não é herdado), e proibido em qualquer outro item, inclusive `Retirement` e relation; o `external_id` também não é herdado pela revisão (linha X4). O dono confirma a tabela no P0.
12. **[DONO] "Refazer o check" implica execução (D30 × D4 × D24 × D28 × mestre seção 2).** Gate avaliável é leitura e cabe em D4; refazer o check no mesmo snapshot e obter o mesmo desfecho (D30, que diz "estado" e "resultado") supõe executar, fora da v1 pela seção 2 do mestre. Além disso "check" (node de texto, D14/D22), "gate mecânico" (D28/D30) e "gate" (consulta, D22) se sobrepõem. Vocabulário fixado pelo [GLOSSARY.md](../../GLOSSARY.md) (D35): **check** é o node de instrução; **gate** é a consulta avaliável; **attestation** é um node de um node type do graph de checks, cuja revision grava o desfecho de um check em um snapshot específico (passou, passou com alerta ou falhou). `requires_snapshot` é propriedade desse node type (a attestation não é um quarto tipo de record). Na v1 a attestation é só um node gravado; refazer o check no mesmo snapshot dar o mesmo desfecho é propriedade a verificar quando a execução entrar. Suporte do passo 3: `TypeDef.requires_snapshot`.
13. **[DONO] O que o gate nativo de PR avalia (D28 × D29 × D24).** Com evidência embutida na decision (D28) não há node de evidência para `supports`, e o exemplo `pre-pr` do Hexlog usa `finding` e `answers` (H:350), que não existem na v1. Esboço mínimo para o passo 4: pergunta 1 `occurred` (pelo menos 1 node `attestation` ligado ao work item (a relation type fica para o passo 4) cuja current revision tem `Snapshot` igual ao SHA de fechamento informado pelo chamador); **tensão registrada, não decidida:** `list` e a pergunta `occurred` olham a current revision, então uma revision da attestation para um snapshot novo apaga da vista corrente o desfecho do snapshot anterior, que só aparece por `lineage_records`; pergunta aberta [DONO], para o passo 4: cada snapshot tem o seu próprio semantic id (uma attestation por snapshot), ou a attestation é uma revision por snapshot (e o gate precisa de `lineage_records` ou de outra consulta)? Sem efeito no passo 3 nem no P0; pergunta 2 `no_open_contradiction` sobre as decisões ligadas ao work item. As demais perguntas e o significado de "passou" do gate (o antigo "aprovado") são do dono (P6 do mestre). Suporte do passo 3: filtro `snapshot` em `ListFilter`.
14. **[DONO] Fechar o work item com gate reprovado (D32 × D4).** D32 diz que o fechamento é quando o gate é avaliado; D4 diz que nada bloqueia. Proposta: o fechamento nunca é recusado na v1; a revisão de fechamento guarda no `data` o resultado do gate e o chain head (`seq`) em que foi avaliado. Passo 4.
15. **[CÓDIGO] O work item não tem casa (D3 × D29 × mestre seção 3).** A tabela da seção 3 lista só `graphs/decisions` e `graphs/checks`, e o passo 4 do mestre diz "decisões e de gates". Proposta: `graphs/work_items` no crate `omhl`; o work item é o terceiro grafo natural e serve de fixture do C7 (com nome neutro no teste). Também, a D6 usa o exemplo `gates.before-commit.run-tests` enquanto D3/D22 chamam o grafo e o node de `check`; os ids de exemplo passam ao namespace `checks.` (a D6 vira `checks.before-commit.run-tests`). O tipo `unit` de D29 e D32 é o work item do glossary (tipo de node `work_item`, com status `open`/`closed`).
16. **[DONO] `@alias` sem uso com ids semânticos (mestre seção 3 × D6 × D21).** Relations apontam ids semânticos, que já são conhecidos ao escrever; `@alias` só serve para referir o id técnico de um item anterior do lote, em `base`. Proposta: manter `alias` apenas em `base`. Como `batch.aliases` entra no corpo do record, a decisão de manter ou cortar `alias` muda o formato e fica no P0.
17. **[DONO] Texto longo no record contra teto de `data` (D25 × H:194, H:644).** D25 guarda o texto longo no SQLite "como parte do record (com hash)" (a D34 tirou a D25 das extensions: o Markdown fica no arquivo, escrito à mão; o teto vale para o texto que o carregador copiar para um record, decisão do passo 5); o teto de `data` é 16.000 caracteres canônicos e os anexos foram descartados (H:644 manda medir antes de portar). Proposta: `Limits.data_max_chars` com padrão 16.000. Começar apertado é seguro porque afrouxar o teto depois é compatível e apertar invalida linha já gravada (H:214). O passo 4 mede os textos reais do native; se passarem, o dono decide entre subir o teto e portar blobs endereçados por hash.
18. **[CÓDIGO] Resíduos de redação.** "Fonte de verdade de tudo" (D9) contra o native fora do banco (D16): leitura adotada, o SQLite é a verdade de tudo do tenant e o native é a verdade do binário. Módulos do mestre (seção 3) contra dois crates (D11): resolvido por E3-A, `engine`, `schema`, `store` dentro de `hexlog`; `native`, `graphs/*`, `render`, `project`, `adapters/*` dentro de `omhl`.
19. **[DONO] Vocabulário estrutural no corpo do record (D1).** `Retirement`, `requires_snapshot`, `Snapshot`, `from_origin`/`to_origin`, `native_catalog` e `def_hash` não são vocabulário de domínio de desenvolvimento, mas são mais que "nodes, tipos, relations, chain e consulta" (D1, com os nomes do glossary). Proposta: aceitar como estrutura do motor, listada em P1 (princípio 1) e coberta pela trava de vocabulário (a trava veta termos de domínio, não estes). O dono confirma no P0 que isso é estrutura e não domínio; se algum desses itens sair, o corpo do record encolhe antes de ser gravado.
20. **[CÓDIGO] Invariantes entre records (D32 × D29 × E5).** A transição do status do work item de aberto para fechado (D32) e a regra "uma attestation exige snapshot" (D30 e D31; a flag `requires_snapshot` do node type cobre só a presença do snapshot) são regras entre records ou sobre valores de `data`; o schema por record e as flags `from`/`to`/`acyclic` não as expressam. Resolução: ficam no adaptador na v1 (o crate `omhl` valida antes de chamar a Facade), e o motor continua neutro. O `third_graph` traz um caso que mostra o padrão (seção 5, C7). Se o dono quiser essas regras no motor, é um marco próprio.
21. **[DONO] Nome e divisão de `TxRead` (a seção 4.4 o usa em `Store` e em `decide`).** Em linguagem simples: `TxRead` é o conjunto das leituras que o engine faz dentro de uma gravação, todas dentro da mesma transação do SQLite, para que as respostas sejam consistentes entre si (qual é o chain head do tenant, qual é o latest record de uma lineage, se uma `key` já foi usada, quais relations estão vigentes). O dono confundiu o nome em conversa; o plano **não o divide nem o renomeia**, porque a decisão não é do plano. Proposta [DONO] de nome: `StoreReader` ("o leitor do store que o engine recebe") no lugar de `TxRead`, que lê como "uma leitura de transação" e esconde que o mesmo objeto serve a `decide`, às consultas e à verificação. Proposta [DONO] de divisão por uso: `WriteReads` (o que `decide` precisa para gravar: `tenant`, `chain_head`, `latest_record`, `record_by_id`, `batch_by_key`, `batch_records`, `live_relation_pairs`), `QueryReads` (o que a Facade usa para responder: `current_nodes`, `revisions`, `latest_relations`) e `VerifyReads` (o que `verify_chain` usa: `tenant`, `records_from`, `batches`). Custo da divisão: três traits, e `append_with` e `read` passam a receber o trait de cada uso; ganho: cada função declara só o que lê. Enquanto o dono não decidir, o plano executa com `TxRead` inteiro e o nome atual. Sem efeito no formato do record; não bloqueia o P0.
22. **[CÓDIGO] `engine` não importa `store` nem `schema` (decisão do dono).** O `decide` precisa do `Catalog` (tipos de node, relation types, validação de `data`), que mora em `schema`. Resolução: o `engine` define em `engine/ports.rs` a porta `Definitions` (o que ele precisa saber: se o tipo de node existe e se exige snapshot, o `def_hash` do tipo, os tipos de node permitidos de um relation type e se ele é acíclico, e validar `data`), e `schema/catalog.rs` implementa essa porta no `Catalog` (a dependência vai de `schema` para `engine`). A Facade, que fica fora do `engine`, guarda o `Catalog` concreto e entrega um `&dyn Definitions` ao `decide`. O `Store` já seguia esse padrão (`store/sqlite` implementa `engine/ports.rs`). Visibilidade dos filhos de `engine`: `ports`, `model`, `snapshot`, `limits` e `chain` são `pub(crate) mod` (o `schema` e o `store` os importam); `decide`, `rules` e `verify` são `pub(in crate::engine) mod`, e a Facade alcança `decide` e `verify_chain` por `pub(crate) use` em `engine/mod.rs`, que entra só junto com o consumidor (T5a e T5c). `CHECKED_FIELDS` e as funções de `chain` são públicas: `engine/mod.rs` usa `pub use verify::CHECKED_FIELDS;` simples e o `lib.rs` reexporta daí (um `pub(crate) use` não pode ser reexportado por `pub use`, erro E0364; o filho `pub(in crate::engine)` não impede o item `pub` de ser alcançado). O que é API pública sai por `pub use` explícito em `lib.rs` e `facade.rs` (seção 4.2 lista as portas que saem). Três testes de `architecture` travam isso (seção 5, C7).
23. **[DONO] Nome do módulo `schema` (glossary × D11).** O módulo `schema` guarda o `Catalog`, e o glossary lista "schema" em _Avoid_ de Catalog; o nome vem de D11 e da tabela da seção 3 do mestre, e o módulo também faz a validação por JSON Schema, onde o termo é correto. O plano **não renomeia**: mantém `schema` até o dono decidir entre manter e `catalog`. Se renomear, mudam o layout da seção 4.2, os caminhos `src/schema/*`, a frase da regra de imports ("não importa `store` nem `schema`") e o teste de imports da seção 5; o formato do record e o P0 não mudam.

## 3. Pontos abertos (seção 5 do plano mestre)

| # | Ponto | Situação | Proposta |
|---|---|---|---|
| P1 | Campos do `.omhl.toml` | Proposta; o dono confirma ao revisar o passo 5 | `version = 1` e `tenant = "<Name>"` (mesma regra de nome do motor: `^[a-z0-9][a-z0-9-]{0,62}$`, herdada de `src/domain/ids.ts:4-5`), e nenhuma chave de projeção: a D34 deixou em aberto o formato e o comando da projeção (vista sob demanda, não regenerada por hook), então o dono define essas chaves quando decidir. Chaves desconhecidas são erro. Nada mais entra na v1. |
| P2 | Layout de `.omhl/` | Proposta; o dono confirma antes do passo 6 | As extensions (D34) são YAML escritos à mão, uma pasta por tipo de node e um arquivo por node: `.omhl/checks/<id>.yaml` e `.omhl/guidelines/<id>.yaml` (o glossary limita as extensions a checks e guidelines; não há pasta de gates, porque o gate é consulta nativa e não node de projeto). O texto longo é Markdown escrito à mão, referenciado pelo YAML, em `.omhl/text/<id>.md` (o glossary o inclui na extension; a D25 deixou de valer para extensions). O nome do arquivo é o semantic id sem transformação: a regra do id (minúsculas, dígitos, hífen e ponto, até 200 caracteres) garante nome seguro em qualquer sistema de arquivos e sem colisão por caixa. O layout das projeções fica fora desta proposta (D34). |
| P3 | Isolamento entre tenants e mover/renomear projeto | Resolvido no código, com ressalva do item 6 | Toda tabela leva `tenant` como primeira coluna da chave; cada tenant tem âncora e cadeia próprias (o nome do tenant entra na âncora e em cada record). O tenant vem do `.omhl.toml` (D10, D23), então mover ou renomear a pasta não muda nada. Renomear o tenant no `.omhl.toml` cria outro tenant vazio; não há operação de renomear na v1. Dois clones do mesmo repositório na mesma máquina compartilham o tenant. Testes: T5c. A ressalva é a máquina nova ou o banco perdido: as extensions recarregam o banco, mas o que só vive nele não volta (item 6). |
| P4 | Teste do `when` bruto | Precisa do dono | É um experimento com sessão real de agente (protocolo em [jury-verdict.md](../research/vocab/jury-verdict.md), linhas 47 e 48), sem código. Fora do passo 3. |
| P5 | Licença do schema.org | Proposta; o dono confirma | O native da v1 não embute nenhum subconjunto do schema.org nem de outro padrão; só guarda URIs em `equivalent` (D15), que são texto de referência, não conteúdo licenciado. Isso evita a verificação de CC BY-SA que ninguém fez (jury-verdict.md, linha 39). |
| P6 | Entidade work item e gate nativo de PR | Precisa do dono | Esboço dos campos para o passo 4: `provider`, `repo`, `number` ou `branch`, `status` (`open`/`closed`). O SHA do snapshot vive no `Snapshot` de cada revision do node `attestation` (node type do graph de checks), não no work item. O glossary prevê um tipo de node especializado por escopo (o pull request é uma especialização do work item); se o tipo do PR será `work_item` ou uma especialização é decisão do passo 4, [DONO]. Itens 12, 13 e 14 da seção 2 listam o que falta decidir. |
| P7 | Forma do campo de snapshot | Proposta; o dono confirma no P0 | `{kind, id}` com `kind` enum fechado em Rust (D31: tipos novos entram direto no núcleo), serializado como `"git-commit"`. Critério mínimo para aceitar um tipo novo, que sirva de regra escrita: (i) o `id` é endereçado pelo conteúdo; (ii) é imutável depois de criado; (iii) uma sonda local, sem rede, o obtém de forma determinística; (iv) a sonda consegue declarar "não reproduzível" (como árvore suja) e então o snapshot é recusado. Validação do SHA e regras de uso: itens 1 e 11 da seção 2. |

## 4. Plano executável do passo 3

### 4.1 Escopo

**Entra:** workspace Cargo; crate `hexlog` com `engine`, `schema` e `store` (SQLite); Facade; cadeia por tenant; id semântico; revisão com `base`; relation, retirement, snapshot tipado; lote atômico; idempotência; verificação da cadeia; testes dos critérios 1, 3 e 7 (e das regras de C2 que dependem só do motor: reservar id).

**Não entra (passos seguintes):** FTS5, cursor com marcador e `changesSince`, anexos e blobs, telemetria, avaliação de `gate`, conteúdo native, carregador de extensions (arquivo para banco, D34), grafos de decisões, de checks e de work item, `render`, `.omhl.toml`, projeção, adapters CLI, MCP e hook, caminho padrão do banco (fica em `omhl`; o motor recebe o caminho pela `Config`). O crate `omhl` recebe apenas um `main` mínimo que confirma que o workspace compila e que a dependência vai de `omhl` para `hexlog`, nunca o contrário.

**Execução (para a fase de handoff):** código de produto por `executor`, testes por `test-engineer`, tudo em sonnet; o `code-simplifier` reporta com escopo `entrega` e não edita.

### 4.2 Layout do workspace e dos crates

```text
oh-my-hexflow/
  Cargo.toml                       # [workspace] members; resolver = "3"; [workspace.package]; [workspace.lints]
  Cargo.lock                       # versionado (há binário)
  rust-toolchain.toml              # canal fixo (1.97.0, o instalado), components rustfmt e clippy
  .gitignore                       # target/
  crates/
    hexlog/
      Cargo.toml
      src/
        lib.rs                     # `mod engine; mod schema; mod store;` privados; `pub use` explícito da Facade, dos tipos públicos, de CHECKED_FIELDS e de `pub mod chain` (funções puras de hash)
        facade.rs                  # Hexlog, Config, Deps, requests e views
        error.rs                   # ErrorCode, HexlogError, Detail
        ids.rs                     # Name, TenantId, SemanticId, TypeName, RelationTypeName, RecordId, Hash, Alias, IdempotencyKey, ExternalId
        engine/
          mod.rs                   # `pub(crate) mod` para ports, model, snapshot, limits e chain; `pub(in crate::engine) mod` para decide, rules e verify (Facade via `pub(crate) use` de `decide` e `verify_chain`, e `pub use verify::CHECKED_FIELDS` simples, cada um só junto com o consumidor); nunca `pub mod`
          model.rs                 # AppendItem, ItemBody, Lineage, BaseRef, Author, Origin, AppendOutcome, AppendedRecord, ChainHead, RecordBody, BatchMeta, ListFilter, FieldName, Scalar, PageRequest, RelationQuery
          snapshot.rs              # Snapshot, SnapshotKind (único lugar que pode citar "git")
          limits.rs                # Limits e constantes
          ports.rs                 # Clock, IdGenerator, NativeNodes, NativeNode, Definitions, NodeTypeInfo, RelationTypeInfo, DataViolation, Store, TxRead, AppendPlan, NewRecord, RecordRow, RecordColumns, LatestRow, BatchRow, TenantRow, LineageKind
          chain.rs                 # canonical, sha256hex, hash_record, anchor, fingerprint
          decide.rs                # decide puro, devolve AppendPlan
          rules.rs                 # regras de revision, snapshot, relation, ciclo, reservado
          verify.rs                # verify_chain e CHECKED_FIELDS
        schema/
          mod.rs
          catalog.rs               # TypeDef, RelationTypeDef, Catalog (implementa `engine::ports::Definitions`), validação de definições
          validate.rs              # validação de data contra o JSON Schema
        store/
          mod.rs                   # só reexporta `sqlite`; os contratos moram em engine/ports.rs
          sqlite/
            mod.rs                 # SqliteStore (Mutex<Connection>), abertura, PRAGMAs
            ddl.rs                 # carrega schema.sql por include_str! e confere user_version
            schema.sql             # o DDL da seção 4.3 (arquivo SQL, fora do alcance da varredura de vocabulário)
            tx.rs                  # transação de escrita, TxRead, leituras
      tests/
        support/mod.rs             # `#![allow(dead_code)]` no topo; relógio e ids fixos, NativeNodes fake; declara catalog, tamper e mutations (T2)
        support/catalog.rs         # catálogo de teste (arquivo vazio na T2, preenchido na T3)
        support/tamper.rs          # conexão crua que desliga as travas (vazio na T2, preenchido na T4)
        support/mutations.rs       # cadeia de referência e tabela de mutações da seção 4.5 (vazio na T2, preenchido na T5c)
        semantic_id.rs  snapshot.rs  jcs_vectors.rs  catalog.rs                        # T2 e T3
        node_lifecycle.rs  snapshot_rules.rs  list_filters.rs  batch.rs  idempotency.rs  # T5a
        relations.rs                                                                     # T5b
        chain_tamper.rs  chain_properties.rs  tenant_isolation.rs                        # T5c
        concurrency.rs                                                                   # T5d
        third_graph.rs  architecture.rs                                                  # T6 e T1
        fixtures/third_graph.json  fixtures/rfc8785_vectors.json  fixtures/golden_records.json  fixtures/scanner_cases.txt
    omhl/
      Cargo.toml                   # depende de hexlog
      src/main.rs                  # --version apenas
```

Testes de unidade que precisam de itens internos (o `SqliteStore`, os triggers) ficam em módulos `#[cfg(test)]` dentro de `src/store/sqlite/`; os de integração em `tests/` usam só a API pública (regra do dono, travada por teste de `architecture`) e, para adulterar, uma conexão `rusqlite` crua ao arquivo do banco. O `engine` não importa `store` nem `schema`: ele define portas em `engine/ports.rs` e os outros dois módulos as implementam (item 22 da seção 2). **Portas que saem na API pública** (por `pub use`): `Clock`, `IdGenerator`, `NativeNodes` e `NativeNode`, porque aparecem em `Deps` e o `omhl` as implementa. **Portas que não saem** (`pub(crate)`): `Store`, `TxRead`, `Definitions`, `NodeTypeInfo`, `RelationTypeInfo`, `DataViolation`, as linhas (`TenantRow`, `LatestRow`, `BatchRow`, `RecordRow`, `RecordColumns`, `NewRecord`, `LineageKind`) e `AppendPlan`. `NodeTypeInfo` e `RelationTypeInfo` são as vistas que o `engine` precisa de `TypeDef` e `RelationTypeDef`; não se fundem com elas por outro motivo: as vistas carregam o `def_hash`, que é derivado, e `TypeDef` carrega `schema` e `equivalent`, que o `engine` não deve conhecer. (Mover `TypeDef` e `RelationTypeDef` para o `engine` também evitaria o import, mas levaria esses campos junto.)

Dependências de `hexlog` (conferidas por `architecture.rs` e, na T1, por `cargo tree`): `serde`, `serde_json` (sem `arbitrary_precision` nem `preserve_order`), `serde_json_canonicalizer`, `sha2`, `rusqlite` (`bundled`), `jsonschema` com **`default-features = false`**, `uuid` (`v7`), `thiserror`, `time`, `url` (só valida a sintaxe de `equivalent`, D15). Dev: `proptest`, `tempfile`, `proc-macro2`. A trava é uma **lista de crates proibidos** no fecho de dependências normais de `hexlog`: `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std`. Regex como dependência transitiva não é vetado: o motor recusa `pattern` no catálogo e nunca constrói regex vindo de schema (H:299).

### 4.3 Esquema SQLite

O caminho do arquivo vem da `Config`; o padrão (`$XDG_DATA_HOME/omhl/omhl.db`, senão `~/.local/share/omhl/omhl.db`, espelhando `src/directory.ts:5-16`, H:465) é do crate `omhl`. `PRAGMA`s na abertura: `journal_mode=WAL`, `synchronous=FULL` (equivale ao `fsync` por lote, H:483), `foreign_keys=ON`, `busy_timeout` configurável (padrão 5.000 ms). Versão do esquema em `PRAGMA user_version`: banco novo e vazio recebe `1`; versão maior que a suportada, ou banco não vazio com `user_version = 0`, é recusado com `STORE_ERROR` e detalhe `unsupported-schema-version` (nunca é aberto "como está"). Não há migração na v1 (D12). Banco em disco de Windows montado no WSL não é suportado (locks); documentar.

**Abertura de banco novo, também com vários processos ou threads ao mesmo tempo (`ddl.rs`).** Sequência fixa: (1) abre a conexão e define `busy_timeout` **antes de qualquer outro PRAGMA**; (2) `journal_mode=WAL`, `synchronous=FULL` e `foreign_keys=ON`; (3) um único `BEGIN IMMEDIATE`; (4) dentro dele, lê `user_version` e a lista de tabelas; se o banco está vazio (nenhuma tabela e versão 0), executa `schema.sql` e `PRAGMA user_version = 1`; se não está vazio, confere a versão e recusa a que não for 1; (5) `COMMIT`. Como o vazio é conferido **depois** de a transação exclusiva ser obtida, o segundo abridor que chega durante a criação espera pelo `busy_timeout`, enxerga o esquema pronto e não tenta criá-lo de novo.

O DDL abaixo é o conteúdo de `src/store/sqlite/schema.sql`, embutido no binário por `include_str!("schema.sql")` em `ddl.rs`. Ele não é literal de string em fonte Rust (o literal do `include_str!` é só o caminho). A varredura de vocabulário da seção 5 (C7) lê também esse arquivo: remove as palavras-chave SQL `CHECK` e `CONSTRAINT`, ignora comentários (`--` e `/* */`), tokeniza o que sobra (literais entre aspas simples contam) e procura a lista de vocabulário nos identificadores restantes. Assim `check` continua vetado como nome de tabela, coluna ou índice e liberado como palavra-chave do SQL.

```sql
CREATE TABLE tenants (
  tenant     TEXT PRIMARY KEY,
  anchor     TEXT NOT NULL,            -- sha256hex(anchor_doc)
  anchor_doc TEXT NOT NULL,            -- JCS de {"format":"hexlog-chain/1","tenant":...,"created_at":...}
  created_at TEXT NOT NULL             -- igual a anchor_doc.created_at
) STRICT;

CREATE TABLE records (
  tenant       TEXT    NOT NULL REFERENCES tenants(tenant),
  seq          INTEGER NOT NULL CHECK (seq >= 1),
  id           TEXT    NOT NULL UNIQUE,                 -- "<tenant>:<uuidv7>"
  prev_hash    TEXT    NOT NULL,
  hash         TEXT    NOT NULL,
  body         TEXT    NOT NULL,                        -- JCS do record sem prev_hash: único insumo do hash
  -- colunas derivadas de body (conferidas pelo verify_chain):
  at           TEXT    NOT NULL,
  type         TEXT    NOT NULL,                        -- tipo do node ou relation type
  lineage_kind TEXT    NOT NULL CHECK (lineage_kind IN ('node','relation')),
  lineage_key  TEXT    NOT NULL,                        -- id semântico, ou "relation_type|from|to"
  retired      INTEGER NOT NULL CHECK (retired IN (0,1)),
  revises      TEXT,                                    -- record id do latest record anterior
  relation_type TEXT,  from_id TEXT,  to_id TEXT,  from_origin TEXT,  to_origin TEXT,
  snapshot_kind TEXT,  snapshot_id TEXT,  external_id TEXT,  def_hash TEXT NOT NULL,
  PRIMARY KEY (tenant, seq)
) STRICT;

CREATE UNIQUE INDEX records_root      ON records (tenant, lineage_kind, lineage_key) WHERE revises IS NULL;
CREATE UNIQUE INDEX records_successor ON records (tenant, revises)                   WHERE revises IS NOT NULL;
CREATE INDEX records_lineage  ON records (tenant, lineage_kind, lineage_key, seq);
CREATE INDEX records_type     ON records (tenant, type, lineage_key);
CREATE INDEX records_snapshot    ON records (tenant, snapshot_kind, snapshot_id) WHERE snapshot_id IS NOT NULL;
CREATE INDEX records_relation_out ON records (tenant, relation_type, from_id) WHERE lineage_kind = 'relation';
CREATE INDEX records_relation_in  ON records (tenant, relation_type, to_id)   WHERE lineage_kind = 'relation';

CREATE TABLE batches (          -- índice de idempotência; só lotes com key
  tenant      TEXT NOT NULL REFERENCES tenants(tenant),
  key         TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  first_seq   INTEGER NOT NULL,
  item_count  INTEGER NOT NULL CHECK (item_count BETWEEN 1 AND 50),
  PRIMARY KEY (tenant, key),
  FOREIGN KEY (tenant, first_seq) REFERENCES records (tenant, seq)
) STRICT;
```

Triggers, também em `schema.sql` (defesa em profundidade; a proteção real é o hash). O `tamper` dos testes remove exatamente estes sete:

```sql
CREATE TRIGGER tenants_no_update BEFORE UPDATE ON tenants BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER tenants_no_delete BEFORE DELETE ON tenants BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER records_no_update   BEFORE UPDATE ON records   BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER records_no_delete   BEFORE DELETE ON records   BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER batches_no_update BEFORE UPDATE ON batches BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER batches_no_delete BEFORE DELETE ON batches BEGIN SELECT RAISE(ABORT, 'append-only'); END;

CREATE TRIGGER records_chain_guard BEFORE INSERT ON records
BEGIN
  SELECT RAISE(ABORT, 'seq out of order')
   WHERE NEW.seq <> COALESCE((SELECT MAX(seq) FROM records WHERE tenant = NEW.tenant), 0) + 1;
  SELECT RAISE(ABORT, 'prev_hash mismatch')
   WHERE NEW.prev_hash <> COALESCE(
         (SELECT hash   FROM records   WHERE tenant = NEW.tenant AND seq = NEW.seq - 1),
         (SELECT anchor FROM tenants WHERE tenant = NEW.tenant));
END;
```

Os seis primeiros recusam `UPDATE` e `DELETE`; o último recusa `seq` diferente do próximo contíguo do tenant e `prev_hash` diferente do `hash` do record anterior (ou da `anchor` quando `seq = 1`). O SQLite não calcula sha256; o hash é conferido em Rust.

**Vigência, sem coluna mutável:** o latest record de uma lineage é o record sem sucessor (`NOT EXISTS (SELECT 1 FROM records s WHERE s.tenant = l.tenant AND s.revises = l.id)`); a lineage está vigente quando o latest record tem `retired = 0`. `records_root` garante uma raiz por lineage e `records_successor` garante no máximo um sucessor por record: bifurcação é fisicamente impossível, e `FORK_REJECTED` é a tradução do erro de índice mais a checagem prévia em `decide`.

**Corpo do record (`body`), JCS de:**

```json
{"v":1,"tenant":"acme","seq":3,"id":"acme:<uuidv7>","at":"2026-11-07T12:00:00.000Z",
 "type":"note","lineage":{"kind":"node","id":"area.sub.item"},
 "revises":"acme:<uuidv7>","retirement":false,"data":{},
 "author":{"agent":"...","client":"cli"},
 "snapshot":{"kind":"git-commit","id":"<40 ou 64 hex>"},"external_id":"...","def_hash":"<64 hex>",
 "batch":{"fingerprint":"<64 hex>","key":"...","count":2,"aliases":{"a":0}}}
```

Regras do corpo: campo opcional ausente é omitido, nunca `null` (acrescentar campo opcional depois não muda hash antigo); `batch` só no primeiro record do lote (H:188); relation troca `lineage` por `{"kind":"relation","relation_type":..,"from":..,"to":..,"from_origin":..,"to_origin":..}` e acrescenta `native_catalog` quando algum extremo é nativo; retirement leva `"retirement":true`, sem `data`, **copia `type` e `def_hash` do latest record anterior** (a definição que valeu quando o latest record foi gravado, não a do catálogo atual); quando a lineage é uma relation, copia também `from_origin`, `to_origin` e `native_catalog` do latest record.

### 4.4 Facade pública (assinaturas, sem implementação)

`Hexlog` é `Send + Sync` e guarda o `SqliteStore` concreto (não um `Box<dyn Store>`): o `SqliteStore` tem uma `Mutex<rusqlite::Connection>` e serializa as chamadas de uma mesma instância; processos distintos (CLI, MCP, hook) abrem instâncias e conexões próprias e se coordenam por WAL, `busy_timeout` e `BEGIN IMMEDIATE`.

```rust
// crates/hexlog/src/facade.rs
pub struct Hexlog { /* privado */ }

pub struct Config {
    pub db_path: std::path::PathBuf,
    pub limits: Limits,                     // padrões da seção 4.5
    pub busy_timeout: std::time::Duration,  // padrão 5 s
    pub verify_policy: VerifyPolicy,
}
pub enum VerifyPolicy { Tail, Full }        // E6
pub struct Deps {
    pub clock: std::sync::Arc<dyn Clock>,
    pub ids: std::sync::Arc<dyn IdGenerator>,
    pub catalog: Catalog,
    pub native: std::sync::Arc<dyn NativeNodes>,
}

impl Hexlog {
    pub fn open(config: Config, deps: Deps) -> Result<Hexlog, HexlogError>;
    pub fn ensure_tenant(&self, tenant: &TenantId) -> Result<TenantInfo, HexlogError>;
    pub fn append(&self, req: AppendRequest) -> Result<AppendOutcome, HexlogError>;
    pub fn get(&self, tenant: &TenantId, id: &SemanticId) -> Result<Option<NodeView>, HexlogError>;
    pub fn lineage_records(&self, tenant: &TenantId, lineage: &Lineage) -> Result<Vec<RevisionView>, HexlogError>;
    pub fn list(&self, tenant: &TenantId, filter: &ListFilter, page: &PageRequest) -> Result<Page<NodeView>, HexlogError>;
    pub fn relations(&self, tenant: &TenantId, query: &RelationQuery) -> Result<Vec<RelationView>, HexlogError>;
    pub fn chain_head(&self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    pub fn verify_chain(&self, tenant: &TenantId, expected_chain_head: Option<&Hash>) -> Result<ChainReport, HexlogError>;
    pub fn catalog(&self) -> &Catalog;
}

// Entrada
pub struct AppendRequest { pub tenant: TenantId, pub author: Author, pub key: Option<IdempotencyKey>, pub items: Vec<AppendItem> } // 1..=50
pub struct AppendItem {
    pub alias: Option<Alias>,
    pub body: ItemBody,
    pub base: Option<BaseRef>,            // obrigatória se a lineage já tem records; proibida se não tem
    pub snapshot: Option<Snapshot>,       // regras da tabela da seção 4.5
    pub external_id: Option<ExternalId>,
}
pub enum ItemBody {
    Node       { id: SemanticId, type_name: TypeName, data: serde_json::Value },
    Relation   { relation_type: RelationTypeName, from: SemanticId, to: SemanticId },
    Retirement { lineage: Lineage },
}
pub enum Lineage { Node(SemanticId), Relation { relation_type: RelationTypeName, from: SemanticId, to: SemanticId } }
pub enum BaseRef { Id(RecordId), Alias(Alias) }
pub struct Author { pub agent: String, pub model: Option<String>, pub client: String } // 1..=100 caracteres; `client` é do adaptador (H:119, H:656)

// Saída de escrita (`AppendOutcome`, `AppendedRecord` e `ChainHead` são definidos em `engine/model.rs` e reexportados aqui)
pub struct AppendOutcome { pub records: Vec<AppendedRecord>, pub replayed: bool, pub chain_head: ChainHead }
pub struct AppendedRecord { pub alias: Option<Alias>, pub id: RecordId, pub seq: u64 }
pub struct TenantInfo { pub tenant: TenantId, pub anchor: Hash, pub created_at: String, pub created: bool }
pub struct ChainHead { pub seq: u64, pub hash: Hash }               // seq 0 e hash = âncora para tenant vazio

// Saída de leitura
pub enum Origin { Tenant, Native }
pub enum EndpointStatus { Current, Retired, MissingNative }
pub struct NodeView {
    pub id: SemanticId, pub type_name: TypeName,
    pub record_id: RecordId,                 // id técnico do latest record: serve de `base` na próxima revisão
    pub seq: u64, pub at: String, pub data: serde_json::Value,
    pub snapshot: Option<Snapshot>, pub external_id: Option<ExternalId>, pub author: Author,
    pub shadowed: bool,                      // true quando `NativeNodes` reconhece o mesmo id (item 2 da seção 2)
}
pub struct RelationView {
    pub record_id: RecordId,                 // id técnico do latest record da relation: serve de `base` de um Retirement
    pub seq: u64, pub at: String, pub relation_type: RelationTypeName,
    pub from: SemanticId, pub to: SemanticId,
    pub from_origin: Origin, pub to_origin: Origin,
    pub from_status: EndpointStatus, pub to_status: EndpointStatus,
    pub native_catalog: Option<String>, pub external_id: Option<ExternalId>, pub author: Author,
    pub retired: bool,
}
pub struct RevisionView {
    pub record_id: RecordId, pub seq: u64, pub at: String, pub revises: Option<RecordId>,
    pub retired: bool, pub data: Option<serde_json::Value>,
    pub snapshot: Option<Snapshot>, pub external_id: Option<ExternalId>, pub author: Author,
}

// Consulta (`ListFilter`, `FieldName`, `Scalar`, `PageRequest` e `RelationQuery` são definidos em `engine/model.rs`, porque `TxRead` os usa, e reexportados aqui)
pub struct ListFilter {
    pub type_name: Option<TypeName>,
    pub id_prefix: Option<String>,           // casa só na fronteira do `.`
    pub snapshot: Option<Snapshot>,
    pub where_eq: Vec<(FieldName, Scalar)>,  // igualdade em campo escalar de primeiro nível de `data` (H:337)
    pub include_retired: bool,
}
pub struct FieldName(String);                // 1..=64 de [A-Za-z0-9_-]; nada além disso entra no caminho JSON
pub enum Scalar { Str(String), Int(i64), Bool(bool) }
pub struct PageRequest { pub limit: u32, pub after_seq: Option<u64> }   // limit 1..=200 (padrão 50, H:209); ordem por `seq` do latest record
pub struct Page<T> { pub items: Vec<T>, pub next_after_seq: Option<u64> }
pub struct RelationQuery { pub relation_type: Option<RelationTypeName>, pub from: Option<SemanticId>, pub to: Option<SemanticId>, pub include_retired: bool }
// consulta pelo extremo `to` entrega a leitura inversa (D33: `before`)

// Verificação
pub struct ChainReport { pub ok: bool, pub total_records: u64, pub chain_head: Hash, pub breaks: Vec<ChainBreak>, pub total_breaks: u64 } // breaks até 100 (H:240)
pub struct ChainBreak { pub seq: Option<u64>, pub reason: BreakReason }   // `seq` é None nas quebras de âncora e de chain head esperado
pub enum BreakReason { DivergingSeq, HashMismatch, InvalidBody, ColumnMismatch, BatchIndexMismatch, AnchorMismatch, ChainHeadNotFound }
```

Portas e catálogo (`engine/ports.rs`, `schema/catalog.rs`). `Definitions` é a porta pela qual o `engine` enxerga as definições sem importar `schema`; o `Catalog` a implementa:

```rust
pub trait Clock: Send + Sync { fn now(&self) -> String; }                 // UTC, ms, "…Z"
pub trait IdGenerator: Send + Sync { fn next(&self) -> uuid::Uuid; }       // uuid v7 em produção
pub trait NativeNodes: Send + Sync {
    fn lookup(&self, id: &SemanticId) -> Option<NativeNode>;              // id reservado = lookup().is_some()
    fn native_relations(&self, relation_type: &RelationTypeName) -> Vec<(SemanticId, SemanticId)>;
    fn catalog_version(&self) -> &str;
}
pub struct NativeNode { pub type_name: TypeName }

pub trait Definitions: Send + Sync {          // implementada por `Catalog` (em `schema`); o `engine` só conhece este trait
    fn node_type(&self, name: &TypeName) -> Option<NodeTypeInfo>;
    fn relation_type(&self, name: &RelationTypeName) -> Option<RelationTypeInfo>;
    fn validate_data(&self, name: &TypeName, data: &serde_json::Value) -> Result<(), Vec<DataViolation>>; // no máximo 50, com ponteiro JSON, e `too-many-errors` quando passa disso: o corte é feito só aqui, o engine repassa
}
pub struct NodeTypeInfo { pub requires_snapshot: bool, pub def_hash: Hash }
pub struct RelationTypeInfo { pub from: Option<Vec<TypeName>>, pub to: Option<Vec<TypeName>>, pub acyclic: bool, pub def_hash: Hash }
pub struct DataViolation { pub pointer: String, pub code: String }

pub struct TypeDef { pub name: TypeName, pub schema: serde_json::Value, pub requires_snapshot: bool, pub equivalent: Option<String> }
pub struct RelationTypeDef { pub name: RelationTypeName, pub from: Option<Vec<TypeName>>, pub to: Option<Vec<TypeName>>, pub acyclic: bool, pub equivalent: Option<String> }
impl Catalog {
    pub fn new(types: Vec<TypeDef>, relations: Vec<RelationTypeDef>) -> Result<Catalog, HexlogError>; // valida tudo na construção
    pub fn type_def(&self, name: &TypeName) -> Option<&TypeDef>;
    pub fn relation_type_def(&self, name: &RelationTypeName) -> Option<&RelationTypeDef>;
}
```

Contratos de armazenamento (`engine/ports.rs`; o motor é dono das portas e o `store/sqlite` as implementa, então `engine` nunca importa `store`). `TxRead` é, em linguagem simples, as leituras que o engine faz dentro de uma gravação (e numa leitura consistente), todas na mesma transação do SQLite; nome e divisão por uso: item 21 da seção 2, [DONO]:

```rust
pub trait Store: Send + Sync {                  // objeto de trait válido: nenhum método é genérico
    fn ensure_tenant(&self, tenant: &TenantId, anchor_doc: &str, anchor: &Hash, created_at: &str) -> Result<(TenantRow, bool), HexlogError>;
    /// Abre transação exclusiva, entrega leitura transacional a `decide` e anexa o plano devolvido de forma atômica.
    fn append_with(&self, tenant: &TenantId, decide: &mut dyn FnMut(&dyn TxRead) -> Result<AppendPlan, HexlogError>) -> Result<AppendPlan, HexlogError>;
    /// Leitura consistente; o resultado sai por variável capturada pelo fechamento.
    fn read(&self, f: &mut dyn FnMut(&dyn TxRead) -> Result<(), HexlogError>) -> Result<(), HexlogError>;
}
pub trait TxRead {
    fn tenant(&self, tenant: &TenantId) -> Result<Option<TenantRow>, HexlogError>;
    fn chain_head(&self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    fn latest_record(&self, tenant: &TenantId, lineage: &Lineage) -> Result<Option<LatestRow>, HexlogError>;
    fn record_by_id(&self, id: &RecordId) -> Result<Option<RecordRow>, HexlogError>;
    fn batch_by_key(&self, tenant: &TenantId, key: &IdempotencyKey) -> Result<Option<BatchRow>, HexlogError>;
    fn batch_records(&self, tenant: &TenantId, first_seq: u64, count: u32) -> Result<Vec<RecordRow>, HexlogError>;
    fn current_nodes(&self, tenant: &TenantId, filter: &ListFilter, page: &PageRequest) -> Result<Vec<RecordRow>, HexlogError>;
    fn revisions(&self, tenant: &TenantId, lineage: &Lineage) -> Result<Vec<RecordRow>, HexlogError>;
    fn latest_relations(&self, tenant: &TenantId, query: &RelationQuery) -> Result<Vec<RecordRow>, HexlogError>;
    /// Pares das relations vigentes de `relation_type` cujos extremos do tenant também estão vigentes (base da detecção de ciclo).
    fn live_relation_pairs(&self, tenant: &TenantId, relation_type: &RelationTypeName) -> Result<Vec<(SemanticId, SemanticId)>, HexlogError>;
    fn records_from(&self, tenant: &TenantId, from_seq: u64, limit: u32) -> Result<Vec<RecordRow>, HexlogError>;   // verificação
    fn batches(&self, tenant: &TenantId) -> Result<Vec<BatchRow>, HexlogError>;
}

pub struct TenantRow { pub tenant: TenantId, pub anchor: Hash, pub anchor_doc: String, pub created_at: String }
pub struct LatestRow { pub record_id: RecordId, pub seq: u64, pub retired: bool, pub type_name: Name, pub def_hash: Hash } // `Name`: a coluna guarda tipo de node ou relation type
pub struct BatchRow { pub key: IdempotencyKey, pub fingerprint: Hash, pub first_seq: u64, pub item_count: u32 }
pub struct RecordRow { pub tenant: TenantId, pub seq: u64, pub id: RecordId, pub prev_hash: Hash, pub hash: Hash, pub body: String, pub columns: RecordColumns }
pub struct RecordColumns {
    pub at: String, pub type_name: Name, pub lineage_kind: LineageKind, pub lineage_key: String, pub retired: bool,
    pub revises: Option<RecordId>, pub relation_type: Option<RelationTypeName>, pub from_id: Option<SemanticId>, pub to_id: Option<SemanticId>,
    pub from_origin: Option<Origin>, pub to_origin: Option<Origin>, pub snapshot: Option<Snapshot>,
    pub external_id: Option<ExternalId>, pub def_hash: Hash,
}
pub enum LineageKind { Node, Relation }
pub struct NewRecord { pub seq: u64, pub id: RecordId, pub prev_hash: Hash, pub hash: Hash, pub body: String, pub columns: RecordColumns }
/// Resultado de `decide`: o que gravar (vazio no replay) e o que devolver ao chamador. `AppendOutcome` vem de `engine/model.rs`.
pub struct AppendPlan { pub records: Vec<NewRecord>, pub batch: Option<BatchRow>, pub outcome: AppendOutcome }
```

### 4.5 Regras do motor (o que `decide` e `verify` implementam)

**Cadeia por tenant (H:222-226, H:628).**

- `hash = sha256hex(prev_hash + JCS(corpo sem prev_hash))`; `prev_hash` do `seq = 1` é a âncora; `anchor = sha256hex(JCS({"format":"hexlog-chain/1","tenant":T,"created_at":...}))`. O nome do tenant está na âncora e em cada corpo, então tenants com o mesmo conteúdo nunca compartilham hash.
- `seq` contíguo de 1 a n por tenant, imposto pelo trigger de inserção e conferido na verificação.
- **Campos que `verify_chain` confere, exatamente:** `pub const CHECKED_FIELDS: &[&str]` em `engine/verify.rs` usa **exatamente os identificadores da coluna `Id` da tabela de mutações abaixo, menos `expected_chain_head`**, que é argumento da chamada e não campo armazenado. Cada linha da tabela tem um `Id` único. Por record: `seq`, `prev_hash`, `hash`, `body_content`, `body_not_canonical`, `body_not_json`, `body_tenant`, `body_seq` e as colunas derivadas `id`, `at`, `type`, `lineage_kind`, `lineage_key`, `retired`, `revises`, `relation_type`, `from_id`, `to_id`, `from_origin`, `to_origin`, `snapshot_kind`, `snapshot_id`, `external_id`, `def_hash`. Por tenant: `tenants_anchor`, `tenants_anchor_doc`, `tenants_created_at`. Por lote: `batches_key`, `batches_fingerprint`, `batches_first_seq`, `batches_item_count`, `batches_row_missing`, `batches_row_extra`. Não é conferido, e portanto não entra no proptest: validade do `data` contra o schema atual (decisão de E5), conteúdo nativo, existência dos extremos das relations.
- **Ordem das quebras em `ChainReport.breaks`:** primeiro as conferências do tenant (`AnchorMismatch`), depois as de lote (`BatchIndexMismatch`), depois as de record em ordem crescente do `seq` armazenado, e por fim `ChainHeadNotFound`. Os testes afirmam sobre `breaks[0]`; quebras seguintes (por exemplo, o record seguinte a um `hash` adulterado) são esperadas e não são afirmadas. A âncora usada pela conferência do primeiro record é **`sha256hex(anchor_doc)` recalculado**, não o valor armazenado em `tenants.anchor`; por isso adulterar só `tenants.anchor` produz uma única quebra.
- **Ordem de conferência por record** (a primeira falha dá o motivo daquele record): `seq` (`DivergingSeq`); `prev_hash` contra o `hash` armazenado do record anterior (ou contra a âncora recalculada, no primeiro) e hash recalculado contra o armazenado, ambos `HashMismatch` (um motivo só para `prev_hash`, `hash` e `body` com conteúdo alterado); `body` legível e igual à sua recanonicalização (`InvalidBody`); `body.tenant` e `body.seq` contra o tenant verificado e o `seq` do record (`ColumnMismatch`); cada coluna derivada contra o valor recalculado do corpo (`ColumnMismatch`). Motivos de quebra (`BreakReason`): `DivergingSeq`, `HashMismatch`, `InvalidBody`, `ColumnMismatch`, `BatchIndexMismatch`, `AnchorMismatch`, `ChainHeadNotFound`. Quebra é resultado, não erro (H:241). Depois de uma quebra a verificação continua do `seq` armazenado seguinte.
- `expected_chain_head`: precisa ser o hash de algum record válido ou a âncora; senão `ChainHeadNotFound` (H:242). Sem ele, apagar a cauda não é detectável: o teste documenta isso.
- Política de verificação (E6, [DONO]): `Tail` confere o último record (hash recalculado igual ao armazenado e igual ao chain head) a cada escrita; `Full` roda a verificação completa antes de cada escrita e leitura. Cadeia quebrada na escrita vira `CHAIN_BROKEN` (equivale a `PROCESS_CORRUPTED`/`broken-chain` do Hexlog).

**Tabela de mutações (fonte única de `tests/support/mutations.rs`).** Cadeia de referência de 6 records: RC1 e RC2 num lote com `key` `K1` (RC1 nó `a.one`, tipo `note`; RC2 nó `a.two`, tipo `requires_snapshot`, com `snapshot` e `external_id`), RC3 revisão de `a.one`, RC4 relation de `a.one` para um nativo (com `native_catalog`), RC5 `Retirement` de `a.two`, RC6 `Retirement` da relation. O alvo é achado por forma (primeiro record que tem o campo), não por posição, para o proptest poder anexar nós extras antes. O helper `tamper` (conexão `rusqlite` crua ao arquivo) **remove os sete triggers da seção 4.3 e liga `PRAGMA foreign_keys = OFF` e `PRAGMA ignore_check_constraints = ON`**; `UNIQUE` e chave primária continuam valendo, e cada valor adulterado abaixo respeita ambos. Todo teste confere que a adulteração alterou exatamente 1 linha antes de verificar.

| Id (`CHECKED_FIELDS`) | Alvo | Valor adulterado | `breaks[0]` | `seq` de `breaks[0]` |
|---|---|---|---|---|
| `seq` | RC3 | `seq := 99` | `DivergingSeq` | alvo + 1 |
| `prev_hash` | RC3 | 64 zeros | `HashMismatch` | alvo |
| `hash` | RC3 | 64 `f` (o RC4 também quebra depois) | `HashMismatch` | alvo |
| `body_content` | RC3 | um valor de `data` trocado, JSON ainda canônico | `HashMismatch` | alvo |
| `id` | RC3 | id novo e não usado | `ColumnMismatch` | alvo |
| `at` | RC3 | `1999-01-01T00:00:00.000Z` | `ColumnMismatch` | alvo |
| `type` | RC3 | `other` | `ColumnMismatch` | alvo |
| `lineage_kind` | RC3 | `relation` | `ColumnMismatch` | alvo |
| `lineage_key` | RC3 | `a.other` | `ColumnMismatch` | alvo |
| `retired` | RC3 | `1` | `ColumnMismatch` | alvo |
| `revises` | RC3 | id técnico válido e não usado | `ColumnMismatch` | alvo |
| `def_hash` | RC3 | 64 zeros | `ColumnMismatch` | alvo |
| `relation_type` | RC4 | `other-relation-type` | `ColumnMismatch` | alvo |
| `from_id` | RC4 | `a.two` | `ColumnMismatch` | alvo |
| `to_id` | RC4 | `n.other` | `ColumnMismatch` | alvo |
| `from_origin` | RC4 | `native` | `ColumnMismatch` | alvo |
| `to_origin` | RC4 | `tenant` | `ColumnMismatch` | alvo |
| `snapshot_kind` | RC2 | `NULL` | `ColumnMismatch` | alvo |
| `snapshot_id` | RC2 | 40 zeros | `ColumnMismatch` | alvo |
| `external_id` | RC2 | `zzz` | `ColumnMismatch` | alvo |
| `body_not_canonical` | RC6 | chaves fora de ordem; **hash recalculado** | `InvalidBody` | alvo |
| `body_not_json` | RC6 | texto `not json`; **hash recalculado** | `InvalidBody` | alvo |
| `body_tenant` | RC6 | `tenant` do corpo `zz`; **hash recalculado** | `ColumnMismatch` | alvo |
| `body_seq` | RC6 | `seq` do corpo `7`; **hash recalculado** | `ColumnMismatch` | alvo |
| `tenants_anchor` | — | `tenants.anchor` := 64 zeros (única quebra) | `AnchorMismatch` | nenhum |
| `tenants_anchor_doc` | — | `anchor_doc` com outro `created_at`, `anchor` e `created_at` intactos (o RC1 quebra depois) | `AnchorMismatch` | nenhum |
| `tenants_created_at` | — | `tenants.created_at` := outro instante | `AnchorMismatch` | nenhum |
| `batches_key` | `K1` | `K9` | `BatchIndexMismatch` | nenhum |
| `batches_fingerprint` | `K1` | 64 zeros | `BatchIndexMismatch` | nenhum |
| `batches_first_seq` | `K1` | `first_seq + 1` | `BatchIndexMismatch` | nenhum |
| `batches_item_count` | `K1` | `3` | `BatchIndexMismatch` | nenhum |
| `batches_row_missing` | `K1` | apagar a linha | `BatchIndexMismatch` | nenhum |
| `batches_row_extra` | — | inserir linha `K8` apontando para RC1 | `BatchIndexMismatch` | nenhum |
| `expected_chain_head` (argumento, fora de `CHECKED_FIELDS`) | — | hash aleatório desconhecido | `ChainHeadNotFound` | nenhum |

A tabela tem 34 linhas e `CHECKED_FIELDS` tem 33 identificadores (todos menos `expected_chain_head`). "Hash recalculado" usa a função pública `hexlog::chain::hash_record` e vale só para o último record, para os seguintes não quebrarem. "alvo + 1" na linha `seq` existe porque o record adulterado muda de posição e quem diverge é o seguinte.

**Id semântico (H:99-107, `src/domain/ids.ts:15-18`).** Segmentos `^[a-z0-9][a-z0-9-]{0,62}$` separados por `.`, no máximo 200 caracteres, sem ponto inicial, final ou duplo. Validador escrito à mão (sem regex), com tabela de aceites e recusas. O id técnico é `<tenant>:<uuidv7>`, atribuído pelo motor, nunca pelo chamador (H:101). Prefixo casa só na fronteira do `.` (`a.b` casa `a.b` e `a.b.c`, não `a.bc`; H:107, `src/domain/gate.ts:61-64`). O mesmo regex vale para tenant, tipo de node e relation type.

**Ordem de recusa da escrita (adaptada de H:305-314; o replay sobe para logo depois de tenant e forma).**

1. Forma do lote: 1 a 50 itens, aliases únicos, `alias` só referido por item posterior, tamanhos, `key` válida (`INVALID_INPUT`).
2. Tenant existe (`TENANT_NOT_FOUND`).
3. **Replay ou conflito por `key`**, antes de qualquer checagem estática ou dependente do histórico gravado: a impressão (`fingerprint`) é calculada sobre os itens de entrada como chegaram, e a consulta a `batches` é uma leitura fora da transação de escrita. Mesma impressão: devolve os ids gravados, `replayed: true`, nada novo gravado. Impressão diferente: `IDEMPOTENCY_CONFLICT`. Assim reenviar um lote antigo continua devolvendo `replayed` mesmo depois de uma atualização do plugin que reservou um id ou mudou o catálogo (H:182). A impressão é `sha256hex(JCS(itens de entrada))`, sem `key` e sem `author` (H:180, `src/domain/chain.ts:59-62`). Limite aceito: o replay não verifica a cadeia; quem quer essa garantia chama `verify_chain`.
4. Estáticas, antes da transação: node type e relation type no catálogo (`TYPE_NOT_FOUND`, `RELATION_TYPE_NOT_FOUND`); `data` válido no schema, com todas as violações do lote em um `INVALID_RECORD` (H:311); `data` com no máximo `data_max_chars` caracteres canônicos; **número em `data` só se for inteiro seguro**; regras de `snapshot` e `external_id` da tabela abaixo; semantic id de node reservado por `NativeNodes` (`RESERVED_ID`, C2); relation de um node consigo mesmo (`self-relation`).
5. Dentro de `BEGIN IMMEDIATE`: cauda da cadeia íntegra (`CHAIN_BROKEN`); **nova consulta de `key`** (outra gravação pode ter vencido a corrida: replay ou conflito, como no passo 3).
6. Dependentes do histórico gravado, item a item com sobreposição em memória dos itens anteriores do mesmo lote (H:160): lineage/`base` (regra abaixo), extremos de relation existentes e vigentes (no tenant com sobreposição, ou em `NativeNodes`; ausente é `INVALID_RECORD` `endpoint-not-found`), tipos permitidos pela `RelationTypeDef` (`from`/`to`, H:144), pelo menos um extremo no tenant, relation entre dois nativos recusada, `acyclic`.

**Números em `data`.** Aceita-se só número que `serde_json` represente como `i64` ou `u64` e que esteja em ±(2^53 − 1). Literal com ponto ou expoente (`1.0`, `1e2`, `-0.0`) chega como `f64` e é recusado, mesmo quando o valor é inteiro (`INVALID_RECORD`, detalhe `number-not-safe-integer`, com o ponteiro). O mesmo vale em qualquer objeto aninhado.

**Regras de `snapshot` e `external_id` (tabela a confirmar no P0; um teste por linha em `snapshot_rules.rs`).**

| # | Item | Node type / relation type | `snapshot` | Resultado |
|---|---|---|---|---|
| S1 | Node (raiz) | `requires_snapshot = true` | ausente | `INVALID_RECORD` `snapshot-required` |
| S2 | Node (raiz) | `requires_snapshot = true` | válido | aceita |
| S3 | Node (raiz) | `requires_snapshot = false` | presente | `INVALID_RECORD` `snapshot-not-allowed` |
| S4 | Node (revisão) | `requires_snapshot = true` | ausente | `snapshot-required` (a revisão não herda o `snapshot` do latest record) |
| S5 | Node (revisão) | `requires_snapshot = false` | presente | `snapshot-not-allowed` |
| S6 | Retirement | qualquer tipo, inclusive `requires_snapshot = true` | presente | `snapshot-not-allowed` |
| S7 | Retirement | qualquer tipo | ausente | aceita |
| S8 | Relation (raiz ou revisão) | qualquer relation type | presente | `snapshot-not-allowed` |
| S9 | qualquer | — | malformado (7, 39, 41 caracteres, maiúsculas, não hex) | `INVALID_RECORD` `snapshot-format` |
| X1 | `external_id` em Node ou Relation | — | 1 a 200 caracteres | aceita |
| X2 | `external_id` em Retirement | — | presente | `INVALID_RECORD` `external-id-not-allowed` |
| X3 | `external_id` | — | vazio ou com mais de 200 | `INVALID_RECORD` `external-id-format` |
| X4 | `external_id` em revisão de Node ou Relation | — | omitido na revisão | aceita; o novo record não tem `external_id` (não é herdado) e `NodeView.external_id` do novo latest record é `None` |

**Regra de um vigente por lineage (H:636, H:157-159).** Para cada item `Node`/`Relation`/`Retirement`, `latest_record` é o latest record da lineage. Sem `latest_record`, `base` precisa ser `None`. Com `latest_record`, `base` precisa ser exatamente o record id dele; senão `FORK_REJECTED` com `details[0].current` = record id do latest record e `retired` indicando se ele é uma retirement (H:158). Revision mantém o tipo (`INVALID_RECORD`, `type-changed`; H:133). Revisar uma lineage cujo latest record é uma retirement é permitido e a recoloca em vigência (o node volta a ter current revision). `Retirement` exige `latest_record` e `base`. Dois escritores concorrentes sobre o mesmo latest record: o segundo `append` relê sob a transação e recebe `FORK_REJECTED` (H:159).

**Relation e ciclo.** Relation com extremo retirado, ou retirada ela mesma, **não entra** na detecção de ciclo (`live_relation_pairs` já a exclui). A busca de ciclo usa as relations vivas do tenant, as de `native_relations` e as do próprio lote, no estilo de `src/domain/relations.ts:100-123`. Relation para extremo que se retira depois continua gravada e passa a mostrar `EndpointStatus::Retired` na leitura.

**Snapshot (D31).** `Snapshot { kind: SnapshotKind, id: String }`, `SnapshotKind` enum fechado e `#[non_exhaustive]`, hoje só `GitCommit` (`"git-commit"`), `id` hexadecimal minúsculo de 40 ou 64 caracteres, sem normalizar e sem conferir existência. Mais um tipo exige: variante nova, validador e a regra de aceitação de P7.

**Equivalent (D15).** `equivalent` em `TypeDef` e `RelationTypeDef`: o `Catalog::new` só valida a sintaxe da URI com o crate `url` e nunca a resolve nem a lê para decidir comportamento (jury-verdict.md, linha 9).

**Catálogo e schema (H:298-303).** JSON Schema 2020-12 com raiz `type: "object"`; `pattern`, `patternProperties` e qualquer `$ref` que não seja interno (`#...`) recusados; `$id` e `$async` recusados. `def_hash = sha256hex(JCS(definição))` gravado em cada record. A validação de `data` não roda de novo na leitura nem em `verify_chain`.

**`where_eq` (ligação segura).** O nome do campo é um `FieldName` validado (1 a 64 de `[A-Za-z0-9_-]`) e vira o caminho JSON `$.data."<nome>"` montado em Rust e passado como **parâmetro ligado** a `json_extract(body, ?)`; o valor também é parâmetro ligado. O prefixo `$.data.` garante que um nome igual a um campo do envelope (`type`, `snapshot`, `tenant`, `id`) lê só o campo de `data`, nunca o do envelope. Nenhum texto do chamador entra no SQL. Só campo escalar de primeiro nível de `data` (H:337).

**Limites (portados de H:194-203, H:209; tetos de entrada, não de formato).**

| Limite | Padrão | Fonte |
|---|---|---|
| `data_max_chars` (JCS) | 16.000 | H:194 |
| itens por lote | 50 | H:195 |
| `key` | 1 a 200 | H:197 |
| aliases por lote | 50 | H:198 |
| `details` por erro | 50 | H:204 |
| página de `list` | padrão 50, máximo 200 | H:209 |
| `author` campos | 1 a 100 | H:119 |
| `external_id` | 1 a 200 | item 2 da seção 2 |

**Catálogo de erros (H:510-540, H:652-653).** `{code, message, details[{path, code, message}]}` com JSON Pointer em `path`. Códigos do passo 3: `INVALID_INPUT`, `INVALID_RECORD`, `INVALID_SCHEMA` (definição de node type ou relation type recusada em `Catalog::new`, H:517), `TYPE_NOT_FOUND`, `RELATION_TYPE_NOT_FOUND`, `TENANT_NOT_FOUND`, `RESERVED_ID`, `FORK_REJECTED`, `CYCLE_REJECTED`, `IDEMPOTENCY_CONFLICT`, `CHAIN_BROKEN`, `BUSY` (substitui `LOCK_TIMEOUT`, H:653), `STORE_ERROR` (substitui `IO_ERROR`; detalhes: código do SQLite em minúsculas, `unsupported-schema-version`), `INTERNAL`. `RELATION_TYPE_NOT_FOUND` vale só para relation type ausente do catálogo; extremo de relation ausente é `INVALID_RECORD` `endpoint-not-found` (diverge de H:538, onde o código cobria o destino ausente). Fora: `LEGACY_DATA`, `PROCESS_TOO_LARGE`, os de anexo e cursor (H:653). Mensagem de erro nunca ecoa valor de `data` nem caminho do banco.

### 4.6 Portão P0 e tarefas ordenadas

**Portão P0 (do dono, antes da T2).** A T2 é a primeira tarefa que grava o formato do record (hash dourado e fixture), e tudo de T3 em diante constrói sobre ele; por isso o corpo do record é congelado antes dela. O dono responde:

- item 5 (relation com origem nativa) e item 10 (`Retirement`);
- item 19 (vocabulário estrutural no corpo: `Retirement`, `requires_snapshot`, `Snapshot`, origens, `native_catalog`, `def_hash`);
- item 1 (a releitura de D31) e P7 (forma do snapshot);
- item 16 (`alias` e `batch.aliases` no corpo);
- a tabela de regras de `snapshot` e `external_id` (item 11, seção 4.5, linhas S1 a S9 e X1 a X4).

Respostas que cortam um campo ou uma variante encolhem o corpo da seção 4.3 antes de existir dado. Depois do P0, mudar o corpo do record é mudar o formato (tenant novo, D12). O P0 trava a T2 e tudo depois dela até o dono responder; é a escolha do plano, e a T1 pode rodar antes.

**Grafo de dependências:** T1; T2 (T1, P0); T3 e T4 (cada uma depende de T2, paralelas entre si); T5a (T3, T4); T5b (T5a); T5c (T5b); T5d (T5c); T6 (T5d). Não há tarefa que use algo que nasce em tarefa posterior; as seções "Pronto" abaixo foram conferidas contra a seção 5 uma a uma.

Cada tarefa fecha com `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` limpos. O teste vem junto do código na mesma tarefa (teste vermelho primeiro quando a regra é nova).

**T1. Esqueleto do workspace, travas de arquitetura e spikes** (dependências: nenhuma)

- Arquivos: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `crates/hexlog/Cargo.toml`, `crates/hexlog/src/lib.rs`, `crates/omhl/Cargo.toml`, `crates/omhl/src/main.rs`, `crates/hexlog/tests/architecture.rs`, `crates/hexlog/tests/fixtures/scanner_cases.txt`.
- Entrega: workspace compila; `[workspace.lints]` com `unsafe_code = "forbid"`; `architecture.rs` com os oito testes de arquitetura da seção 5 (C7; um deles é o do próprio varredor, que lê `.rs` e `schema.sql`); todos tratam arquivo ou pasta ainda inexistente (como `engine/`, `engine/snapshot.rs` e `schema.sql`) sem falhar, e cada um dos três testes de imports, visibilidade e API pública prova a regra dos dois lados com um trecho embutido no próprio teste. Spikes curtos, descartáveis, cujos resultados vão para os seguimentos do ADR: (a) `serde_json_canonicalizer` contra os vetores do RFC 8785; (b) `rusqlite` `bundled` expõe `STRICT`, `json_extract`, `PRAGMA journal_mode=WAL` e FTS5; (c) `jsonschema` com `default-features = false`: `cargo tree -p hexlog -e normal` não lista nenhum crate da lista proibida, `$ref` externo é recusado e a validação 2020-12 roda sem rede; se a crate precisar de uma feature que traga crate proibido, trocar para `boon` ou outro validador antes de seguir.
- Pronto: os oito testes de `architecture.rs` verdes (um deles é o do próprio varredor; três travam imports do `engine`, visibilidade dos filhos e API pública dos testes); saída de `cargo tree` sem crate proibido registrada; se (a) reprovar, aplicar E2-C antes de seguir.

**T2. Primitivas de domínio e cadeia pura** (depende de T1 e do P0)

- Arquivos: `src/lib.rs` (declara `mod engine; mod schema; mod store;` e reexporta tipos de id, erros, `Snapshot`, tipos de `model.rs` e o módulo público `chain`; **a T2 declara os três módulos de uma vez**, para T3 e T4, que são paralelas, não disputarem a declaração), `src/schema/mod.rs` e `src/store/mod.rs` (vazios; a T3 e a T4 os preenchem), `src/ids.rs`, `src/error.rs`, `src/engine/{mod,model,snapshot,limits,ports,chain}.rs`, `tests/support/mod.rs` (com `#![allow(dead_code)]` no topo, porque cada arquivo de teste usa só parte do suporte; relógio e ids fixos, `NativeNodes` fake com lista alterável; **declara os três módulos `catalog`, `tamper` e `mutations`**) e os arquivos vazios `tests/support/{catalog,tamper,mutations}.rs`, que T3, T4 e T5c preenchem sem editar o `mod.rs` (T3 e T4 são paralelas); testes `tests/{semantic_id,snapshot,jcs_vectors}.rs`, `tests/fixtures/{rfc8785_vectors,golden_records}.json`.
- Entrega: tipos de id com validação, `Snapshot`, `Author`, `AppendItem`, `AppendOutcome` e demais tipos de `model.rs`, os contratos de `ports.rs` (só tipos e traits), `canonical`, `sha256hex`, `hash_record`, `anchor`, `fingerprint`, catálogo de erros. Os módulos de `engine/` que ainda não têm consumidor levam `#![allow(dead_code)]` temporário no topo de `engine/mod.rs`, com o comentário `removido na T6`, para o `clippy -D warnings` não acusar código sem uso até a T5c (o último consumidor, `verify`, nasce nela); o `store/mod.rs` recebe o mesmo `allow` na T4. Os `pub(crate) use` de `engine/mod.rs` entram só junto com o consumidor (T5a e T5c).
- Fixture dourada: `golden_records.json` guarda 3 documentos de record (um simples, um com Unicode e um com inteiros nos limites ±(2^53 − 1)), cada um com `prev_hash` e o **hash calculado pelo Hexlog TS** (`sha256hex(prevHash + canonicalize(corpo))`, a mesma conta de `src/domain/chain.ts:64-67`, rodada uma vez em `/home/gabriel/personal/hexlog` no commit `5d1e22b`), com o comando e o commit anotados na própria fixture. O teste Rust recalcula e compara com o valor do TS; nenhum valor dourado nasce da implementação Rust.
- Pronto: `semantic_id` (2 testes), `snapshot` (2 testes) e `jcs_vectors` (3 testes) verdes; os 3 hashes da fixture iguais aos do TS; `architecture` continua verde.

**T3. Catálogo e validação de schema** (depende de T2; paralela à T4)

- Arquivos: `src/schema/{mod,catalog,validate}.rs`, `src/lib.rs` (**a T3 é a dona do `lib.rs` entre T3 e T4**: acrescenta só o `pub use` de `Catalog`, `TypeDef` e `RelationTypeDef`, que o `tests/catalog.rs` usa pela API pública; a T4 não edita o `lib.rs`), `src/ids.rs` (só o `Deserialize` de `TypeName` e `RelationTypeName`), `tests/support/catalog.rs` (preenche o arquivo vazio da T2); testes `tests/catalog.rs`.
- Entrega: `TypeDef` e `RelationTypeDef` com `Serialize` e `Deserialize` (a leitura de `TypeName` e `RelationTypeName` passa pela validação de nome e recusa nome inválido), o que a T6 exige para carregar `third_graph.json`; `Catalog::new` com todas as recusas da seção 4.5 e `def_hash`; `Catalog` implementa a porta `Definitions` de `engine/ports.rs` (a dependência vai de `schema` para `engine`, nunca o contrário); `validate_data` que devolve todas as violações com ponteiro JSON (limite de 50) e recusa número que não seja inteiro seguro.
- Pronto: os 10 testes de `catalog.rs` verdes e o teste de unidade `it_should_expose_requires_snapshot_def_hash_and_acyclic_through_the_definitions_port` (módulo `#[cfg(test)]` em `src/schema/catalog.rs`, fora da contagem de 10) verde; ele é o primeiro consumidor da porta `Definitions`.

**T4. Store SQLite** (depende de T2; paralela à T3)

- Arquivos: `src/store/mod.rs` (declara `sqlite` e leva o `#![allow(dead_code)]` temporário: sem `Hexlog::open` até a T5a, `SqliteStore` e `ddl` não têm consumidor e o `clippy -D warnings` do Pronto desta tarefa acusaria `never constructed`; o `store` não é exportado), `src/store/sqlite/{mod,ddl,tx}.rs`, `src/store/sqlite/schema.sql`, `tests/support/tamper.rs` (preenche o arquivo vazio da T2); testes de unidade em módulo `#[cfg(test)]` de `src/store/sqlite/`.
- Entrega: o DDL e os triggers da seção 4.3 em `schema.sql`, `ddl.rs` com `include_str!`, a sequência de abertura da seção 4.3 (`busy_timeout` primeiro; vazio, `schema.sql` e `user_version = 1` dentro de um único `BEGIN IMMEDIATE`) e a verificação de `user_version`, `ensure_tenant` idempotente, `append_with` em `BEGIN IMMEDIATE` com `busy_timeout`, `TxRead` completo, `SqliteStore` com `Mutex<Connection>`; `tamper` abre conexão crua que remove os sete triggers e desliga `foreign_keys` e `ignore_check_constraints` (seção 4.5).
- Pronto: os 9 testes de `sqlite_store` da seção 5 verdes; `architecture` continua verde com o DDL em `schema.sql`, lido pelo varredor sem as palavras-chave SQL.

**T5a. `decide`, nós, revisão, `Retirement` de nó, lote, idempotência e Facade básica** (depende de T3 e T4)

- Arquivos: `src/engine/{decide,rules}.rs`, `src/engine/mod.rs` (acrescenta `pub(crate) use decide::decide;` junto com o consumidor, para não gerar `unused_imports`, que o `allow(dead_code)` não cobre), `src/facade.rs`, `src/lib.rs` (`pub use` da Facade e dos tipos públicos, incluindo as portas públicas `Clock`, `IdGenerator`, `NativeNodes` e `NativeNode`); testes `tests/{node_lifecycle,snapshot_rules,list_filters,batch,idempotency}.rs`.
- Entrega: `decide` puro sobre `TxRead` e `Definitions` com as etapas 1 a 6 da seção 4.5 para itens `Node` e `Retirement` de nó, regra de um vigente, tabela de `snapshot` e `external_id` (linhas que não envolvem relation), política `Tail` na escrita (recalcula o hash do último record com `hash_record`; a política `Full` só é ligada na T5c), `ensure_tenant`, `append`, `get` (com `shadowed`), `lineage_records`, `list` (com `snapshot`, `where_eq`, prefixo e paginação), `chain_head`. Itens `Relation` e `Retirement` de relation devolvem, por enquanto, `INVALID_INPUT` `relation-items-not-supported`, que a T5b remove.
- Pronto: todos os testes dos arquivos listados verdes (seção 5, C1, lote, idempotência e filtros).

**T5b. Relations, `NativeNodes` e ciclos** (depende de T5a)

- Arquivos: extensões em `src/engine/{decide,rules}.rs` e `src/facade.rs` (`relations`, `RelationView`, `EndpointStatus`); teste `tests/relations.rs`.
- Entrega: validação de extremos (tenant com sobreposição de lote, nativo via `NativeNodes`), tipos da `RelationTypeDef`, `acyclic` com relations vivas, nativas e do lote, `Retirement` de relation copiando origens e `native_catalog`, linhas S8 e X1 de relation; remoção do ramo `relation-items-not-supported`.
- Pronto: todos os testes de `relations.rs` verdes; `grep -rn "relation-items-not-supported" crates/hexlog/src` não encontra nada.

**T5c. `verify_chain`, `CHECKED_FIELDS`, política `Full` e testes de C3** (depende de T5b, porque a cadeia de referência tem relation e `Retirement` de relation)

- Arquivos: `src/engine/verify.rs`, `src/engine/mod.rs` (`pub use verify::CHECKED_FIELDS;` simples e `pub(crate) use verify::verify_chain;`, ambos junto com o consumidor), `src/lib.rs` (reexporta `CHECKED_FIELDS`), ajuste de `src/facade.rs` e `src/engine/decide.rs` para ligar `VerifyPolicy::Full`; `tests/support/mutations.rs` (cadeia de referência e a tabela da seção 4.5); testes `tests/{chain_tamper,chain_properties,tenant_isolation}.rs`.
- Entrega: `verify_chain` com `expected_chain_head` e a ordem de conferência da seção 4.5; `CHECKED_FIELDS`; política `Full` na escrita e na leitura.
- Pronto: todos os testes de `chain_tamper`, `chain_properties` e `tenant_isolation` verdes, inclusive `it_should_have_a_mutation_for_every_checked_field`.

**T5d. Concorrência, política de verificação e `Send + Sync`** (depende de T5c)

- Arquivos: `tests/concurrency.rs`; ajustes em `src/store/sqlite/*` só se a corrida expuser lacuna.
- Entrega: testes com duas instâncias de `Hexlog` (duas conexões, threads) e com uma instância compartilhada; `CHAIN_BROKEN` na escrita com `Tail` e com `Full`; limite do `Tail` documentado em teste; abertura concorrente de banco novo.
- Pronto: os 9 testes de `concurrency.rs` verdes e estáveis em 20 repetições seguidas (`cargo test -p hexlog --test concurrency` em laço de 20 execuções). Com exceção do de abertura concorrente (que só precisa de `Hexlog::open`) e do `it_should_be_send_and_sync` (asserção em tempo de compilação), todos dependem de `verify_chain` e do `tamper`, que já existem.

**T6. Terceiro grafo, travas finais e fechamento** (depende de T5d)

- Arquivos: `tests/third_graph.rs`, `tests/fixtures/third_graph.json`; revisão final de `tests/architecture.rs`; `src/engine/mod.rs` e `src/store/mod.rs` (G: removem o `#![allow(dead_code)]` temporário; o executor apaga os itens realmente sem uso que isso expuser e não reintroduz o `allow`).
- Entrega: o terceiro grafo só por dado e o caso do invariante no adaptador; varredura final.
- Pronto: `third_graph` (2 testes) e `architecture` (8 testes) verdes; todos os comandos de verificação da seção 5 limpos; nenhum ramo `unsupported`, `todo` nem `unimplemented` restante (`grep -rnE "todo!|unimplemented!|relation-items-not-supported" crates/hexlog/src` vazio); os `allow(dead_code)` temporários de `engine/mod.rs` (T2) e de `store/mod.rs` (T4) foram removidos (`grep -rn "allow(dead_code)" crates/hexlog/src` vazio; o `allow` de `tests/support/mod.rs` fica, porque está em `tests/`). O formato do record não muda aqui: o P0 já o congelou.

## 5. Testes e verificação

Nomes são `fn it_should_...` em inglês, completos; um comportamento por teste, estrutura arruma/age/afirma, relógio e gerador de id fixos de `tests/support`. Cada cabeçalho traz a tarefa que entrega os testes; nenhum teste usa algo que só nasce em tarefa posterior.

### C1. Gravar um node com semantic id e consultá-lo devolve a current revision

**Arquivo `semantic_id` (T2)**

| Teste | Afirma |
|---|---|
| `it_should_accept_valid_semantic_ids` | tabela: `area.sub.item`, `a`, `a.b-c.d1`, 200 caracteres no limite, segmento de 63 |
| `it_should_reject_invalid_semantic_ids` | tabela: vazio, `.a`, `a.`, `a..b`, `A.b`, `a_b`, `-a`, segmento de 64, 201 caracteres, não ASCII, espaço; `INVALID_INPUT` com ponteiro |

**Arquivo `snapshot` (T2, formato)**

| Teste | Afirma |
|---|---|
| `it_should_accept_a_lowercase_hex_snapshot_of_40_or_64_characters` | linha S2 de formato |
| `it_should_reject_a_snapshot_with_7_39_or_41_characters_uppercase_or_non_hex` | linha S9 |

**Arquivo `catalog` (T3)**

| Teste | Afirma |
|---|---|
| `it_should_reject_a_schema_with_pattern_or_pattern_properties` | `Catalog::new` dá `INVALID_SCHEMA` com detalhe `pattern-not-allowed` |
| `it_should_reject_an_external_ref` | `$ref` que não começa por `#` dá `INVALID_SCHEMA` |
| `it_should_reject_a_schema_whose_root_is_not_an_object` | recusa |
| `it_should_reject_a_malformed_equivalent_uri_and_accept_a_wellformed_one_without_resolving_it` | sintaxe validada, nenhuma consulta de rede |
| `it_should_compute_a_stable_def_hash` | mesma definição, mesmo hash em duas execuções; definição diferente, hash diferente |
| `it_should_list_every_violation_with_json_pointers_up_to_50` | dado com 60 violações devolve 50 e `too-many-errors` |
| `it_should_refuse_numbers_that_are_not_safe_integers_in_data` | `1.0`, `1e2`, `0.5`, 2^53 recusados com `number-not-safe-integer`; `7` e 2^53 − 1 aceitos |
| `it_should_accept_data_that_satisfies_the_schema` | caminho feliz |
| `it_should_load_type_and_relation_type_definitions_from_json` | JSON embutido no teste com 2 node types e 1 relation type vira `TypeDef`/`RelationTypeDef` e `Catalog::new` os aceita |
| `it_should_reject_an_invalid_type_name_when_deserializing` | `"Bad Name"` num `TypeDef` em JSON falha na leitura |

Teste de unidade da T3, fora da contagem de 10 (módulo `#[cfg(test)]` em `src/schema/catalog.rs`, porque a porta `Definitions` não é API pública): `it_should_expose_requires_snapshot_def_hash_and_acyclic_through_the_definitions_port` monta um `Catalog` com um node type `requires_snapshot` e um relation type `acyclic` e confere o que `node_type`, `relation_type` e `validate_data` devolvem (`requires_snapshot`, `def_hash` igual ao `sha256hex(JCS(definição))` calculado no próprio teste, `acyclic`, as violações limitadas a 50).

**Arquivo `node_lifecycle` (T5a)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_return_the_current_revision_when_getting_by_semantic_id` | tenant `acme`, tipo `note`; grava `area.sub.item` e a revisão 2 com `base` da 1 | `get` devolve a revisão 2 (id técnico, `data`, `seq`) |
| `it_should_list_the_records_of_a_lineage_in_order_when_reading_lineage_records` | as mesmas duas revisões | `lineage_records` devolve 2 itens em ordem de `seq`, a 2ª com `revises` = id da 1ª |
| `it_should_return_none_when_the_id_was_never_written` | tenant vazio | `get` devolve `None` |
| `it_should_hide_a_retired_node_and_allow_revising_on_top_of_the_retirement` | grava, retira com `Retirement`, regrava com `base` da retirement | `get` é `None` após retirar e volta com o dado novo depois |
| `it_should_copy_type_and_def_hash_from_the_latest_record_when_retiring` | tipo cujo catálogo muda entre a gravação e a retirement | o record `Retirement` tem o `def_hash` e o `type` do latest record, não os do catálogo novo |
| `it_should_reject_a_second_root_for_an_existing_semantic_id` | nó já gravado | `base: None` dá `FORK_REJECTED` com `current` = id do latest record |
| `it_should_reject_a_revision_with_a_stale_base` | revisões 1 e 2 | revisar de novo com `base` da 1 dá `FORK_REJECTED` |
| `it_should_reject_a_base_for_a_semantic_id_that_was_never_written` | id nunca escrito | `base: Some(..)` dá `INVALID_RECORD` `base-unknown` |
| `it_should_reject_a_revision_that_changes_the_type` | nó do tipo `note` | revisão com outro tipo dá `INVALID_RECORD` `type-changed` |
| `it_should_reject_data_violating_the_schema_and_list_every_violation_of_the_batch` | lote com 2 itens inválidos | um `INVALID_RECORD` com as violações dos dois, ponteiros JSON corretos |
| `it_should_reject_a_reserved_semantic_id` | `NativeNodes` fake reconhecendo `reserved.example.id` | gravar esse id dá `RESERVED_ID` e nada é gravado (parte do motor de C2) |
| `it_should_mark_a_tenant_node_as_shadowed_when_a_native_gains_the_same_id` | nó gravado; o fake passa a reconhecer o mesmo id | `NodeView.shadowed = true` |
| `it_should_not_echo_data_values_in_error_messages` | `data` inválido com valor distintivo | nenhuma `message` nem `details` contém o valor |

**Arquivo `snapshot_rules` (T5a; linhas da tabela da seção 4.5 que não envolvem relation)**

| Teste | Linha |
|---|---|
| `it_should_reject_a_node_without_snapshot_when_the_type_requires_it` | S1 |
| `it_should_accept_a_node_with_a_valid_snapshot_when_the_type_requires_it` | S2 |
| `it_should_reject_a_node_with_snapshot_when_the_type_does_not_require_it` | S3 |
| `it_should_require_snapshot_again_on_each_revision_of_a_requires_snapshot_type` | S4 |
| `it_should_reject_snapshot_on_a_revision_of_a_type_without_requires_snapshot` | S5 |
| `it_should_reject_snapshot_on_a_retirement_of_a_requires_snapshot_type` | S6 |
| `it_should_accept_a_retirement_without_snapshot_for_a_requires_snapshot_type` | S7 |
| `it_should_accept_external_id_on_a_node` | X1 (nó) |
| `it_should_reject_external_id_on_a_retirement` | X2 |
| `it_should_reject_external_id_that_is_empty_or_longer_than_200_characters` | X3 |
| `it_should_not_inherit_external_id_when_a_revision_omits_it` | X4 (nó) |

**Arquivo `list_filters` (T5a)**

| Teste | Afirma |
|---|---|
| `it_should_match_prefixes_only_at_dot_boundaries` | ids `a.b`, `a.b.c`, `a.bc`; `list` com prefixo `a.b` devolve os dois primeiros |
| `it_should_filter_nodes_by_snapshot` | dois records com SHAs diferentes; `list` com `snapshot` devolve só o correspondente |
| `it_should_filter_by_a_top_level_scalar_field` | `where_eq` com texto, inteiro e booleano devolve os records certos |
| `it_should_match_a_where_field_only_inside_data_even_when_its_name_equals_an_envelope_field` | nó A com `data.type = "x"` e `data.snapshot = "y"`; nó B com `data.type = "z"`; ambos de tipo `note` | `where_eq(type, "x")` devolve só A; `where_eq(type, "note")` devolve vazio (o `type` do envelope não casa) |
| `it_should_reject_a_where_field_name_with_json_path_or_sql_metacharacters` | `a.b`, `a[0]`, `a"b`, `a'b`, vazio e 65 caracteres dão `INVALID_INPUT` |
| `it_should_treat_a_hostile_scalar_value_as_data_and_leave_the_tables_intact` | valor `'; DROP TABLE records; --`: devolve vazio e as tabelas continuam |
| `it_should_page_by_seq_with_a_keyset_cursor` | com 5 nós e `limit` 2, devolve três páginas sem repetir nem pular, `next_after_seq` correto |
| `it_should_reject_a_page_limit_outside_1_to_200` | `limit` 0 e 201 dão `INVALID_INPUT` |

**Arquivos `batch` e `idempotency` (T5a)**

| Teste | Afirma |
|---|---|
| `it_should_persist_nothing_when_any_item_of_the_batch_is_invalid` (`batch`) | com o 3º item inválido, `chain_head` não muda |
| `it_should_reject_empty_and_oversized_batches` (`batch`) | 0 e 51 itens dão `INVALID_INPUT` |
| `it_should_resolve_an_alias_base_to_a_previous_item_of_the_same_batch` (`batch`) | nó e sua revisão no mesmo lote gravam |
| `it_should_reject_a_forward_or_duplicate_alias` (`batch`) | `INVALID_INPUT` |
| `it_should_replay_the_same_key_and_items_without_writing` (`idempotency`) | `replayed: true`, mesmos ids e aliases, `chain_head` igual |
| `it_should_reject_the_same_key_with_different_items` (`idempotency`) | `IDEMPOTENCY_CONFLICT` |
| `it_should_replay_before_history_dependent_rules_when_the_base_was_superseded` (`idempotency`) | reenvio depois de nova revisão ainda dá `replayed` (H:182) |
| `it_should_replay_before_static_rules_when_the_id_was_reserved_afterwards` (`idempotency`) | lote gravado; o `NativeNodes` fake passa a reservar o id; reenvio dá `replayed` e não `RESERVED_ID` |
| `it_should_replay_regardless_of_the_author_and_scope_keys_per_tenant` (`idempotency`) | autor diferente repete; mesma `key` em outro tenant grava |

**Arquivo `jcs_vectors` (T2)**

| Teste | Afirma |
|---|---|
| `it_should_canonicalize_per_rfc_8785_vectors` | `rfc8785_vectors.json`: saída igual à esperada |
| `it_should_match_the_hexlog_ts_hash_for_each_golden_record` | os 3 records de `golden_records.json` dão o hash calculado pelo TS |
| `it_should_derive_the_fingerprint_without_key_and_author` | mesmos itens com `key` e `author` diferentes, mesma impressão |

### Relation (arquivo `relations`, T5b)

| Teste | Afirma |
|---|---|
| `it_should_accept_a_relation_to_a_native_node_and_record_its_origin` | `to_origin = native` e `native_catalog` no corpo; `RelationView.to_status = Current` |
| `it_should_accept_a_relation_whose_source_is_native_and_target_is_tenant` | condicionado ao item 5; recusado com `INVALID_RECORD` `native-source-not-allowed` se o dono disser que não |
| `it_should_reject_a_relation_to_a_missing_node` | `INVALID_RECORD` `endpoint-not-found`, ponteiro no extremo ausente |
| `it_should_reject_a_relation_with_an_unknown_relation_type` | `RELATION_TYPE_NOT_FOUND` |
| `it_should_reject_a_relation_between_two_native_nodes` | `INVALID_RECORD` `native-only-relation` |
| `it_should_reject_a_self_relation` | `INVALID_RECORD` `self-relation` |
| `it_should_reject_a_relation_whose_endpoint_type_the_relation_forbids` | `INVALID_RECORD` `endpoint-type` |
| `it_should_see_nodes_of_previous_items_when_validating_a_relation_in_the_same_batch` | nó e relation no mesmo lote gravam |
| `it_should_reject_a_cycle_on_an_acyclic_relation_including_native_relations` | ciclo via relation nativa fake dá `CYCLE_REJECTED` |
| `it_should_ignore_relations_with_a_retired_endpoint_when_detecting_cycles` | retirar um extremo desfaz o ciclo para uma relation nova |
| `it_should_report_retired_endpoint_status_after_the_node_is_retired` | `RelationView.to_status = Retired` |
| `it_should_report_missing_native_endpoint_status_when_the_native_disappears` | o fake esquece o id; `RelationView.to_status = MissingNative`, sem erro |
| `it_should_retire_a_relation_using_the_relation_latest_record_id_as_base` | `base` = `RelationView.record_id`; depois `relations` sem `include_retired` não a lista |
| `it_should_copy_origins_and_native_catalog_from_the_latest_record_when_retiring_a_relation` | o record `Retirement` da relation tem `from_origin`, `to_origin` e `native_catalog` do latest record |
| `it_should_return_inbound_relations_when_querying_by_target` | `relations` com `to` entrega a leitura inversa (`before` de D33) |
| `it_should_reject_snapshot_on_a_relation` | linha S8 |
| `it_should_accept_external_id_on_a_relation_and_not_inherit_it_on_revision` | linhas X1 (relation) e X4 (relation) |

### C3. A verificação detecta qualquer record adulterado; um tenant não depende do outro

**Arquivo `sqlite_store` (T4, testes de unidade em `src/store/sqlite/`)**

| Teste | Afirma |
|---|---|
| `it_should_refuse_updates_and_deletes` | `UPDATE`/`DELETE` em `tenants`, `records` e `batches` abortam |
| `it_should_refuse_out_of_order_seq_and_a_wrong_prev_hash` | inserção direta com `seq` pulado e com `prev_hash` errado aborta |
| `it_should_refuse_a_second_root_for_the_same_lineage_at_the_index_level` | duas raízes da mesma lineage violam `records_root` |
| `it_should_refuse_a_second_successor_of_the_same_record_at_the_index_level` | dois sucessores do mesmo record violam `records_successor` |
| `it_should_initialize_an_empty_database_with_user_version_1` | banco novo recebe versão 1 |
| `it_should_refuse_a_database_with_a_newer_user_version` | `user_version = 2` dá `STORE_ERROR` `unsupported-schema-version` |
| `it_should_refuse_a_non_empty_database_with_user_version_zero` | banco estranho com tabelas e versão 0 é recusado |
| `it_should_keep_all_data_after_reopening_the_database` | reabrir devolve o mesmo `chain_head` |
| `it_should_create_tenants_idempotently` | `ensure_tenant` duas vezes: `created` verdadeiro e depois falso, mesma âncora |

**Arquivo `chain_tamper` (T5c; usa a cadeia de referência e a tabela de mutações da seção 4.5)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_verify_a_clean_chain` | cadeia de referência | `ok = true`, `total_records = 6`, `chain_head` igual ao de `chain_head()` |
| `it_should_detect_a_change_in_each_checked_field` | para cada linha da tabela de mutações | aplica o valor adulterado (1 linha alterada), `verify_chain` dá `ok = false`; `breaks[0]` tem o motivo e o `seq` da tabela |
| `it_should_have_a_mutation_for_every_checked_field` | `CHECKED_FIELDS` e a coluna `Id` da tabela | os `Id` são únicos e iguais a `CHECKED_FIELDS` mais `expected_chain_head`, sem sobra nem falta |
| `it_should_detect_a_deleted_middle_record` | apaga RC3 | `DivergingSeq` no record seguinte |
| `it_should_detect_an_inserted_forged_record` | insere record com hash forjado e `seq` válido no fim | quebra no record forjado |
| `it_should_not_detect_a_truncated_tail_without_expected_chain_head` | apaga o último record | `ok = true` (documenta o limite, H:242) |
| `it_should_detect_a_truncated_tail_with_expected_chain_head` | guarda o chain head antes; apaga o último record | `ChainHeadNotFound` |
| `it_should_detect_a_rewritten_and_rechained_tail_with_expected_chain_head` | reescreve a cauda recalculando hashes | `ChainHeadNotFound` |

**Arquivo `tenant_isolation` (T5c)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_keep_tenant_a_valid_when_tenant_b_is_tampered` | dois tenants; adultera B | A `ok`, B `ok = false` |
| `it_should_keep_tenant_a_valid_when_every_row_of_tenant_b_is_removed` | apaga todos os records de B | A `ok` |
| `it_should_derive_different_anchors_for_different_tenants` | A e B criados | âncoras diferentes |
| `it_should_give_different_hashes_to_identical_content_in_different_tenants` | mesmo conteúdo em A e B | hashes diferentes |
| `it_should_scope_semantic_ids_per_tenant` | mesmo id em A e B | duas lineages independentes |

**Arquivo `chain_properties` (T5c, `proptest`)**

| Teste | Afirma |
|---|---|
| `it_should_verify_after_any_sequence_of_valid_appends` | sequências aleatórias de lotes válidos: `verify_chain` sempre `ok` |
| `it_should_detect_the_mutation_of_any_row_of_the_table_after_extra_appends` | sorteia uma linha da tabela de mutações e de 0 a 5 nós extras anexados antes da cadeia de referência; `ok = false` e `breaks[0]` com o motivo da linha, sempre |

**Arquivo `concurrency` (T5d)**

| Teste | Afirma |
|---|---|
| `it_should_accept_only_one_of_two_concurrent_revisions_of_the_same_base` | duas instâncias, duas conexões, threads: um `Ok`, um `FORK_REJECTED` (H:571) |
| `it_should_write_once_when_the_same_key_races` | uma gravada, uma `replayed` |
| `it_should_keep_seq_contiguous_when_many_threads_append` | `seq` 1..N, `verify_chain` ok |
| `it_should_be_send_and_sync` | asserção em tempo de compilação sobre `Hexlog` |
| `it_should_serialize_calls_from_threads_sharing_one_instance` | uma `Arc<Hexlog>`, N threads, `seq` 1..N |
| `it_should_fail_with_chain_broken_on_write_when_the_tail_record_is_tampered` | política `Tail`, adultera o último record: `append` dá `CHAIN_BROKEN` e não grava |
| `it_should_not_notice_a_tampered_middle_record_on_write_under_the_tail_policy` | adultera o record 2 de 5: `append` grava e `verify_chain` reporta `HashMismatch` (documenta o desvio de H:243) |
| `it_should_fail_with_chain_broken_on_write_for_a_tampered_middle_record_under_the_full_policy` | mesma adulteração com política `Full`: `CHAIN_BROKEN` |
| `it_should_open_a_fresh_database_from_many_threads_without_error` | N threads, cada uma com sua `Hexlog::open` no mesmo caminho de banco novo: todas abrem sem erro; `sqlite_master` tem cada tabela e cada um dos sete triggers uma só vez; `user_version = 1` |

### C7. Um terceiro tipo de grafo não exige mudar o `engine`

**Arquivo `architecture` (T1; o varredor lê os arquivos `.rs` e `schema.sql`)**

| Teste | Afirma |
|---|---|
| `it_should_not_use_domain_vocabulary_in_identifiers_or_string_literals_of_the_hexlog_sources` | tokeniza todo `crates/hexlog/src/**/*.rs` (inclui `lib.rs`, `error.rs`, `facade.rs`, `ids.rs`); ignora comentários e `#[doc]`; divide identificadores por `snake_case` e `CamelCase` e literais por não alfanuméricos; e lê também `crates/hexlog/src/store/sqlite/schema.sql`: remove as palavras-chave SQL `CHECK` e `CONSTRAINT`, ignora comentários, e procura a lista nos identificadores restantes; compara **palavra inteira, sem distinção de caixa**, com a lista: decision, decisão, premise, premissa, guideline, directive, diretriz, check, gate, attestation, unit, work, prd, review, omc, omhl, hexflow, claude. A lista mantém `check`, e `directive`, `diretriz` e `unit` ficam nela como nomes antigos de guideline e de work item, que o motor também não pode usar; `work` entra porque, dividido por `snake_case`, `work_item` vira `work` e `item`, e `item` é legítimo (`AppendItem`), então só `work` pega o nome do work item (decisão da rodada 2 de linguagem, seção 12). Portagem de `test/no-flow-terms.spec.ts:8-10` (H:86) |
| `it_should_mention_git_only_in_engine_snapshot` | a palavra inteira `git` só aparece em `src/engine/snapshot.rs` (`digit` e `legitimate` não casam) |
| `it_should_flag_forbidden_words_in_identifiers_and_literals_and_ignore_them_in_comments_and_sql_keywords` | roda o varredor sobre `fixtures/scanner_cases.txt`: acusa a palavra num identificador e num literal Rust (inclusive um literal Rust com `CHECK (`, que não é DDL de `schema.sql`), acusa `gate_id` como coluna num trecho SQL, não acusa `CHECK (x IN ('a','b'))` num trecho SQL, não acusa em comentário Rust, `#[doc]` nem comentário SQL, acusa `work_item` (pela palavra `work`) e não acusa `digit`, `workspace` nem `network` (prova os dois lados da trava, como em H:85) |
| `it_should_not_depend_on_forbidden_crates` | `cargo metadata`: o fecho de dependências normais de `hexlog` não contém `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std` |
| `it_should_not_depend_on_omhl` | `omhl` não aparece no fecho de `hexlog` |
| `it_should_not_import_store_or_schema_from_the_engine_sources` | tokeniza `crates/hexlog/src/engine/**/*.rs` (ignora comentários) e acusa: o identificador minúsculo `store` ou `schema` quando está ao lado de `::` ou dentro de uma declaração `use`, inclusive dentro de grupo `{}` (variável local ou parâmetro com esse nome, fora de `::` e de `use`, não conta); e todo glob (`*`) em `use` fora de módulo `#[cfg(test)]`, qualquer que seja o prefixo (`crate`, `hexlog`, `super`, cadeia de `super`), porque `use super::*` e `use super::super::*` importam a raiz do crate sem nomear `store` nem `schema`. A pasta `engine/` ainda inexistente passa. Prova os dois lados com trechos embutidos: acusados `use crate::store::X;`, `use crate::{store::X};`, `use crate::{engine::a, schema};`, `use crate::*;`, `use super::*;` e `use super::super::*;`; aceitos `use crate::engine::ports::Store;`, `use super::model::AppendItem;`, `let store = 1;` e `use super::*;` dentro de `#[cfg(test)] mod tests` (o `Store` com maiúscula é o trait, não o módulo) |
| `it_should_declare_every_engine_child_module_with_restricted_visibility` | em `src/engine/mod.rs`, `ports`, `model`, `snapshot`, `limits` e `chain` têm de ser `pub(crate) mod`, e `decide`, `rules` e `verify` têm de ser `pub(in crate::engine) mod`; `pub mod`, `mod` sem visibilidade e qualquer outro filho declarado são acusados (filho novo entra na tabela do teste); em `src/lib.rs`, `engine`, `schema` e `store` são declarados sem `pub`; módulo ainda não declarado passa; prova os dois lados com trechos embutidos |
| `it_should_keep_integration_tests_on_the_public_api` | tokeniza `crates/hexlog/tests/**/*.rs` e acusa `#[path = ...]`, `include!` ou `include_str!` de arquivo sob `src/` e os caminhos `hexlog::engine`, `hexlog::schema` e `hexlog::store`; a conexão `rusqlite` crua de `support/tamper.rs` é permitida (abre o arquivo do banco, não o código); prova os dois lados com trechos embutidos |

**Arquivo `third_graph` (T6)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_accept_a_third_graph_defined_only_by_data` | `fixtures/third_graph.json` com três grafos de nomes neutros (dois com tipos e um relation type `acyclic`; o terceiro, `ticket`, com tipo `requires_snapshot` e relation type novo), carregados num `Catalog`, e só a API pública da Facade | grava, consulta, liga por relation, revisa, retira e `verify_chain` dá `ok`; o arquivo do teste não importa nada fora de `hexlog::` |
| `it_should_let_the_adapter_enforce_a_cross_record_invariant_the_engine_does_not_know` | tipo `ticket` com `status` `open`/`closed`; função `transition_allowed` definida dentro do teste, no papel do adaptador | o motor aceita a revisão `closed` → `open`; a função do teste a recusa antes de chamar a Facade |

Observações. (1) A prova de que um grafo novo não toca o `engine` é por construção (o teste só usa a API pública e um arquivo de dados) mais as travas de arquitetura; não há como provar "nunca precisou mudar" além disso. A trava de vocabulário não veta os termos estruturais do item 19; veta termos de domínio. (2) O teste do invariante no adaptador usa uma função local ao teste: ele **documenta o padrão** (o motor não conhece a regra; quem a conhece valida antes) e não exercita código do adaptador real, que só nasce no crate `omhl` do passo 4. (3) A escolha de colocar o DDL em `schema.sql` em vez de tirar `check` da lista é do item "Mudanças da rodada 3" (seção 9); o varredor também lê esse arquivo, conforme o item M1 da seção 10.

### Comandos de verificação

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo tree -p hexlog -e normal` (T1: sem crate da lista proibida)
- Por critério: `cargo test -p hexlog --test node_lifecycle --test semantic_id --test snapshot --test catalog --test snapshot_rules --test list_filters --test batch --test idempotency --test relations` (C1 e regras de C2 do motor), mais `cargo test -p hexlog --lib it_should_expose_requires_snapshot_def_hash_and_acyclic_through_the_definitions_port` (teste de unidade da T3, que o `--test catalog` não executa), `cargo test -p hexlog --lib --test chain_tamper --test chain_properties --test tenant_isolation --test jcs_vectors --test concurrency` (C3; `--lib` roda `sqlite_store` e também o teste de unidade da T3), `cargo test -p hexlog --test third_graph --test architecture` (C7).

## 6. Riscos e mitigações

| # | Risco | Mitigação |
|---|---|---|
| R1 | O JCS do crate escolhido diverge do RFC (número, escape de Unicode, ordem de chaves) e o hash fica incompatível para sempre. | Spike da T1 contra os vetores do RFC; só inteiros seguros em `data` (recusa `1.0` e `1e2`); hashes dourados calculados pelo Hexlog TS e comparados no teste (nenhum valor nasce do Rust); fallback E2-C. |
| R2 | Travar um formato de record ruim (tudo que entra no corpo fica). | Campos opcionais omitidos (acrescentar não quebra); `v: 1` e `format` na âncora; sem dado real até o passo 4; **o formato é congelado no P0, antes da T2**, com os itens 1, 5, 10, 11, 16 e 19 respondidos. Mudar depois do P0 é tenant novo (D12: sem migração). |
| R3 | Coluna derivada diverge do `body` e a consulta devolve algo que o hash não cobre. | Payload sempre lido de `body`; colunas só filtram; `verify_chain` confere coluna contra corpo (`CHECKED_FIELDS`); tabela de mutações com um teste por campo, proptest sobre a mesma tabela e conferência de que cada adulteração alterou 1 linha. |
| R4 | Concorrência e desempenho: `busy`, WAL em sistema de arquivos errado, `Tail` esconde adulteração no meio. | `BEGIN IMMEDIATE` + `busy_timeout` + erro `BUSY` repetível; testes com conexões e threads separadas, repetidos 20 vezes na T5d; banco fora de `/mnt` no WSL; `VerifyPolicy::Full` nos testes; o desvio de H:243 é [DONO] (E6); medir 5.000 records (referência de H:556) e reavaliar E6. |
| R5 | O motor perde a neutralidade aos poucos (palavra de domínio, crate de git, import de `store` ou `schema`). | `architecture.rs` na T1, antes de qualquer regra; varredor testado dos dois lados; lista de crates proibidos; teste de imports do `engine`, de visibilidade dos filhos e de API pública dos testes (item 22 da seção 2). |
| R6 | O `Snapshot` é confiado ao chamador: a árvore pode estar suja ou o SHA ser de outro repositório. | Dito no ADR e no item 1; a sonda e o teste de árvore suja ficam no passo 5, no adaptador. |
| R7 | Crescimento de escopo (FTS5, projeção, gate, nativos). | Lista "não entra" da seção 4.1; revisão de entrega confere os arquivos contra o layout da seção 4.2. |
| R8 | Decisões do dono ainda abertas mudam o corpo do record (`Retirement`, origem nativa em relation, `alias`, tabela de `snapshot`). | Portão P0 antes de qualquer código que grave o formato; depois dele o corpo só cresce por campo opcional. |
| R9 | Versões e comportamento das crates não verificados por este plano. | Spikes da T1; `Cargo.lock` versionado; `rust-toolchain.toml` fixo. |
| R10 | Mensagem de erro que vaze conteúdo de `data` ou caminho do banco. | `details` só com ponteiro, código e mensagem fixa; teste `it_should_not_echo_data_values_in_error_messages`. |
| R11 | Skew de versão entre processos: CLI, MCP e hook rodando com `Catalog` ou `NativeNodes` diferentes (binário velho depois de uma atualização do plugin) gravam sob regras antigas ou reservam ids diferentes. | O `def_hash` por record e o `native_catalog` das relations deixam a divergência auditável; `user_version` impede binário velho de abrir banco de esquema novo; o replay antes das regras estáticas evita falsos conflitos. Resíduo aceito até o passo 5, quando o `omhl` compara a versão do plugin com a do binário e recusa gravar em desacordo. |
| R12 | Varredor de vocabulário com falso positivo ou falso negativo. | Tokenização por `proc-macro2` (ignora comentários), palavra inteira sem caixa, teste do próprio varredor com fixture; DDL fora dos `.rs` (`schema.sql` com `include_str!`), que o varredor também lê, sem as palavras-chave SQL `CHECK` e `CONSTRAINT`, de modo que `check` continua vetado como identificador de armazenamento; se `proc-macro2` não servir, escrever um lexer mínimo (ignorar `//`, `/* */`, `///`). |

## 7. ADR

**Decisão.** Implementar o passo 3 como crate `hexlog` (módulos `engine`, `schema`, `store`) mais um `omhl` mínimo num workspace Cargo; SQLite via `rusqlite` com `bundled` e `Mutex<Connection>` dentro de `Hexlog` (`Send + Sync`); records com corpo JCS hasheado por sha256 e encadeados por tenant com `seq` contíguo; revisão por `base` explícito com índices únicos parciais; `Retirement` como variante do corpo; definições de node type e relation type injetadas por um `Catalog`, vistas pelo `engine` só pela porta `Definitions` e fixadas por `def_hash` em cada record; `engine` sem importar `store` nem `schema` (três testes de `architecture`); conteúdo native reconhecido por uma porta `NativeNodes`; snapshot tipado fechado (`GitCommit`) validado só por formato; escrita como `decide` puro dentro de uma transação exclusiva, com replay idempotente logo depois da forma do lote e do tenant; verificação de cauda na escrita e completa sob demanda; DDL em `schema.sql` embutido por `include_str!`; contratos de armazenamento no `engine/ports.rs`; formato do record congelado no portão P0, antes da T2; ordem das tarefas T1, T2, T3/T4, T5a, T5b, T5c (verificação), T5d (concorrência), T6.

**Drivers.** Integridade verificável sem arrependimento de formato; neutralidade do motor demonstrável por teste; execução por agentes sonnet/haiku com verificação mecânica.

**Alternativas consideradas.** `sqlx`, `diesel`/`sea-orm`, SQLite do sistema (E1); `serde_jcs` e canonicalizador próprio (E2); quatro crates ou um só (E3); revisão implícita ou `supersedes` explícito (E4); definições como records ou manifesto fixado por tenant (E5); verificação completa em toda leitura (E6); `engine` importar `schema` para ler o `Catalog` direto (invalidada pela regra do dono de que `engine` não importa `store` nem `schema`).

**Por que esta.** É a combinação que preserva as regras do Hexlog que o mestre manda portar (cadeia, lote, `key`, um vigente por lineage), cumpre D1 e D11 sem criar crates extras, deixa C7 testável por dado, e torna a bifurcação fisicamente impossível em vez de só verificada. O `Catalog` com `def_hash` evita o processo imutável do Hexlog (H:642) sem inventar bootstrap circular. Congelar o formato antes da T2 custa uma espera do dono e evita construir cinco tarefas sobre um corpo que ainda pode mudar.

**Consequências.**

- O núcleo confia no `Snapshot` do chamador; a regra de D31 de que snapshot com alterações não commitadas não é atestado é do adaptador. Isso é uma releitura de D31, marcada [DONO].
- Sem classificação de mudança compatível/quebra de definição (o Hexlog tinha, H:289-293); o histórico é provado pelo `def_hash`, não impedido.
- **Invariantes entre records ficam no adaptador na v1** (transição do status do work item de aberto para fechado, a regra "uma attestation exige snapshot" além da flag `requires_snapshot`, regras de `supports`/`contradicts`). O motor valida cada record contra o schema do tipo e cada relation contra a `RelationTypeDef`; nada mais. O `third_graph` mostra o padrão com um caso.
- `data` aceita só inteiros seguros; o campo de nota fracionária precisa de outro desenho (texto ou milésimos).
- `expected_chain_head` é necessário para detectar cauda apagada; quem quiser essa garantia precisa guardar o chain head fora do banco.
- A política `Tail` é um desvio de H:243 ("falha fechado em toda leitura"): adulteração no meio só aparece no `verify_chain`. [DONO].
- O vocabulário estrutural do corpo do record (`Retirement`, `requires_snapshot`, `Snapshot`, origens, `native_catalog`) é mais que o mínimo de D1. [DONO].
- O `engine` fala com as definições pela porta `Definitions` e com o armazenamento pelas portas de `engine/ports.rs`: uma indireção a mais, paga para impor a regra de imports por teste.
- `TxRead` fica com o nome e o tamanho atuais até o dono decidir o item 21 da seção 2 ([DONO]).
- O native não participa da cadeia; relations para nativos podem ficar penduradas após atualização do plugin (item 2 da seção 2).
- Formato do record congelado no P0; mudá-lo depois é tenant novo.

**Próximos passos.**

- O dono responde o P0 (itens 1, 5, 10, 11, 16, 19 e P7) para liberar a T2; e, antes dos passos 4 e 6, os itens 3, 6, 7, 8 e 12 a 14, além do desvio de E6, do item 21 (nome e divisão de `TxRead`) e do item 23 (nome do módulo `schema`); nenhum dos dois últimos bloqueia o P0.
- Passo 4: grafos de decisões, de checks e de work item, conteúdo native e `NativeNodes` real, avaliação das quatro perguntas de gate sobre `relations` e `list`, validações de transição no adaptador, medição do texto das instruções contra o teto de `data` (item 17).
- Passo 5: `SnapshotProbe` com `git`, `.omhl.toml`, `render`, adapters e a comparação de versão plugin/binário (R11).
- Passos 5 e 6: carregador das extensions (arquivo para banco, D34) e projeção sob demanda (formato e comando em aberto, item 6).
- Reavaliar E6 e E2 com os números da T1 e da T6.

## 8. Mudanças da rodada 2

Nota de linguagem: as seções 8, 9 e 10 são histórico e usam os nomes anteriores à rodada de linguagem (elo, state, edge, tip, head, unit, approval, ForeignNodes e afins); não foram reescritas. O mapa dos nomes novos está na seção 11.

Nota: os nomes de tarefa desta seção são os da rodada 2 (T5a nós, T5b arestas, T5c concorrência). A numeração atual está em §4.6 e a correspondência na seção 9.

Fonte: [round1-reviews.md](../drafts/round1-reviews.md) e a mensagem do lead. Nenhum achado foi recusado; dois foram aceitos com ressalva (marcados). Os itens de mesma natureza dos dois revisores estão juntos.

**Architect**

- A1 (IMPORTANTE, tipos faltantes): aceito. §4.4 define `EdgeView` (com `record_id` da ponta e `EndpointState` por extremo), `RevisionView`, `Page`, `PageRequest`, `TenantInfo`, `TipRow`, `BatchRow`, `TenantRow`, `LinkRow`, `LinkColumns`, `NewLink`, `ChainBreak`, `BreakReason`, `VerifyPolicy`, e `Decision` virou `AppendPlan`.
- A2 (IMPORTANTE, `jsonschema` arrasta rede): aceito com ressalva. §4.2 fixa `default-features = false`, troca o veto transitivo por lista de crates proibidos, e T1(c) confere com `cargo tree`; a ressalva é que, se a crate não funcionar sem a feature, a T1 manda trocar de validador em vez de afrouxar a lista.
- A3 (IMPORTANTE, varredura de comentários): aceito. §5 C7 varre só identificadores e literais, ignora comentários e `#[doc]`, palavra inteira sem caixa, `git` restrito a `state.rs`, mais teste do varredor.
- A4 (IMPORTANTE, `Send + Sync`): aceito. §4.4 e §1 (E1) definem `Mutex<Connection>`; T5a, T5c e §5 trazem os testes `it_should_be_send_and_sync` e `it_should_serialize_calls_from_threads_sharing_one_instance`.
- A5 (IMPORTANTE, replay depois das estáticas): aceito. §4.5 move o replay para o passo 3, logo depois de forma e tenant, com reconsulta dentro da transação; teste `it_should_replay_before_static_rules_when_the_id_was_reserved_afterwards`.
- A6 (MENOR, `Retire` e `def_hash`, ciclo): aceito. §4.3 faz o `Retire` copiar `type` e `def_hash` da ponta; §4.5 exclui aresta com extremo aposentado do ciclo; testes `it_should_copy_type_and_def_hash_from_the_tip_when_retiring` e `it_should_ignore_edges_with_a_retired_endpoint_when_detecting_cycles`.
- A7 (MENOR, `anchor-mismatch`, `anchor_doc`, `user_version`): aceito. §5 C3 e `sqlite_store` trazem um teste de cada; §4.3 define a recusa de `user_version` maior ou inválido com detalhe `unsupported-schema-version`.
- A8 (MENOR, `1.0` e `1e2`): aceito. §4.5 "Números em `data`" recusa qualquer literal que não seja inteiro seguro; teste `it_should_refuse_numbers_that_are_not_safe_integers_in_data`.
- Ponto de atenção D31: aceito. Item 1 diz que é releitura de D31 e continua [DONO]; ADR repete.
- Ponto de atenção D1: aceito. Novo item 19 [DONO]; princípio 1 e ADR citam.
- Síntese (antecipar o congelamento, invariantes no adaptador, caso no `third_graph`): aceito. Portão P0 antes da T2 (§4.6), ADR, item 20 e teste `it_should_let_the_adapter_enforce_a_cross_record_invariant_the_engine_does_not_know`.

**Critic**

- C1 (travas de C7 contraditórias): aceito. `AppendPlan`, varredura por identificadores e literais, palavra inteira sem caixa, `error.rs` e `lib.rs` cobertos (varre todo `src/`), lista de crates proibidos. A fórmula `omhl-chain/1` na âncora virou `hexlog-chain/1` para não violar a própria trava.
- C2 (comportamento sem teste nem tipo): aceito. `EndpointState` e `shadowed` definidos (§4.4); testes para `CHAIN_BROKEN` com `Tail`, `where_eq` (nome validado, parâmetro ligado, valor hostil), `Page`/`PageRequest` (cursor por `seq`), `anchor-mismatch` (três variantes), `invalid-body`, `head-not-found`; `CHECKED_FIELDS` listado em §4.5 e o proptest sorteia só dele.
- C3 (regras de `state` ambíguas): aceito. Tabela S1 a S9 e X1 a X3 em §4.5, um teste por linha em `state_rules`, confirmada no P0 (item 11).
- C4 (dividir T5): aceito. T5a, T5b, T5c em §4.6.
- C5 (hash dourado gerado pelo Rust): aceito. Fixture `golden_links.json` com hashes calculados pelo Hexlog TS; teste Rust compara (T2).
- C6 (reticências nos nomes): aceito. Todos os testes têm nome completo em §5.
- C7 (replay depois das estáticas): aceito, é A5.
- C8 (skew de versão, `Mutex` ou pool): aceito. R11 novo; `Mutex<Connection>` em §4.4.
- Alternativas e riscos (E6 sem [DONO]): aceito. E6, ADR e R4 marcam o desvio de H:243 como [DONO].

**Lead**

- Item 3a (travas de C7): aceito, ver C1 e A2/A3.
- Item 3b (tipos de §4.4, `EdgeView`, `shadowed`): aceito, ver A1.
- Item 3c (replay logo após forma e tenant): aceito, ver A5.
- Item 3d (`Send + Sync` explícito): aceito, ver A4.
- Item 4 (tabela de `state`, testes por comportamento, `CHECKED_FIELDS`, literais numéricos, `Retire`/ciclo, T5 dividida, fixture dourada, nomes, skew): aceito, ver C2, C3, C4, C5, C6, C8, A6, A7, A8.
- Item 5 (portão antes da T2 e ADR sobre invariantes no adaptador): aceito, ver síntese do Architect.
- Item 6 ([DONO] para `Tail`, releitura de D31 e vocabulário estrutural): aceito. E6/R4/ADR; item 1; item 19. Nenhum desses foi decidido neste plano.

## 9. Mudanças da rodada 3

Nota de linguagem: esta seção é histórico e usa os nomes anteriores à rodada de linguagem; ver o mapa na seção 11.

Nota: onde esta seção diz que o varredor lê só arquivos `.rs`, vale o item M1 da seção 10: o varredor passou a ler também `schema.sql`, sem as palavras-chave SQL.

Fonte: [round2-reviews.md](../drafts/round2-reviews.md) e a mensagem do lead. Nenhum achado foi recusado; um foi aceito com ajuste (marcado). A numeração de tarefas mudou: a antiga T5b (arestas) é a T5b, a antiga T5c (concorrência) é a T5d, e a T5c passou a ser a verificação da cadeia.

**Solução única para a trava de vocabulário contra o DDL (Architect N1, Critic B).** Escolhi o DDL em `src/store/sqlite/schema.sql`, embutido por `include_str!` (sugestão do Critic), e **mantive `check` na lista** (contra a sugestão do Architect de tirá-lo). Motivos: `check` é um dos dois nomes de grafo de D3, então é exatamente o tipo de palavra que a trava existe para pegar; o varredor lê identificadores e literais de arquivos `.rs`, e SQL é dialeto de armazenamento, não vocabulário de domínio nem código Rust, então um arquivo `.sql` fora do alcance não enfraquece a trava; e reescrever `CHECK` como `CONSTRAINT` só contornaria o varredor. Aplicado em §4.2 (layout), §4.3 (parágrafo antes do DDL), §5 C7 (lista mantida, teste do varredor e `scanner_cases.txt` com caso de literal Rust contendo `CHECK (` e caso do `include_str!`), T1, T4 e R12.

**Architect**

- N1 (IMPORTANTE, DDL contra a trava): aceito com a solução acima.
- N2 (IMPORTANTE, `where_eq` lê o envelope): aceito. §4.5 passa a `$.data."<nome>"`; teste `it_should_match_a_where_field_only_inside_data_even_when_its_name_equals_an_envelope_field` em `list_filters` (T5a).
- N3 (MENOR, `Retire` de aresta): aceito. §4.3 faz o `Retire` de aresta copiar `from_origin`, `to_origin` e `native_catalog`; teste `it_should_copy_origins_and_native_catalog_from_the_tip_when_retiring_an_edge` (T5b).
- N4 (MENOR, `Store::read` genérico): aceito. §4.4: `read` não é genérico (o resultado sai por variável capturada), `Store` é objeto de trait válido, e o texto diz que `Hexlog` guarda o `SqliteStore` concreto.
- N5 (MENOR, teste do invariante no adaptador): aceito. Observação (2) de §5 C7 diz que o teste usa função local e só documenta o padrão.
- Antítese e tensão (P0 trava a execução): aceito como registro. §4.6 diz que o P0 trava a T2 e tudo depois, que é escolha do plano e que a T1 roda antes; o plano não assume respostas do dono.

**Critic**

- A (MAIOR, ordem das tarefas): aceito. `verify.rs`, `CHECKED_FIELDS`, `verify_chain` e a política `Full` viram a T5c, antes da concorrência (T5d). O grafo de dependências está em §4.6 e cada "Pronto" lista só testes cujos pré-requisitos já existem: T5a só `Tail`; testes de `Full`, `verify_chain` e `tamper` ficam na T5d; os testes que precisam de aresta ficam na T5b; os de `catalog`, do guarda de números, passaram da T5a para a T3 (onde `validate_data` nasce); os de unidade do store ficaram na T4 como testes dentro do crate.
- B (MAIOR, DDL contra a trava): aceito, ver a solução única acima.
- C (MAIOR, tabela de mutação e motivos): aceito. §4.5 traz a tabela única campo, alvo, valor adulterado, motivo e `seq` reportado; `prev_hash`, `hash` e `body` alterado dão `HashMismatch` (um motivo só); o `tamper` remove triggers e desliga `foreign_keys` e `ignore_check_constraints`, e cada valor respeita `UNIQUE` e chave primária; `CHECKED_FIELDS` tem os mesmos nomes da tabela (teste `it_should_have_a_mutation_for_every_checked_field`); o proptest sorteia linhas dessa tabela.
- 3 (MENOR, código do erro de extremo ausente): aceito. `INVALID_RECORD` `endpoint-not-found` em §4.5 (passo 6 e catálogo de erros) e em §5; `RELATION_NOT_FOUND` fica para nome de relação ausente do catálogo, com teste próprio.
- 4 (MENOR, circularidade e `TypeName`): aceito com ajuste. `AppendOutcome`, `AppendedRecord` e `ChainHead` estão em `engine/model.rs`, e os contratos de armazenamento (`Store`, `TxRead`, linhas e `AppendPlan`) foram para `engine/ports.rs`, de modo que `engine` nunca importa `store`. O ajuste: `TipRow` e `LinkColumns` usam `Name`, não `TypeName`, porque a mesma coluna guarda tipo de nó ou nome de relação; os demais campos tipados foram tipados (`RelationName`, `SemanticId`, `ExternalId`).
- 5 (MENOR, linha X4): aceito. Linha X4 em §4.5, teste `it_should_not_inherit_external_id_when_a_revision_omits_it` (nó, T5a) e o de aresta em `edges`; item 11 da §2, o P0 e a lista do portão citam X1 a X4.

**Lead (itens 3 a 6)**

- Item 3 (uma solução para o DDL, aplicada em todos os pontos): aceito, ver acima.
- Item 4: `where_eq` com `$.data.`, ordem das tarefas, tabela de mutações com `tamper`, `Retire` de aresta, `Store::read`, nota do invariante, `endpoint-not-found`, `AppendOutcome` e tipos, linha X4: aceitos, ver as linhas correspondentes.
- Item 5 (conferência de ponta a ponta de dependências e "Pronto"): feita. Verifiquei, tarefa a tarefa, que cada teste de §5 só usa funções, arquivos de suporte e fixtures criados na mesma tarefa ou antes: `tests/support/mod.rs` na T2, `catalog.rs` na T3, `tamper.rs` na T4, `mutations.rs` na T5c; as fixtures `rfc8785_vectors` e `golden_links` na T2, `scanner_cases` na T1, `third_graph` na T6; o ramo `edge-items-not-supported` nasce na T5a e a T5b e a T6 conferem que sumiu.
- Item 6 ([DONO]): mantido. Nenhum item que depende do dono foi resolvido: política `Tail` (E6), releitura de D31 (item 1), vocabulário estrutural (item 19), itens 5, 10, 11, 16 e P7 seguem [DONO].

## 10. Ajustes finais do consenso

Nota de linguagem: esta seção é histórico e usa os nomes anteriores à rodada de linguagem; ver o mapa na seção 11. A conferência final abaixo conta 5 testes de `architecture`; com a rodada de linguagem são 8 (seção 11).

Fonte: a mensagem do lead com o APPROVE do Critic na rodada 3 e os achados do Architect e do Critic. Estes ajustes são de texto e não abrem nova rodada de revisão. A versão anterior está em [omhl-v2-core-step3.r3.md](omhl-v2-core-step3.r3.md). Nenhum item foi recusado; nenhum item [DONO] foi resolvido.

**Architect**

- M0 (abertura concorrente de banco novo): aceito. §4.3 ganhou a sequência fixa de abertura (`busy_timeout` antes de qualquer PRAGMA; vazio, `schema.sql` e `user_version = 1` dentro de um único `BEGIN IMMEDIATE` que confere o vazio depois de obter a transação); T4 (Entrega) a implementa; teste `it_should_open_a_fresh_database_from_many_threads_without_error` na T5d (§5, `concurrency`), e o "Pronto" da T5d passou de 8 para 9 testes.
- M1 (varredor lê `schema.sql`): aceito. §4.3 (parágrafo antes do DDL), §5 C7 (teste de vocabulário e teste do varredor, agora `..._ignore_them_in_comments_and_sql_keywords`, com casos SQL em `scanner_cases.txt`), T1, T4 e R12 dizem que o varredor remove `CHECK` e `CONSTRAINT` e procura a lista nos identificadores restantes. A nota da seção 9 aponta para este item.
- M2 (SQL dos triggers): aceito. §4.3 traz o SQL dos seis triggers de `UPDATE`/`DELETE` e do `links_chain_guard`, que são os sete que o `tamper` remove (§4.5, T4).
- M3 (`#![allow(dead_code)]`): aceito. Registrado no layout de §4.2 e na T2 (`tests/support/mod.rs`).

**Critic**

- 1 (identificador único por linha da tabela de mutação): aceito. §4.5 passa a usar a coluna `Id` (`body_content`, `body_not_canonical`, `body_not_json`, `body_tenant`, `body_seq`, `tenants_anchor`, e assim por diante), `CHECKED_FIELDS` são exatamente esses `Id` menos `expected_head` (33 identificadores contra 34 linhas), e o teste `it_should_have_a_mutation_for_every_checked_field` confere unicidade e igualdade.
- 2 (ordem das quebras): aceito. §4.5 fixa `breaks`: tenant, depois lote, depois elos por `seq` armazenado, depois `HeadNotFound`; os testes afirmam sobre `breaks[0]`; o `prev_hash` do L1 é comparado com `sha256(anchor_doc)` recalculado, o que faz `tenants_anchor` produzir uma só quebra; a linha `hash` do L3 anota que o L4 quebra depois e a linha `tenants_anchor_doc` anota que o L1 quebra depois. Os testes de §5 passaram a afirmar `breaks[0]`.
- 3 (`Deserialize`): aceito. T3 (Arquivos e Entrega) acrescenta `Serialize`/`Deserialize` de `TypeDef` e `RelationDef` e o `Deserialize` validante de `TypeName` e `RelationName`; dois testes novos em `catalog`, e o "Pronto" da T3 passou de 8 para 10 testes.
- 4 (`mod.rs` compartilhado por T3 e T4): aceito. A T2 declara `catalog`, `tamper` e `mutations` e cria os três arquivos vazios; T3, T4 e T5c os preenchem sem editar o `mod.rs`.

**Conferência final.** Contagens dos "Pronto" depois dos ajustes: T1, 5 testes de `architecture`; T2, 2 de `semantic_id`, 2 de `state` e 3 de `jcs_vectors`; T3, 10 de `catalog`; T4, 9 de `sqlite_store`; T5d, 9 de `concurrency`; T6, 2 de `third_graph` e 5 de `architecture`. A ordem T1; T2; T3 e T4; T5a; T5b; T5c; T5d; T6 e o grafo de dependências de §4.6 não mudaram.

## 11. Rodada de linguagem

Fonte: [ralplan-brief-language.md](../drafts/ralplan-brief-language.md), o [GLOSSARY.md](../../GLOSSARY.md) e as decisões 34 e 35 do mestre. A versão anterior está em [omhl-v2-core-step3.r4.md](omhl-v2-core-step3.r4.md). Nenhum item [DONO] nem o P0 foi resolvido; a ordem T1, T2, T3/T4, T5a, T5b, T5c, T5d, T6, a tabela de mutações (34 linhas, 33 `CHECKED_FIELDS`), as travas de C7 e as contagens dos "Pronto" foram preservadas, exceto a de `architecture`.

**Renomeios aplicados (seções 1 a 7)**

- Record (antes elo, registro, link): `LinkBody`, `LinkRow`, `LinkColumns`, `NewLink` viram `RecordBody`, `RecordRow`, `RecordColumns`, `NewRecord`; `AppendPlan.links` vira `records`; a tabela `links` vira `records`, o índice `links_state` vira `records_snapshot` e os demais `links_*` viram `records_*`; `hash_link` vira `hash_record`; `golden_links.json` vira `golden_records.json`; "elo da cadeia" vira record. Os records da cadeia de referência da tabela de mutações, antes L1 a L6, passam a RC1 a RC6 (RC de reference chain), para não repetir o nome antigo nem colidir com R1 a R12 (riscos).
- Snapshot (antes state, estado): `State`, `StateKind`, `requires_state` viram `Snapshot`, `SnapshotKind`, `requires_snapshot`; colunas `state_kind` e `state_id` viram `snapshot_kind` e `snapshot_id`; os erros `state-required`, `state-not-allowed`, `state-format` viram `snapshot-required`, `snapshot-not-allowed`, `snapshot-format`; `engine/state.rs`, `snapshot_rules` e `tests/state.rs` viram `snapshot.rs`, `snapshot_rules.rs` e `tests/snapshot.rs`; as linhas S1 a S9 e X1 a X4 mantêm os ids. `EndpointState`, `from_state` e `to_state` não são snapshot (são o status do extremo de uma relation) e viram `EndpointStatus`, `from_status`, `to_status`. Em português, "estado" ficou só onde significa outra coisa, e o item "Dependentes de estado" da ordem de recusa virou "Dependentes do histórico gravado".
- Relation (antes edge, aresta): `EdgeView`, `EdgeQuery`, `edges()`, `ItemBody::Edge` e `tests/edges.rs` viram `RelationView`, `RelationQuery`, `relations()`, `ItemBody::Relation` e `tests/relations.rs`. O tipo da relation passa a relation type: `RelationDef` vira `RelationTypeDef`, `RelationName` vira `RelationTypeName`, o campo `rel` vira `relation_type` (coluna, corpo, `RelationQuery`) e `RELATION_NOT_FOUND` vira `RELATION_TYPE_NOT_FOUND`.
- Latest record (antes tip, ponta): `TipRow` vira `LatestRow`, `tip()` vira `latest_record()`, `edge_tips` vira `latest_relations`. Current revision aparece só para a revision que vale hoje.
- Chain head (antes head, cabeça): `head()` vira `chain_head()`, `expected_head` vira `expected_chain_head`, `HeadNotFound` vira `ChainHeadNotFound`, `tenant_head` saiu do plano (item 6); `ChainHead` ficou.
- Lineage (antes subject, linhagem, sujeito): `Subject` vira `Lineage`, `SubjectKind` vira `LineageKind`, colunas `subject_kind` e `subject_key` viram `lineage_kind` e `lineage_key`, o campo `subject` do corpo vira `lineage`; o valor `'edge'` do `lineage_kind` vira `'relation'`.
- Work item (antes unit): `graphs/units` vira `graphs/work_items`, o tipo `unit` vira `work_item`, e o estado aberto e fechado vira o status do work item. `unit` fica só na lista de vetos do varredor e nas notas de tradução, como nome antigo.
- Attestation (antes approval, aprovação): o `approval` vira o node type `attestation` do graph de checks (decisão do dono na rodada 1 de linguagem; a primeira versão deste plano o tratava como record), com desfecho de 3 valores (passou, passou com alerta, falhou) no item 12; "aprovado" do gate virou "passou" no item 13.
- Retirement (antes Retire, aposentadoria): `ItemBody::Retire` vira `ItemBody::Retirement`, o campo `retire` do corpo vira `retirement`; o verbo "aposentar" virou "retirar".
- `ForeignNodes`, `ForeignNode`, `native_edges` viram `NativeNodes`, `NativeNode`, `native_relations`; `Deps.foreign` vira `Deps.native`. "Hospedeiro" virou "o `omhl`" (E5); "host" não aparece neste plano, porque o plano não fala do ambiente do agente.
- Os exemplos de id com `gates.` (D6) viram `checks.` (item 15).

**Coerência com as decisões 34 e 35**

- D34: o item 6 (banco vazio em máquina nova) foi reescrito: um clone novo recarrega o banco a partir das extensions, e a proposta de marcador `tenant_head` com regeneração que recusa sobrescrever saiu, porque a regeneração por `post-merge` (D26, D27) deixou de valer para os YAML. O item 7 passou a tratar do commit de extensions contra a attestation amarrada ao snapshot. P1 perdeu a chave `[projection] regenerate`; P2 passou a descrever extensions escritas à mão (`checks/` e `guidelines/`, sem pasta de gates, texto longo em Markdown à mão); o item 17 registra que a D34 tirou a D25 das extensions. O passo 3 continua sem carregar extensions nem gerar projeções.
- D35: o glossary prevalece; o cabeçalho avisa que o mestre ainda usa os nomes antigos nas decisões 1 a 34, e a contagem de decisões citadas passou de 33 para 35.

**Mudanças além dos renomeios**

- Imports: o `engine` não importa `store` nem `schema`; os filhos de `engine` são `pub(crate)` ou `pub(in crate::engine)`; os testes de `tests/` usam só a API pública. Três testes novos em `architecture` (agora 8, antes 5): imports do `engine`, visibilidade dos filhos e API pública dos testes; T1 "Pronto" e T6 "Pronto" passam de 5 para 8; os demais "Pronto" não mudam (T2: 2, 2 e 3; T3: 10; T4: 9; T5d: 9; T6: 2 de `third_graph`).
- Consequência técnica do teste de imports, sinalizada ao Architect: o `decide` não pode importar o `Catalog`, então o `engine` ganhou a porta `Definitions` (com `NodeTypeInfo`, `RelationTypeInfo` e `DataViolation`) em `engine/ports.rs`, implementada pelo `Catalog` em `schema/catalog.rs` (item 22 da seção 2). Isso não muda o formato do record nem o P0.
- `TxRead`: não foi dividido nem renomeado. A seção 4.4 o define em linguagem simples e o item 21 da seção 2 propõe, marcado [DONO], o nome `StoreReader` e a divisão em `WriteReads`, `QueryReads` e `VerifyReads`.
- Seções 8, 9 e 10 mantidas como histórico, com a nota de nomes antigos. As citações `H:n` foram conferidas contra `hexlog-current-model.md` (754 linhas, maior citação 661, arquivo sem alteração desde o plano) e continuam válidas; o Hexlog TS usa os nomes antigos nas fontes que o relatório descreve, e isso fica como está.

**Nomes antigos que ainda aparecem neste plano, e por quê**

- Nas seções 8, 9 e 10 e nas notas de tradução (cabeçalho, itens 1, 12, 13, 15 e esta seção): histórico, citações de decisões do mestre (D30, D31 e D32 dizem "estado", "aprovado") e a explicação da mudança.
- `approved` e `no_pending` (item 8): nomes de perguntas do Hexlog TS; o nome das perguntas no `omhl` é do passo 4.
- `supersedes` e `revokes` (E4-C, itens 9 e 10): relation types do Hexlog TS, descritos como alternativa e como análogo, não como nome do `omhl`.
- `HEAD` (item 1) é o `HEAD` do git, que o glossary distingue do chain head. `FOREIGN KEY` é palavra-chave SQL.
- `pending approval` no Status é o rótulo de plano exigido pelo fluxo, não uma attestation.

**Sem resposta (ver também os itens [DONO] da seção 2)**

- O glossary não traz o termo em português de node, graph, revision, batch e chain, e o brief não os lista como renomeio: o plano manteve "nó", "grafo", "revisão", "lote" e "cadeia" na prosa e usou os termos em inglês nos identificadores. Se o dono quiser a prosa toda em inglês para esses termos, é uma passada mecânica.
- Se o tipo do PR será `work_item` ou uma especialização por escopo, como o glossary descreve (P6).
- O que fazer com o conteúdo que só vive no banco quando a máquina é nova (item 6).
- Se `ItemBody` e `AppendItem` confundem com work item: nenhum revisor apontou até aqui; a nota fica registrada.

## 12. Mudanças da rodada 2 de linguagem

Fonte: [round1-language-reviews.md](../drafts/round1-language-reviews.md) (Architect: 3 IMPORTANTE e 3 MENOR, sem BLOQUEANTE; Critic: ITERATE com 2 IMPORTANTE e 8 menores) e a decisão do dono sobre attestation, tomada depois das revisões. A versão anterior está em [omhl-v2-core-step3.r5.md](omhl-v2-core-step3.r5.md). Nenhum item [DONO] nem o P0 foi resolvido. As contagens dos "Pronto" não mudaram (T1 e T6: 8 testes de `architecture`; T2: 2, 2 e 3; T3: 10 mais um teste de unidade fora da contagem; T4: 9; T5d: 9; T6: 2 de `third_graph`) e a tabela de mutações segue com 34 linhas e 33 `CHECKED_FIELDS`.

**Decisão do dono**

- Attestation é um node de um node type do graph de checks, e cada revision dele carrega o desfecho de um check em um snapshot; não é um quarto tipo de record. Aplicado: item 12 (reescrito; `requires_snapshot` é propriedade do node type), item 13 (pergunta `occurred` conta nodes `attestation` ligados ao work item, relation type no passo 4), item 20 (a regra "uma attestation exige snapshot" agora cita D30 e D31, não D28), P6, item 6 ("nodes gravados em uso") e a linha de attestation da seção 11. Fecha o achado 6 do Architect e o 4 do Critic.

**Architect**

- 1 (IMPORTANTE, trava de imports deixava passar grupo e glob): aceito. Seção 5, C7, teste `it_should_not_import_store_or_schema_from_the_engine_sources`: acusa `store` ou `schema` em qualquer posição de caminho e dentro de qualquer `use`, inclusive em grupo `{}`, e todo glob de `crate` ou `hexlog`; trechos embutidos cobrem `use crate::{store::X}`, `use crate::{engine::a, schema}` e `use crate::*`.
- 2 (IMPORTANTE, visibilidade por filho): aceito. Item 22, layout de `engine/mod.rs` na seção 4.2 e o teste de visibilidade: `ports`, `model`, `snapshot`, `limits` e `chain` são `pub(crate)`; `decide`, `rules` e `verify` são `pub(in crate::engine)`, alcançados pela Facade por `pub(crate) use`.
- 3 (IMPORTANTE, quais portas são API pública): aceito. Seção 4.2, parágrafo após o layout: saem `Clock`, `IdGenerator`, `NativeNodes` e `NativeNode`; não saem `Store`, `TxRead`, `Definitions`, as vistas, as linhas e `AppendPlan`.
- 4 (MENOR, `Definitions` sem consumidor antes da T5a): aceito. Teste de unidade na T3 (Entrega e Pronto da T3; seção 5, abaixo da tabela de `catalog`), fora da contagem de 10.
- 5 (MENOR, layout de `ports.rs`): aceito. Lista de `ports.rs` na seção 4.2 e o comentário do código agora citam `NativeNode`, `NodeTypeInfo`, `RelationTypeInfo` e `DataViolation`.
- 6 (MENOR, attestation): resolvido pela decisão do dono.
- Antítese, pares de structs quase iguais: recusado. `NodeTypeInfo` e `RelationTypeInfo` são as vistas que o `engine` precisa; fundi-las com `TypeDef` e `RelationTypeDef` exigiria o `engine` importar `schema`. O parágrafo da seção 4.2 registra o motivo.
- Antítese, corte de 50 violações feito duas vezes: aceito. O comentário de `Definitions::validate_data` diz que o corte é feito só no `Catalog` e o engine repassa.

**Critic**

- 1 (IMPORTANTE, "base nativa" contra o glossary): aceito. Trocado por "o native" ou "conteúdo native" nos 13 pontos das seções 1 a 7 (E5, itens 2, 3, 4, 17, 18, P5, seção 4.1, ADR e próximos passos); só o nome de arquivo `native-base-candidates.md` mantém "base". As seções 8, 9 e 10 ficam como histórico.
- 2 (IMPORTANTE, "Não entra" sem o carregador de extensions): aceito. Seção 4.1 agora lista "carregador de extensions (arquivo para banco, D34)", o que mantém R7 coerente.
- 3 (MENOR, `history()`): aceito. Renomeado para `lineage_records()` (seção 4.4, T5a, teste `it_should_list_the_records_of_a_lineage_in_order_when_reading_lineage_records`), porque o glossary lista "history" em _Avoid_ de Lineage. A palavra "histórico" na prosa em português ("dependentes do histórico gravado") fica: o glossary só veta o termo em inglês.
- 4 (MENOR, attestation com dois sentidos): resolvido pela decisão do dono.
- 5 (MENOR, qual visibilidade vale para cada filho): aceito, é o achado 2 do Architect.
- 6 (MENOR, import agrupado e glob): aceito, é o achado 1 do Architect.
- 7 (MENOR, vetar `work_item` no varredor): aceito com salvaguarda. A razão: o varredor divide por `snake_case`, então `work_item` vira `work` e `item`, e `item` é legítimo (`AppendItem`); sem vetar `work`, o nome do work item entra no motor sem que nada acuse. O risco de falso positivo é baixo porque o varredor compara palavra inteira e só olha identificadores e literais Rust e o `schema.sql` de `src/` (comentários ignorados): `workspace`, `network` e `framework` não casam. A trava fica coberta dos dois lados no `scanner_cases.txt`: acusa `work_item`, não acusa `workspace` nem `network`. Se um dia um literal legítimo com "work" aparecer em `src/`, a saída é trocar a palavra por um veto ao par adjacente `work` e `item`; não vale afrouxar agora.
- 8 (MENOR, `dead_code` em `ports.rs` até a T5a): aceito, com o prazo corrigido para a T6. Confiança do Critic era média, mas o custo é pequeno. A T2 põe `#![allow(dead_code)]` temporário em `engine/mod.rs` (o último consumidor, `verify`, só nasce na T5c, então a T5a não basta), e o "Pronto" da T6 confere `grep -rn "allow(dead_code)" crates/hexlog/src` vazio.
- 9a (faltando, item 20 cita D28): aceito, agora D30 e D31.
- 9b (T5d, comentário falso para `it_should_be_send_and_sync`): aceito. A exceção agora cita o teste de abertura concorrente e o `it_should_be_send_and_sync`.
- 10 (pergunta aberta, módulo `schema`): item 23 da seção 2, marcado [DONO], sem renomear; descreve o impacto de uma troca para `catalog`.

**Nomes antigos e termos que seguem**

- Mesma lista da seção 11, sem mudança, mais: "base" aparece só em `native-base-candidates.md`, em `BaseRef`/`base` (o campo de revision, nome próprio do plano) e nas seções 8, 9 e 10.

## 13. Mudanças da rodada 3 de linguagem

Fonte: [round2-language-reviews.md](../drafts/round2-language-reviews.md) (Architect: 2 IMPORTANTE e 3 MENOR novos; Critic: ITERATE leve com 2 IMPORTANTE e 3 MENOR novos; A e B foram apontados pelos dois). A versão anterior está em [omhl-v2-core-step3.r6.md](omhl-v2-core-step3.r6.md). Esta é a última rodada. Nenhum item [DONO] nem o P0 foi resolvido; as contagens dos "Pronto" e a tabela de mutações não mudaram.

- A (IMPORTANTE, `src/lib.rs` fora de T3 e T4): aceito. T2 declara `mod engine; mod schema; mod store;` com `schema/mod.rs` e `store/mod.rs` vazios; a T3 é a dona do `lib.rs` entre T3 e T4 e acrescenta só o `pub use` de `Catalog`, `TypeDef` e `RelationTypeDef`; a T4 não edita o `lib.rs`; o `pub use` da Facade e das portas públicas entra na T5a. Lugares: "Arquivos" de T2, T3, T4 e T5a.
- B (IMPORTANTE, `allow(dead_code)` só em `engine/mod.rs`): aceito. A T4 acrescenta o mesmo `allow` temporário em `store/mod.rs` (T4 "Arquivos" e T2 "Entrega"); os `pub(crate) use` de `engine/mod.rs` entram só junto com o consumidor (T5a para `decide`, T5c para `verify_chain`), para não gerar `unused_imports`; o "Pronto" da T6 confere por grep, em todo `crates/hexlog/src`, que os dois `allow` saíram.
- C (MENOR, glob com `super`): aceito. Seção 5, C7, teste de imports: acusa todo glob em `use` fora de `#[cfg(test)]`, qualquer que seja o prefixo; trechos embutidos acusam `use super::*;` e `use super::super::*;` e aceitam `use super::*;` dentro de `#[cfg(test)] mod tests`.
- D (MENOR, `CHECKED_FIELDS` e `chain` públicos): aceito. Item 22 e layout de `engine/mod.rs`: `pub use verify::CHECKED_FIELDS;` simples, reexportado pelo `lib.rs` (um `pub(crate) use` não pode ser reexportado por `pub use`, E0364); só `decide` e `verify_chain` saem por `pub(crate) use`. T5c "Arquivos" lista `engine/mod.rs`.
- E (MENOR, "segmento de caminho" ambíguo): aceito. O teste de imports passa a dizer "ao lado de `::` ou dentro de `use`" e ganha o caso aceito `let store = 1;`.
- F (MENOR, teste de unidade da T3 lê outro teste; comando incompleto): aceito. O `def_hash` é comparado com `sha256hex(JCS(definição))` calculado no próprio teste (seção 5, abaixo da tabela de `catalog`); a nota de C3 diz que `--lib` roda também esse teste de unidade, e o comando de C1 ganhou a linha com o filtro `it_should_expose_requires_snapshot_...`, porque `--test catalog` não o executa.
- G (MENOR, T6 sem `src/engine/mod.rs`): aceito. "Arquivos" da T6 lista `src/engine/mod.rs` e `src/store/mod.rs`; o executor remove os itens realmente sem uso que a retirada do `allow` expuser e não reintroduz o `allow`.
- H (MENOR, motivo da recusa da fusão de structs): aceito a correção do motivo; a recusa fica. O parágrafo da seção 4.2 agora diz que as vistas carregam `def_hash`, que é derivado, e que `TypeDef` carrega `schema` e `equivalent`, que o `engine` não deve conhecer; mover `TypeDef` e `RelationTypeDef` para o `engine` também evitaria o import, mas levaria esses campos junto. O motivo escrito na seção 12 (que a fusão exigiria importar `schema`) fica como histórico e está corrigido aqui.
- I (pergunta aberta, attestation como node): registrada sem decidir. Item 13 da seção 2: `list` e a pergunta `occurred` olham a current revision, então uma revision para um snapshot novo apaga da vista corrente o desfecho do snapshot anterior, que só aparece por `lineage_records`; [DONO], passo 4: um semantic id por snapshot, ou uma revision por snapshot.

