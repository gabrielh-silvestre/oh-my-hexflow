# Plano executável do passo 3: núcleo `engine` e `store` em Rust

- **Status:** `pending approval` (consenso RALPLAN-DR, modo SHORT, rodada 3, a última, do Planner). Nada aprovado, nada iniciado, nenhum código escrito.
- **Histórico:** a rodada 1 está congelada em [omhl-v2-core-step3.r1.md](omhl-v2-core-step3.r1.md) e a rodada 2 em [omhl-v2-core-step3.r2.md](omhl-v2-core-step3.r2.md); as revisões estão em [round1-reviews.md](../drafts/round1-reviews.md) e [round2-reviews.md](../drafts/round2-reviews.md). As seções 8 e 9 listam o que mudou, achado por achado.
- **Plano mestre:** [omhl-v2-graph-engine.md](omhl-v2-graph-engine.md) (33 decisões do dono, passo 3 da seção 6, critérios de aceite 1, 3 e 7 da seção 4). Este plano não altera o mestre.
- **Apoio:** [hexlog-current-model.md](../research/hexlog-current-model.md), [jury-verdict.md](../research/vocab/jury-verdict.md), [native-base-candidates.md](../research/native-base-candidates.md). Briefing do consenso: [ralplan-brief.md](../drafts/ralplan-brief.md).
- **Convenções de citação:** `H:n` é a linha `n` de `hexlog-current-model.md`; o `caminho:linha` ao lado é o do repositório do Hexlog TypeScript, como o relatório o cita. `D<n>` é a decisão `n` do plano mestre; `C<n>` é o critério de aceite `n`; `P<n>` é o ponto aberto `n` da seção 5 do mestre, na ordem em que ela os lista.
- **Modo SHORT:** não há segredo, pagamento nem migração de dado (D12: recomeça do zero). O custo de errar o formato da cadeia é real, mas é barato até o primeiro tenant de verdade existir (passo 4); por isso o formato do elo é congelado no portão P0, **antes** de qualquer código que o grave (seção 4.6).

## 1. RALPLAN-DR

### Princípios

1. **O motor conhece estrutura, não significado.** Tipos, relações e regras de domínio chegam como dado (D1, D2, D13). O que o motor conhece de fábrica é pouco: revisão, aposentadoria, aresta, estado tipado (D31), cadeia. Fonte: separação genérico/embutido em H:577-602. Esse "pouco" é vocabulário estrutural, e o dono o confirma no portão P0 (item 19 da seção 2).
2. **A cadeia é a prova; as colunas do SQLite são índice.** O hash cobre o JSON canônico do elo. Toda coluna usada para filtrar é derivada desse JSON e conferida pelo `verify_chain`. Nenhum dado de payload é lido de coluna.
3. **Nada é reescrito.** Anexar-só é imposto em três camadas: a API não tem atualização, o banco recusa `UPDATE`/`DELETE` e índices únicos impedem bifurcação, e o hash detecta o que escapar das duas primeiras (invariante em H:457).
4. **Um só caminho de escrita.** `decide` é função pura chamada dentro de uma transação `BEGIN IMMEDIATE`; a Facade é a única entrada (D7, H:80, H:655).
5. **Portar regras, não mecanismos.** Levamos as regras do Hexlog que a tabela de H:626-661 manda portar (cadeia, lote, `key`, um vigente) e deixamos para trás lock por diretório, linhas rasgadas, escrita atômica manual e teto de 64 MiB (H:630, H:649-651).

### Decision Drivers (os 3 principais)

1. **Integridade verificável sem arrependimento de formato** (C3). O que entra no corpo hasheado do elo fica para sempre; por isso o formato é decidido antes de ser gravado.
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
| **A. Revisão por `base` explícito + índices únicos parciais (recomendada)** | Preserva `FORK_REJECTED` (H:636, H:157-159) com ids semânticos: gravar sobre revisão que não é a ponta é recusado. Bifurcação fica fisicamente impossível. | O agente precisa conhecer a ponta (a resposta do `get` e do `edges` traz o id técnico). |
| B. Revisão implícita, última escrita vence | Mais simples para o agente. | Perde a regra de um vigente por linhagem que o mestre manda portar (mestre seção 3); duas escritas concorrentes se atropelam. |
| C. `supersedes` explícito como no Hexlog | Fiel ao original. | Transforma uma propriedade do motor em relação do usuário, contra D13/D21; e `supersedes` não cruzava processos (H:635), o que não casa com a sobreposição futura. |

**E5. Onde moram as definições de tipo e relação**

| Opção | Prós | Contras |
|---|---|---|
| **A. `Catalog` injetado pelo hospedeiro; cada elo grava o `def_hash` que o validou (recomendada)** | Sem manifesto imutável por processo (H:642 aponta esse problema); a cadeia prova com que definição cada registro foi validado; grafos novos são só dado (C7). | Mudança de definição não é detectada como quebra; ficamos sem a classificação compatível/quebra do Hexlog (H:289-293, que o relatório marca como "mudar"). Invariantes entre registros (por exemplo, transição aberto→fechado, D32) não cabem no schema por registro: ficam no adaptador na v1 (ADR). |
| B. Definições como registros na cadeia | Auditável. | Bootstrap circular (tipo do tipo) e peso desnecessário na v1. |
| C. Manifesto fixado por processo como no Hexlog | Fiel ao original. | Tenant fica imutável a mudanças de tipo; conflita com a base nativa versionada com o plugin (D16). |

**E6. Política de verificação da cadeia — [DONO]**

| Opção | Prós | Contras |
|---|---|---|
| **A. A escrita confere só a cauda; verificação completa em `verify_chain` (recomendada)** | Escrita O(1). Leituras indexadas do SQLite ficam baratas. | **Desvio de H:243**: o Hexlog "falha fechado" em toda leitura. Com `Tail`, adulteração no meio da cadeia só aparece no `verify_chain` (teste documenta o limite). |
| B. Verificação completa em toda leitura e escrita | Falha fechado, como o original. | O(n) por chamada; o Hexlog já media orçamento de 500 ms para 5.000 registros (H:556). |

O dono decide se aceita o desvio. Enquanto não decidir, `Config.verify_policy` aceita `Tail` e `Full`, o padrão de produção fica `Tail` por recomendação e os testes usam `Full`. Reavaliar com medição (R4).

**Escolhas sem alternativa material:** `sha2` para sha256, `uuid` (v7) para id técnico, `thiserror` para o catálogo de erros, `proptest` para propriedades (H:568), `tempfile` para bancos de teste, `proc-macro2` (só em teste) para tokenizar fontes na trava de vocabulário. Versões vistas no registro hoje (`cargo search`): `rusqlite 0.41.1`, `serde_json_canonicalizer 0.4.1`, `sha2 0.11.1`, `jsonschema 0.60.2`, `uuid 1.29.1`, `thiserror 2.0.21`. Fixar as versões exatas só depois do spike da T1; a documentação dessas crates não foi consultada neste plano.

## 2. Contradições e lacunas entre as decisões

Marcação: **[DONO]** = só o dono decide; o plano traz proposta e não a executa como decisão. **[CÓDIGO]** = resolvida pelo desenho do passo 3. Os itens 1, 5, 10, 16 e 19, mais a tabela de regras de `state` (item 11), são o conteúdo do portão P0 (seção 4.6): a T2 só começa depois de respondidos.

1. **[DONO] Estado no núcleo contra motor agnóstico (D31 × D1).** D31 põe o SHA do git no núcleo; D1 diz que o motor não conhece domínio. **Esta proposta é uma releitura de D31, não a letra dela:** o núcleo conhece só o tipo `State { kind: StateKind, id: String }`, com `StateKind` enum fechado (hoje `GitCommit`) e validação de formato; não invoca git nem importa crate de git. A sonda que lê `HEAD` e checa árvore limpa é uma porta (`StateProbe`) implementada no crate `omhl` (passos 4 e 5). Consequência que o dono precisa aceitar: o núcleo confia no `State` que o chamador entrega e não prova que o SHA pertence ao repositório nem que a árvore estava limpa; "estado com alterações não commitadas não pode ser aprovado" (D31) é imposto pelo adaptador, não pelo motor. O teste de arquitetura permite a palavra `git` somente em `src/engine/state.rs`. A formatação de `GitCommit` é hexadecimal minúsculo de **40 ou 64** caracteres, mais estrita que o `git-sha` do Hexlog (7 a 40, sem normalizar; `src/domain/formats.ts:3-4`), porque abreviação não é "tão reproduzível quanto o git" (D31). Se o dono quiser a letra de D31 (o núcleo chamar o git), a mudança é uma porta a menos e uma dependência proibida a menos na trava; o formato do elo não muda.
2. **[DONO] "Só adiciona" contra ids semânticos e `externalId` (D17 × D6 × D21).** Quatro lacunas.
   - O motor não conhece a base nativa (D16, fora da cadeia), mas precisa recusar gravação com id semântico nativo (C2). Proposta [CÓDIGO]: porta `ForeignNodes` injetada na Facade; id reservado é o que `lookup` reconhece (casamento exato).
   - Com revisão por id não há o que bifurcar sem um ponto de comparação: o mestre manda portar `FORK_REJECTED` (mestre seção 3) mas não diz como. Proposta [CÓDIGO]: campo `base` (E4-A).
   - `externalId` é "reservado" sem definição. Proposta [CÓDIGO]: coluna opcional `external_id`, texto de 1 a 200 caracteres, sem unicidade, entra no hash quando presente e é omitido quando ausente (assim acrescentar o uso depois não muda o hash dos elos antigos). H:634 diz que o análogo atual é a raiz da linhagem.
   - Atualização do plugin pode introduzir um id nativo que já existe no tenant, ou remover um id nativo ao qual arestas do projeto apontam. Proposta [DONO]: o nativo prevalece e a leitura marca o nó do projeto como `shadowed` (campo de `NodeView`); nativos nunca saem dentro de uma versão maior. Fora do passo 3: aqui só se entrega a porta e o teste com fake.
3. **[DONO] Exemplo de sobreposição por id contra "substituição fica para depois" (D16 × D17, candidato 5).** [native-base-candidates.md](../research/native-base-candidates.md) (linha 24) traz a política de commit como "exemplo de sobreposição por id": o projeto sobrepõe o padrão conservador. D17 proíbe alterar ou substituir nativo na v1. Proposta: o candidato 5 entra na base v1 só como diretriz nativa fixa, e o projeto acrescenta uma diretriz própria com outro id; o `render` mostra as duas e o agente resolve a precedência por texto. Alternativa: retirar o candidato 5 da v1. Escolha do dono.
4. **[CÓDIGO] Arestas do projeto para nós nativos (D16 × D20 × D33).** O destino nativo está fora da cadeia. Proposta: `ForeignNodes::lookup` devolve existência e tipo; a escrita valida existência e os tipos permitidos da `RelationDef` (como em H:144 e H:187-208); o elo grava `from_origin`/`to_origin` (`tenant` ou `native`) e a versão do catálogo nativo no momento da escrita (`native_catalog`), informativa e dentro do hash. A leitura devolve `EndpointState` por extremo (`Current`, `Retired`, `MissingNative`) em `EdgeView` e nunca falha por nativo ausente; `verify_chain` ignora conteúdo nativo. Aresta entre dois nativos é recusada. A detecção de ciclo inclui as arestas internas da base via `ForeignNodes::native_edges(rel)`.
5. **[DONO] "Antes de um nativo" exige aresta com origem nativa (D17 × D20 × D33).** D33 só tem `after`; "P antes de N" é `N after P`, aresta cuja origem é o nativo N. É alterar N? Proposta: a aresta é registro do projeto (D13), não altera o nativo; permitida com pelo menos um extremo no tenant. Se o dono recusar, a restrição é uma linha em `engine/rules.rs` e um teste; `from_origin` continua no corpo do elo (a coluna existe por simetria e por `to_origin`). Resposta necessária no P0.
6. **[DONO] Projeção só banco→arquivo contra banco global por máquina (D9 × D18 × D26 × D27).** O banco é global do usuário e não está no repositório; a projeção é o único rastro versionado. D27 trata a divergência como "trabalho de outra pessoa", mas o caso real é o mesmo usuário em outra máquina, em clone novo ou com banco apagado: o tenant está vazio, o `post-merge` "regenera a partir do banco" e apaga a projeção, sem caminho de importação. Proposta: o motor expõe `head(tenant)`; a projeção grava um marcador `tenant_head` (seq e hash) junto dos arquivos; a regeneração recusa sobrescrever (avisa, não escreve) quando o marcador dos arquivos não existe na cadeia do banco ou está à frente dela. Isso muda a letra de D27 ("avisa e regenera") só no caso em que regenerar perde dado; o dono decide. Entrega no passo 6; no passo 3 só `head` e `verify_chain` com `expected_head`.
7. **[DONO] Projeção no repositório contra aprovação amarrada ao SHA (D18 × D26 × D30 × D31).** Gerar ou regenerar `.omhl/` altera a árvore: o hook `post-merge` pode deixar a árvore suja, e D31 proíbe aprovar com alterações não commitadas; commitar a projeção troca o SHA e invalida aprovações feitas no SHA anterior. Proposta: ordem de uso projetar, commitar, aprovar; C8 (bytes idênticos para o mesmo estado) evita diferença espúria. Fica registrado como risco; o dono decide se commits de `.omhl/` são esperados antes da aprovação. Nada a fazer no passo 3.
8. **[DONO] Quatro perguntas "como estão" contra herança de linhagem descartada (D24 × H:639).** As perguntas `approved` e `no_pending` do Hexlog têm a opção `inherit` (H:337) apoiada na herança por linhagem (H:164-171), que o relatório manda descartar na v1. Proposta: portar as perguntas sem `inherit` (caminho `inherit: false`, que é o padrão). Fora do passo 3 (a avaliação mora em `graphs/*`); o passo 3 deixa as consultas genéricas que elas precisam (`edges`, `list` com `state` e `where_eq`).
9. **[CÓDIGO] Relações como registros contra oito espécies em código (D13 × plano seção 2 × H:129-146, H:637).** Resolução: nenhuma "espécie" fica em código. O motor entende só revisão (`base`), aposentadoria (item 10) e aresta; a semântica de cada relação é dado em `RelationDef` (`from`, `to`, `acyclic`, `equivalent`). `supports`, `contradicts` e `answers`, de que D24 depende, passam a ser nomes definidos pelo catálogo do grafo de decisões (passo 4). `needsReview` e herança ficam fora (item 8). Identidade de uma aresta: a tripla (`rel`, `from`, `to`); a chave de linhagem da aresta é `rel|from|to` (o separador `|` nunca ocorre em nome ou id).
10. **[DONO] Sem desfazer em um modelo "só adiciona" (D17 × D13 × D20).** Uma aresta `after` errada, ou um nó do projeto que deixou de valer, não tem como sair. Revisar uma aresta não resolve (a identidade é a tripla). Proposta: variante `Retire` (aposentadoria = revisão sem dado, `base` obrigatória), que tira o sujeito da vigência sem apagar nada, equivalente ao `revokes` sem sucessor (H:134). Incluir já no passo 3 porque adicionar uma variante ao corpo do elo depois do congelamento muda o formato. Se o dono recusar, remover a variante no P0. Resposta necessária no P0.
11. **[DONO] Regras de `state` dentro do corpo hasheado (D31 × D28 × D30).** O corpo do elo carrega `state` e `external_id`; duas leituras de "quando o `state` vale" gravam corpos diferentes e o formato congela. Proposta: a tabela da seção 4.5 (tipo × item × `state`), com um teste por linha. Em resumo: o `state` é obrigatório nos nós de tipo `requires_state` e em cada revisão deles (não é herdado), e proibido em qualquer outro item, inclusive `Retire` e aresta; o `external_id` também não é herdado pela revisão (linha X4). O dono confirma a tabela no P0.
12. **[DONO] "Refazer o check" implica execução (D30 × D4 × D24 × D28 × mestre seção 2).** Gate avaliável é leitura e cabe em D4; "refazer o check no mesmo estado e obter o mesmo resultado" (D30) supõe executar, fora da v1 pela seção 2 do mestre. Além disso "check" (nó de texto, D14/D22), "gate mecânico" (D28/D30) e "gate" (consulta, D22) se sobrepõem. Proposta de glossário: **check** é o nó de instrução; **gate** é a consulta avaliável; **approval** é um registro com `State` que afirma "este estado passou neste gate". Na v1 aprovação é registro; determinismo é propriedade a verificar quando a execução entrar. Suporte do passo 3: `TypeDef.requires_state`.
13. **[DONO] O que o gate nativo de PR avalia (D28 × D29 × D24).** Com evidência embutida na decisão (D28) não há nó de evidência para `supports`, e o exemplo `pre-pr` do Hexlog usa `finding` e `answers` (H:350), que não existem na v1. Esboço mínimo para o passo 4: pergunta 1 `occurred` (pelo menos 1 registro `approval` do `unit` cujo `State` é igual ao SHA de fechamento informado pelo chamador); pergunta 2 `no_open_contradiction` sobre as decisões ligadas ao `unit`. As demais perguntas e o significado de "aprovado" são do dono (P6 do mestre). Suporte do passo 3: filtro `state` em `ListFilter`.
14. **[DONO] Fechar o `unit` com gate reprovado (D32 × D4).** D32 diz que o fechamento é quando o gate é avaliado; D4 diz que nada bloqueia. Proposta: o fechamento nunca é recusado na v1; a revisão de fechamento guarda no `data` o resultado do gate e a cabeça da cadeia (`seq`) em que foi avaliado. Passo 4.
15. **[CÓDIGO] O `unit` não tem casa (D3 × D29 × mestre seção 3).** A tabela da seção 3 lista só `graphs/decisions` e `graphs/checks`, e o passo 4 do mestre diz "decisões e de gates". Proposta: `graphs/units` no crate `omhl`; o `unit` é o terceiro grafo natural e serve de fixture do C7 (com nome neutro no teste). Também, D6 usa o exemplo `gates.before-commit.run-tests` enquanto D3/D22 chamam o grafo e o nó de `check`; propor ids de exemplo sob `checks.`.
16. **[DONO] `@alias` sem uso com ids semânticos (mestre seção 3 × D6 × D21).** Arestas apontam ids semânticos, que já são conhecidos ao escrever; `@alias` só serve para referir o id técnico de um item anterior do lote, em `base`. Proposta: manter `alias` apenas em `base`. Como `batch.aliases` entra no corpo do elo, a decisão de manter ou cortar `alias` muda o formato e fica no P0.
17. **[DONO] Texto longo no registro contra teto de `data` (D25 × H:194, H:644).** D25 guarda o texto longo no SQLite "como parte do registro (com hash)"; o teto de `data` é 16.000 caracteres canônicos e os anexos foram descartados (H:644 manda medir antes de portar). Proposta: `Limits.data_max_chars` com padrão 16.000. Começar apertado é seguro porque afrouxar o teto depois é compatível e apertar invalida linha já gravada (H:214). O passo 4 mede os textos reais da base; se passarem, o dono decide entre subir o teto e portar blobs endereçados por hash.
18. **[CÓDIGO] Resíduos de redação.** "Fonte de verdade de tudo" (D9) contra base nativa fora do banco (D16): leitura adotada, o SQLite é a verdade de tudo do tenant e a base nativa é a verdade do binário. Módulos do mestre (seção 3) contra dois crates (D11): resolvido por E3-A, `engine`, `schema`, `store` dentro de `hexlog`; `native`, `graphs/*`, `render`, `project`, `adapters/*` dentro de `omhl`.
19. **[DONO] Vocabulário estrutural no corpo do elo (D1).** `Retire`, `requires_state`, `State`, `from_origin`/`to_origin`, `native_catalog` e `def_hash` não são vocabulário de domínio de desenvolvimento, mas são mais que "entidades, tipos, relações, cadeia e consulta" (D1). Proposta: aceitar como estrutura do motor, listada em P1 (princípio 1) e coberta pela trava de vocabulário (a trava veta termos de domínio, não estes). O dono confirma no P0 que isso é estrutura e não domínio; se algum desses itens sair, o corpo do elo encolhe antes de ser gravado.
20. **[CÓDIGO] Invariantes entre registros (D32 × D29 × E5).** Transição aberto→fechado do `unit` (D32) e "aprovação exige estado" (D28) são regras entre registros ou sobre valores de `data`; o schema por registro e as flags `from`/`to`/`acyclic` não as expressam. Resolução: ficam no adaptador na v1 (o crate `omhl` valida antes de chamar a Facade), e o motor continua neutro. O `third_graph` traz um caso que mostra o padrão (seção 5, C7). Se o dono quiser essas regras no motor, é um marco próprio.

## 3. Pontos abertos (seção 5 do plano mestre)

| # | Ponto | Situação | Proposta |
|---|---|---|---|
| P1 | Campos do `.omhl.toml` | Proposta; o dono confirma ao revisar o passo 5 | `version = 1` e `tenant = "<Name>"` (mesma regra de nome do motor: `^[a-z0-9][a-z0-9-]{0,62}$`, herdada de `src/domain/ids.ts:4-5`), mais `[projection] regenerate = "post-merge"`, valores `write`, `on-demand`, `session-start`, `post-merge` (D26). Chaves desconhecidas são erro. Nada mais entra na v1. |
| P2 | Layout de `.omhl/` | Proposta; o dono confirma antes do passo 6 | Uma pasta por tipo de nó e um arquivo por nó: `.omhl/checks/<id>.yaml`, `.omhl/guidelines/<id>.yaml`, `.omhl/gates/<id>.yaml`; textos longos em `.omhl/text/<id>.md` (D25). O nome do arquivo é o id semântico sem transformação: a regra do id (minúsculas, dígitos, hífen e ponto, até 200 caracteres) garante nome seguro em qualquer sistema de arquivos e sem colisão por caixa. |
| P3 | Isolamento entre tenants e mover/renomear projeto | Resolvido no código, com ressalva do item 6 | Toda tabela leva `tenant` como primeira coluna da chave; cada tenant tem âncora e cadeia próprias (o nome do tenant entra na âncora e em cada elo). O tenant vem do `.omhl.toml` (D10, D23), então mover ou renomear a pasta não muda nada. Renomear o tenant no `.omhl.toml` cria outro tenant vazio; não há operação de renomear na v1. Dois clones do mesmo repositório na mesma máquina compartilham o tenant. Testes: T5c. A ressalva é a máquina nova ou o banco perdido (item 6). |
| P4 | Teste do `when` bruto | Precisa do dono | É um experimento com sessão real de agente (protocolo em [jury-verdict.md](../research/vocab/jury-verdict.md), linhas 47 e 48), sem código. Fora do passo 3. |
| P5 | Licença do schema.org | Proposta; o dono confirma | A base nativa da v1 não embute nenhum subconjunto do schema.org nem de outro padrão; só guarda URIs em `equivalent` (D15), que são texto de referência, não conteúdo licenciado. Isso evita a verificação de CC BY-SA que ninguém fez (jury-verdict.md, linha 39). |
| P6 | Entidade `unit` e gate nativo de PR | Precisa do dono | Esboço dos campos para o passo 4: `provider`, `repo`, `number` ou `branch`, `status` (`open`/`closed`). O SHA de cabeça vive no `State` do registro `approval`, não no `unit`. Itens 12, 13 e 14 da seção 2 listam o que falta decidir. |
| P7 | Forma do campo de estado | Proposta; o dono confirma no P0 | `{kind, id}` com `kind` enum fechado em Rust (D31: tipos novos entram direto no núcleo), serializado como `"git-commit"`. Critério mínimo para aceitar um tipo novo, que sirva de regra escrita: (i) o `id` é endereçado pelo conteúdo; (ii) é imutável depois de criado; (iii) uma sonda local, sem rede, o obtém de forma determinística; (iv) a sonda consegue declarar "não reproduzível" (como árvore suja) e então o estado é recusado. Validação do SHA e regras de uso: itens 1 e 11 da seção 2. |

## 4. Plano executável do passo 3

### 4.1 Escopo

**Entra:** workspace Cargo; crate `hexlog` com `engine`, `schema` e `store` (SQLite); Facade; cadeia por tenant; id semântico; revisão com `base`; aresta, aposentadoria, estado tipado; lote atômico; idempotência; verificação da cadeia; testes dos critérios 1, 3 e 7 (e das regras de C2 que dependem só do motor: reservar id).

**Não entra (passos seguintes):** FTS5, cursor com marcador e `changesSince`, anexos e blobs, telemetria, avaliação de `gate`, base nativa, grafos de decisões, de checks e de `unit`, `render`, `.omhl.toml`, projeção, adapters CLI, MCP e hook, caminho padrão do banco (fica em `omhl`; o motor recebe o caminho pela `Config`). O crate `omhl` recebe apenas um `main` mínimo que confirma que o workspace compila e que a dependência vai de `omhl` para `hexlog`, nunca o contrário.

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
        lib.rs                     # reexporta a Facade, os tipos públicos, CHECKED_FIELDS e as funções puras de hash (module `chain`)
        facade.rs                  # Hexlog, Config, Deps, requests e views
        error.rs                   # ErrorCode, HexlogError, Detail
        ids.rs                     # Name, TenantId, SemanticId, TypeName, RelationName, RecordId, Hash, Alias, IdempotencyKey, ExternalId
        engine/
          mod.rs
          model.rs                 # AppendItem, ItemBody, Subject, BaseRef, Author, Origin, AppendOutcome, AppendedRecord, ChainHead, LinkBody, BatchMeta, ListFilter, FieldName, Scalar, PageRequest, EdgeQuery
          state.rs                 # State, StateKind (único lugar que pode citar "git")
          limits.rs                # Limits e constantes
          ports.rs                 # Clock, IdGenerator, ForeignNodes, Store, TxRead, AppendPlan, NewLink, LinkRow, LinkColumns, TipRow, BatchRow, TenantRow, SubjectKind
          chain.rs                 # canonical, sha256hex, hash_link, anchor, fingerprint
          decide.rs                # decide puro, devolve AppendPlan
          rules.rs                 # regras de revisão, state, aresta, ciclo, reservado
          verify.rs                # verify_chain e CHECKED_FIELDS
        schema/
          mod.rs
          catalog.rs               # TypeDef, RelationDef, Catalog, validação de definições
          validate.rs              # validação de data contra o JSON Schema
        store/
          mod.rs                   # só reexporta `sqlite`; os contratos moram em engine/ports.rs
          sqlite/
            mod.rs                 # SqliteStore (Mutex<Connection>), abertura, PRAGMAs
            ddl.rs                 # carrega schema.sql por include_str! e confere user_version
            schema.sql             # o DDL da seção 4.3 (arquivo SQL, fora do alcance da varredura de vocabulário)
            tx.rs                  # transação de escrita, TxRead, leituras
      tests/
        support/mod.rs             # relógio e ids fixos, ForeignNodes fake (T2); T3, T4 e T5c acrescentam uma linha `pub mod` cada
        support/catalog.rs         # catálogo de teste (T3)
        support/tamper.rs          # conexão crua que desliga as travas (T4)
        support/mutations.rs       # cadeia de referência e tabela de mutações da seção 4.5 (T5c)
        semantic_id.rs  state.rs  jcs_vectors.rs  catalog.rs                       # T2 e T3
        node_lifecycle.rs  state_rules.rs  list_filters.rs  batch.rs  idempotency.rs  # T5a
        edges.rs                                                                    # T5b
        chain_tamper.rs  chain_properties.rs  tenant_isolation.rs                   # T5c
        concurrency.rs                                                              # T5d
        third_graph.rs  architecture.rs                                             # T6 e T1
        fixtures/third_graph.json  fixtures/rfc8785_vectors.json  fixtures/golden_links.json  fixtures/scanner_cases.txt
    omhl/
      Cargo.toml                   # depende de hexlog
      src/main.rs                  # --version apenas
```

Testes de unidade que precisam de itens internos (o `SqliteStore`, os triggers) ficam em módulos `#[cfg(test)]` dentro de `src/store/sqlite/`; os de integração em `tests/` usam só a API pública e, para adulterar, uma conexão `rusqlite` crua ao arquivo do banco.

Dependências de `hexlog` (conferidas por `architecture.rs` e, na T1, por `cargo tree`): `serde`, `serde_json` (sem `arbitrary_precision` nem `preserve_order`), `serde_json_canonicalizer`, `sha2`, `rusqlite` (`bundled`), `jsonschema` com **`default-features = false`**, `uuid` (`v7`), `thiserror`, `time`, `url` (só valida a sintaxe de `equivalent`, D15). Dev: `proptest`, `tempfile`, `proc-macro2`. A trava é uma **lista de crates proibidos** no fecho de dependências normais de `hexlog`: `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std`. Regex como dependência transitiva não é vetado: o motor recusa `pattern` no catálogo e nunca constrói regex vindo de schema (H:299).

### 4.3 Esquema SQLite

O caminho do arquivo vem da `Config`; o padrão (`$XDG_DATA_HOME/omhl/omhl.db`, senão `~/.local/share/omhl/omhl.db`, espelhando `src/directory.ts:5-16`, H:465) é do crate `omhl`. `PRAGMA`s na abertura: `journal_mode=WAL`, `synchronous=FULL` (equivale ao `fsync` por lote, H:483), `foreign_keys=ON`, `busy_timeout` configurável (padrão 5.000 ms). Versão do esquema em `PRAGMA user_version`: banco novo e vazio recebe `1`; versão maior que a suportada, ou banco não vazio com `user_version = 0`, é recusado com `STORE_ERROR` e detalhe `unsupported-schema-version` (nunca é aberto "como está"). Não há migração na v1 (D12). Banco em disco de Windows montado no WSL não é suportado (locks); documentar.

O DDL abaixo é o conteúdo de `src/store/sqlite/schema.sql`, embutido no binário por `include_str!("schema.sql")` em `ddl.rs`. Ele não é literal de string em fonte Rust, então a varredura de vocabulário da seção 5 (C7), que lê só identificadores e literais de arquivos `.rs`, não o vê; a palavra `CHECK` do SQL é palavra-chave de armazenamento, não vocabulário de domínio.

```sql
CREATE TABLE tenants (
  tenant     TEXT PRIMARY KEY,
  anchor     TEXT NOT NULL,            -- sha256hex(anchor_doc)
  anchor_doc TEXT NOT NULL,            -- JCS de {"format":"hexlog-chain/1","tenant":...,"created_at":...}
  created_at TEXT NOT NULL             -- igual a anchor_doc.created_at
) STRICT;

CREATE TABLE links (
  tenant       TEXT    NOT NULL REFERENCES tenants(tenant),
  seq          INTEGER NOT NULL CHECK (seq >= 1),
  id           TEXT    NOT NULL UNIQUE,                 -- "<tenant>:<uuidv7>"
  prev_hash    TEXT    NOT NULL,
  hash         TEXT    NOT NULL,
  body         TEXT    NOT NULL,                        -- JCS do elo sem prev_hash: único insumo do hash
  -- colunas derivadas de body (conferidas pelo verify_chain):
  at           TEXT    NOT NULL,
  type         TEXT    NOT NULL,                        -- tipo do nó ou nome da relação
  subject_kind TEXT    NOT NULL CHECK (subject_kind IN ('node','edge')),
  subject_key  TEXT    NOT NULL,                        -- id semântico, ou "rel|from|to"
  retired      INTEGER NOT NULL CHECK (retired IN (0,1)),
  revises      TEXT,                                    -- id técnico da ponta anterior
  rel          TEXT,  from_id TEXT,  to_id TEXT,  from_origin TEXT,  to_origin TEXT,
  state_kind   TEXT,  state_id TEXT,  external_id TEXT,  def_hash TEXT NOT NULL,
  PRIMARY KEY (tenant, seq)
) STRICT;

CREATE UNIQUE INDEX links_root      ON links (tenant, subject_kind, subject_key) WHERE revises IS NULL;
CREATE UNIQUE INDEX links_successor ON links (tenant, revises)                   WHERE revises IS NOT NULL;
CREATE INDEX links_subject  ON links (tenant, subject_kind, subject_key, seq);
CREATE INDEX links_type     ON links (tenant, type, subject_key);
CREATE INDEX links_state    ON links (tenant, state_kind, state_id) WHERE state_id IS NOT NULL;
CREATE INDEX links_edge_out ON links (tenant, rel, from_id) WHERE subject_kind = 'edge';
CREATE INDEX links_edge_in  ON links (tenant, rel, to_id)   WHERE subject_kind = 'edge';

CREATE TABLE batches (          -- índice de idempotência; só lotes com key
  tenant      TEXT NOT NULL REFERENCES tenants(tenant),
  key         TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  first_seq   INTEGER NOT NULL,
  item_count  INTEGER NOT NULL CHECK (item_count BETWEEN 1 AND 50),
  PRIMARY KEY (tenant, key),
  FOREIGN KEY (tenant, first_seq) REFERENCES links (tenant, seq)
) STRICT;
```

Triggers (defesa em profundidade; a proteção real é o hash):

- `BEFORE UPDATE` e `BEFORE DELETE` em `tenants`, `links` e `batches` fazem `RAISE(ABORT, 'append-only')`.
- `BEFORE INSERT` em `links` recusa `seq` diferente de `COALESCE(MAX(seq),0)+1` do tenant, e `prev_hash` diferente do `hash` do elo anterior (ou da `anchor` quando `seq = 1`). O SQLite não calcula sha256; o hash é conferido em Rust.

**Vigência, sem coluna mutável:** a ponta de um sujeito é o elo sem sucessor (`NOT EXISTS (SELECT 1 FROM links s WHERE s.tenant = l.tenant AND s.revises = l.id)`); o sujeito está vigente quando a ponta tem `retired = 0`. `links_root` garante uma raiz por sujeito e `links_successor` garante no máximo um sucessor por elo: bifurcação é fisicamente impossível, e `FORK_REJECTED` é a tradução do erro de índice mais a checagem prévia em `decide`.

**Corpo do elo (`body`), JCS de:**

```json
{"v":1,"tenant":"acme","seq":3,"id":"acme:<uuidv7>","at":"2026-11-07T12:00:00.000Z",
 "type":"note","subject":{"kind":"node","id":"area.sub.item"},
 "revises":"acme:<uuidv7>","retire":false,"data":{},
 "author":{"agent":"...","client":"cli"},
 "state":{"kind":"git-commit","id":"<40 ou 64 hex>"},"external_id":"...","def_hash":"<64 hex>",
 "batch":{"fingerprint":"<64 hex>","key":"...","count":2,"aliases":{"a":0}}}
```

Regras do corpo: campo opcional ausente é omitido, nunca `null` (acrescentar campo opcional depois não muda hash antigo); `batch` só no primeiro elo do lote (H:188); aresta troca `subject` por `{"kind":"edge","rel":..,"from":..,"to":..,"from_origin":..,"to_origin":..}` e acrescenta `native_catalog` quando algum extremo é nativo; aposentadoria leva `"retire":true`, sem `data`, **copia `type` e `def_hash` da ponta anterior** (a definição que valeu quando a ponta foi gravada, não a do catálogo atual); quando o sujeito é uma aresta, copia também `from_origin`, `to_origin` e `native_catalog` da ponta.

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
    pub foreign: std::sync::Arc<dyn ForeignNodes>,
}

impl Hexlog {
    pub fn open(config: Config, deps: Deps) -> Result<Hexlog, HexlogError>;
    pub fn ensure_tenant(&self, tenant: &TenantId) -> Result<TenantInfo, HexlogError>;
    pub fn append(&self, req: AppendRequest) -> Result<AppendOutcome, HexlogError>;
    pub fn get(&self, tenant: &TenantId, id: &SemanticId) -> Result<Option<NodeView>, HexlogError>;
    pub fn history(&self, tenant: &TenantId, subject: &Subject) -> Result<Vec<RevisionView>, HexlogError>;
    pub fn list(&self, tenant: &TenantId, filter: &ListFilter, page: &PageRequest) -> Result<Page<NodeView>, HexlogError>;
    pub fn edges(&self, tenant: &TenantId, query: &EdgeQuery) -> Result<Vec<EdgeView>, HexlogError>;
    pub fn head(&self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    pub fn verify_chain(&self, tenant: &TenantId, expected_head: Option<&Hash>) -> Result<ChainReport, HexlogError>;
    pub fn catalog(&self) -> &Catalog;
}

// Entrada
pub struct AppendRequest { pub tenant: TenantId, pub author: Author, pub key: Option<IdempotencyKey>, pub items: Vec<AppendItem> } // 1..=50
pub struct AppendItem {
    pub alias: Option<Alias>,
    pub body: ItemBody,
    pub base: Option<BaseRef>,            // obrigatória se o sujeito já tem linhagem; proibida se não tem
    pub state: Option<State>,             // regras da tabela da seção 4.5
    pub external_id: Option<ExternalId>,
}
pub enum ItemBody {
    Node   { id: SemanticId, type_name: TypeName, data: serde_json::Value },
    Edge   { rel: RelationName, from: SemanticId, to: SemanticId },
    Retire { subject: Subject },
}
pub enum Subject { Node(SemanticId), Edge { rel: RelationName, from: SemanticId, to: SemanticId } }
pub enum BaseRef { Id(RecordId), Alias(Alias) }
pub struct Author { pub agent: String, pub model: Option<String>, pub client: String } // 1..=100 caracteres; `client` é do adaptador (H:119, H:656)

// Saída de escrita (`AppendOutcome`, `AppendedRecord` e `ChainHead` são definidos em `engine/model.rs` e reexportados aqui)
pub struct AppendOutcome { pub records: Vec<AppendedRecord>, pub replayed: bool, pub head: ChainHead }
pub struct AppendedRecord { pub alias: Option<Alias>, pub id: RecordId, pub seq: u64 }
pub struct TenantInfo { pub tenant: TenantId, pub anchor: Hash, pub created_at: String, pub created: bool }
pub struct ChainHead { pub seq: u64, pub hash: Hash }               // seq 0 e hash = âncora para tenant vazio

// Saída de leitura
pub enum Origin { Tenant, Native }
pub enum EndpointState { Current, Retired, MissingNative }
pub struct NodeView {
    pub id: SemanticId, pub type_name: TypeName,
    pub record_id: RecordId,                 // id técnico da ponta: serve de `base` na próxima revisão
    pub seq: u64, pub at: String, pub data: serde_json::Value,
    pub state: Option<State>, pub external_id: Option<ExternalId>, pub author: Author,
    pub shadowed: bool,                      // true quando `ForeignNodes` reconhece o mesmo id (item 2 da seção 2)
}
pub struct EdgeView {
    pub record_id: RecordId,                 // id técnico da ponta da aresta: serve de `base` de um Retire
    pub seq: u64, pub at: String, pub rel: RelationName,
    pub from: SemanticId, pub to: SemanticId,
    pub from_origin: Origin, pub to_origin: Origin,
    pub from_state: EndpointState, pub to_state: EndpointState,
    pub native_catalog: Option<String>, pub external_id: Option<ExternalId>, pub author: Author,
    pub retired: bool,
}
pub struct RevisionView {
    pub record_id: RecordId, pub seq: u64, pub at: String, pub revises: Option<RecordId>,
    pub retired: bool, pub data: Option<serde_json::Value>,
    pub state: Option<State>, pub external_id: Option<ExternalId>, pub author: Author,
}

// Consulta (`ListFilter`, `FieldName`, `Scalar`, `PageRequest` e `EdgeQuery` são definidos em `engine/model.rs`, porque `TxRead` os usa, e reexportados aqui)
pub struct ListFilter {
    pub type_name: Option<TypeName>,
    pub id_prefix: Option<String>,           // casa só na fronteira do `.`
    pub state: Option<State>,
    pub where_eq: Vec<(FieldName, Scalar)>,  // igualdade em campo escalar de primeiro nível de `data` (H:337)
    pub include_retired: bool,
}
pub struct FieldName(String);                // 1..=64 de [A-Za-z0-9_-]; nada além disso entra no caminho JSON
pub enum Scalar { Str(String), Int(i64), Bool(bool) }
pub struct PageRequest { pub limit: u32, pub after_seq: Option<u64> }   // limit 1..=200 (padrão 50, H:209); ordem por `seq` da ponta
pub struct Page<T> { pub items: Vec<T>, pub next_after_seq: Option<u64> }
pub struct EdgeQuery { pub rel: Option<RelationName>, pub from: Option<SemanticId>, pub to: Option<SemanticId>, pub include_retired: bool }
// consulta pelo extremo `to` entrega a leitura inversa (D33: `before`)

// Verificação
pub struct ChainReport { pub ok: bool, pub total_records: u64, pub head: Hash, pub breaks: Vec<ChainBreak>, pub total_breaks: u64 } // breaks até 100 (H:240)
pub struct ChainBreak { pub seq: Option<u64>, pub reason: BreakReason }   // `seq` é None nas quebras de âncora e de cabeça esperada
pub enum BreakReason { DivergingSeq, HashMismatch, InvalidBody, ColumnMismatch, BatchIndexMismatch, AnchorMismatch, HeadNotFound }
```

Portas e catálogo (`engine/ports.rs`, `schema/catalog.rs`):

```rust
pub trait Clock: Send + Sync { fn now(&self) -> String; }                 // UTC, ms, "…Z"
pub trait IdGenerator: Send + Sync { fn next(&self) -> uuid::Uuid; }       // uuid v7 em produção
pub trait ForeignNodes: Send + Sync {
    fn lookup(&self, id: &SemanticId) -> Option<ForeignNode>;              // id reservado = lookup().is_some()
    fn native_edges(&self, rel: &RelationName) -> Vec<(SemanticId, SemanticId)>;
    fn catalog_version(&self) -> &str;
}
pub struct ForeignNode { pub type_name: TypeName }

pub struct TypeDef { pub name: TypeName, pub schema: serde_json::Value, pub requires_state: bool, pub equivalent: Option<String> }
pub struct RelationDef { pub name: RelationName, pub from: Option<Vec<TypeName>>, pub to: Option<Vec<TypeName>>, pub acyclic: bool, pub equivalent: Option<String> }
impl Catalog {
    pub fn new(types: Vec<TypeDef>, relations: Vec<RelationDef>) -> Result<Catalog, HexlogError>; // valida tudo na construção
    pub fn type_def(&self, name: &TypeName) -> Option<&TypeDef>;
    pub fn relation_def(&self, name: &RelationName) -> Option<&RelationDef>;
}
```

Contratos de armazenamento (`engine/ports.rs`; o motor é dono das portas e o `store/sqlite` as implementa, então `engine` nunca importa `store`):

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
    fn head(&self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    fn tip(&self, tenant: &TenantId, subject: &Subject) -> Result<Option<TipRow>, HexlogError>;
    fn link_by_id(&self, id: &RecordId) -> Result<Option<LinkRow>, HexlogError>;
    fn batch_by_key(&self, tenant: &TenantId, key: &IdempotencyKey) -> Result<Option<BatchRow>, HexlogError>;
    fn batch_links(&self, tenant: &TenantId, first_seq: u64, count: u32) -> Result<Vec<LinkRow>, HexlogError>;
    fn current_nodes(&self, tenant: &TenantId, filter: &ListFilter, page: &PageRequest) -> Result<Vec<LinkRow>, HexlogError>;
    fn revisions(&self, tenant: &TenantId, subject: &Subject) -> Result<Vec<LinkRow>, HexlogError>;
    fn edge_tips(&self, tenant: &TenantId, query: &EdgeQuery) -> Result<Vec<LinkRow>, HexlogError>;
    /// Pares das arestas vigentes de `rel` cujos extremos do tenant também estão vigentes (base da detecção de ciclo).
    fn live_edge_pairs(&self, tenant: &TenantId, rel: &RelationName) -> Result<Vec<(SemanticId, SemanticId)>, HexlogError>;
    fn links_from(&self, tenant: &TenantId, from_seq: u64, limit: u32) -> Result<Vec<LinkRow>, HexlogError>;   // verificação
    fn batches(&self, tenant: &TenantId) -> Result<Vec<BatchRow>, HexlogError>;
}

pub struct TenantRow { pub tenant: TenantId, pub anchor: Hash, pub anchor_doc: String, pub created_at: String }
pub struct TipRow { pub record_id: RecordId, pub seq: u64, pub retired: bool, pub type_name: Name, pub def_hash: Hash } // `Name`: a coluna guarda tipo de nó ou nome de relação
pub struct BatchRow { pub key: IdempotencyKey, pub fingerprint: Hash, pub first_seq: u64, pub item_count: u32 }
pub struct LinkRow { pub tenant: TenantId, pub seq: u64, pub id: RecordId, pub prev_hash: Hash, pub hash: Hash, pub body: String, pub columns: LinkColumns }
pub struct LinkColumns {
    pub at: String, pub type_name: Name, pub subject_kind: SubjectKind, pub subject_key: String, pub retired: bool,
    pub revises: Option<RecordId>, pub rel: Option<RelationName>, pub from_id: Option<SemanticId>, pub to_id: Option<SemanticId>,
    pub from_origin: Option<Origin>, pub to_origin: Option<Origin>, pub state: Option<State>,
    pub external_id: Option<ExternalId>, pub def_hash: Hash,
}
pub enum SubjectKind { Node, Edge }
pub struct NewLink { pub seq: u64, pub id: RecordId, pub prev_hash: Hash, pub hash: Hash, pub body: String, pub columns: LinkColumns }
/// Resultado de `decide`: o que gravar (vazio no replay) e o que devolver ao chamador. `AppendOutcome` vem de `engine/model.rs`.
pub struct AppendPlan { pub links: Vec<NewLink>, pub batch: Option<BatchRow>, pub outcome: AppendOutcome }
```

### 4.5 Regras do motor (o que `decide` e `verify` implementam)

**Cadeia por tenant (H:222-226, H:628).**

- `hash = sha256hex(prev_hash + JCS(corpo sem prev_hash))`; `prev_hash` do `seq = 1` é a âncora; `anchor = sha256hex(JCS({"format":"hexlog-chain/1","tenant":T,"created_at":...}))`. O nome do tenant está na âncora e em cada corpo, então tenants com o mesmo conteúdo nunca compartilham hash.
- `seq` contíguo de 1 a n por tenant, imposto pelo trigger de inserção e conferido na verificação.
- **Campos que `verify_chain` confere, exatamente:** a lista `pub const CHECKED_FIELDS: &[&str]` em `engine/verify.rs`, com os mesmos nomes da primeira coluna da tabela de mutações abaixo. Por elo: `seq`, `prev_hash`, `hash`, `body` (legível e canônico), `body.tenant`, `body.seq` e as colunas derivadas `id`, `at`, `type`, `subject_kind`, `subject_key`, `retired`, `revises`, `rel`, `from_id`, `to_id`, `from_origin`, `to_origin`, `state_kind`, `state_id`, `external_id`, `def_hash`. Por tenant: `tenants.anchor`, `tenants.anchor_doc`, `tenants.created_at`. Por lote: `batches.key`, `batches.fingerprint`, `batches.first_seq`, `batches.item_count`, mais linha ausente e linha sobrando. Não é conferido, e portanto não entra no proptest: validade do `data` contra o schema atual (decisão de E5), conteúdo nativo, existência dos extremos das arestas.
- **Ordem de conferência por elo** (a primeira falha dá o motivo daquele elo): `seq` (`DivergingSeq`); `prev_hash` contra o `hash` armazenado do elo anterior e hash recalculado contra o armazenado, ambos `HashMismatch` (um motivo só para `prev_hash`, `hash` e `body` com conteúdo alterado); `body` legível e igual à sua recanonicalização (`InvalidBody`); `body.tenant` e `body.seq` contra o tenant verificado e o `seq` do elo (`ColumnMismatch`); cada coluna derivada contra o valor recalculado do corpo (`ColumnMismatch`). Motivos de quebra (`BreakReason`): `DivergingSeq`, `HashMismatch`, `InvalidBody`, `ColumnMismatch`, `BatchIndexMismatch`, `AnchorMismatch`, `HeadNotFound`. Quebra é resultado, não erro (H:241). Depois de uma quebra a verificação continua do `seq` armazenado seguinte.
- `expected_head`: precisa ser o hash de algum elo válido ou a âncora; senão `HeadNotFound` (H:242). Sem ele, apagar a cauda não é detectável: o teste documenta isso.
- Política de verificação (E6, [DONO]): `Tail` confere o último elo (hash recalculado igual ao armazenado e igual à cabeça) a cada escrita; `Full` roda a verificação completa antes de cada escrita e leitura. Cadeia quebrada na escrita vira `CHAIN_BROKEN` (equivale a `PROCESS_CORRUPTED`/`broken-chain` do Hexlog).

**Tabela de mutações (fonte única de `tests/support/mutations.rs`).** Cadeia de referência de 6 elos: L1 e L2 num lote com `key` `K1` (L1 nó `a.one`, tipo `note`; L2 nó `a.two`, tipo `requires_state`, com `state` e `external_id`), L3 revisão de `a.one`, L4 aresta de `a.one` para um nativo (com `native_catalog`), L5 `Retire` de `a.two`, L6 `Retire` da aresta. O alvo é achado por forma (primeiro elo que tem o campo), não por posição, para o proptest poder anexar nós extras antes. O helper `tamper` (conexão `rusqlite` crua ao arquivo) **remove todos os triggers e liga `PRAGMA foreign_keys = OFF` e `PRAGMA ignore_check_constraints = ON`**; `UNIQUE` e chave primária continuam valendo, e cada valor adulterado abaixo respeita ambos. Todo teste confere que a adulteração alterou exatamente 1 linha antes de verificar.

| Campo (`CHECKED_FIELDS`) | Alvo | Valor adulterado | Motivo | `seq` reportado |
|---|---|---|---|---|
| `seq` | L3 | `seq := 99` | `DivergingSeq` | alvo + 1 |
| `prev_hash` | L3 | 64 zeros | `HashMismatch` | alvo |
| `hash` | L3 | 64 `f` | `HashMismatch` | alvo |
| `body` (conteúdo) | L3 | um valor de `data` trocado, JSON ainda canônico | `HashMismatch` | alvo |
| `id` | L3 | id novo e não usado | `ColumnMismatch` | alvo |
| `at` | L3 | `1999-01-01T00:00:00.000Z` | `ColumnMismatch` | alvo |
| `type` | L3 | `other` | `ColumnMismatch` | alvo |
| `subject_kind` | L3 | `edge` | `ColumnMismatch` | alvo |
| `subject_key` | L3 | `a.other` | `ColumnMismatch` | alvo |
| `retired` | L3 | `1` | `ColumnMismatch` | alvo |
| `revises` | L3 | id técnico válido e não usado | `ColumnMismatch` | alvo |
| `def_hash` | L3 | 64 zeros | `ColumnMismatch` | alvo |
| `rel` | L4 | `rel2` | `ColumnMismatch` | alvo |
| `from_id` | L4 | `a.two` | `ColumnMismatch` | alvo |
| `to_id` | L4 | `n.other` | `ColumnMismatch` | alvo |
| `from_origin` | L4 | `native` | `ColumnMismatch` | alvo |
| `to_origin` | L4 | `tenant` | `ColumnMismatch` | alvo |
| `state_kind` | L2 | `NULL` | `ColumnMismatch` | alvo |
| `state_id` | L2 | 40 zeros | `ColumnMismatch` | alvo |
| `external_id` | L2 | `zzz` | `ColumnMismatch` | alvo |
| `body` (não canônico) | L6 | chaves fora de ordem; **hash recalculado** | `InvalidBody` | alvo |
| `body` (não JSON) | L6 | texto `not json`; **hash recalculado** | `InvalidBody` | alvo |
| `body.tenant` | L6 | `tenant` do corpo `zz`; **hash recalculado** | `ColumnMismatch` | alvo |
| `body.seq` | L6 | `seq` do corpo `7`; **hash recalculado** | `ColumnMismatch` | alvo |
| `tenants.anchor` | — | 64 zeros | `AnchorMismatch` | nenhum |
| `tenants.anchor_doc` | — | documento com outro `created_at`, `anchor` intacto | `AnchorMismatch` | nenhum |
| `tenants.created_at` | — | outro instante | `AnchorMismatch` | nenhum |
| `batches.key` | `K1` | `K9` | `BatchIndexMismatch` | nenhum |
| `batches.fingerprint` | `K1` | 64 zeros | `BatchIndexMismatch` | nenhum |
| `batches.first_seq` | `K1` | `first_seq + 1` | `BatchIndexMismatch` | nenhum |
| `batches.item_count` | `K1` | `3` | `BatchIndexMismatch` | nenhum |
| `batches.row_missing` | `K1` | apagar a linha | `BatchIndexMismatch` | nenhum |
| `batches.row_extra` | — | inserir linha `K8` apontando para L1 | `BatchIndexMismatch` | nenhum |
| `expected_head` (argumento) | — | hash aleatório desconhecido | `HeadNotFound` | nenhum |

"Hash recalculado" usa a função pública `hexlog::chain::hash_link` e vale só para o último elo, para os seguintes não quebrarem. "alvo + 1" na linha `seq` existe porque o elo adulterado muda de posição e quem diverge é o seguinte.

**Id semântico (H:99-107, `src/domain/ids.ts:15-18`).** Segmentos `^[a-z0-9][a-z0-9-]{0,62}$` separados por `.`, no máximo 200 caracteres, sem ponto inicial, final ou duplo. Validador escrito à mão (sem regex), com tabela de aceites e recusas. O id técnico é `<tenant>:<uuidv7>`, atribuído pelo motor, nunca pelo chamador (H:101). Prefixo casa só na fronteira do `.` (`a.b` casa `a.b` e `a.b.c`, não `a.bc`; H:107, `src/domain/gate.ts:61-64`). O mesmo regex vale para tenant, tipo e nome de relação.

**Ordem de recusa da escrita (adaptada de H:305-314; o replay sobe para logo depois de tenant e forma).**

1. Forma do lote: 1 a 50 itens, aliases únicos, `alias` só referido por item posterior, tamanhos, `key` válida (`INVALID_INPUT`).
2. Tenant existe (`TENANT_NOT_FOUND`).
3. **Replay ou conflito por `key`**, antes de qualquer checagem estática ou de estado: a impressão (`fingerprint`) é calculada sobre os itens de entrada como chegaram, e a consulta a `batches` é uma leitura fora da transação de escrita. Mesma impressão: devolve os ids gravados, `replayed: true`, nada novo gravado. Impressão diferente: `IDEMPOTENCY_CONFLICT`. Assim reenviar um lote antigo continua devolvendo `replayed` mesmo depois de uma atualização do plugin que reservou um id ou mudou o catálogo (H:182). A impressão é `sha256hex(JCS(itens de entrada))`, sem `key` e sem `author` (H:180, `src/domain/chain.ts:59-62`). Limite aceito: o replay não verifica a cadeia; quem quer essa garantia chama `verify_chain`.
4. Estáticas, antes da transação: tipo/relação no catálogo (`TYPE_NOT_FOUND`, `RELATION_NOT_FOUND`); `data` válido no schema, com todas as violações do lote em um `INVALID_RECORD` (H:311); `data` com no máximo `data_max_chars` caracteres canônicos; **número em `data` só se for inteiro seguro**; regras de `state` e `external_id` da tabela abaixo; id semântico de nó reservado por `ForeignNodes` (`RESERVED_ID`, C2); autorrelação.
5. Dentro de `BEGIN IMMEDIATE`: cauda da cadeia íntegra (`CHAIN_BROKEN`); **nova consulta de `key`** (outra gravação pode ter vencido a corrida: replay ou conflito, como no passo 3).
6. Dependentes de estado, item a item com sobreposição em memória dos itens anteriores do mesmo lote (H:160): linhagem/`base` (regra abaixo), extremos de aresta existentes e vigentes (no tenant com sobreposição, ou em `ForeignNodes`; ausente é `INVALID_RECORD` `endpoint-not-found`), tipos permitidos pela `RelationDef` (`from`/`to`, H:144), pelo menos um extremo no tenant, aresta entre dois nativos recusada, `acyclic`.

**Números em `data`.** Aceita-se só número que `serde_json` represente como `i64` ou `u64` e que esteja em ±(2^53 − 1). Literal com ponto ou expoente (`1.0`, `1e2`, `-0.0`) chega como `f64` e é recusado, mesmo quando o valor é inteiro (`INVALID_RECORD`, detalhe `number-not-safe-integer`, com o ponteiro). O mesmo vale em qualquer objeto aninhado.

**Regras de `state` e `external_id` (tabela a confirmar no P0; um teste por linha em `state_rules.rs`).**

| # | Item | Tipo/relação | `state` | Resultado |
|---|---|---|---|---|
| S1 | Node (raiz) | `requires_state = true` | ausente | `INVALID_RECORD` `state-required` |
| S2 | Node (raiz) | `requires_state = true` | válido | aceita |
| S3 | Node (raiz) | `requires_state = false` | presente | `INVALID_RECORD` `state-not-allowed` |
| S4 | Node (revisão) | `requires_state = true` | ausente | `state-required` (a revisão não herda o `state` da ponta) |
| S5 | Node (revisão) | `requires_state = false` | presente | `state-not-allowed` |
| S6 | Retire | qualquer tipo, inclusive `requires_state = true` | presente | `state-not-allowed` |
| S7 | Retire | qualquer tipo | ausente | aceita |
| S8 | Edge (raiz ou revisão) | qualquer relação | presente | `state-not-allowed` |
| S9 | qualquer | — | malformado (7, 39, 41 caracteres, maiúsculas, não hex) | `INVALID_RECORD` `state-format` |
| X1 | `external_id` em Node ou Edge | — | 1 a 200 caracteres | aceita |
| X2 | `external_id` em Retire | — | presente | `INVALID_RECORD` `external-id-not-allowed` |
| X3 | `external_id` | — | vazio ou com mais de 200 | `INVALID_RECORD` `external-id-format` |
| X4 | `external_id` em revisão de Node ou Edge | — | omitido na revisão | aceita; o novo elo não tem `external_id` (não é herdado) e `NodeView.external_id` da nova ponta é `None` |

**Regra de um vigente por linhagem (H:636, H:157-159).** Para cada item `Node`/`Edge`/`Retire`, a ponta do sujeito é `tip`. Sem `tip`, `base` precisa ser `None`. Com `tip`, `base` precisa ser exatamente o id da ponta; senão `FORK_REJECTED` com `details[0].current` = id da ponta e `retired` indicando se a ponta é uma aposentadoria (H:158). Revisão mantém o tipo (`INVALID_RECORD`, `type-changed`; H:133). Revisar uma ponta aposentada é permitido e ressuscita o sujeito. `Retire` exige `tip` e `base`. Dois escritores concorrentes sobre a mesma ponta: o segundo `append` relê sob a transação e recebe `FORK_REJECTED` (H:159).

**Aresta e ciclo.** Aresta com extremo aposentado, ou aposentada ela mesma, **não entra** na detecção de ciclo (`live_edge_pairs` já a exclui). A busca de ciclo usa as arestas vivas do tenant, as de `native_edges` e as do próprio lote, no estilo de `src/domain/relations.ts:100-123`. Aresta para extremo que se aposenta depois continua gravada e passa a mostrar `EndpointState::Retired` na leitura.

**Estado (D31).** `State { kind: StateKind, id: String }`, `StateKind` enum fechado e `#[non_exhaustive]`, hoje só `GitCommit` (`"git-commit"`), `id` hexadecimal minúsculo de 40 ou 64 caracteres, sem normalizar e sem conferir existência. Mais um tipo exige: variante nova, validador e a regra de aceitação de P7.

**Equivalent (D15).** `equivalent` em `TypeDef` e `RelationDef`: o `Catalog::new` só valida a sintaxe da URI com o crate `url` e nunca a resolve nem a lê para decidir comportamento (jury-verdict.md, linha 9).

**Catálogo e schema (H:298-303).** JSON Schema 2020-12 com raiz `type: "object"`; `pattern`, `patternProperties` e qualquer `$ref` que não seja interno (`#...`) recusados; `$id` e `$async` recusados. `def_hash = sha256hex(JCS(definição))` gravado em cada elo. A validação de `data` não roda de novo na leitura nem em `verify_chain`.

**`where_eq` (ligação segura).** O nome do campo é um `FieldName` validado (1 a 64 de `[A-Za-z0-9_-]`) e vira o caminho JSON `$.data."<nome>"` montado em Rust e passado como **parâmetro ligado** a `json_extract(body, ?)`; o valor também é parâmetro ligado. O prefixo `$.data.` garante que um nome igual a um campo do envelope (`type`, `state`, `tenant`, `id`) lê só o campo de `data`, nunca o do envelope. Nenhum texto do chamador entra no SQL. Só campo escalar de primeiro nível de `data` (H:337).

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

**Catálogo de erros (H:510-540, H:652-653).** `{code, message, details[{path, code, message}]}` com JSON Pointer em `path`. Códigos do passo 3: `INVALID_INPUT`, `INVALID_RECORD`, `INVALID_SCHEMA` (definição de tipo ou relação recusada em `Catalog::new`, H:517), `TYPE_NOT_FOUND`, `RELATION_NOT_FOUND`, `TENANT_NOT_FOUND`, `RESERVED_ID`, `FORK_REJECTED`, `CYCLE_REJECTED`, `IDEMPOTENCY_CONFLICT`, `CHAIN_BROKEN`, `BUSY` (substitui `LOCK_TIMEOUT`, H:653), `STORE_ERROR` (substitui `IO_ERROR`; detalhes: código do SQLite em minúsculas, `unsupported-schema-version`), `INTERNAL`. `RELATION_NOT_FOUND` vale só para nome de relação ausente do catálogo; extremo de aresta ausente é `INVALID_RECORD` `endpoint-not-found` (diverge de H:538, onde o código cobria o destino ausente). Fora: `LEGACY_DATA`, `PROCESS_TOO_LARGE`, os de anexo e cursor (H:653). Mensagem de erro nunca ecoa valor de `data` nem caminho do banco.

### 4.6 Portão P0 e tarefas ordenadas

**Portão P0 (do dono, antes da T2).** A T2 é a primeira tarefa que grava o formato do elo (hash dourado e fixture), e tudo de T3 em diante constrói sobre ele; por isso o corpo do elo é congelado antes dela. O dono responde:

- item 5 (aresta com origem nativa) e item 10 (`Retire`);
- item 19 (vocabulário estrutural no corpo: `Retire`, `requires_state`, `State`, origens, `native_catalog`, `def_hash`);
- item 1 (a releitura de D31) e P7 (forma do estado);
- item 16 (`alias` e `batch.aliases` no corpo);
- a tabela de regras de `state` e `external_id` (item 11, seção 4.5, linhas S1 a S9 e X1 a X4).

Respostas que cortam um campo ou uma variante encolhem o corpo da seção 4.3 antes de existir dado. Depois do P0, mudar o corpo do elo é mudar o formato (tenant novo, D12). O P0 trava a T2 e tudo depois dela até o dono responder; é a escolha do plano, e a T1 pode rodar antes.

**Grafo de dependências:** T1; T2 (T1, P0); T3 e T4 (cada uma depende de T2, paralelas entre si); T5a (T3, T4); T5b (T5a); T5c (T5b); T5d (T5c); T6 (T5d). Não há tarefa que use algo que nasce em tarefa posterior; as seções "Pronto" abaixo foram conferidas contra a seção 5 uma a uma.

Cada tarefa fecha com `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` limpos. O teste vem junto do código na mesma tarefa (teste vermelho primeiro quando a regra é nova).

**T1. Esqueleto do workspace, travas de arquitetura e spikes** (dependências: nenhuma)

- Arquivos: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `crates/hexlog/Cargo.toml`, `crates/hexlog/src/lib.rs`, `crates/omhl/Cargo.toml`, `crates/omhl/src/main.rs`, `crates/hexlog/tests/architecture.rs`, `crates/hexlog/tests/fixtures/scanner_cases.txt`.
- Entrega: workspace compila; `[workspace.lints]` com `unsafe_code = "forbid"`; `architecture.rs` com os testes de arquitetura da seção 5 (C7) e o teste do próprio varredor, que tratam arquivo ainda inexistente (como `engine/state.rs`) sem falhar. Spikes curtos, descartáveis, cujos resultados vão para os seguimentos do ADR: (a) `serde_json_canonicalizer` contra os vetores do RFC 8785; (b) `rusqlite` `bundled` expõe `STRICT`, `json_extract`, `PRAGMA journal_mode=WAL` e FTS5; (c) `jsonschema` com `default-features = false`: `cargo tree -p hexlog -e normal` não lista nenhum crate da lista proibida, `$ref` externo é recusado e a validação 2020-12 roda sem rede; se a crate precisar de uma feature que traga crate proibido, trocar para `boon` ou outro validador antes de seguir.
- Pronto: os cinco testes de `architecture.rs` verdes (um deles é o do próprio varredor); saída de `cargo tree` sem crate proibido registrada; se (a) reprovar, aplicar E2-C antes de seguir.

**T2. Primitivas de domínio e cadeia pura** (depende de T1 e do P0)

- Arquivos: `src/lib.rs` (reexporta tipos de id, erros, `State`, tipos de `model.rs` e o módulo público `chain`), `src/ids.rs`, `src/error.rs`, `src/engine/{mod,model,state,limits,ports,chain}.rs`, `tests/support/mod.rs` (relógio e ids fixos, `ForeignNodes` fake com lista alterável); testes `tests/{semantic_id,state,jcs_vectors}.rs`, `tests/fixtures/{rfc8785_vectors,golden_links}.json`.
- Entrega: tipos de id com validação, `State`, `Author`, `AppendItem`, `AppendOutcome` e demais tipos de `model.rs`, os contratos de `ports.rs` (só tipos e traits), `canonical`, `sha256hex`, `hash_link`, `anchor`, `fingerprint`, catálogo de erros.
- Fixture dourada: `golden_links.json` guarda 3 documentos de elo (um simples, um com Unicode e um com inteiros nos limites ±(2^53 − 1)), cada um com `prev_hash` e o **hash calculado pelo Hexlog TS** (`sha256hex(prevHash + canonicalize(corpo))`, a mesma conta de `src/domain/chain.ts:64-67`, rodada uma vez em `/home/gabriel/personal/hexlog` no commit `5d1e22b`), com o comando e o commit anotados na própria fixture. O teste Rust recalcula e compara com o valor do TS; nenhum valor dourado nasce da implementação Rust.
- Pronto: `semantic_id` (2 testes), `state` (2 testes) e `jcs_vectors` (3 testes) verdes; os 3 hashes da fixture iguais aos do TS; `architecture` continua verde.

**T3. Catálogo e validação de schema** (depende de T2; paralela à T4)

- Arquivos: `src/schema/{mod,catalog,validate}.rs`, `tests/support/catalog.rs`; testes `tests/catalog.rs`.
- Entrega: `TypeDef`, `RelationDef`, `Catalog::new` com todas as recusas da seção 4.5 e `def_hash`; `validate_data` que devolve todas as violações com ponteiro JSON (limite de 50) e recusa número que não seja inteiro seguro.
- Pronto: os 8 testes de `catalog.rs` verdes.

**T4. Store SQLite** (depende de T2; paralela à T3)

- Arquivos: `src/store/mod.rs`, `src/store/sqlite/{mod,ddl,tx}.rs`, `src/store/sqlite/schema.sql`, `tests/support/tamper.rs`; testes de unidade em módulo `#[cfg(test)]` de `src/store/sqlite/`.
- Entrega: o DDL e os triggers da seção 4.3 em `schema.sql`, `ddl.rs` com `include_str!` e a verificação de `user_version`, `ensure_tenant` idempotente, `append_with` em `BEGIN IMMEDIATE` com `busy_timeout`, `TxRead` completo, `SqliteStore` com `Mutex<Connection>`; `tamper` abre conexão crua que remove os triggers e desliga `foreign_keys` e `ignore_check_constraints` (seção 4.5).
- Pronto: os 9 testes de `sqlite_store` da seção 5 verdes; `architecture` continua verde com o DDL fora do `.rs`.

**T5a. `decide`, nós, revisão, `Retire` de nó, lote, idempotência e Facade básica** (depende de T3 e T4)

- Arquivos: `src/engine/{decide,rules}.rs`, `src/facade.rs`, `src/lib.rs`; testes `tests/{node_lifecycle,state_rules,list_filters,batch,idempotency}.rs`.
- Entrega: `decide` puro sobre `TxRead` com as etapas 1 a 6 da seção 4.5 para itens `Node` e `Retire` de nó, regra de um vigente, tabela de `state` e `external_id` (linhas que não envolvem aresta), política `Tail` na escrita (recalcula o hash do último elo com `hash_link`; a política `Full` só é ligada na T5c), `ensure_tenant`, `append`, `get` (com `shadowed`), `history`, `list` (com `state`, `where_eq`, prefixo e paginação), `head`. Itens `Edge` e `Retire` de aresta devolvem, por enquanto, `INVALID_INPUT` `edge-items-not-supported`, que a T5b remove.
- Pronto: todos os testes dos arquivos listados verdes (seção 5, C1, lote, idempotência e filtros).

**T5b. Arestas, `ForeignNodes` e ciclos** (depende de T5a)

- Arquivos: extensões em `src/engine/{decide,rules}.rs` e `src/facade.rs` (`edges`, `EdgeView`, `EndpointState`); teste `tests/edges.rs`.
- Entrega: validação de extremos (tenant com sobreposição de lote, nativo via `ForeignNodes`), tipos da `RelationDef`, `acyclic` com arestas vivas, nativas e do lote, `Retire` de aresta copiando origens e `native_catalog`, linhas S8 e X1 de aresta; remoção do ramo `edge-items-not-supported`.
- Pronto: todos os testes de `edges.rs` verdes; `grep -rn "edge-items-not-supported" crates/hexlog/src` não encontra nada.

**T5c. `verify_chain`, `CHECKED_FIELDS`, política `Full` e testes de C3** (depende de T5b, porque a cadeia de referência tem aresta e `Retire` de aresta)

- Arquivos: `src/engine/verify.rs`, `src/lib.rs` (reexporta `CHECKED_FIELDS`), ajuste de `src/facade.rs` e `src/engine/decide.rs` para ligar `VerifyPolicy::Full`; `tests/support/mutations.rs` (cadeia de referência e a tabela da seção 4.5); testes `tests/{chain_tamper,chain_properties,tenant_isolation}.rs`.
- Entrega: `verify_chain` com `expected_head` e a ordem de conferência da seção 4.5; `CHECKED_FIELDS`; política `Full` na escrita e na leitura.
- Pronto: todos os testes de `chain_tamper`, `chain_properties` e `tenant_isolation` verdes, inclusive `it_should_have_a_mutation_for_every_checked_field`.

**T5d. Concorrência, política de verificação e `Send + Sync`** (depende de T5c)

- Arquivos: `tests/concurrency.rs`; ajustes em `src/store/sqlite/*` só se a corrida expuser lacuna.
- Entrega: testes com duas instâncias de `Hexlog` (duas conexões, threads) e com uma instância compartilhada; `CHAIN_BROKEN` na escrita com `Tail` e com `Full`; limite do `Tail` documentado em teste.
- Pronto: os 8 testes de `concurrency.rs` verdes e estáveis em 20 repetições seguidas (`cargo test -p hexlog --test concurrency` em laço de 20 execuções). Todos dependem de `verify_chain` e do `tamper`, que já existem.

**T6. Terceiro grafo, travas finais e fechamento** (depende de T5d)

- Arquivos: `tests/third_graph.rs`, `tests/fixtures/third_graph.json`; revisão final de `tests/architecture.rs`.
- Entrega: o terceiro grafo só por dado e o caso do invariante no adaptador; varredura final.
- Pronto: `third_graph` (2 testes) e `architecture` (5 testes) verdes; todos os comandos de verificação da seção 5 limpos; nenhum ramo `unsupported`, `todo` nem `unimplemented` restante (`grep -rnE "todo!|unimplemented!|edge-items-not-supported" crates/hexlog/src` vazio). O formato do elo não muda aqui: o P0 já o congelou.

## 5. Testes e verificação

Nomes são `fn it_should_...` em inglês, completos; um comportamento por teste, estrutura arruma/age/afirma, relógio e gerador de id fixos de `tests/support`. Cada cabeçalho traz a tarefa que entrega os testes; nenhum teste usa algo que só nasce em tarefa posterior.

### C1. Registrar um nó com id semântico e consultá-lo devolve a revisão vigente

**Arquivo `semantic_id` (T2)**

| Teste | Afirma |
|---|---|
| `it_should_accept_valid_semantic_ids` | tabela: `area.sub.item`, `a`, `a.b-c.d1`, 200 caracteres no limite, segmento de 63 |
| `it_should_reject_invalid_semantic_ids` | tabela: vazio, `.a`, `a.`, `a..b`, `A.b`, `a_b`, `-a`, segmento de 64, 201 caracteres, não ASCII, espaço; `INVALID_INPUT` com ponteiro |

**Arquivo `state` (T2, formato)**

| Teste | Afirma |
|---|---|
| `it_should_accept_a_lowercase_hex_state_of_40_or_64_characters` | linha S2 de formato |
| `it_should_reject_a_state_with_7_39_or_41_characters_uppercase_or_non_hex` | linha S9 |

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

**Arquivo `node_lifecycle` (T5a)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_return_the_current_revision_when_getting_by_semantic_id` | tenant `acme`, tipo `note`; grava `area.sub.item` e a revisão 2 com `base` da 1 | `get` devolve a revisão 2 (id técnico, `data`, `seq`) |
| `it_should_list_revisions_in_order_when_reading_history` | as mesmas duas revisões | `history` devolve 2 itens em ordem de `seq`, a 2ª com `revises` = id da 1ª |
| `it_should_return_none_when_the_id_was_never_written` | tenant vazio | `get` devolve `None` |
| `it_should_hide_a_retired_node_and_allow_revising_on_top_of_the_retirement` | grava, aposenta com `Retire`, regrava com `base` da aposentadoria | `get` é `None` após aposentar e volta com o dado novo depois |
| `it_should_copy_type_and_def_hash_from_the_tip_when_retiring` | tipo cujo catálogo muda entre a gravação e a aposentadoria | o elo `Retire` tem o `def_hash` e o `type` da ponta, não os do catálogo novo |
| `it_should_reject_a_second_root_for_an_existing_semantic_id` | nó já gravado | `base: None` dá `FORK_REJECTED` com `current` = id da ponta |
| `it_should_reject_a_revision_with_a_stale_base` | revisões 1 e 2 | revisar de novo com `base` da 1 dá `FORK_REJECTED` |
| `it_should_reject_a_base_when_the_subject_has_no_lineage` | id nunca escrito | `base: Some(..)` dá `INVALID_RECORD` `base-unknown` |
| `it_should_reject_a_revision_that_changes_the_type` | nó do tipo `note` | revisão com outro tipo dá `INVALID_RECORD` `type-changed` |
| `it_should_reject_data_violating_the_schema_and_list_every_violation_of_the_batch` | lote com 2 itens inválidos | um `INVALID_RECORD` com as violações dos dois, ponteiros JSON corretos |
| `it_should_reject_a_reserved_semantic_id` | `ForeignNodes` fake reconhecendo `reserved.example.id` | gravar esse id dá `RESERVED_ID` e nada é gravado (parte do motor de C2) |
| `it_should_mark_a_tenant_node_as_shadowed_when_a_native_gains_the_same_id` | nó gravado; o fake passa a reconhecer o mesmo id | `NodeView.shadowed = true` |
| `it_should_not_echo_data_values_in_error_messages` | `data` inválido com valor distintivo | nenhuma `message` nem `details` contém o valor |

**Arquivo `state_rules` (T5a; linhas da tabela da seção 4.5 que não envolvem aresta)**

| Teste | Linha |
|---|---|
| `it_should_reject_a_node_without_state_when_the_type_requires_it` | S1 |
| `it_should_accept_a_node_with_a_valid_state_when_the_type_requires_it` | S2 |
| `it_should_reject_a_node_with_state_when_the_type_does_not_require_it` | S3 |
| `it_should_require_state_again_on_each_revision_of_a_requires_state_type` | S4 |
| `it_should_reject_state_on_a_revision_of_a_type_without_requires_state` | S5 |
| `it_should_reject_state_on_a_retire_of_a_requires_state_type` | S6 |
| `it_should_accept_a_retire_without_state_for_a_requires_state_type` | S7 |
| `it_should_accept_external_id_on_a_node` | X1 (nó) |
| `it_should_reject_external_id_on_a_retire` | X2 |
| `it_should_reject_external_id_that_is_empty_or_longer_than_200_characters` | X3 |
| `it_should_not_inherit_external_id_when_a_revision_omits_it` | X4 (nó) |

**Arquivo `list_filters` (T5a)**

| Teste | Afirma |
|---|---|
| `it_should_match_prefixes_only_at_dot_boundaries` | ids `a.b`, `a.b.c`, `a.bc`; `list` com prefixo `a.b` devolve os dois primeiros |
| `it_should_filter_nodes_by_state` | dois registros com SHAs diferentes; `list` com `state` devolve só o correspondente |
| `it_should_filter_by_a_top_level_scalar_field` | `where_eq` com texto, inteiro e booleano devolve os registros certos |
| `it_should_match_a_where_field_only_inside_data_even_when_its_name_equals_an_envelope_field` | nó A com `data.type = "x"` e `data.state = "y"`; nó B com `data.type = "z"`; ambos de tipo `note` | `where_eq(type, "x")` devolve só A; `where_eq(type, "note")` devolve vazio (o `type` do envelope não casa) |
| `it_should_reject_a_where_field_name_with_json_path_or_sql_metacharacters` | `a.b`, `a[0]`, `a"b`, `a'b`, vazio e 65 caracteres dão `INVALID_INPUT` |
| `it_should_treat_a_hostile_scalar_value_as_data_and_leave_the_tables_intact` | valor `'; DROP TABLE links; --`: devolve vazio e as tabelas continuam |
| `it_should_page_by_seq_with_a_keyset_cursor` | com 5 nós e `limit` 2, devolve três páginas sem repetir nem pular, `next_after_seq` correto |
| `it_should_reject_a_page_limit_outside_1_to_200` | `limit` 0 e 201 dão `INVALID_INPUT` |

**Arquivos `batch` e `idempotency` (T5a)**

| Teste | Afirma |
|---|---|
| `it_should_persist_nothing_when_any_item_of_the_batch_is_invalid` (`batch`) | com o 3º item inválido, `head` não muda |
| `it_should_reject_empty_and_oversized_batches` (`batch`) | 0 e 51 itens dão `INVALID_INPUT` |
| `it_should_resolve_an_alias_base_to_a_previous_item_of_the_same_batch` (`batch`) | nó e sua revisão no mesmo lote gravam |
| `it_should_reject_a_forward_or_duplicate_alias` (`batch`) | `INVALID_INPUT` |
| `it_should_replay_the_same_key_and_items_without_writing` (`idempotency`) | `replayed: true`, mesmos ids e aliases, `head` igual |
| `it_should_reject_the_same_key_with_different_items` (`idempotency`) | `IDEMPOTENCY_CONFLICT` |
| `it_should_replay_before_state_rules_when_the_base_was_superseded` (`idempotency`) | reenvio depois de nova revisão ainda dá `replayed` (H:182) |
| `it_should_replay_before_static_rules_when_the_id_was_reserved_afterwards` (`idempotency`) | lote gravado; o `ForeignNodes` fake passa a reservar o id; reenvio dá `replayed` e não `RESERVED_ID` |
| `it_should_replay_regardless_of_the_author_and_scope_keys_per_tenant` (`idempotency`) | autor diferente repete; mesma `key` em outro tenant grava |

**Arquivo `jcs_vectors` (T2)**

| Teste | Afirma |
|---|---|
| `it_should_canonicalize_per_rfc_8785_vectors` | `rfc8785_vectors.json`: saída igual à esperada |
| `it_should_match_the_hexlog_ts_hash_for_each_golden_link` | os 3 elos de `golden_links.json` dão o hash calculado pelo TS |
| `it_should_derive_the_fingerprint_without_key_and_author` | mesmos itens com `key` e `author` diferentes, mesma impressão |

### Aresta (arquivo `edges`, T5b)

| Teste | Afirma |
|---|---|
| `it_should_accept_an_edge_to_a_native_node_and_record_its_origin` | `to_origin = native` e `native_catalog` no corpo; `EdgeView.to_state = Current` |
| `it_should_accept_an_edge_whose_source_is_native_and_target_is_tenant` | condicionado ao item 5; recusado com `INVALID_RECORD` `native-source-not-allowed` se o dono disser que não |
| `it_should_reject_an_edge_to_a_missing_node` | `INVALID_RECORD` `endpoint-not-found`, ponteiro no extremo ausente |
| `it_should_reject_an_edge_with_an_unknown_relation_name` | `RELATION_NOT_FOUND` |
| `it_should_reject_an_edge_between_two_native_nodes` | `INVALID_RECORD` `native-only-edge` |
| `it_should_reject_a_self_relation` | `INVALID_RECORD` `self-relation` |
| `it_should_reject_an_edge_whose_endpoint_type_the_relation_forbids` | `INVALID_RECORD` `endpoint-type` |
| `it_should_see_nodes_of_previous_items_when_validating_an_edge_in_the_same_batch` | nó e aresta no mesmo lote gravam |
| `it_should_reject_a_cycle_on_an_acyclic_relation_including_native_edges` | ciclo via aresta nativa fake dá `CYCLE_REJECTED` |
| `it_should_ignore_edges_with_a_retired_endpoint_when_detecting_cycles` | aposentar um extremo desfaz o ciclo para uma aresta nova |
| `it_should_report_retired_endpoint_state_after_the_node_is_retired` | `EdgeView.to_state = Retired` |
| `it_should_report_missing_native_endpoint_state_when_the_native_disappears` | o fake esquece o id; `EdgeView.to_state = MissingNative`, sem erro |
| `it_should_retire_an_edge_using_the_edge_tip_id_as_base` | `base` = `EdgeView.record_id`; depois `edges` sem `include_retired` não a lista |
| `it_should_copy_origins_and_native_catalog_from_the_tip_when_retiring_an_edge` | o elo `Retire` da aresta tem `from_origin`, `to_origin` e `native_catalog` da ponta |
| `it_should_return_inbound_edges_when_querying_by_target` | `edges` com `to` entrega a leitura inversa (`before` de D33) |
| `it_should_reject_state_on_an_edge` | linha S8 |
| `it_should_accept_external_id_on_an_edge_and_not_inherit_it_on_revision` | linhas X1 (aresta) e X4 (aresta) |

### C3. A verificação detecta qualquer registro adulterado; um tenant não depende do outro

**Arquivo `sqlite_store` (T4, testes de unidade em `src/store/sqlite/`)**

| Teste | Afirma |
|---|---|
| `it_should_refuse_updates_and_deletes` | `UPDATE`/`DELETE` em `tenants`, `links` e `batches` abortam |
| `it_should_refuse_out_of_order_seq_and_a_wrong_prev_hash` | inserção direta com `seq` pulado e com `prev_hash` errado aborta |
| `it_should_refuse_a_second_root_for_the_same_subject_at_the_index_level` | duas raízes do mesmo sujeito violam `links_root` |
| `it_should_refuse_a_second_successor_of_the_same_link_at_the_index_level` | dois sucessores do mesmo elo violam `links_successor` |
| `it_should_initialize_an_empty_database_with_user_version_1` | banco novo recebe versão 1 |
| `it_should_refuse_a_database_with_a_newer_user_version` | `user_version = 2` dá `STORE_ERROR` `unsupported-schema-version` |
| `it_should_refuse_a_non_empty_database_with_user_version_zero` | banco estranho com tabelas e versão 0 é recusado |
| `it_should_keep_all_data_after_reopening_the_database` | reabrir devolve o mesmo `head` |
| `it_should_create_tenants_idempotently` | `ensure_tenant` duas vezes: `created` verdadeiro e depois falso, mesma âncora |

**Arquivo `chain_tamper` (T5c; usa a cadeia de referência e a tabela de mutações da seção 4.5)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_verify_a_clean_chain` | cadeia de referência | `ok = true`, `total_records = 6`, `head` igual ao de `head()` |
| `it_should_detect_a_change_in_each_checked_field` | para cada linha da tabela de mutações | aplica o valor adulterado (1 linha alterada), `verify_chain` dá `ok = false`, o motivo e o `seq` da tabela |
| `it_should_have_a_mutation_for_every_checked_field` | `CHECKED_FIELDS` e a tabela | os nomes coincidem, sem sobra nem falta |
| `it_should_detect_a_deleted_middle_link` | apaga L3 | `DivergingSeq` no elo seguinte |
| `it_should_detect_an_inserted_forged_link` | insere elo com hash forjado e `seq` válido no fim | quebra no elo forjado |
| `it_should_not_detect_a_truncated_tail_without_expected_head` | apaga o último elo | `ok = true` (documenta o limite, H:242) |
| `it_should_detect_a_truncated_tail_with_expected_head` | guarda a cabeça antes; apaga o último elo | `HeadNotFound` |
| `it_should_detect_a_rewritten_and_rechained_tail_with_expected_head` | reescreve a cauda recalculando hashes | `HeadNotFound` |

**Arquivo `tenant_isolation` (T5c)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_keep_tenant_a_valid_when_tenant_b_is_tampered` | dois tenants; adultera B | A `ok`, B `ok = false` |
| `it_should_keep_tenant_a_valid_when_every_row_of_tenant_b_is_removed` | apaga todos os elos de B | A `ok` |
| `it_should_derive_different_anchors_for_different_tenants` | A e B criados | âncoras diferentes |
| `it_should_give_different_hashes_to_identical_content_in_different_tenants` | mesmo conteúdo em A e B | hashes diferentes |
| `it_should_scope_semantic_ids_per_tenant` | mesmo id em A e B | dois sujeitos independentes |

**Arquivo `chain_properties` (T5c, `proptest`)**

| Teste | Afirma |
|---|---|
| `it_should_verify_after_any_sequence_of_valid_appends` | sequências aleatórias de lotes válidos: `verify_chain` sempre `ok` |
| `it_should_detect_the_mutation_of_any_row_of_the_table_after_extra_appends` | sorteia uma linha da tabela de mutações e de 0 a 5 nós extras anexados antes da cadeia de referência; `ok = false` e o motivo da linha, sempre |

**Arquivo `concurrency` (T5d)**

| Teste | Afirma |
|---|---|
| `it_should_accept_only_one_of_two_concurrent_revisions_of_the_same_base` | duas instâncias, duas conexões, threads: um `Ok`, um `FORK_REJECTED` (H:571) |
| `it_should_write_once_when_the_same_key_races` | uma gravada, uma `replayed` |
| `it_should_keep_seq_contiguous_when_many_threads_append` | `seq` 1..N, `verify_chain` ok |
| `it_should_be_send_and_sync` | asserção em tempo de compilação sobre `Hexlog` |
| `it_should_serialize_calls_from_threads_sharing_one_instance` | uma `Arc<Hexlog>`, N threads, `seq` 1..N |
| `it_should_fail_with_chain_broken_on_write_when_the_tail_link_is_tampered` | política `Tail`, adultera o último elo: `append` dá `CHAIN_BROKEN` e não grava |
| `it_should_not_notice_a_tampered_middle_link_on_write_under_the_tail_policy` | adultera o elo 2 de 5: `append` grava e `verify_chain` reporta `HashMismatch` (documenta o desvio de H:243) |
| `it_should_fail_with_chain_broken_on_write_for_a_tampered_middle_link_under_the_full_policy` | mesma adulteração com política `Full`: `CHAIN_BROKEN` |

### C7. Um terceiro tipo de grafo não exige mudar o `engine`

**Arquivo `architecture` (T1; o varredor lê só arquivos `.rs`)**

| Teste | Afirma |
|---|---|
| `it_should_not_use_domain_vocabulary_in_identifiers_or_string_literals_of_the_hexlog_sources` | tokeniza todo `crates/hexlog/src/**/*.rs` (inclui `lib.rs`, `error.rs`, `facade.rs`, `ids.rs`); ignora comentários e `#[doc]`; divide identificadores por `snake_case` e `CamelCase` e literais por não alfanuméricos; compara **palavra inteira, sem distinção de caixa**, com a lista: decision, decisão, premise, premissa, directive, diretriz, check, gate, unit, prd, review, omc, omhl, hexflow, claude. A lista mantém `check`. Portagem de `test/no-flow-terms.spec.ts:8-10` (H:86) |
| `it_should_mention_git_only_in_engine_state` | a palavra inteira `git` só aparece em `src/engine/state.rs` (`digit` e `legitimate` não casam) |
| `it_should_flag_forbidden_words_in_identifiers_and_literals_and_ignore_them_in_comments_and_sql_files` | roda o varredor sobre `fixtures/scanner_cases.txt`: acusa a palavra num identificador e num literal Rust (inclusive um literal com `CHECK (`), não acusa em comentário nem em `#[doc]`, não acusa `digit`, e confirma que `include_str!("schema.sql")` só tem o caminho como literal e que o varredor não abre `.sql` (prova os dois lados da trava, como em H:85) |
| `it_should_not_depend_on_forbidden_crates` | `cargo metadata`: o fecho de dependências normais de `hexlog` não contém `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std` |
| `it_should_not_depend_on_omhl` | `omhl` não aparece no fecho de `hexlog` |

**Arquivo `third_graph` (T6)**

| Teste | Arruma | Afirma |
|---|---|---|
| `it_should_accept_a_third_graph_defined_only_by_data` | `fixtures/third_graph.json` com três grafos de nomes neutros (dois com tipos e uma relação `acyclic`; o terceiro, `ticket`, com tipo `requires_state` e relação nova), carregados num `Catalog`, e só a API pública da Facade | grava, consulta, liga por aresta, revisa, aposenta e `verify_chain` dá `ok`; o arquivo do teste não importa nada fora de `hexlog::` |
| `it_should_let_the_adapter_enforce_a_cross_record_invariant_the_engine_does_not_know` | tipo `ticket` com `status` `open`/`closed`; função `transition_allowed` definida dentro do teste, no papel do adaptador | o motor aceita a revisão `closed` → `open`; a função do teste a recusa antes de chamar a Facade |

Observações. (1) A prova de que um grafo novo não toca o `engine` é por construção (o teste só usa a API pública e um arquivo de dados) mais as travas de arquitetura; não há como provar "nunca precisou mudar" além disso. A trava de vocabulário não veta os termos estruturais do item 19; veta termos de domínio. (2) O teste do invariante no adaptador usa uma função local ao teste: ele **documenta o padrão** (o motor não conhece a regra; quem a conhece valida antes) e não exercita código do adaptador real, que só nasce no crate `omhl` do passo 4. (3) A escolha de colocar o DDL em `schema.sql` em vez de tirar `check` da lista é do item "Mudanças da rodada 3" (seção 9).

### Comandos de verificação

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo tree -p hexlog -e normal` (T1: sem crate da lista proibida)
- Por critério: `cargo test -p hexlog --test node_lifecycle --test semantic_id --test state --test catalog --test state_rules --test list_filters --test batch --test idempotency --test edges` (C1 e regras de C2 do motor), `cargo test -p hexlog --lib --test chain_tamper --test chain_properties --test tenant_isolation --test jcs_vectors --test concurrency` (C3; `--lib` roda `sqlite_store`), `cargo test -p hexlog --test third_graph --test architecture` (C7).

## 6. Riscos e mitigações

| # | Risco | Mitigação |
|---|---|---|
| R1 | O JCS do crate escolhido diverge do RFC (número, escape de Unicode, ordem de chaves) e o hash fica incompatível para sempre. | Spike da T1 contra os vetores do RFC; só inteiros seguros em `data` (recusa `1.0` e `1e2`); hashes dourados calculados pelo Hexlog TS e comparados no teste (nenhum valor nasce do Rust); fallback E2-C. |
| R2 | Travar um formato de elo ruim (tudo que entra no corpo fica). | Campos opcionais omitidos (acrescentar não quebra); `v: 1` e `format` na âncora; sem dado real até o passo 4; **o formato é congelado no P0, antes da T2**, com os itens 1, 5, 10, 11, 16 e 19 respondidos. Mudar depois do P0 é tenant novo (D12: sem migração). |
| R3 | Coluna derivada diverge do `body` e a consulta devolve algo que o hash não cobre. | Payload sempre lido de `body`; colunas só filtram; `verify_chain` confere coluna contra corpo (`CHECKED_FIELDS`); tabela de mutações com um teste por campo, proptest sobre a mesma tabela e conferência de que cada adulteração alterou 1 linha. |
| R4 | Concorrência e desempenho: `busy`, WAL em sistema de arquivos errado, `Tail` esconde adulteração no meio. | `BEGIN IMMEDIATE` + `busy_timeout` + erro `BUSY` repetível; testes com conexões e threads separadas, repetidos 20 vezes na T5d; banco fora de `/mnt` no WSL; `VerifyPolicy::Full` nos testes; o desvio de H:243 é [DONO] (E6); medir 5.000 elos (referência de H:556) e reavaliar E6. |
| R5 | O motor perde a neutralidade aos poucos (palavra de domínio, crate de git). | `architecture.rs` na T1, antes de qualquer regra; varredor testado dos dois lados; lista de crates proibidos. |
| R6 | O `State` é confiado ao chamador: a árvore pode estar suja ou o SHA ser de outro repositório. | Dito no ADR e no item 1; a sonda e o teste de árvore suja ficam no passo 5, no adaptador. |
| R7 | Crescimento de escopo (FTS5, projeção, gate, nativos). | Lista "não entra" da seção 4.1; revisão de entrega confere os arquivos contra o layout da seção 4.2. |
| R8 | Decisões do dono ainda abertas mudam o corpo do elo (`Retire`, origem nativa em aresta, `alias`, tabela de `state`). | Portão P0 antes de qualquer código que grave o formato; depois dele o corpo só cresce por campo opcional. |
| R9 | Versões e comportamento das crates não verificados por este plano. | Spikes da T1; `Cargo.lock` versionado; `rust-toolchain.toml` fixo. |
| R10 | Mensagem de erro que vaze conteúdo de `data` ou caminho do banco. | `details` só com ponteiro, código e mensagem fixa; teste `it_should_not_echo_data_values_in_error_messages`. |
| R11 | Skew de versão entre processos: CLI, MCP e hook rodando com `Catalog` ou `ForeignNodes` diferentes (binário velho depois de uma atualização do plugin) gravam sob regras antigas ou reservam ids diferentes. | O `def_hash` por elo e o `native_catalog` das arestas deixam a divergência auditável; `user_version` impede binário velho de abrir banco de esquema novo; o replay antes das regras estáticas evita falsos conflitos. Resíduo aceito até o passo 5, quando o `omhl` compara a versão do plugin com a do binário e recusa gravar em desacordo. |
| R12 | Varredor de vocabulário com falso positivo ou falso negativo. | Tokenização por `proc-macro2` (ignora comentários), palavra inteira sem caixa, teste do próprio varredor com fixture; DDL fora dos `.rs` (`schema.sql` com `include_str!`), de modo que palavra-chave SQL não dispare a trava; se `proc-macro2` não servir, escrever um lexer mínimo (ignorar `//`, `/* */`, `///`). |

## 7. ADR

**Decisão.** Implementar o passo 3 como crate `hexlog` (módulos `engine`, `schema`, `store`) mais um `omhl` mínimo num workspace Cargo; SQLite via `rusqlite` com `bundled` e `Mutex<Connection>` dentro de `Hexlog` (`Send + Sync`); elos com corpo JCS hasheado por sha256 e encadeados por tenant com `seq` contíguo; revisão por `base` explícito com índices únicos parciais; `Retire` como variante do corpo; definições de tipo e relação injetadas por um `Catalog` e fixadas por `def_hash` em cada elo; base nativa reconhecida por uma porta `ForeignNodes`; estado tipado fechado (`GitCommit`) validado só por formato; escrita como `decide` puro dentro de uma transação exclusiva, com replay idempotente logo depois da forma do lote e do tenant; verificação de cauda na escrita e completa sob demanda; DDL em `schema.sql` embutido por `include_str!`; contratos de armazenamento no `engine/ports.rs`; formato do elo congelado no portão P0, antes da T2; ordem das tarefas T1, T2, T3/T4, T5a, T5b, T5c (verificação), T5d (concorrência), T6.

**Drivers.** Integridade verificável sem arrependimento de formato; neutralidade do motor demonstrável por teste; execução por agentes sonnet/haiku com verificação mecânica.

**Alternativas consideradas.** `sqlx`, `diesel`/`sea-orm`, SQLite do sistema (E1); `serde_jcs` e canonicalizador próprio (E2); quatro crates ou um só (E3); revisão implícita ou `supersedes` explícito (E4); definições como registros ou manifesto fixado por tenant (E5); verificação completa em toda leitura (E6).

**Por que esta.** É a combinação que preserva as regras do Hexlog que o mestre manda portar (cadeia, lote, `key`, um vigente por linhagem), cumpre D1 e D11 sem criar crates extras, deixa C7 testável por dado, e torna a bifurcação fisicamente impossível em vez de só verificada. O `Catalog` com `def_hash` evita o processo imutável do Hexlog (H:642) sem inventar bootstrap circular. Congelar o formato antes da T2 custa uma espera do dono e evita construir cinco tarefas sobre um corpo que ainda pode mudar.

**Consequências.**

- O núcleo confia no `State` do chamador; "árvore suja não aprova" (D31) é do adaptador. Isso é uma releitura de D31, marcada [DONO].
- Sem classificação de mudança compatível/quebra de definição (o Hexlog tinha, H:289-293); o histórico é provado pelo `def_hash`, não impedido.
- **Invariantes entre registros ficam no adaptador na v1** (transição aberto→fechado do `unit`, "aprovação exige estado" além da flag `requires_state`, regras de `supports`/`contradicts`). O motor valida cada registro contra o schema do tipo e cada aresta contra a `RelationDef`; nada mais. O `third_graph` mostra o padrão com um caso.
- `data` aceita só inteiros seguros; o campo de nota fracionária precisa de outro desenho (texto ou milésimos).
- `expected_head` é necessário para detectar cauda apagada; quem quiser essa garantia precisa guardar a cabeça fora do banco.
- A política `Tail` é um desvio de H:243 ("falha fechado em toda leitura"): adulteração no meio só aparece no `verify_chain`. [DONO].
- O vocabulário estrutural do corpo do elo (`Retire`, `requires_state`, `State`, origens, `native_catalog`) é mais que o mínimo de D1. [DONO].
- A base nativa não participa da cadeia; arestas para nativos podem ficar penduradas após atualização do plugin (item 2 da seção 2).
- Formato do elo congelado no P0; mudá-lo depois é tenant novo.

**Próximos passos.**

- O dono responde o P0 (itens 1, 5, 10, 11, 16, 19 e P7) para liberar a T2; e, antes dos passos 4 e 6, os itens 3, 6, 7, 8 e 12 a 14, além do desvio de E6.
- Passo 4: grafos de decisões, de checks e de `unit`, base nativa e `ForeignNodes` real, avaliação das quatro perguntas de gate sobre `edges` e `list`, validações de transição no adaptador, medição do texto das instruções contra o teto de `data` (item 17).
- Passo 5: `StateProbe` com `git`, `.omhl.toml`, `render`, adapters e a comparação de versão plugin/binário (R11).
- Passo 6: projeção com o marcador `tenant_head` (item 6).
- Reavaliar E6 e E2 com os números da T1 e da T6.

## 8. Mudanças da rodada 2

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
