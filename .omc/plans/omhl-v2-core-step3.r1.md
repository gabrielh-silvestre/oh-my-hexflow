# Plano executável do passo 3: núcleo `engine` e `store` em Rust

- **Status:** `pending approval` (consenso RALPLAN-DR, modo SHORT, rodada 1 do Planner). Nada aprovado, nada iniciado, nenhum código escrito.
- **Plano mestre:** [omhl-v2-graph-engine.md](omhl-v2-graph-engine.md) (33 decisões do dono, passo 3 da seção 6, critérios de aceite 1, 3 e 7 da seção 4). Este plano não altera o mestre.
- **Apoio:** [hexlog-current-model.md](../research/hexlog-current-model.md), [jury-verdict.md](../research/vocab/jury-verdict.md), [native-base-candidates.md](../research/native-base-candidates.md). Briefing do consenso: [ralplan-brief.md](../drafts/ralplan-brief.md).
- **Convenções de citação:** `H:n` é a linha `n` de `hexlog-current-model.md`; o `caminho:linha` ao lado é o do repositório do Hexlog TypeScript, como o relatório o cita. `D<n>` é a decisão `n` do plano mestre; `C<n>` é o critério de aceite `n`; `P<n>` é o ponto aberto `n` da seção 5 do mestre, na ordem em que ela os lista.
- **Modo SHORT:** não há segredo, pagamento nem migração de dado (D12: recomeça do zero). O custo de errar o formato da cadeia é real, mas é barato até o primeiro tenant de verdade existir (passo 4); por isso o congelamento do formato é um portão explícito ao fim da tarefa T6.

## 1. RALPLAN-DR

### Princípios

1. **O motor conhece estrutura, não significado.** Tipos, relações e regras de domínio chegam como dado (D1, D2, D13). O que o motor conhece de fábrica é pouco: revisão, aposentadoria, aresta, estado tipado (D31), cadeia. Fonte: separação genérico/embutido em H:577-602.
2. **A cadeia é a prova; as colunas do SQLite são índice.** O hash cobre o JSON canônico do elo. Toda coluna usada para filtrar é derivada desse JSON e conferida pelo `verify_chain`. Nenhum dado de payload é lido de coluna.
3. **Nada é reescrito.** Anexar-só é imposto em três camadas: a API não tem atualização, o banco recusa `UPDATE`/`DELETE` e índices únicos impedem bifurcação, e o hash detecta o que escapar das duas primeiras (invariante em H:457).
4. **Um só caminho de escrita.** `decide` é função pura chamada dentro de uma transação `BEGIN IMMEDIATE`; a Facade é a única entrada (D7, H:80, H:655).
5. **Portar regras, não mecanismos.** Levamos as regras do Hexlog que a tabela de H:626-661 manda portar (cadeia, lote, `key`, um vigente) e deixamos para trás lock por diretório, linhas rasgadas, escrita atômica manual e teto de 64 MiB (H:630, H:649-651).

### Decision Drivers (os 3 principais)

1. **Integridade verificável sem arrependimento de formato** (C3). O que entra no corpo hasheado do elo fica para sempre.
2. **Neutralidade do motor demonstrável por teste** (C7, D1, D31): sem vocabulário de domínio e sem dependência de git ou de rede, e sem mudar o `engine` para um grafo novo.
3. **Executável por agentes sonnet/haiku com verificação mecânica:** API síncrona, poucas peças móveis, testes determinísticos (relógio e gerador de id injetados, como em H:78).

### Opções viáveis por escolha técnica

**E1. Crate SQLite**

| Opção | Prós | Contras |
|---|---|---|
| **A. `rusqlite` com feature `bundled` (recomendada)** | Síncrona, que é o desenho da Facade. SQLite embutido dá versão fixa com JSON1 e FTS5 (D9, plano seção 3) e `STRICT`. Controle direto de `BEGIN IMMEDIATE`, triggers e PRAGMAs. | Compila C (precisa de `cc`; `gcc 13.3` está na máquina). Binário maior. |
| B. `sqlx` (sqlite) | Verificação de SQL em tempo de compilação. | Async sem necessidade; exige runtime; macros pedem banco em tempo de build; mais peças para o executor errar. |
| C. `diesel` / `sea-orm` | Modelo tipado. | ORM sobre um esquema de 3 tabelas com triggers e índices parciais atrapalha mais do que ajuda. |
| D. SQLite do sistema (sem `bundled`) | Binário menor. | Versão varia por máquina; sem garantia de `STRICT`, JSON1 e FTS5; testes deixam de ser reproduzíveis. |

**E2. Canonicalização JCS (RFC 8785)**

| Opção | Prós | Contras |
|---|---|---|
| **A. `serde_json_canonicalizer` + vetores do RFC + conferência cruzada com o `hashLink` do Hexlog TS + proibir número não inteiro em `data` (recomendada)** | Mantém o JCS que o plano manda portar (H:628). O guarda de inteiros remove a classe de divergência mais comum (formatação de ponto flutuante). | Crate e versão não verificados contra o RFC por este plano; a tarefa T1 faz o spike. Proibir float limita campos futuros (por exemplo, nota 0.5). |
| B. `serde_jcs` | Mais antigo. | Mesma verificação pendente; menos ativo nas buscas de hoje (0.2.0). |
| C. Canonicalizador próprio sobre `serde_json` | Controle total. | Reimplementar RFC 8785 é risco sem ganho; só se A e B reprovarem nos vetores. |

**E3. Layout do workspace (D11 manda dois crates: `hexlog` e `omhl`)**

| Opção | Prós | Contras |
|---|---|---|
| **A. Dois crates; `hexlog` com módulos `engine`, `schema`, `store` (recomendada)** | Segue D11 literalmente e a tabela da seção 3 do mestre. Fronteira imposta por teste de arquitetura. | Rust não proíbe `use` entre módulos do mesmo crate; a trava é um teste que varre `src/` e confere `cargo metadata`. |
| B. Crates separados (`hexlog-engine`, `hexlog-store`, ...) | O compilador impõe a fronteira. | Contradiz D11 (dois crates) e multiplica manifestos antes de haver segundo consumidor. |
| C. Um crate só | Mais simples. | Perde a separação motor/plugin de D11 e a prova de que o motor não depende do plugin. |

**E4. Modelo de revisão (D6, D21)**

| Opção | Prós | Contras |
|---|---|---|
| **A. Revisão por `base` explícito + índices únicos parciais (recomendada)** | Preserva `FORK_REJECTED` (H:636, H:157-159) com ids semânticos: gravar sobre revisão que não é a ponta é recusado. Bifurcação fica fisicamente impossível. | O agente precisa conhecer a ponta (resposta do `get` traz `id`). |
| B. Revisão implícita, última escrita vence | Mais simples para o agente. | Perde a regra de um vigente por linhagem que o mestre manda portar (mestre seção 3); duas escritas concorrentes se atropelam. |
| C. `supersedes` explícito como no Hexlog | Fiel ao original. | Transforma uma propriedade do motor em relação do usuário, contra D13/D21; e `supersedes` não cruzava processos (H:635), o que não casa com a sobreposição futura. |

**E5. Onde moram as definições de tipo e relação**

| Opção | Prós | Contras |
|---|---|---|
| **A. `Catalog` injetado pelo hospedeiro; cada elo grava o `def_hash` que o validou (recomendada)** | Sem manifesto imutável por processo (H:642 aponta esse problema); a cadeia prova com que definição cada registro foi validado; grafos novos são só dado (C7). | Mudança de definição não é detectada como quebra; ficamos sem a classificação compatível/quebra do Hexlog (H:289-293, que o relatório marca como "mudar"). |
| B. Definições como registros na cadeia | Auditável. | Bootstrap circular (tipo do tipo) e peso desnecessário na v1. |
| C. Manifesto fixado por processo como no Hexlog | Fiel ao original. | Tenant fica imutável a mudanças de tipo; conflita com a base nativa versionada com o plugin (D16). |

**E6. Política de verificação da cadeia**

| Opção | Prós | Contras |
|---|---|---|
| **A. A escrita confere só a cauda; verificação completa em `verify_chain` (recomendada)** | Escrita O(1). Leituras indexadas do SQLite ficam baratas. | Adulteração no meio só aparece no `verify_chain`; o Hexlog "falha fechado" em toda leitura (H:243). |
| B. Verificação completa em toda leitura e escrita | Falha fechado, como o original. | O(n) por chamada; o Hexlog já media orçamento de 500 ms para 5.000 registros (H:556). |

`Config.verify_policy` aceita `Tail` (padrão) e `Full`; `Full` é usado nos testes. Reavaliar com medição (R4).

**Escolhas sem alternativa material:** `sha2` para sha256, `uuid` (v7) para id técnico, `thiserror` para o catálogo de erros, `proptest` para propriedades (H:568), `tempfile` para bancos de teste. Versões vistas no registro hoje (`cargo search`): `rusqlite 0.41.1`, `serde_json_canonicalizer 0.4.1`, `sha2 0.11.1`, `jsonschema 0.60.2`, `uuid 1.29.1`, `thiserror 2.0.21`. Fixar as versões exatas só depois do spike da T1; a documentação dessas crates não foi consultada neste plano.

## 2. Contradições e lacunas entre as decisões

Marcação: **[DONO]** = só o dono decide; o plano traz proposta e o executor segue a proposta como padrão reversível até a resposta. **[CÓDIGO]** = resolvida pelo desenho do passo 3.

1. **[DONO] Estado no núcleo contra motor agnóstico (D31 × D1).** D31 põe o SHA do git no núcleo; D1 diz que o motor não conhece domínio. Proposta: o núcleo conhece só o tipo `State { kind: StateKind, id: String }`, com `StateKind` enum fechado (hoje `GitCommit`) e validação de formato; não invoca git nem importa crate de git. A sonda que lê `HEAD` e checa árvore limpa é uma porta (`StateProbe`) implementada no crate `omhl` (passos 4 e 5). Consequência que o dono precisa aceitar: o núcleo confia no `State` que o chamador entrega e não prova que o SHA pertence ao repositório nem que a árvore estava limpa; "estado com alterações não commitadas não pode ser aprovado" (D31) é imposto pelo adaptador, não pelo motor. O teste de arquitetura permite a palavra `git` somente em `src/engine/state.rs`. A formatação de `GitCommit` é hexadecimal minúsculo de **40 ou 64** caracteres, mais estrita que o `git-sha` do Hexlog (7 a 40, sem normalizar; `src/domain/formats.ts:3-4`), porque abreviação não é "tão reproduzível quanto o git" (D31).
2. **[DONO] "Só adiciona" contra ids semânticos e `externalId` (D17 × D6 × D21).** Quatro lacunas.
   - O motor não conhece a base nativa (D16, fora da cadeia), mas precisa recusar gravação com id semântico nativo (C2). Proposta [CÓDIGO]: porta `ForeignNodes` injetada na Facade; id reservado é o que `lookup` reconhece (casamento exato).
   - Com revisão por id não há o que bifurcar sem um ponto de comparação: o mestre manda portar `FORK_REJECTED` (mestre seção 3) mas não diz como. Proposta [CÓDIGO]: campo `base` (E4-A).
   - `externalId` é "reservado" sem definição. Proposta [CÓDIGO]: coluna opcional `external_id`, texto de 1 a 200 caracteres, sem unicidade, entra no hash quando presente e é omitido quando ausente (assim acrescentar o uso depois não muda o hash dos elos antigos). H:634 diz que o análogo atual é a raiz da linhagem.
   - Atualização do plugin pode introduzir um id nativo que já existe no tenant, ou remover um id nativo ao qual arestas do projeto apontam. Proposta [DONO]: o nativo prevalece e a leitura marca o nó do projeto como `shadowed`; nativos nunca saem dentro de uma versão maior. Fora do passo 3: aqui só se entrega a porta e o teste com fake.
3. **[DONO] Exemplo de sobreposição por id contra "substituição fica para depois" (D16 × D17, candidato 5).** [native-base-candidates.md](../research/native-base-candidates.md) (linha 24) traz a política de commit como "exemplo de sobreposição por id": o projeto sobrepõe o padrão conservador. D17 proíbe alterar ou substituir nativo na v1. Proposta: o candidato 5 entra na base v1 só como diretriz nativa fixa, e o projeto acrescenta uma diretriz própria com outro id; o `render` mostra as duas e o agente resolve a precedência por texto. Alternativa: retirar o candidato 5 da v1. Escolha do dono.
4. **[CÓDIGO] Arestas do projeto para nós nativos (D16 × D20 × D33).** O destino nativo está fora da cadeia. Proposta: `ForeignNodes::lookup` devolve existência e tipo; a escrita valida existência e os tipos permitidos da `RelationDef` (como em H:144 e H:187-208); o elo grava `from_origin`/`to_origin` (`tenant` ou `native`) e a versão do catálogo nativo no momento da escrita (`native_catalog`), informativa e dentro do hash. A leitura devolve `endpoint_state` (`current`, `retired`, `missing_native`) e nunca falha por nativo ausente; `verify_chain` ignora conteúdo nativo. Aresta entre dois nativos é recusada. A detecção de ciclo inclui as arestas internas da base via `ForeignNodes::native_edges(rel)`.
5. **[DONO] "Antes de um nativo" exige aresta com origem nativa (D17 × D20 × D33).** D33 só tem `after`; "P antes de N" é `N after P`, aresta cuja origem é o nativo N. É alterar N? Proposta: a aresta é registro do projeto (D13), não altera o nativo; permitida com pelo menos um extremo no tenant. Se o dono recusar, a restrição é uma linha em `engine/rules.rs` e um teste, antes do congelamento da T6.
6. **[DONO] Projeção só banco→arquivo contra banco global por máquina (D9 × D18 × D26 × D27).** O banco é global do usuário e não está no repositório; a projeção é o único rastro versionado. D27 trata a divergência como "trabalho de outra pessoa", mas o caso real é o mesmo usuário em outra máquina, em clone novo ou com banco apagado: o tenant está vazio, o `post-merge` "regenera a partir do banco" e apaga a projeção, sem caminho de importação. Proposta: o motor expõe `head(tenant)`; a projeção grava um marcador `tenant_head` (seq e hash) junto dos arquivos; a regeneração recusa sobrescrever (avisa, não escreve) quando o marcador dos arquivos não existe na cadeia do banco ou está à frente dela. Isso muda a letra de D27 ("avisa e regenera") só no caso em que regenerar perde dado; o dono decide. Entrega no passo 6; no passo 3 só `head` e `verify_chain` com `expected_head`.
7. **[DONO] Projeção no repositório contra aprovação amarrada ao SHA (D18 × D26 × D30 × D31).** Gerar ou regenerar `.omhl/` altera a árvore: o hook `post-merge` pode deixar a árvore suja, e D31 proíbe aprovar com alterações não commitadas; commitar a projeção troca o SHA e invalida aprovações feitas no SHA anterior. Proposta: ordem de uso projetar, commitar, aprovar; C8 (bytes idênticos para o mesmo estado) evita diferença espúria. Fica registrado como risco; o dono decide se commits de `.omhl/` são esperados antes da aprovação. Nada a fazer no passo 3.
8. **[DONO] Quatro perguntas "como estão" contra herança de linhagem descartada (D24 × H:639).** As perguntas `approved` e `no_pending` do Hexlog têm a opção `inherit` (H:337) apoiada na herança por linhagem (H:164-171), que o relatório manda descartar na v1. Proposta: portar as perguntas sem `inherit` (caminho `inherit: false`, que é o padrão). Fora do passo 3 (a avaliação mora em `graphs/*`); o passo 3 deixa as consultas genéricas que elas precisam (`edges`, `list` com `state`).
9. **[CÓDIGO] Relações como registros contra oito espécies em código (D13 × plano seção 2 × H:129-146, H:637).** Resolução: nenhuma "espécie" fica em código. O motor entende só revisão (`base`), aposentadoria (item 10) e aresta; a semântica de cada relação é dado em `RelationDef` (`from`, `to`, `acyclic`, `equivalent`). `supports`, `contradicts` e `answers`, de que D24 depende, passam a ser nomes definidos pelo catálogo do grafo de decisões (passo 4). `needsReview` e herança ficam fora (item 8). Identidade de uma aresta: a tripla (`rel`, `from`, `to`); a chave de linhagem da aresta é `rel|from|to` (o separador `|` nunca ocorre em nome ou id).
10. **[DONO] Sem desfazer em um modelo "só adiciona" (D17 × D13 × D20).** Uma aresta `after` errada, ou um nó do projeto que deixou de valer, não tem como sair. Revisar uma aresta não resolve (a identidade é a tripla). Proposta: variante `Retire` (aposentadoria = revisão sem dado, `base` obrigatória), que tira o sujeito da vigência sem apagar nada, equivalente ao `revokes` sem sucessor (H:134). Incluir já no passo 3 porque adicionar uma variante ao corpo do elo depois do congelamento muda o formato. Se o dono recusar, remover a variante antes do congelamento da T6.
11. **[DONO] "Refazer o check" implica execução (D30 × D4 × D24 × D28 × mestre seção 2).** Gate avaliável é leitura e cabe em D4; "refazer o check no mesmo estado e obter o mesmo resultado" (D30) supõe executar, fora da v1 pela seção 2 do mestre. Além disso "check" (nó de texto, D14/D22), "gate mecânico" (D28/D30) e "gate" (consulta, D22) se sobrepõem. Proposta de glossário: **check** é o nó de instrução; **gate** é a consulta avaliável; **approval** é um registro com `State` que afirma "este estado passou neste gate". Na v1 aprovação é registro; determinismo é propriedade a verificar quando a execução entrar. Suporte do passo 3: `TypeDef.requires_state` (o tipo declara que o registro exige `State`).
12. **[DONO] O que o gate nativo de PR avalia (D28 × D29 × D24).** Com evidência embutida na decisão (D28) não há nó de evidência para `supports`, e o exemplo `pre-pr` do Hexlog usa `finding` e `answers` (H:350), que não existem na v1. Esboço mínimo para o passo 4: pergunta 1 `occurred` (pelo menos 1 registro `approval` do `unit` cujo `State` é igual ao SHA de fechamento informado pelo chamador); pergunta 2 `no_open_contradiction` sobre as decisões ligadas ao `unit`. As demais perguntas e o significado de "aprovado" são do dono (P6 do mestre). Suporte do passo 3: filtro `state` em `ListFilter`.
13. **[DONO] Fechar o `unit` com gate reprovado (D32 × D4).** D32 diz que o fechamento é quando o gate é avaliado; D4 diz que nada bloqueia. Proposta: o fechamento nunca é recusado na v1; a revisão de fechamento guarda no `data` o resultado do gate e a cabeça da cadeia (`seq`) em que foi avaliado. Passo 4.
14. **[CÓDIGO] O `unit` não tem casa (D3 × D29 × mestre seção 3).** A tabela da seção 3 lista só `graphs/decisions` e `graphs/checks`, e o passo 4 do mestre diz "decisões e de gates". Proposta: `graphs/units` no crate `omhl`; o `unit` é o terceiro grafo natural e serve de fixture do C7. Também, D6 usa o exemplo `gates.before-commit.run-tests` enquanto D3/D22 chamam o grafo e o nó de `check`; propor ids de exemplo sob `checks.`.
15. **[DONO] Texto longo no registro contra teto de `data` (D25 × H:194, H:644).** D25 guarda o texto longo no SQLite "como parte do registro (com hash)"; o teto de `data` é 16.000 caracteres canônicos e os anexos foram descartados (H:644 manda medir antes de portar). Proposta: `Limits.data_max_chars` com padrão 16.000. Começar apertado é seguro porque afrouxar o teto depois é compatível e apertar invalida linha já gravada (H:214). O passo 4 mede os textos reais da base; se passarem, o dono decide entre subir o teto e portar blobs endereçados por hash.
16. **[CÓDIGO] `@alias` sem uso com ids semânticos (mestre seção 3 × D6 × D21).** Arestas apontam ids semânticos, que já são conhecidos ao escrever; `@alias` só serve para referir o id técnico de um item anterior do lote. Proposta: manter `alias` apenas em `base` (revisar no mesmo lote o que acabou de ser gravado). Se o dono preferir, `alias` sai do formato.
17. **[CÓDIGO] "Fonte de verdade de tudo" (D9) contra base nativa fora do banco (D16).** Leitura adotada: o SQLite é a verdade de tudo do tenant; a base nativa é a verdade do binário. Sem consequência de código.
18. **[CÓDIGO] Módulos do mestre contra dois crates (mestre seção 3 × D11).** Resolvido por E3-A: `engine`, `schema`, `store` dentro de `hexlog`; `native`, `graphs/*`, `render`, `project`, `adapters/*` dentro de `omhl`.

## 3. Pontos abertos (seção 5 do plano mestre)

| # | Ponto | Situação | Proposta |
|---|---|---|---|
| P1 | Campos do `.omhl.toml` | Proposta; o dono confirma ao revisar o passo 5 | `version = 1` e `tenant = "<Name>"` (mesma regra de nome do motor: `^[a-z0-9][a-z0-9-]{0,62}$`, herdada de `src/domain/ids.ts:4-5`), mais `[projection] regenerate = "post-merge"`, valores `write`, `on-demand`, `session-start`, `post-merge` (D26). Chaves desconhecidas são erro. Nada mais entra na v1. |
| P2 | Layout de `.omhl/` | Proposta; o dono confirma antes do passo 6 | Uma pasta por tipo de nó e um arquivo por nó: `.omhl/checks/<id>.yaml`, `.omhl/guidelines/<id>.yaml`, `.omhl/gates/<id>.yaml`; textos longos em `.omhl/text/<id>.md` (D25). O nome do arquivo é o id semântico sem transformação: a regra do id (minúsculas, dígitos, hífen e ponto, até 200 caracteres) garante nome seguro em qualquer sistema de arquivos e sem colisão por caixa. |
| P3 | Isolamento entre tenants e mover/renomear projeto | Resolvido no código, com ressalva do item 6 | Toda tabela leva `tenant` como primeira coluna da chave; cada tenant tem âncora e cadeia próprias (o nome do tenant entra na âncora e em cada elo). O tenant vem do `.omhl.toml` (D10, D23), então mover ou renomear a pasta não muda nada. Renomear o tenant no `.omhl.toml` cria outro tenant vazio; não há operação de renomear na v1. Dois clones do mesmo repositório na mesma máquina compartilham o tenant. Testes: T6. A ressalva é a máquina nova ou o banco perdido (item 6). |
| P4 | Teste do `when` bruto | Precisa do dono | É um experimento com sessão real de agente (protocolo em [jury-verdict.md](../research/vocab/jury-verdict.md), linhas 47 e 48), sem código. Fora do passo 3. |
| P5 | Licença do schema.org | Proposta; o dono confirma | A base nativa da v1 não embute nenhum subconjunto do schema.org nem de outro padrão; só guarda URIs em `equivalent` (D15), que são texto de referência, não conteúdo licenciado. Isso evita a verificação de CC BY-SA que ninguém fez (jury-verdict.md, linha 39). |
| P6 | Entidade `unit` e gate nativo de PR | Precisa do dono | Esboço dos campos para o passo 4: `provider`, `repo`, `number` ou `branch`, `status` (`open`/`closed`). O SHA de cabeça vive no `State` do registro `approval`, não no `unit`. Itens 11, 12 e 13 da seção 2 listam o que falta decidir. |
| P7 | Forma do campo de estado | Proposta; o dono confirma | `{kind, id}` com `kind` enum fechado em Rust (D31: tipos novos entram direto no núcleo), serializado como `"git-commit"`. Critério mínimo para aceitar um tipo novo, que sirva de regra escrita: (i) o `id` é endereçado pelo conteúdo; (ii) é imutável depois de criado; (iii) uma sonda local, sem rede, o obtém de forma determinística; (iv) a sonda consegue declarar "não reproduzível" (como árvore suja) e então o estado é recusado. Validação do SHA: item 1 da seção 2. |

## 4. Plano executável do passo 3

### 4.1 Escopo

**Entra:** workspace Cargo; crate `hexlog` com `engine`, `schema` e `store` (SQLite); Facade; cadeia por tenant; id semântico; revisão com `base`; aresta, aposentadoria, estado tipado; lote atômico; idempotência; verificação da cadeia; testes dos critérios 1, 3 e 7 (e das regras de C2 que dependem só do motor: o ganho de reservar id).

**Não entra (passos seguintes):** FTS5, cursor com marcador e `changesSince`, anexos e blobs, telemetria, avaliação de `gate`, base nativa, grafos de decisões, de checks e de `unit`, `render`, `.omhl.toml`, projeção, adapters CLI, MCP e hook. O crate `omhl` recebe apenas um `main` mínimo que confirma que o workspace compila e que a dependência vai de `omhl` para `hexlog`, nunca o contrário.

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
        lib.rs                     # reexporta só a Facade e os tipos públicos
        facade.rs                  # Hexlog, Config, Deps, requests e views
        error.rs                   # ErrorCode, HexlogError, Detail
        ids.rs                     # Name, TenantId, SemanticId, TypeName, RelationName, RecordId, Hash, Alias, IdempotencyKey, ExternalId
        engine/
          mod.rs
          model.rs                 # AppendItem, ItemBody, Subject, BaseRef, Author, Link, LinkBody, BatchMeta
          state.rs                 # State, StateKind (único lugar que pode citar "git")
          limits.rs                # Limits e constantes
          ports.rs                 # Clock, IdGenerator, ForeignNodes, TxRead
          chain.rs                 # canonical, sha256hex, hash_link, anchor, fingerprint
          decide.rs                # decide puro
          rules.rs                 # regras de revisão, aresta, ciclo, reservado
          verify.rs                # verify_chain
        schema/
          mod.rs
          catalog.rs               # TypeDef, RelationDef, Catalog, validação de definições
          validate.rs              # validação de data contra o JSON Schema
        store/
          mod.rs                   # trait Store, NewLink, Decision
          sqlite/
            mod.rs                 # SqliteStore, abertura, PRAGMAs
            ddl.rs                 # DDL e migração por user_version
            tx.rs                  # transação de escrita, TxRead, leituras
      tests/
        support/mod.rs             # relógio e ids fixos, ForeignNodes fake, catálogo de teste, helper de adulteração
        semantic_id.rs  state.rs  node_lifecycle.rs  edges.rs  batch.rs  idempotency.rs  concurrency.rs
        jcs_vectors.rs  chain_tamper.rs  chain_properties.rs  tenant_isolation.rs  sqlite_append_only.rs
        third_graph.rs  architecture.rs
        fixtures/third_graph.json  fixtures/rfc8785_vectors.json
    omhl/
      Cargo.toml                   # depende de hexlog
      src/main.rs                  # --version apenas
```

Dependências de `hexlog` (lista fechada, conferida por `architecture.rs`): `serde`, `serde_json`, `serde_json_canonicalizer`, `sha2`, `rusqlite` (`bundled`), `jsonschema`, `uuid` (`v7`), `thiserror`, `time`, `url` (só valida a sintaxe de `equivalent`, D15). Dev: `proptest`, `tempfile`. Nenhum crate de git, rede, async ou regex vindo de schema (H:299: `pattern` é recusado).

### 4.3 Esquema SQLite

Arquivo em `$XDG_DATA_HOME/omhl/omhl.db` (senão `~/.local/share/omhl/omhl.db`), espelhando `src/directory.ts:5-16` (H:465); o caminho vem da `Config`, nunca de variável global dentro do motor. `PRAGMA`s na abertura: `journal_mode=WAL`, `synchronous=FULL` (equivale ao `fsync` por lote, H:483), `foreign_keys=ON`, `busy_timeout` configurável (padrão 5.000 ms), `user_version=1`. Banco em disco de Windows montado no WSL não é suportado (locks); documentar.

```sql
CREATE TABLE tenants (
  tenant     TEXT PRIMARY KEY,
  anchor     TEXT NOT NULL,            -- sha256hex(JCS(anchor_doc))
  anchor_doc TEXT NOT NULL,            -- JCS de {"format":"omhl-chain/1","tenant":...,"created_at":...}
  created_at TEXT NOT NULL
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
 "type":"check","subject":{"kind":"node","id":"checks.project.lint"},
 "revises":"acme:<uuidv7>","retire":false,"data":{},
 "author":{"agent":"...","client":"cli"},
 "state":{"kind":"git-commit","id":"<40 ou 64 hex>"},"external_id":"...","def_hash":"<64 hex>",
 "batch":{"fingerprint":"<64 hex>","key":"...","count":2,"aliases":{"a":0}}}
```

Regras do corpo: campo opcional ausente é omitido, nunca `null` (acrescentar campo opcional depois não muda hash antigo); `batch` só no primeiro elo do lote (H:188); aresta troca `subject` por `{"kind":"edge","rel":..,"from":..,"to":..,"from_origin":..,"to_origin":..}` e acrescenta `native_catalog` quando algum extremo é nativo; aposentadoria leva `"retire":true`, sem `data`, e copia `type` da ponta anterior.

### 4.4 Facade pública (assinaturas, sem implementação)

```rust
// crates/hexlog/src/facade.rs
pub struct Hexlog { /* privado */ }

pub struct Config {
    pub db_path: std::path::PathBuf,
    pub limits: Limits,                     // padrões da seção 4.6
    pub busy_timeout: std::time::Duration,  // padrão 5 s
    pub verify_policy: VerifyPolicy,        // Tail (padrão) | Full
}
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
    pub fn list(&self, tenant: &TenantId, filter: &ListFilter, page: PageRequest) -> Result<Page<NodeView>, HexlogError>;
    pub fn edges(&self, tenant: &TenantId, query: &EdgeQuery) -> Result<Vec<EdgeView>, HexlogError>;
    pub fn head(&self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    pub fn verify_chain(&self, tenant: &TenantId, expected_head: Option<&Hash>) -> Result<ChainReport, HexlogError>;
    pub fn catalog(&self) -> &Catalog;
}

pub struct AppendRequest { pub tenant: TenantId, pub author: Author, pub key: Option<IdempotencyKey>, pub items: Vec<AppendItem> } // 1..=50
pub struct AppendItem {
    pub alias: Option<Alias>,
    pub body: ItemBody,
    pub base: Option<BaseRef>,            // obrigatória se o sujeito já tem linhagem; proibida se não tem
    pub state: Option<State>,             // obrigatória quando TypeDef.requires_state
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
pub struct AppendOutcome { pub records: Vec<AppendedRecord>, pub replayed: bool, pub head: ChainHead }
pub struct AppendedRecord { pub alias: Option<Alias>, pub id: RecordId, pub seq: u64 }
pub struct NodeView { pub id: SemanticId, pub type_name: TypeName, pub record_id: RecordId, pub seq: u64, pub at: String, pub data: serde_json::Value, pub state: Option<State>, pub external_id: Option<ExternalId>, pub author: Author }
pub struct ListFilter { pub type_name: Option<TypeName>, pub id_prefix: Option<String>, pub state: Option<State>, pub where_eq: Vec<(String, serde_json::Value)>, pub include_retired: bool }
pub struct EdgeQuery { pub rel: Option<RelationName>, pub from: Option<SemanticId>, pub to: Option<SemanticId> }  // consulta pelo extremo `to` entrega a leitura inversa (D33: `before`)
pub struct ChainHead { pub seq: u64, pub hash: Hash }                   // seq 0 e hash = âncora para tenant vazio
pub struct ChainReport { pub ok: bool, pub total_records: u64, pub head: Hash, pub breaks: Vec<ChainBreak>, pub total_breaks: u64 }  // breaks limitado a 100 (H:240)
```

Portas e catálogo (também em `facade.rs`/`engine/ports.rs`/`schema/catalog.rs`):

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

// crates/hexlog/src/store/mod.rs
pub trait Store: Send + Sync {
    fn ensure_tenant(&self, tenant: &TenantId, anchor_doc: &str, anchor: &Hash, created_at: &str) -> Result<TenantRow, HexlogError>;
    /// Abre transação exclusiva, entrega leitura transacional a `decide` e anexa o resultado de forma atômica.
    fn append_with(&self, tenant: &TenantId, decide: &mut dyn FnMut(&dyn TxRead) -> Result<Decision, HexlogError>) -> Result<Decision, HexlogError>;
    fn read(&self) -> Result<Box<dyn TxRead + '_>, HexlogError>;
}
pub trait TxRead {
    fn head(&self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    fn tip(&self, tenant: &TenantId, subject: &Subject) -> Result<Option<TipRow>, HexlogError>;
    fn batch_by_key(&self, tenant: &TenantId, key: &IdempotencyKey) -> Result<Option<BatchRow>, HexlogError>;
    fn edges_of(&self, tenant: &TenantId, rel: &RelationName) -> Result<Vec<(SemanticId, SemanticId)>, HexlogError>; // vigentes
    /* leituras de get, list, history, edges e das linhas para verify */
}
```

### 4.5 Regras do motor (o que `decide` e `verify` implementam)

**Cadeia por tenant (H:222-226, H:628).**

- `hash = sha256hex(prev_hash + JCS(corpo sem prev_hash))`; `prev_hash` do `seq = 1` é a âncora; `anchor = sha256hex(JCS({"format":"omhl-chain/1","tenant":T,"created_at":...}))`. O nome do tenant está na âncora e em cada corpo, então tenants com o mesmo conteúdo nunca compartilham hash.
- `seq` contíguo de 1 a n por tenant, imposto pelo trigger de inserção e conferido na verificação.
- `verify_chain` percorre os elos em ordem e confere, por elo: `seq` esperado, `prev_hash` igual ao `hash` armazenado do elo anterior, hash recalculado igual ao armazenado, e colunas derivadas iguais às recalculadas a partir do `body` (inclusive os índices de `batches`). Motivos de quebra: `diverging-seq`, `hash-mismatch`, `invalid-body`, `column-mismatch`, `batch-index-mismatch`, `anchor-mismatch`, `head-not-found`. Quebra é resultado, não erro (H:241). Depois de uma quebra a verificação continua do `seq` armazenado seguinte.
- `expected_head`: precisa ser o hash de algum elo válido ou a âncora; senão `head-not-found` (H:242). Sem ele, apagar a cauda não é detectável: o teste documenta isso.
- Política de verificação: `Tail` confere o último elo (hash recalculado igual ao armazenado e igual à cabeça) a cada escrita; `Full` roda a verificação completa antes de cada escrita e leitura. `PROCESS_CORRUPTED` vira `CHAIN_BROKEN`.

**Id semântico (H:99-107, `src/domain/ids.ts:15-18`).** Segmentos `^[a-z0-9][a-z0-9-]{0,62}$` separados por `.`, no máximo 200 caracteres, sem ponto inicial, final ou duplo. Validador escrito à mão (sem regex), com tabela de aceites e recusas. O id técnico é `<tenant>:<uuidv7>`, atribuído pelo motor, nunca pelo chamador (H:101). Prefixo casa só na fronteira do `.` (`a.b` casa `a.b` e `a.b.c`, não `a.bc`; H:107, `src/domain/gate.ts:61-64`). O mesmo regex vale para tenant, tipo e nome de relação.

**Ordem de recusa da escrita (adaptada de H:305-314).**

1. Forma do lote: 1 a 50 itens, aliases únicos, `alias` só referido por item posterior, tamanhos (`INVALID_INPUT`).
2. Tenant existe (`TENANT_NOT_FOUND`).
3. Estáticas, antes da transação: tipo/relação no catálogo (`TYPE_NOT_FOUND`, `RELATION_NOT_FOUND`); `data` válido no schema, com todas as violações do lote em um `INVALID_RECORD` (H:311), `data` com no máximo `data_max_chars` caracteres canônicos, números só inteiros seguros (±2^53); `State` presente quando `requires_state`, formato válido; id semântico de nó reservado por `ForeignNodes` (`RESERVED_ID`, C2); autorrelação.
4. Dentro de `BEGIN IMMEDIATE`: cauda da cadeia íntegra (`CHAIN_BROKEN`).
5. `key`: replay (mesma impressão: devolve os ids gravados, `replayed: true`, nada novo gravado) ou `IDEMPOTENCY_CONFLICT`. **O replay vem antes das regras que dependem de estado**, de modo que reenviar um lote cujo `base` já foi superado ainda devolve `replayed` (H:182). A impressão é `sha256hex(JCS(itens de entrada))`, sem `key` e sem `author` (H:180, `src/domain/chain.ts:59-62`).
6. Dependentes de estado, item a item com sobreposição em memória dos itens anteriores do mesmo lote (H:160): linhagem/`base` (regra abaixo), extremos de aresta existentes e vigentes (no tenant com sobreposição, ou em `ForeignNodes`), tipos permitidos pela `RelationDef` (`from`/`to`, H:144), pelo menos um extremo no tenant, aresta entre dois nativos recusada, `acyclic` (busca de ciclo com as arestas vigentes do tenant mais as de `native_edges`, mais as do lote, no estilo de `src/domain/relations.ts:100-123`).

**Regra de um vigente por linhagem (H:636, H:157-159).** Para cada item `Node`/`Edge`/`Retire`, a ponta do sujeito é `tip`. Sem `tip`, `base` precisa ser `None`. Com `tip`, `base` precisa ser exatamente o id da ponta; senão `FORK_REJECTED` com `details[0].current` = id da ponta e `retired` indicando se a ponta é uma aposentadoria (H:158). Revisão mantém o tipo (`INVALID_RECORD`, `type-changed`; H:133). Revisar uma ponta aposentada é permitido e ressuscita o sujeito. `Retire` exige `tip` e `base`. Dois escritores concorrentes sobre a mesma ponta: o segundo `append` relê sob a transação e recebe `FORK_REJECTED` (H:159).

**Estado (D31).** `State { kind: StateKind, id: String }`, `StateKind` enum fechado e `#[non_exhaustive]`, hoje só `GitCommit` (`"git-commit"`), `id` hexadecimal minúsculo de 40 ou 64 caracteres, sem normalizar e sem conferir existência. Mais um tipo exige: variante nova, validador e a regra de aceitação de P7.

**Equivalent (D15).** `equivalent` em `TypeDef` e `RelationDef`: o `Catalog::new` só valida a sintaxe da URI com o crate `url` e nunca a resolve nem a lê para decidir comportamento (jury-verdict.md, linha 9).

**Catálogo e schema (H:298-303).** JSON Schema 2020-12 com raiz `type: "object"`; `pattern`, `patternProperties` e qualquer `$ref` que não seja interno (`#...`) recusados; `$id` e `$async` recusados. `def_hash = sha256hex(JCS(definição))` gravado em cada elo. A validação de `data` não roda de novo na leitura nem em `verify_chain`.

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

**Catálogo de erros (H:510-540, H:652-653).** `{code, message, details[{path, code, message}]}` com JSON Pointer em `path`. Códigos do passo 3: `INVALID_INPUT`, `INVALID_RECORD`, `TYPE_NOT_FOUND`, `RELATION_NOT_FOUND`, `TENANT_NOT_FOUND`, `RESERVED_ID`, `FORK_REJECTED`, `CYCLE_REJECTED`, `IDEMPOTENCY_CONFLICT`, `CHAIN_BROKEN`, `BUSY` (substitui `LOCK_TIMEOUT`, H:653), `STORE_ERROR` (substitui `IO_ERROR`, só o código do erro do SQLite em minúsculas), `INTERNAL`. Fora: `LEGACY_DATA`, `PROCESS_TOO_LARGE`, os de anexo e cursor (H:653).

### 4.6 Tarefas ordenadas

Cada tarefa fecha com `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` limpos. O teste vem junto do código na mesma tarefa (testes de vermelho primeiro quando a regra é nova).

**T1. Esqueleto do workspace, travas de arquitetura e spikes** (dependências: nenhuma)

- Arquivos: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `crates/hexlog/Cargo.toml`, `crates/hexlog/src/lib.rs`, `crates/omhl/Cargo.toml`, `crates/omhl/src/main.rs`, `crates/hexlog/tests/architecture.rs`.
- Entrega: workspace compila; `[workspace.lints]` com `unsafe_code = "forbid"`; `architecture.rs` com os três testes de C7 (vocabulário proibido, lista fechada de dependências via `cargo metadata`, `hexlog` sem caminho para `omhl`). Spikes curtos, descartáveis, cujos resultados vão para a seção de seguimento do ADR: (a) `serde_json_canonicalizer` contra os vetores do RFC 8785; (b) `rusqlite` `bundled` expõe `STRICT`, `json_extract`, `PRAGMA journal_mode=WAL` e FTS5; (c) `jsonschema` recusa `$ref` externo e roda 2020-12 sem rede.
- Pronto: `cargo test --workspace` verde com os três testes de arquitetura; se (a) reprovar, aplicar E2-C antes de seguir; resultado dos spikes anotado.

**T2. Primitivas de domínio e cadeia pura** (depende de T1)

- Arquivos: `src/ids.rs`, `src/error.rs`, `src/engine/{mod,model,state,limits,ports,chain}.rs`; testes `tests/{semantic_id,state,jcs_vectors}.rs`, `tests/fixtures/rfc8785_vectors.json`.
- Entrega: tipos de id com validação, `State`, `Author`, modelos de elo, `canonical`, `sha256hex`, `hash_link`, `anchor`, `fingerprint`, catálogo de erros, portas.
- Pronto: tabelas de id e de estado verdes; vetores do RFC verdes; um elo fixo tem hash dourado congelado em teste. Conferência única, fora do CI: o mesmo documento passado pelo `hashLink` do Hexlog TS (`src/domain/chain.ts:64-67`, `/home/gabriel/personal/hexlog`) dá o mesmo hash.

**T3. Catálogo e validação de schema** (depende de T2; paralela à T4)

- Arquivos: `src/schema/{mod,catalog,validate}.rs`; testes dentro do próprio módulo e em `tests/node_lifecycle.rs` (casos de schema).
- Entrega: `TypeDef`, `RelationDef`, `Catalog::new` com todas as recusas da seção 4.5 e `def_hash`; `validate_data` que devolve todas as violações com ponteiro JSON (limite de 50).
- Pronto: catálogo com `pattern`, `$ref` externo, raiz não objeto ou `equivalent` malformado é recusado; `def_hash` estável entre execuções; dado inválido lista todas as violações.

**T4. Store SQLite** (depende de T2; paralela à T3)

- Arquivos: `src/store/{mod}.rs`, `src/store/sqlite/{mod,ddl,tx}.rs`; testes `tests/sqlite_append_only.rs`.
- Entrega: DDL e triggers da seção 4.3, migração por `user_version`, `ensure_tenant` idempotente, `append_with` em `BEGIN IMMEDIATE` com `busy_timeout`, `TxRead` e leituras por id, prefixo, estado, aresta e `history`; helper `tamper` em `tests/support` (conexão crua que remove os triggers antes de adulterar, como o `tamper` do Hexlog, H:552).
- Pronto: `UPDATE`, `DELETE` e `INSERT` fora de ordem de `seq` ou com `prev_hash` errado abortam; dois tenants coexistem; reabrir o banco preserva tudo.

**T5. `decide`, regras e Facade** (depende de T2, T3, T4)

- Arquivos: `src/engine/{decide,rules}.rs`, `src/facade.rs`, `src/lib.rs`; testes `tests/{node_lifecycle,edges,batch,idempotency,concurrency}.rs`, `tests/support/mod.rs`.
- Entrega: `decide` puro sobre `TxRead` implementando a ordem de recusa e as regras da seção 4.5; Facade com todos os métodos da seção 4.4; `Hexlog` é `Send + Sync`.
- Pronto: todos os testes de C1 da seção 5 verdes, mais os de lote, idempotência, aresta e concorrência.

**T6. Verificação da cadeia, C3, C7 e congelamento do formato** (depende de T5)

- Arquivos: `src/engine/verify.rs`; testes `tests/{chain_tamper,chain_properties,tenant_isolation,third_graph}.rs`, `tests/fixtures/third_graph.json`; ajuste final de `tests/architecture.rs`.
- Entrega: `verify_chain` com `expected_head`; propriedades; isolamento; terceiro grafo só por dado.
- Pronto: todos os testes de C3 e C7 da seção 5 verdes. **Portão de congelamento:** o dono responde aos itens 5 e 10 da seção 2 (aresta com origem nativa e `Retire`) e confirma o corpo do elo da seção 4.3 antes do passo 4; a partir daí, mudar o corpo do elo é mudar o formato.

## 5. Testes e verificação

Nomes são `fn it_should_...` em inglês; um comportamento por teste, estrutura arruma/age/afirma, relógio e gerador de id fixos de `tests/support`.

### C1. Registrar um nó com id semântico e consultá-lo devolve a revisão vigente

| Teste (arquivo) | Arruma | Afirma |
|---|---|---|
| `it_should_return_the_current_revision_when_getting_by_semantic_id` (`node_lifecycle`) | tenant `acme`, tipo `note`; grava `checks.project.lint` e a revisão 2 com `base` da 1 | `get` devolve a revisão 2 (id técnico, `data`, `seq`) |
| `it_should_list_revisions_in_order_when_reading_history` | as mesmas duas revisões | `history` devolve 2 itens em ordem de `seq` |
| `it_should_return_none_when_the_id_was_never_written` | tenant vazio | `get` devolve `None` |
| `it_should_hide_a_retired_node_and_allow_revising_on_top_of_the_retirement` | grava, aposenta com `Retire`, regrava com `base` da aposentadoria | `get` é `None` após aposentar e volta com o dado novo depois |
| `it_should_reject_a_second_root_for_an_existing_semantic_id` | nó já gravado | `base: None` dá `FORK_REJECTED` com `current` = id da ponta |
| `it_should_reject_a_revision_with_a_stale_base` | revisões 1 e 2 | revisar de novo com `base` da 1 dá `FORK_REJECTED` |
| `it_should_reject_a_revision_that_changes_the_type` | nó do tipo `note` | revisão com outro tipo dá `INVALID_RECORD` `type-changed` |
| `it_should_reject_data_violating_the_schema_and_list_every_violation_of_the_batch` | lote com 2 itens inválidos | um `INVALID_RECORD` com as violações dos dois, ponteiros JSON corretos |
| `it_should_reject_a_reserved_semantic_id` | `ForeignNodes` fake reconhecendo `checks.native.example` | gravar esse id dá `RESERVED_ID` e nada é gravado (parte do motor de C2) |
| `it_should_accept_valid_semantic_ids` / `it_should_reject_invalid_semantic_ids` (`semantic_id`) | tabelas: `gates.before-commit.run-tests`, `a`, 200 caracteres no limite, segmento de 63; vazio, `.a`, `a.`, `a..b`, `A.b`, `a_b`, `-a`, segmento de 64, 201 caracteres, não ASCII, espaço | aceita e recusa conforme a tabela, com `INVALID_INPUT` e ponteiro |
| `it_should_match_prefixes_only_at_dot_boundaries` | ids `a.b`, `a.b.c`, `a.bc` | `list` com prefixo `a.b` devolve os dois primeiros |
| `it_should_require_state_when_the_type_demands_it` / `it_should_validate_state_format` (`state`) | tipo com `requires_state` | sem `State` recusa; aceita 40 e 64 hex minúsculos; recusa 7, 39, 41, maiúsculas e não hex |
| `it_should_filter_nodes_by_state` | dois registros com SHAs diferentes | `list` com `state` devolve só o correspondente |

Lote, idempotência, aresta e concorrência (dão base a C1, C2 e ao passo 4):

| Teste | Afirma |
|---|---|
| `it_should_persist_nothing_when_any_item_of_the_batch_is_invalid` (`batch`) | com o 3º item inválido, `head` não muda |
| `it_should_reject_empty_and_oversized_batches` | 0 e 51 itens dão `INVALID_INPUT` |
| `it_should_resolve_an_alias_base_to_a_previous_item_of_the_same_batch` | nó e sua revisão no mesmo lote gravam |
| `it_should_reject_a_forward_or_duplicate_alias` | `INVALID_INPUT` |
| `it_should_replay_the_same_key_and_items_without_writing` (`idempotency`) | `replayed: true`, mesmos ids, `head` igual |
| `it_should_reject_the_same_key_with_different_items` | `IDEMPOTENCY_CONFLICT` |
| `it_should_replay_before_state_rules_when_the_base_was_superseded` | reenvio depois de nova revisão ainda dá `replayed` (H:182) |
| `it_should_replay_regardless_of_the_author_and_scope_keys_per_tenant` | autor diferente repete; mesma `key` em outro tenant grava |
| `it_should_accept_an_edge_to_a_native_node_and_record_its_origin` (`edges`) | `to_origin = native` e `native_catalog` no corpo |
| `it_should_reject_an_edge_to_a_missing_node`, `..._between_two_native_nodes`, `..._a_self_relation`, `..._endpoint_types_the_relation_forbids` | `RELATION_NOT_FOUND`/`INVALID_RECORD` conforme o caso |
| `it_should_see_nodes_of_previous_items_when_validating_an_edge_in_the_same_batch` | nó e aresta no mesmo lote gravam |
| `it_should_reject_a_cycle_on_an_acyclic_relation_including_native_edges` | ciclo via aresta nativa fake dá `CYCLE_REJECTED` |
| `it_should_return_inbound_edges_when_querying_by_target` | `edges` com `to` entrega a leitura inversa (`before` de D33) |
| `it_should_accept_only_one_of_two_concurrent_revisions_of_the_same_base` (`concurrency`) | duas instâncias, duas conexões, threads: um `Ok`, um `FORK_REJECTED` (H:571) |
| `it_should_write_once_when_the_same_key_races` | uma gravada, uma `replayed` |
| `it_should_keep_seq_contiguous_when_many_threads_append` | `seq` 1..N, `verify_chain` ok |

### C3. A verificação detecta qualquer registro adulterado; um tenant não depende do outro

| Teste (arquivo) | Arruma | Afirma |
|---|---|---|
| `it_should_verify_a_clean_chain` (`chain_tamper`) | 5 elos | `ok = true`, `total_records = 5` |
| `it_should_detect_a_tampered_body`, `..._hash`, `..._prev_hash`, `..._type_column` | adultera o campo com o `tamper` | `ok = false` com o `reason` e o `seq` do elo (`hash-mismatch`, `column-mismatch`) |
| `it_should_detect_a_deleted_middle_link` | apaga o elo 3 | `diverging-seq` no elo seguinte |
| `it_should_detect_an_inserted_forged_link` | insere elo com hash forjado | quebra no elo forjado |
| `it_should_not_detect_a_truncated_tail_without_expected_head` e `it_should_detect_it_with_expected_head` | apaga o último elo; guarda a cabeça antes | sem `expected_head` passa (documenta o limite, H:242); com ele, `head-not-found` |
| `it_should_detect_a_rewritten_and_rechained_tail_with_expected_head` | reescreve a cauda recalculando hashes | `head-not-found` |
| `it_should_detect_a_tampered_batch_index` | altera `batches.fingerprint` | `batch-index-mismatch` |
| `it_should_keep_tenant_a_valid_when_tenant_b_is_tampered` (`tenant_isolation`) | dois tenants; adultera B | A `ok`, B `ok = false` |
| `it_should_keep_tenant_a_valid_when_every_row_of_tenant_b_is_removed` | apaga todos os elos de B | A `ok` |
| `it_should_derive_different_anchors_for_different_tenants` e `it_should_give_different_hashes_to_identical_content_in_different_tenants` | mesmo conteúdo em A e B | âncoras e hashes diferentes |
| `it_should_verify_after_any_sequence_of_valid_appends` (`chain_properties`, proptest) | sequências aleatórias de lotes válidos | `verify_chain` sempre `ok` |
| `it_should_detect_a_single_byte_change_in_any_stored_field` (proptest) | escolhe elo e campo ao acaso, altera | `ok = false` sempre |
| `it_should_match_the_golden_hash_for_a_fixed_link` (`jcs_vectors`) | elo fixo | hash igual ao congelado |
| `it_should_canonicalize_per_rfc_8785_vectors` | `rfc8785_vectors.json` | saída igual à esperada |
| `it_should_refuse_non_integer_numbers_in_data` | `data` com `0.5` | `INVALID_RECORD` |
| `it_should_refuse_updates_and_deletes` (`sqlite_append_only`) | conexão normal | `UPDATE`/`DELETE` abortam; `seq` fora de ordem e `prev_hash` errado abortam |

### C7. Um terceiro tipo de grafo não exige mudar o `engine`

| Teste (arquivo) | Arruma | Afirma |
|---|---|---|
| `it_should_accept_a_third_graph_defined_only_by_data` (`third_graph`) | `fixtures/third_graph.json` com três grafos de nomes neutros (dois com tipos e uma relação `acyclic`, o terceiro com tipo `requires_state` e relação nova), carregados num `Catalog`, e só a API pública da Facade | grava, consulta, liga por aresta, revisa, aposenta e `verify_chain` dá `ok`; o arquivo do teste não importa nada fora de `hexlog::` |
| `it_should_not_mention_domain_vocabulary_in_engine_schema_or_store_sources` (`architecture`) | varre `src/engine`, `src/schema`, `src/store`, `src/ids.rs`, `src/facade.rs` | nenhuma palavra inteira de uma lista (decision, decisão, premise, premissa, directive, diretriz, check, gate, unit, prd, review, omc, omhl, hexflow, claude), comentários incluídos; portagem de `test/no-flow-terms.spec.ts:8-10` (H:86) |
| `it_should_mention_git_only_in_engine_state` | varre os mesmos caminhos | `git` só em `src/engine/state.rs` |
| `it_should_depend_only_on_the_allowlisted_crates` | `cargo metadata` | dependências diretas e transitivas de `hexlog` sem git, rede, async ou regex externo |
| `it_should_not_depend_on_omhl` | `cargo metadata` | `omhl` não aparece no fecho de `hexlog` |

Observação sobre o alcance de C7: a prova de que um grafo novo não toca o `engine` é por construção (o teste só usa a API pública e um arquivo de dados) mais as travas de arquitetura; não há como provar "nunca precisou mudar" além disso.

### Comandos de verificação

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- Por critério: `cargo test -p hexlog --test node_lifecycle --test semantic_id --test state` (C1), `cargo test -p hexlog --test chain_tamper --test chain_properties --test tenant_isolation --test jcs_vectors --test sqlite_append_only` (C3), `cargo test -p hexlog --test third_graph --test architecture` (C7).
- Conferência única (T2): hash dourado contra o `hashLink` do Hexlog TS.

## 6. Riscos e mitigações

| # | Risco | Mitigação |
|---|---|---|
| R1 | O JCS do crate escolhido diverge do RFC (número, escape de Unicode, ordem de chaves) e o hash fica incompatível para sempre. | Spike da T1 contra os vetores do RFC; só inteiros seguros em `data`; hash dourado congelado; conferência única contra o Hexlog TS; fallback E2-C. |
| R2 | Travar um formato de elo ruim (tudo que entra no corpo fica). | Campos opcionais omitidos (acrescentar não quebra); `v: 1` e `format` na âncora; sem dado real até o passo 4; portão de congelamento ao fim da T6 com os itens 5 e 10 da seção 2 respondidos. Mudar depois do portão é tenant novo (D12: sem migração). |
| R3 | Coluna derivada diverge do `body` e a consulta devolve algo que o hash não cobre. | Payload sempre lido de `body`; colunas só filtram; `verify_chain` confere coluna contra corpo; teste de adulteração de coluna. |
| R4 | Concorrência e desempenho: `busy`, WAL em sistema de arquivos errado, `Tail` esconde adulteração no meio. | `BEGIN IMMEDIATE` + `busy_timeout` + erro `BUSY` repetível; testes com conexões e threads separadas; banco fora de `/mnt` no WSL; `VerifyPolicy::Full` nos testes; medir 5.000 elos (referência de H:556) e reavaliar E6 se a verificação completa for barata. |
| R5 | O motor perde a neutralidade aos poucos (palavra de domínio, crate de git). | `architecture.rs` na T1, antes de qualquer regra; lista fechada de dependências. |
| R6 | O `State` é confiado ao chamador: a árvore pode estar suja ou o SHA ser de outro repositório. | Dito no ADR e no item 1; a sonda e o teste de árvore suja ficam no passo 5, no adaptador. |
| R7 | Crescimento de escopo (FTS5, projeção, gate, nativos). | Lista "não entra" da seção 4.1; revisão de entrega confere os arquivos contra o layout da seção 4.2. |
| R8 | Decisões do dono ainda abertas mudam o corpo do elo (`Retire`, origem nativa em aresta, `alias`). | Padrões reversíveis e isolados (variante de enum, uma linha de regra); o portão da T6 cobra a resposta antes do passo 4. |
| R9 | Versões e comportamento das crates não verificados por este plano. | Spikes da T1; `Cargo.lock` versionado; `rust-toolchain.toml` fixo. |
| R10 | Mensagem de erro que vaze conteúdo de `data` ou caminho do banco. | `details` só com ponteiro, código e mensagem fixa; teste de erro não ecoar o valor. |

## 7. ADR

**Decisão.** Implementar o passo 3 como crate `hexlog` (módulos `engine`, `schema`, `store`) mais um `omhl` mínimo num workspace Cargo; SQLite via `rusqlite` com `bundled`; elos com corpo JCS hasheado por sha256 e encadeados por tenant com `seq` contíguo; revisão por `base` explícito com índices únicos parciais; `Retire` como variante do corpo; definições de tipo e relação injetadas por um `Catalog` e fixadas por `def_hash` em cada elo; base nativa reconhecida por uma porta `ForeignNodes`; estado tipado fechado (`GitCommit`) validado só por formato; escrita como `decide` puro dentro de uma transação exclusiva; verificação de cauda na escrita e completa sob demanda.

**Drivers.** Integridade verificável sem arrependimento de formato; neutralidade do motor demonstrável por teste; execução por agentes sonnet/haiku com verificação mecânica.

**Alternativas consideradas.** `sqlx`, `diesel`/`sea-orm`, SQLite do sistema (E1); `serde_jcs` e canonicalizador próprio (E2); quatro crates ou um só (E3); revisão implícita ou `supersedes` explícito (E4); definições como registros ou manifesto fixado por tenant (E5); verificação completa em toda leitura (E6).

**Por que esta.** É a combinação que preserva as regras do Hexlog que o mestre manda portar (cadeia, lote, `key`, um vigente por linhagem), cumpre D1 e D11 sem criar crates extras, deixa C7 testável por dado, e torna a bifurcação fisicamente impossível em vez de só verificada. O `Catalog` com `def_hash` evita o processo imutável do Hexlog (H:642) sem inventar bootstrap circular.

**Consequências.**

- O núcleo confia no `State` do chamador; "árvore suja não aprova" (D31) é do adaptador.
- Sem classificação de mudança compatível/quebra de definição (o Hexlog tinha, H:289-293); o histórico é provado pelo `def_hash`, não impedido.
- `data` aceita só inteiros; o campo de nota fracionária precisa de outro desenho (texto ou milésimos).
- `expected_head` é necessário para detectar cauda apagada; quem quiser essa garantia precisa guardar a cabeça fora do banco.
- A verificação de cauda deixa adulteração no meio visível só em `verify_chain` (E6).
- A base nativa não participa da cadeia; arestas para nativos podem ficar penduradas após atualização do plugin (item 2 da seção 2).
- Formato do elo congelado ao fim da T6.

**Próximos passos.**

- O dono responde os itens marcados [DONO] da seção 2, em especial 1, 5 e 10 (antes do portão da T6) e 3, 6 e 7 (antes dos passos 4 e 6).
- Passo 4: grafos de decisões, de checks e de `unit`, base nativa e `ForeignNodes` real, avaliação das quatro perguntas de gate sobre `edges` e `list`, medição do texto das instruções contra o teto de `data` (item 15).
- Passo 5: `StateProbe` com `git`, `.omhl.toml`, `render` e adapters.
- Passo 6: projeção com o marcador `tenant_head` (item 6).
- Reavaliar E6 e E2 com os números da T1 e da T6.
