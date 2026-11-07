# Revisão do glossary contra o schema.org 30.1

Referência para qualquer agente que precise do melhor `equivalent` do schema.org para um termo do projeto: leia este arquivo antes de pesquisar de novo. Termos revisados: [GLOSSARY.md](../../../GLOSSARY.md). Nota anterior sobre o schema.org: [schema-org.md](schema-org.md). Veredito do júri sobre nomes: [jury-verdict.md](jury-verdict.md).

Revisor: um subagente opus (a pedido do dono, exceção à regra de sonnet e haiku). Fonte: o dump oficial `schemaorg-current-https.jsonld` (3256 nós) baixado em 2026-11-07; a versão atual é 30.1, de 2026-09-16. Nomes, domínios, intervalos e definições foram lidos no dump; se cada termo encaixa é inferência do revisor. A URL de cada termo é `https://schema.org/<Termo>`.

## Resultado

Nenhum TROCAR. Quatro CONSIDERAR (Gate, Decision, Catalog, Author), todos sobre o `equivalent` e não sobre o nome. Os outros 28 são MANTER, alguns com um `equivalent` honesto e outros sem nenhum.

## Termos com `equivalent` honesto

| Termo | schema.org mais próximo | Observação |
|---|---|---|
| Node | `schema:Thing` | genérico demais, mas correto |
| Relation | `schema:Role` ("informação adicional sobre uma relação", com startDate e endDate) | aproximação, como aresta reificada |
| Relation type | `schema:Property` (equivale a rdf:Property) | |
| Check | `schema:HowTo` | aproximação. **Não** `CheckAction`, que é o ato de inspecionar |
| Attestation | `schema:CheckAction` concluída com `result` | aproximação. **Não** `Certification` (autoridade externa, só active e inactive) |
| Premise | `schema:Statement` | parcial |
| Decision | `schema:ChooseAction` (com `actionOption` e `result`) | descreve o ato de escolher, não o registro com evidência embutida |
| Catalog | `schema:DefinedTermSet` | cada tipo de node seria um `DefinedTerm` e cada relation type um `Property`. **Nunca** `DataCatalog` (coleção de datasets) |
| Author | `schema:author` | só aceita Person ou Organization; um agente de IA fica fora do intervalo e o modelo usado é `instrument`. Guardar o modelo em campo próprio |
| External id | `schema:identifier` (PropertyValue e `propertyID`) | |
| Semantic id | `schema:identifier` | `exampleOfWork` faz a ponte da revision para o node |
| Record id | `schema:identifier` | |
| Gate | `schema:ActionAccessSpecification` | só aproximação: as propriedades são comerciais e o domínio de `actionAccessibilityRequirement` é só `ConsumeAction` |
| Projection | `schema:DataDownload` | fraco: cobre a forma do arquivo, não a derivação |

## Termos sem `equivalent`

Record, Tenant, Batch, Host, Chain, Snapshot, Work item, Native, Extension, Graph, Guideline, Lineage, Latest record, Current revision, Chain head, Retirement, Revision (o PROV já cobre), Origin.

Falsos amigos a evitar: `Project` (é uma Organization, por isso `Tenant` não deve se chamar project), `MedicalGuideline` e `guideline` (só saúde), `Recommendation` (é uma Review), `version` (é um rótulo, por isso `Revision` não deve se chamar version), `FailedActionStatus` (significa "falhou em completar", e não "rodou e reprovou").

## Achados que corrigem a nota anterior

1. A nota [schema-org.md](schema-org.md) diz que "não existe Decision"; `ChooseAction` cobre o ato de decidir.
2. A mesma nota diz que "não há conceito de gate ou pré-condição"; `ActionAccessSpecification` é um conjunto de requisitos para executar uma Action, mas só em termos comerciais.
3. `ActionStatusType` não serve para o desfecho de uma attestation (passou, passou com alerta, falhou): não tem estado de alerta, e um teste que rodou e reprovou é `CompletedActionStatus` com `result` de falha.

## Limites

- O revisor usou o dump e não abriu cada página HTML; presume o mesmo conteúdo.
- Não olhou o attic atrás de termos aposentados.
- `InstantaneousEvent`, `RuntimePlatform`, `creativeWorkStatus` e `Error` estão em pending e podem cair.
- A licença CC BY-SA ficou fora do escopo.
- Não comparou com PROV, DCAT ou Dublin Core além dos `owl:equivalent*` declarados no próprio dump.
