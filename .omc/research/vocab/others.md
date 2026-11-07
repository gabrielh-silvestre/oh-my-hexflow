# Vocabulários curados além de schema.org, CloudEvents/CDEvents, OTel/ECS e PROV-O/ActivityStreams

- **Data:** 2026-11-06. **Contexto:** [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md) (grafos de decisões e de gates; `when` estruturado `{vocabulary, event}`).
- **Escopo:** candidatos que as outras pesquisas não cobrem, para (i) eventos e etapas de verificação do ciclo de desenvolvimento e (ii) registro de decisões, premissas, evidências e planos.
- **Regra de fonte:** só especificações e repositórios oficiais. O que não consegui verificar está em "Lacunas".

## 1. Resumo

Nenhum dos 8 candidatos define os **eventos do ciclo** (`before-commit`, `before-push`, `pre-tool-use`). Esse vocabulário continua vindo dos hosts (Claude Code, git, CI) e do CDEvents. O que estes candidatos entregam é (a) o **envelope de conhecimento** (OKF), (b) o **vocabulário de decisão** (MADR, DCTerms, SACM/GSN, DMN) e (c) o **vocabulário de resultado de verificação** (in-toto, SARIF), útil quando os gates passarem a executar (marco futuro do plano, seção 2).

| # | Candidato | Cobre | Veredito |
|---|---|---|---|
| 1 | OKF v0.2 | envelope de conceitos (frontmatter, proveniência, confiança, ciclo de vida) | adotar subconjunto |
| 2 | in-toto Attestation v1.2 (+ predicados) | resultado de verificação sobre um artefato | só emprestar nomes |
| 3 | MADR 4.0.0 | registro de decisão (status, participantes, opções, resultado) | adotar subconjunto |
| 4 | SACM 2.3 / GSN | argumento: claim, evidência, premissa, justificativa, contexto | só emprestar nomes |
| 5 | SARIF 2.1.0 | achados de análise estática | só emprestar nomes (futuro) |
| 6 | DMN 1.6 / BPMN 2.0.2 | decisão com requisitos; eventos de processo | rejeitar |
| 7 | Dublin Core Terms | relações entre recursos (`replaces`, `references`, `conformsTo`) | só emprestar nomes |
| 8 | SKOS | taxonomia de conceitos (`broader`, `narrower`) | rejeitar (por ora) |

## 2. Avaliação dos 4 mais promissores

Notas de 0 a 3. Critérios: **a** eventos e ações do ciclo; **b** tipos e relações de um grafo de decisões; **c** curadoria e estabilidade; **d** extensão; **e** forma legível por máquina e licença; **f** suporte em Rust; **g** custo de adoção.

### 2.1 OKF (Open Knowledge Format) v0.2

Fonte primária: [SPEC.md](https://raw.githubusercontent.com/GoogleCloudPlatform/open-knowledge-format/main/SPEC.md) (versão 0.2) e o [repositório](https://github.com/GoogleCloudPlatform/open-knowledge-format) (Apache-2.0, criado em 2026-08-11, último push em 2026-11-06, sem releases publicadas).

| Critério | Nota | Justificativa |
|---|---|---|
| a | 0 | Não define eventos nem etapas. O exemplo de §4.4 usa seções `# Trigger` e `# Steps` no corpo, sem vocabulário. |
| b | 1 | Conceito tem `type` obrigatório (texto livre, sem registro central), `sources` e links. Mas "o tipo da relação é dado pela prosa, não pelo link" (§6.1) e a linhagem explícita (`derived_from`) está fora do escopo da v0.2 (§5.1). Não há premissa, evidência nem substituição. |
| c | 1 | Versão 0.2, repositório com 3 meses, sem releases. Tem regras de versionamento (§12: minor aditivo, major quebra) e conformidade (§11), mas é jovem. |
| d | 3 | "Produtores PODEM incluir chaves adicionais; consumidores NÃO DEVEM rejeitar campos desconhecidos" e tipos desconhecidos são tolerados (§4.1). |
| e | 3 | Markdown com frontmatter YAML, legível por humano e por agente. Apache-2.0. |
| f | 2 | Não verifiquei crate dedicado. O formato é frontmatter YAML mais corpo markdown, lido com um parser YAML comum. |
| g | 3 | Custo baixo. A pasta `wiki/` do próprio harness já segue OKF (skills `wiki-ingest` e `wiki-lint`), então a convenção é conhecida. |

Campos úteis (todos de §4 e §5): `type`, `title`, `description`, `resource`, `tags`, `sources[].{id,resource,title,author,last_modified}`, `generated`/`verified` com `by` e `at`, `status: draft | stable | deprecated`, `stale_after`, convenção de ator (`<producer>/<version>`, `human:<id>`, §7) e links absolutos a partir da raiz do bundle (§6.1).

### 2.2 in-toto Attestation Framework v1.2

Fonte primária: [spec/v1](https://raw.githubusercontent.com/in-toto/attestation/main/spec/v1/README.md) (v1.2), [statement.md](https://raw.githubusercontent.com/in-toto/attestation/main/spec/v1/statement.md), [test-result.md](https://raw.githubusercontent.com/in-toto/attestation/main/spec/predicates/test-result.md), [svr.md](https://raw.githubusercontent.com/in-toto/attestation/main/spec/predicates/svr.md). Release v1.2.0 em 2026-03-18; [LICENSE](https://raw.githubusercontent.com/in-toto/attestation/main/LICENSE) Apache-2.0.

| Critério | Nota | Justificativa |
|---|---|---|
| a | 1 | Predicados de resultado: `test-result` (`result: PASSED \| WARNED \| FAILED`, listas `passedTests`/`warnedTests`/`failedTests`), `svr` (verificador avaliou o artefato contra políticas), `vsa`, `link`, `runtime-trace`, `vuln`. Descreve o resultado de uma verificação, não o evento que a dispara nem o bloqueio. |
| b | 1 | Statement liga `subject` (artefato com `digest`) a um `predicate` tipado. Serve como forma de evidência. Não tem decisão, premissa nem substituição. |
| c | 2 | Statement v1 estável, com `GOVERNANCE.md` e `MAINTAINERS.md` no repositório. Predicados individuais ainda são v0.x (`test-result` v0.1, `svr` v0.2). |
| d | 3 | `predicateType` é uma URI e o `predicate` é livre; `test-result` diz que o esquema "pode ser estendido". |
| e | 3 | JSON (e protobuf). Apache-2.0. |
| f | 2 | Há bindings Rust no repositório (`rust/Cargo.toml`, `in_toto_attestation` 0.2.0, protobuf). No crates.io a última versão publicada é 0.1.0 (2025-08-28). |
| g | 1 | `subject` exige `digest` de artefato imutável. Nós de grafo que não são artefatos não se encaixam sem adaptação. |

### 2.3 MADR (Markdown Architectural Decision Records) 4.0.0

Fonte primária: [site](https://adr.github.io/madr/), [template](https://raw.githubusercontent.com/adr/madr/develop/template/adr-template.md) e [repositório](https://github.com/adr/madr). Release 4.0.0 em 2024-09-17; repositório ativo (push em 2026-11-02); licença `MIT OR CC0-1.0` (declarada no site).

| Critério | Nota | Justificativa |
|---|---|---|
| a | 0 | Não trata de eventos nem de verificação. |
| b | 2 | Cobre a decisão: `status`, `date`, `decision-makers`, `consulted`, `informed`, e as seções "Decision Drivers", "Considered Options", "Decision Outcome", "Consequences", "Confirmation", "Pros and Cons of the Options". Substituição é só texto (`superseded by ADR-0123`); premissa e evidência não existem como campo. |
| c | 2 | Projeto comunitário com histórico desde 2017 e releases numeradas, mas não é norma de órgão de padrões. |
| d | 2 | Todos os elementos de metadado são opcionais ("Feel free to remove any of them"). Sem mecanismo formal de extensão. |
| e | 3 | Markdown com frontmatter YAML. MIT ou CC0. |
| f | 3 | Não há dependência: só nomes de campo e de seção. |
| g | 3 | Custo mínimo. |

### 2.4 SACM 2.3 e GSN (assurance case)

Fonte primária: [OMG SACM](https://www.omg.org/spec/SACM/) (2.3 formal em outubro de 2023; 2.4 beta em setembro de 2026; IPR Mode "Non-Assert"), metamodelo XMI [SACM2.4_Metamodel.xml](https://www.omg.org/spec/SACM/20260504/SACM2.4_Metamodel.xml) (versão beta; conferi os nomes nele), e [GSN Standard](https://scsc.uk/gsn) (SCSC, licença CC BY 4.0).

| Critério | Nota | Justificativa |
|---|---|---|
| a | 0 | Modela argumento de garantia, não eventos. |
| b | 2 | É o único candidato com premissa e evidência como tipos de primeira classe: `Claim`, `ArtifactReference`, `AssertedEvidence`, `AssertedInference`, `AssertedContext`, `AssertedRelationship` com atributo `isCounter`, enumeração `AssertionDeclarationKind` (`axiomatic`, `assumed`, `asserted`, `byRule`) e pacote GSN (`GSNGoal`, `GSNStrategy`, `GSNSolution`, `GSNAssumption`, `GSNJustification`, `GoalContext`). Não encontrei substituição de versão. |
| c | 3 | Especificação OMG com versões formais desde 1.0 (2013) e histórico de adoção publicado. |
| d | 1 | Não verifiquei mecanismo de extensão. |
| e | 1 | PDF e XMI (UML), sem JSON Schema que eu tenha verificado. GSN em CC BY 4.0. |
| f | 1 | Não verifiquei suporte em Rust. Só os nomes seriam usados. |
| g | 1 | O metamodelo inteiro é grande (116 classes no XMI) e orientado a casos de segurança. Pegar só os nomes custa pouco. |

## 3. Os demais, uma linha cada

- **SARIF 2.1.0** ([OASIS Standard + Errata 01, 2023-08-28](https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/sarif-v2.1.0-errata01-os-complete.html)). Por que não agora: modela *achados* de ferramenta (localização, regra), e na v1 nenhum gate executa. Quando executarem, `result.kind` (`pass`, `fail`, `review`, `open`, `notApplicable`, `informational`), `level` (`error`, `warning`, `note`, `none`), `baselineState` (`new`, `unchanged`, `updated`, `absent`) e `suppressions[].status` (`accepted`, `underReview`, `rejected`) são o vocabulário mais completo para o resultado de um check. Rust: crate `serde-sarif` 0.8.0 (terceiro, 2025-05-09).
- **DMN 1.6 / BPMN 2.0.2** ([DMN](https://www.omg.org/spec/DMN/), [BPMN](https://www.omg.org/spec/BPMN/2.0.2/), [DMN.xsd](https://www.omg.org/spec/DMN/DMN.xsd)). Por que não: DMN trata de *decisão como tabela executável* (`Decision`, `InputData`, `KnowledgeSource`, `informationRequirement`, `authorityRequirement`, campos `question`, `allowedAnswers`, `decisionMaker`, `decisionOwner`), não de decisão como registro histórico; BPMN modela fluxo de processo de negócio (formal 2.0.2, janeiro de 2014), não eventos de ferramentas de desenvolvimento. Ambos são XML e a DMN tem IPR "RF-Limited".
- **Dublin Core Terms** ([DCMI Metadata Terms, 2020-01-20, CC BY 4.0](https://www.dublincore.org/specifications/dublin-core/dcmi-terms/)). Só nomes: `replaces` ("recurso que é suplantado, deslocado ou substituído pelo descrito") e o inverso `isReplacedBy`, `references`/`isReferencedBy`, `requires`/`isRequiredBy`, `conformsTo`, `source`, `hasVersion`/`isVersionOf`, `created`, `modified`, `issued`, `valid`, `provenance`. Cobre bem a substituição, mas não é um modelo de grafo.
- **SKOS** ([W3C Recommendation, 2009-08-18](https://www.w3.org/TR/skos-reference/)). Rejeitar por ora: `broader`/`narrower`/`related`, `ConceptScheme`, `definition`, `changeNote`, `historyNote` organizam *conceitos*, e a v1 não tem taxonomia de eventos para organizar. Reavaliar se os vocabulários absorvidos crescerem. Rust: `sophia` 0.10.0 (RDF).

### Triados fora da lista de 8

- **OpenLineage 1.55.0** ([spec](https://openlineage.io/docs/spec/run-cycle), Apache-2.0): estados de execução `START`, `RUNNING`, `COMPLETE`, `ABORT`, `FAIL`, `OTHER`, com extensão por *facets*. Gira em torno de jobs e datasets de dados; os estados só valem como nomes.
- **OSCAL v1.2.3** ([NIST](https://pages.nist.gov/OSCAL/)): controles de conformidade e avaliação de segurança; fora do domínio.
- **SPDX 3.0.1** ([spec](https://spdx.github.io/spdx-spec/v3.0.1/)): tem um perfil `Build` (`buildStartTime`, `buildEndTime`, `buildType`, `configSourceUri`), mas descreve pacote e licença, não etapa de verificação.
- **DOAP** ([repositório](https://github.com/ewilderj/doap), Apache-2.0): descreve o *projeto* de software (último push em 2024-06-11), não eventos nem decisões.
- **Wikidata** ([licenciamento](https://www.wikidata.org/wiki/Wikidata:Licensing), dados CC0): base de entidades do mundo, sem termos de ciclo de desenvolvimento e com curadoria comunitária contínua.

## 4. Veredito

| Candidato | Veredito | Termos |
|---|---|---|
| OKF | **Adotar subconjunto** | `type`, `title`, `description`, `resource`, `tags`, `sources[].{id,resource,title,author,last_modified}`, `generated`/`verified` (`by`, `at`), `status` (`draft`, `stable`, `deprecated`), `stale_after`, convenção de ator (`<producer>/<version>`, `human:<id>`), links absolutos a partir da raiz. Não herdar "links sem tipo": o motor tem relações tipadas. |
| in-toto | **Só emprestar nomes** | `result` com `PASSED`/`WARNED`/`FAILED`; padrão `predicateType` = URI versionada; `subject`/`predicate`. Não adotar o envelope, porque exige artefato com `digest`. |
| MADR | **Adotar subconjunto** | `status`, `date`, `decision-makers`, `consulted`, `informed`; seções "Context and Problem Statement", "Decision Drivers", "Considered Options", "Decision Outcome", "Consequences", "Confirmation". Valores de `status`: `proposed`, `rejected`, `accepted`, `deprecated`, `superseded by <id>`. |
| SACM/GSN | **Só emprestar nomes** | `Claim`, `ArtifactReference`, `AssertedEvidence`, `AssertedInference`, `AssertedContext`, `isCounter`, `axiomatic`/`assumed`/`asserted`; do GSN: Goal, Strategy, Solution, Assumption, Justification, Context. |

## 5. Termos concretos úteis

| Termo | Nome oficial | Para que usaríamos |
|---|---|---|
| Tipo de nó | `type` (OKF §4.1) | Campo obrigatório e livre do nó; consumidor tolera tipo desconhecido. |
| Ciclo de vida do nó | `status: draft \| stable \| deprecated` (OKF §5.4) | Estado de um nó nativo ou de projeto. |
| Quem escreveu e quem revisou | `generated` / `verified` com `by`, `at` (OKF §5.2) | Autoria do agente e aprovação humana de um nó. |
| Ator | `<producer>/<version>`, `human:<id>` (OKF §7) | Valor de `by`. |
| Fonte do nó | `sources[].resource` (OKF §5.1) | Apontar para o ADR, PR ou página que originou um gate. |
| Validade | `stale_after` (OKF §5.5) | Marcar instrução que precisa de revisão. |
| Status da decisão | `status` com `proposed`, `accepted`, `rejected`, `deprecated`, `superseded by` (MADR) | Ciclo de vida do tipo `decision`. |
| Participantes | `decision-makers`, `consulted`, `informed` (MADR) | Papéis no tipo `decision`. |
| Estrutura da decisão | "Decision Drivers", "Considered Options", "Decision Outcome", "Consequences", "Confirmation" (MADR) | Campos do tipo `decision`; "Confirmation" casa com o gate que verifica a decisão. |
| Substituição | `replaces` / `isReplacedBy` (DCTerms) | Nome da relação equivalente ao `supersedes` do Hexlog atual. |
| Referência e dependência | `references`, `requires`, `conformsTo` (DCTerms) | Relações entre nós; `conformsTo` liga um gate à norma que ele aplica. |
| Premissa | `assumed` em `AssertionDeclarationKind`; `GSNAssumption` (SACM/GSN) | Tipo `assumption` do grafo de decisões. |
| Evidência | `AssertedEvidence`, `ArtifactReference`, `GSNSolution` (SACM/GSN) | Tipo `evidence` e a relação que o liga a uma decisão. |
| Contra-argumento | `AssertedRelationship.isCounter` (SACM) | Equivalente ao `contradicts` do Hexlog atual. |
| Resultado de check | `PASSED` / `WARNED` / `FAILED` (in-toto test-result) | Resultado simples de gate, quando executar. |
| Resultado detalhado | `kind`, `level`, `baselineState`, `suppressions[].status` (SARIF) | Resultado rico de gate, quando executar. |
| Tipo versionado de predicado | `predicateType` URI (in-toto) | Padrão para versionar o vocabulário de um gate (`{vocabulary, event}` com versão). |
| Estado de execução | `START`, `COMPLETE`, `FAIL`, `ABORT` (OpenLineage) | Só se o gate registrar execução. |

## 6. Lacunas

- **Eventos de desenvolvimento (`before-commit`, `before-push`):** nenhum candidato os define. O vocabulário vem dos hosts e do CDEvents; esta nota não verifica nada sobre eles.
- **GSN:** não consegui abrir o texto do GSN Community Standard (a página `scsc.uk/gsn` carrega e confirma a licença CC BY 4.0, mas não extraí dela a versão e a data do padrão; as URLs de padrão e metamodelo que tentei adivinhar deram 404). Os nomes GSN vêm do pacote GSN dentro do XMI do SACM 2.4 beta, não da norma do SCSC.
- **SACM:** conferi nomes no XMI da **2.4 beta** (setembro de 2026), não na 2.3 formal. Não verifiquei se os mesmos nomes existem na 2.3, nem mecanismo de extensão, nem suporte em Rust.
- **Rust:** só consultei o crates.io. Não vi se existe crate para OKF ou MADR; não procurei implementação para SACM nem DMN.
- **MADR:** não encontrei JSON Schema oficial; "sem esquema" é por ausência nas páginas que li.
- **in-toto:** o campo de licença do GitHub retornou `NOASSERTION`; a licença Apache-2.0 vem do cabeçalho do arquivo `LICENSE`. Não li o `GOVERNANCE.md` (só vi que existe), então a afirmação de governança é só de existência.
- **OKF:** repositório de 3 meses e sem releases; não avaliei estabilidade real nem adoção fora do Google.
- **DMN:** o documento da especificação (PDF) não foi lido; os nomes vêm do `DMN.xsd` linkado na página oficial (versão `20260504`, associada à 1.7 beta; a 1.6 consta como formal em setembro de 2026).
- **Dublin Core, SKOS, OpenLineage, OSCAL, SPDX, DOAP, Wikidata:** li apenas as páginas citadas, o suficiente para os nomes e as licenças indicados, sem leitura integral.
- **Termos do plano:** o mapeamento para `supersedes`/`supports`/`contradicts` usa a lista de relações da ferramenta `register` do Hexlog (supersedes, revokes, supports, contradicts, answers, derivesFrom, complements, reopens), não uma decisão do dono.
