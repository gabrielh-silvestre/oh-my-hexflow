# Avaliação: W3C PROV-O e Activity Streams 2.0 / Activity Vocabulary

- **Contexto:** plano [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md), seção 1 (decisões 3 e 6) e seção 5 (campos do nó de gate).
- **Data da pesquisa:** 2026-11-07. Todas as URLs foram abertas nesta data.
- **Escopo:** dois candidatos avaliados separadamente na mesma rubrica, porque o resultado é bem diferente.

## Resumo

| Critério | PROV-O | Activity Streams 2.0 |
|---|---|---|
| (a) Eventos do ciclo de desenvolvimento | 1 | 1 |
| (b) Grafo de decisões e revisões | 2 (revisões: 3) | 1 (revisões: 0) |
| (c) Curadoria e estabilidade | 3 | 2 |
| (d) Extensão | 3 | 2 |
| (e) Forma legível por máquina e licença | 2 | 3 |
| (f) Suporte em Rust | 1 (sem crate PROV; RDF genérico existe) | 1 (crate parado desde 2023) |
| (g) Custo de adoção | 2 (baixo como vocabulário de nomes, médio como RDF) | 1 |

Escala: 0 = não serve, 1 = serve de longe, 2 = serve com ressalvas, 3 = serve bem. Em (f) e (g), nota alta = suporte bom e custo baixo.

## Fontes primárias usadas

- PROV-O, W3C Recommendation, 2013-04-30: <https://www.w3.org/TR/prov-o/> (versão fixa: <https://www.w3.org/TR/2013/REC-prov-o-20130430/>)
- PROV-DM, W3C Recommendation, 2013-04-30: <https://www.w3.org/TR/prov-dm/>
- PROV-Overview, W3C Working Group Note, 2013-04-30: <https://www.w3.org/TR/prov-overview/>
- Activity Streams 2.0 (Core), W3C Recommendation, 2017-05-23: <https://www.w3.org/TR/activitystreams-core/>
- Activity Vocabulary, W3C Recommendation, 2017-05-23: <https://www.w3.org/TR/activitystreams-vocabulary/>
- Arquivos da ontologia: <https://www.w3.org/ns/prov-o.ttl>, <https://www.w3.org/ns/prov.ttl>, <https://www.w3.org/ns/prov.owl> e contexto JSON-LD <https://www.w3.org/ns/activitystreams>
- Crates: <https://crates.io/api/v1/crates/oxrdf>, `/oxttl`, `/json-ld`, `/sophia`, `/activitystreams`, consultados em 2026-11-07

---

# Parte 1: PROV-O

## O que é

PROV-O expressa o PROV Data Model (PROV-DM) em OWL2. O modelo gira em torno de três classes: `Entity` (coisa física, digital ou conceitual com aspectos fixos), `Activity` (algo que ocorre num período e age sobre entidades) e `Agent`. Fontes: <https://www.w3.org/TR/prov-dm/#term-entity>, <https://www.w3.org/TR/prov-o/#Activity>.

PROV descreve **o que aconteceu** (proveniência retrospectiva). Não descreve o que deve acontecer. Isso pesa nos critérios (a) e (b).

## (a) Eventos e ações do ciclo de desenvolvimento: nota 1

- Não há termo para commit, push, PR, review, teste ou deploy. `Activity` é genérica; o tipo concreto vem de `prov:type` ou de subclasse própria (extensão, critério d). Fonte: <https://www.w3.org/TR/prov-dm/#section-extensibility> (seção 6).
- Há o necessário para ordem temporal: `startedAtTime`, `endedAtTime`, `wasInformedBy` (uma atividade informada por outra), `wasStartedBy` e `wasEndedBy` (entidade gatilho). Fonte: <https://www.w3.org/TR/prov-o/#cross-reference-expanded-terms>. Mas isso registra "A começou depois que B terminou", não "A deve rodar antes de B".
- "Bloqueante" não existe. `Plan` (`hadPlan` em `Association`) é "conjunto de ações ou passos que um agente pretendia executar", e o texto diz que a especificação de planos está fora do escopo do PROV-O. Fonte: <https://www.w3.org/TR/prov-o/#hadPlan>.
- Conclusão: PROV-O não fornece o vocabulário de `when.event`. Ele serviria, no futuro (execução de gates fora da v1), para **registrar** que uma execução de gate aconteceu: `Activity` com `used` (o diff), `wasAssociatedWith` (agente) e `hadPlan` apontando para o nó de gate.

## (b) Grafo de decisões e revisões: nota 2 (revisões: 3)

### Revisões e id semântico: encaixe direto

O exemplo do próprio PROV-O ("Version 2 of the post") usa exatamente o padrão do plano: um permalink estável, versões com id próprio, ligadas por `specializationOf` e `wasRevisionOf`. Fonte: <https://www.w3.org/TR/prov-o/> (seção de exemplos; busca por `post9821v2`).

```turtle
:post9821v2
   a prov:Entity ;
   prov:specializationOf :more-crime-happens-in-cities ;  # permalink da revisão mais recente
   prov:wasRevisionOf    :post9821v1 ;
   prov:alternateOf      :post9821v1 .
```

Mapeamento para o motor:

| Conceito do motor | Termo PROV | Base na spec |
|---|---|---|
| Nó com id semântico estável (`gates.before-commit.run-tests`) | `prov:Entity` geral | PROV-DM 5.5.1: a entidade geral é aquela de que as específicas são `specializationOf` |
| Revisão (id técnico próprio) | `prov:Entity` específica, `specializationOf` o id semântico | PROV-DM 5.5.1: "a especialização compartilha todos os aspectos da geral e apresenta aspectos mais específicos"; o tempo de vida da geral contém o de qualquer especialização |
| Revisão N+1 sobre revisão N | `prov:wasRevisionOf` | PROV-O: subpropriedade de `wasDerivedFrom`, domínio e imagem `Entity` |
| Atributos da revisão (quem, por quê) | `prov:qualifiedRevision` com instância de `prov:Revision` | PROV-O: permite pendurar propriedades próprias (`ex:peerReviewed false` no exemplo oficial) |
| Quem escreveu a revisão | `prov:wasAttributedTo` (agente) | PROV-O |
| Quando foi criada | `prov:generatedAtTime` | PROV-O |

Ressalvas:

- `wasRevisionOf` implica "a entidade resultante contém conteúdo substancial da original" (PROV-DM 5.2.2). Uma sobreposição de nó nativo por nó de projeto que reescreve tudo seria, a rigor, só `wasDerivedFrom`. Na prática o motor trata toda sobreposição como revisão; isso é uma escolha nossa, não da spec.
- **`externalId` apontando para a primeira revisão:** PROV não tem termo para "primeira revisão da cadeia". O candidato mais próximo, `prov:hadPrimarySource`, é definido como algo produzido por quem tem experiência direta, sem retrospecto (<https://www.w3.org/TR/prov-o/#PrimarySource>). Isso não descreve "a revisão nativa". O `externalId` deve continuar um campo próprio do motor. A primeira revisão também é alcançável por fecho transitivo de `wasRevisionOf`.
- `prov:alternateOf` (duas entidades que apresentam aspectos da mesma coisa) é redundante aqui, pois `specializationOf` já o implica na hierarquia do PROV-O (`specializationOf` é subpropriedade de `alternateOf`). Fonte: <https://www.w3.org/TR/prov-o/#alternateOf>.

### Substituição e invalidação: encaixe parcial

- `prov:wasInvalidatedBy` é "o início da destruição, cessação ou expiração de uma entidade por uma atividade; a entidade não está mais disponível para uso depois". É instantânea. Fonte: <https://www.w3.org/TR/prov-dm/#term-Invalidation>.
- Modelagem possível de uma sobreposição: uma `Activity` ("override") que `used` a revisão nativa, gerou (`wasGeneratedBy`) a revisão do projeto e invalidou (`wasInvalidatedBy`) a revisão nativa. O PROV-DM exige ao menos um entre `id`, `activity`, `time` e `attributes` na invalidação.
- **Descompasso:** no plano, a revisão anterior continua existindo na cadeia e a base nunca é editada, só sobreposta. Invalidação em PROV significa fim de vida da entidade, o que casa com "deixou de ser a revisão vigente", mas não com "foi removida". Quem consulta precisa de uma regra própria: a vigente é a especialização do id semântico sem `wasInvalidatedBy`.
- **Não existe `supersedes` ou `replaces` em PROV-O.** Para "decisão B substitui decisão A" há duas opções: `B wasRevisionOf A` (se B é versão revisada de A) ou `B wasDerivedFrom A` mais `A wasInvalidatedBy <atividade que produziu B>`. A segunda forma não distingue "substituída" de "consumida", e isso não está resolvido pela spec.

### Decisões, premissas e evidências: cobertura fraca

- `Entity` serve como supertipo de decisão, premissa e evidência. `Agent` (`Person`, `SoftwareAgent`, `Organization`) serve para o autor da decisão.
- Relações "premissa sustenta decisão", "evidência contradiz premissa" não existem. O mais próximo é `prov:wasInfluencedBy` (genérica) e `prov:used` (atividade usa entidade). Fonte: <https://www.w3.org/TR/prov-o/#wasInfluencedBy>. O próprio PROV-DM recomenda trocar `wasInfluencedBy` pelas subrelações específicas quando aplicável. Os verbos `supports`/`contradicts` do motor ficam fora do PROV.
- `Bundle` (conjunto nomeado de descrições de proveniência, ele mesmo uma `Entity`) é um precedente para "proveniência da proveniência", mas não é necessário aqui. Fonte: <https://www.w3.org/TR/prov-o/#Bundle>.

## (c) Curadoria e estabilidade: nota 3

- Publicado pelo W3C (o nome do grupo de trabalho não foi conferido no texto lido); Recommendation de 2013-04-30, declarada "estável e pode ser usada como material de referência". Fonte: <https://www.w3.org/TR/prov-o/> (seção "Endorsed By W3C").
- Há errata publicada para o documento; o conteúdo das correções não foi lido (Lacunas).
- O arquivo `prov.ttl` traz `owl:versionInfo "Recommendation version 2013-04-30"` e o servidor informa `last-modified: 2025-09-03`. Fonte: <https://www.w3.org/ns/prov.ttl>. Não foi verificado o que mudou em 2025.
- Estável porque congelado há mais de 13 anos: ótimo para não quebrar, ruim se o motor precisar de termo novo (não haverá versão nova do W3C; extensão é por prefixo próprio).

## (d) Extensão para termos do projeto e vocabulários de hosts: nota 3

- O PROV-DM reserva `prov:type`, `prov:role` e `prov:location` como pontos de extensão; subtipos e sub-relações saem de `prov:type` (exemplo oficial: `wasDerivedFrom(e2, e1, [prov:type='ex:Translation'])`). Fonte: <https://www.w3.org/TR/prov-dm/#section-extensibility>.
- Em OWL, o PROV-O mostra criar `my:wasConductedBy rdfs:subPropertyOf prov:wasAssociatedWith` e qualificá-la com `prov:Association`, de modo que sistemas PROV sem conhecer a relação nova ainda reconhecem a influência. Fonte: <https://www.w3.org/TR/prov-o/> (exemplo de `AgentInfluence`).
- Condição: "especializações que usam os pontos de extensão devem preservar a semântica do PROV-DM e do PROV-CONSTRAINTS". O PROV-CONSTRAINTS não foi lido por inteiro (Lacunas).
- Para o nosso caso: `{vocabulary: "claude-code", event: "PreToolUse"}` poderia virar `Activity` com `prov:type = claude-code:PreToolUse`. O PROV não define o vocabulário do host; ele só dá o ponto de encaixe.

## (e) Forma legível por máquina e licença: nota 2

- OWL2/RDF: verificados `https://www.w3.org/ns/prov-o.ttl` (Turtle, ~39 classes e ~44 propriedades de objeto pela contagem de `owl:Class` e `ObjectProperty` no arquivo), `https://www.w3.org/ns/prov.owl` e `https://www.w3.org/ns/prov-o.owl` (ambos `application/rdf+xml`, HTTP 200). A contagem foi feita por `grep` e é aproximada.
- JSON-LD: a requisição a `https://www.w3.org/ns/prov.jsonld` retornou HTTP 300 (opções de formato); não foi confirmado contexto JSON-LD oficial do PROV-O. Ver Lacunas.
- Licença: a página do PROV-O diz "W3C liability, trademark and document use rules apply" (<https://www.w3.org/TR/prov-o/>). O texto do documento segue a licença de documento do W3C. A licença do arquivo `.ttl` da ontologia não foi encontrada no arquivo (nenhum `dct:license`). Para o nosso uso (referenciar IRIs e nomes), a licença do texto não bloqueia; para embarcar uma cópia do `.ttl` no repositório, falta confirmar. Por isso nota 2 e não 3.

## (f) Suporte em Rust: nota 1

- Não há crate dedicado ao PROV-O. Busca em `https://crates.io/api/v1/crates?q=prov-o` e `?q=provenance+w3c` (2026-11-07) só trouxe crates de domínio específico que usam PROV-O internamente (ex.: `oxiephemeris-rdf` 0.1.1) ou stores RDF. Nenhum serve como biblioteca de modelo PROV.
- RDF genérico existe e é mantido: `oxrdf` 0.3.5 (MIT OR Apache-2.0, 2026-10-18, ~1,98 M de downloads), `oxttl` 0.2.5 (MIT OR Apache-2.0, 2026-10-18), `json-ld` 0.22.0 (MIT/Apache-2.0, 2026-10-30), `sophia` 0.10.0 (Apache-2.0 / CECILL-B, 2026-05-19). Fonte: API do crates.io, URLs acima.
- Consequência: usar PROV-O como RDF completo (parse, inferência OWL) em Rust seria trabalho próprio. Usar como vocabulário de nomes (strings/IRIs num enum) não exige crate.

## (g) Custo de adoção: nota 2

O custo depende do nível de adoção:

| Nível | O que o motor implementa | Custo |
|---|---|---|
| Só emprestar nomes | Tabela de mapeamento nome interno -> IRI `prov:` na documentação | Quase zero |
| Subconjunto como esquema de tipos e relações | Definições de tipo/relação (dados, como no `schema` do plano) que declaram `prov:Entity`, `prov:wasRevisionOf` etc. como `iri` de cada tipo | Baixo |
| Exportação RDF | Serializar a cadeia em Turtle com `oxrdf` + `oxttl` | Médio, e opcional |
| RDF/OWL como modelo interno | Armazenar triplas, validar domínio/imagem, raciocinar | Alto e fora do escopo da v1 |

Recomendação de custo: nível 2, com a exportação no nível 3 como possibilidade futura.

## Veredito (PROV-O): adotar subconjunto

Adotar como **vocabulário de nomes e semântica para entidades, revisões e autoria**, com IRIs `prov:` registrados como metadado dos tipos e relações. Não adotar como modelo de armazenamento. Não usar para eventos do ciclo de desenvolvimento nem para `when`.

### Termos concretos úteis (PROV-O)

Namespace: `http://www.w3.org/ns/prov#`.

| Termo | URI oficial | Para que usaríamos |
|---|---|---|
| Entity | `prov:Entity` | Supertipo de nó (decisão, premissa, evidência, gate) e de revisão |
| Agent / Person / SoftwareAgent | `prov:Agent`, `prov:Person`, `prov:SoftwareAgent` | Quem registrou a revisão (usuário ou agente de IA) |
| Activity | `prov:Activity` | Uma sobreposição, ou (futuro) uma execução de gate |
| specializationOf | `prov:specializationOf` | Revisão -> id semântico estável |
| wasRevisionOf | `prov:wasRevisionOf` | Revisão N+1 -> revisão N |
| qualifiedRevision / Revision | `prov:qualifiedRevision`, `prov:Revision` | Dados da revisão (motivo, autor) sem inventar propriedade |
| wasDerivedFrom | `prov:wasDerivedFrom` | Decisão derivada de outra sem ser versão revisada |
| wasInvalidatedBy / invalidatedAtTime | `prov:wasInvalidatedBy`, `prov:invalidatedAtTime` | Marcar a revisão deixada de ser vigente e quando |
| wasGeneratedBy / generatedAtTime | `prov:wasGeneratedBy`, `prov:generatedAtTime` | Atividade e instante em que a revisão foi criada |
| wasAttributedTo | `prov:wasAttributedTo` | Autoria sem precisar descrever a atividade |
| used | `prov:used` | (Futuro) execução de gate usa um diff/arquivo; evidência usada por decisão |
| wasAssociatedWith / hadPlan / Plan | `prov:wasAssociatedWith`, `prov:hadPlan`, `prov:Plan` | (Futuro) ligar execução de gate ao nó de gate |
| wasInformedBy / wasStartedBy / wasEndedBy | `prov:wasInformedBy`, `prov:wasStartedBy`, `prov:wasEndedBy` | (Futuro) ordem entre execuções e gatilho |
| wasInfluencedBy | `prov:wasInfluencedBy` | Relação genérica de reserva quando nenhuma específica serve |
| prov:type | atributo reservado (PROV-DM) | Subtipos próprios do projeto e eventos de hosts |

Não usar: `alternateOf` (implícita em `specializationOf`), `hadPrimarySource` (semântica não casa com "revisão nativa"), `Bundle`, `Collection`.

### Lacunas (PROV-O)

- Não li o PROV-CONSTRAINTS (<https://www.w3.org/TR/prov-constraints/>, Recommendation de 2013-04-30, estado confirmado só pelo cabeçalho); não sei se alguma restrição conflita com o modelo de revisões do motor (por exemplo, ordem entre geração e invalidação).
- Não li a errata do PROV-O nem o que mudou no `prov.ttl` em 2025-09-03.
- Licença do arquivo da ontologia (`prov-o.ttl`) não confirmada.
- Contexto JSON-LD oficial do PROV-O não confirmado (HTTP 300 em `prov.jsonld`). PROV-JSON e PROV-JSONLD não foram avaliados.
- Não verifiquei se existe mapeamento publicado de PROV para git/CI (ideia: ferramentas "git2prov"); não afirmo que exista.
- Os nomes de classes de eventos do ciclo de desenvolvimento ficaram sem fonte neste candidato; outros pesquisadores cobrem.

---

# Parte 2: Activity Streams 2.0 / Activity Vocabulary

## O que é

Formato JSON (serialização JSON-LD) e vocabulário para descrever atividades de redes sociais ("Sally curtiu um post"). Um `Activity` tem `actor`, `object`, `target`, `result`, `origin`, `instrument`. Fonte: <https://www.w3.org/TR/activitystreams-core/>, <https://www.w3.org/TR/activitystreams-vocabulary/>.

## (a) Eventos e ações do ciclo de desenvolvimento: nota 1

- Tipos de atividade definidos na spec (lista completa): Accept, Add, Announce, Arrive, Block, Create, Delete, Dislike, Flag, Follow, Ignore, Invite, Join, Leave, Like, Listen, Move, Offer, Question, Reject, Read, Remove, TentativeReject, TentativeAccept, Travel, Undo, Update, View. Fonte: <https://www.w3.org/TR/activitystreams-vocabulary/#activity-types>.
- Aproximações possíveis: `Create` (criar commit/PR), `Update`, `Delete`, `Add`/`Remove` (adicionar a uma coleção), `Accept`/`Reject` (aprovar ou rejeitar algo, como num review), `Flag`.
- Armadilha: `Block` significa "o ator bloqueia o objeto" no sentido social (bloquear um usuário, "forma mais forte de Ignore"), não "bloqueante" no sentido de gate. Fonte: mesma página, tipo `Block`.
- Busca textual na spec por `commit`, `deploy`, `version`, `revision` e `supersede` não encontrou termo do vocabulário (só ocorrências do texto do próprio documento W3C).
- O tipo `Update` diz explicitamente: "este vocabulário não define mecanismo para descrever o conjunto real de modificações feitas ao objeto". Fonte: <https://www.w3.org/TR/activitystreams-vocabulary/#dfn-update>.
- Não há relação de ordem "antes de/depois de" entre atividades; só `startTime`, `endTime`, `published`, `updated` (propriedades de instante).

## (b) Grafo de decisões e revisões: nota 1 (revisões: 0)

- **Revisões: não cobre.** A única propriedade temporal de mudança é `updated` (um timestamp por objeto). Não há versão, id de revisão nem ligação entre versões. Não há termo equivalente a `wasRevisionOf` ou `supersedes` (varredura da lista de propriedades da spec, 2026-11-07).
- **Substituição/invalidação:** o mais próximo é `Tombstone` (com `formerType` e `deleted`), que marca um objeto apagado. Não representa "substituído por outro". Fonte: <https://www.w3.org/TR/activitystreams-vocabulary/#dfn-tombstone>.
- **Tipos e relações de decisão:** `Relationship` (com `subject`, `relationship`, `object`) serve para relação arbitrária entre dois objetos, mas o vocabulário a descreve como relação entre indivíduos (pessoas). `Note`, `Article` e `Document` poderiam ser texto de decisão. Nada sobre premissa, evidência ou suporte.
- O modelo é centrado em quem fez o quê, publicado como fluxo (stream) de atividades. Um grafo de entidades versionadas não é o formato natural.

## (c) Curadoria e estabilidade: nota 2

- W3C Recommendation de 2017-05-23, copyright atribuído ao Activity Streams Working Group, IBM e W3C; repositório em <https://github.com/w3c/activitystreams>. Errata mantida em arquivo no GitHub (`ERRATA.md`); o conteúdo não foi lido. Fontes: cabeçalho de <https://www.w3.org/TR/activitystreams-vocabulary/>.
- Sem versão posterior encontrada nas páginas consultadas. Estado atual do grupo de trabalho não verificado (Lacunas).
- Nota 2 e não 3 porque o escopo (web social) é distante do nosso e não foi confirmada manutenção ativa.

## (d) Extensão: nota 2

- Extensão = "qualquer propriedade, atividade, ator ou tipo de objeto não definido pelo Activity Vocabulary". Consumidores **devem** ignorar extensões desconhecidas sem erro e **não há modelo normativo de processamento** para extensões; JSON-LD é o mecanismo recomendado. Fonte: <https://www.w3.org/TR/activitystreams-core/#extensibility>.
- Implementações "**devem evitar** tipos ou propriedades de extensão que dupliquem ou sobreponham indevidamente" o vocabulário estendido. Fonte: <https://www.w3.org/TR/activitystreams-vocabulary/#extended-types>.
- O contexto normativo não pode ser sobrescrito, só aumentado. Fonte: <https://www.w3.org/TR/activitystreams-core/#jsonld>.
- Aviso da spec: o algoritmo JSON-LD ignora silenciosamente propriedades não definidas no `@context`. Para nós, um `when: {vocabulary, event}` fora do contexto sumiria sem erro num consumidor JSON-LD ingênuo.

## (e) Forma legível por máquina e licença: nota 3

- Contexto JSON-LD oficial verificado: `https://www.w3.org/ns/activitystreams` responde `Content-Type: application/ld+json` com `Accept: application/ld+json` (HTTP 200, 2026-11-07).
- Licença: o rodapé da spec cita "permissive document license" do W3C (<https://www.w3.org/Consortium/Legal/2015/copyright-software-and-document>). A licença permissiva cobre o uso do contexto e dos termos.

## (f) Suporte em Rust: nota 1

- Crate `activitystreams` 0.6.2, atualizada em 2023-07-06, ~136 mil downloads, repositório em <https://git.asonix.dog/asonix/activitystreams> (hospedagem fora do GitHub). Fonte: <https://crates.io/api/v1/crates/activitystreams>. É um conjunto de tipos e traits para o formato do fediverso, não um modelo de domínio genérico. Versões pré-lançamento e licença da crate não foram verificadas.
- Para JSON-LD genérico, `json-ld` 0.22.0 (ver parte PROV-O, critério f).

## (g) Custo de adoção: nota 1

- O motor teria de aceitar `@context`, `type`/`actor`/`object` como forma de entrada e saída, e traduzir para tipos próprios, para ganhar um vocabulário que não tem os termos de que precisamos (revisão, commit, gate).
- O ganho de interoperabilidade (com o fediverso) não é objetivo do plano.

## Veredito (Activity Streams 2.0): rejeitar

Rejeitar como base do vocabulário. Não cobre revisões nem desenvolvimento de software, e a estrutura `actor`/`object` não é a forma do grafo. Se o dono quiser reaproveitar algo, o máximo é **emprestar nomes** dos poucos verbos com semântica estável (`Create`, `Update`, `Delete`), sabendo que o nome idêntico induz a leitura social.

### Termos concretos úteis (Activity Streams 2.0)

Namespace: `https://www.w3.org/ns/activitystreams#`.

| Termo | URI oficial | Para que usaríamos |
|---|---|---|
| Create / Update / Delete | `as:Create`, `as:Update`, `as:Delete` | Nomes de operação do registro (criar, sobrepor, remover nó), só como inspiração de nome |
| Tombstone (`formerType`, `deleted`) | `as:Tombstone`, `as:formerType`, `as:deleted` | Referência de como representar "existia e foi removido"; PROV `wasInvalidatedBy` é mais completo |
| Relationship | `as:Relationship` | Não usar: escopo é relação entre pessoas |
| published / updated | `as:published`, `as:updated` | Equivalentes a `generatedAtTime`; preferir PROV |

### Lacunas (Activity Streams 2.0)

- Não li `ERRATA.md` nem verifiquei se o Activity Streams WG foi encerrado ou se há revisão em andamento.
- Não verifiquei pré-lançamentos nem licença da crate `activitystreams`.
- Não avaliei o vocabulário ActivityPub (protocolo) nem extensões listadas em <https://www.w3.org/ns/activitystreams#extensions>.
- Não procurei extensões do AS 2.0 para desenvolvimento de software (por exemplo, o ForgeFed, que estende ActivityPub para forjas de código). Pode haver termos de commit/ticket/PR ali; fica como pista para o debate, sem verificação.

---

# Achados para o debate

1. PROV-O resolve revisão + id semântico estável: `specializationOf` (id semântico) + `wasRevisionOf` (cadeia) é o padrão do exemplo oficial do PROV-O.
2. `externalId` (primeira revisão) não tem termo PROV; `hadPrimarySource` parece servir mas tem semântica diferente. Fica campo próprio.
3. PROV não tem `supersedes`; `wasInvalidatedBy` marca fim de vida da revisão, e a consulta "revisão vigente" é regra do motor.
4. Nenhum dos dois cobre eventos do ciclo de desenvolvimento (commit, push, PR, teste, deploy) nem "bloqueante". O `when.event` precisa vir de outro vocabulário.
5. PROV é retrospectivo (o que aconteceu), gates são prescritivos (o que deve acontecer). PROV serve ao registro de execuções no futuro, não à declaração do `when`.
