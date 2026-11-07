# Avaliação de vocabulário: schema.org

- **Data da pesquisa:** 2026-11-06.
- **Versão avaliada:** schema.org 30.1, publicada em 2026-09-16 ([releases](https://schema.org/docs/releases.html); tag GitHub `v30.1`, 2026-09-15, [release](https://github.com/schemaorg/schemaorg/releases/tag/v30.1)).
- **Contexto:** plano [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md). Nó de gate com `when` estruturado `{vocabulary, event}`, `how`, `blocking`; grafos de decisões e de gates.
- **Método:** baixei o dump oficial `schemaorg-current-https.jsonld` (versão 30.1, 1016 classes, 1694 propriedades, [developers](https://schema.org/docs/developers.html)) e consultei a hierarquia, os `domainIncludes`/`rangeIncludes` e os comentários por script. Todo termo citado abaixo existe nesse dump. A URL de cada termo é `https://schema.org/<Termo>`.

## Resumo das notas

| Critério | Nota (0-3) |
|---|---|
| (a) Eventos e ações do ciclo de desenvolvimento | 1 |
| (b) Tipos e relações de um grafo de decisões | 1 |
| (c) Curadoria e estabilidade | 3 |
| (d) Mecanismo de extensão | 2 |
| (e) Forma legível por máquina e licença | 2 |
| (f) Suporte em Rust | 2 |
| (g) Custo de adoção | 2 (só nomes) / 1 (semântica completa) |

## (a) Eventos e ações do ciclo de desenvolvimento: 1

- **O que existe.** `Action` descreve "uma ação praticada por um agente direto e participantes indiretos sobre um objeto direto" ([Action](https://schema.org/Action), 30.1). Propriedades: `agent`, `object`, `instrument`, `result`, `error`, `target`, `startTime`, `endTime`, `participant`, `location`, `provider`, `actionStatus`, `actionProcess`. O ciclo de vida da ação é o enum `ActionStatusType`: `PotentialActionStatus`, `ActiveActionStatus`, `CompletedActionStatus`, `FailedActionStatus` ([ActionStatusType](https://schema.org/ActionStatusType)).
- **Subtipos úteis por analogia.** `CheckAction` (inspeciona e examina "precisão, qualidade, condição ou estado" de um objeto; serve para teste e lint), `ReviewAction`, `InstallAction`, `WriteAction`, `UpdateAction` (`AddAction`, `DeleteAction`, `ReplaceAction`), `AcceptAction`/`RejectAction`/`AuthorizeAction`, `ScheduleAction`, `CancelAction`, `ConfirmAction`, `CommentAction`, `ActivateAction`/`SuspendAction`/`ResumeAction`, `DownloadAction`, `SendAction`.
- **O que falta.** Não há termo para commit, push, pull request, merge, build, deploy, release ou hook. Uma busca no dump por `commit`, `pull`, `deploy`, `build` e `blocking` não achou nenhum termo de desenvolvimento (só falsos positivos: `specialCommitments`, `GovernmentBuilding`). `release` aparece só em música (`MusicRelease`) e em `releaseNotes` de `SoftwareApplication`.
- **Antes/depois e bloqueio.** Não há relação de precedência entre ações nem conceito de gate ou pré-condição. `potentialAction` descreve "uma ação idealizada em que esta coisa teria o papel de `object`" ([potentialAction](https://schema.org/potentialAction)), o que é uma capacidade, não "antes de X". `Event` modela algo "que acontece em certo tempo e lugar, como um show ou palestra" ([Event](https://schema.org/Event)); suas propriedades são de plateia, ingresso e lotação, e `eventStatus` só tem `EventScheduled`, `EventCancelled`, `EventPostponed`, `EventRescheduled`, `EventMovedOnline` ([EventStatusType](https://schema.org/EventStatusType)). Serve mal para evento de host.
- **Achado relevante (pending).** `InstantaneousEvent` ("evento sem duração, como uma entrada de log de computador", `owl:equivalentClass prov:InstantaneousEvent`) e `Error` (subclasse de `sarif:Result` e de `InstantaneousEvent`) estão em `pending` ([pending.schema.org](https://pending.schema.org), issues [#4527](https://github.com/schemaorg/schemaorg/issues/4527) e [#4579](https://github.com/schemaorg/schemaorg/issues/4579)). Indicam que o projeto começa a aceitar vocabulário de telemetria, mas ainda sem consenso.

## (b) Grafo de decisões: 1

- **O que existe.**
  - `Claim` (afirmação factual que pode ser `itemReviewed` de um `ClaimReview`), subtipo de `CreativeWork`; propriedades próprias: `appearance`, `firstAppearance`, `claimInterpreter` ([Claim](https://schema.org/Claim)).
  - `Statement` ("afirmação sobre algo"; para afirmações formais manda usar `Claim`) ([Statement](https://schema.org/Statement)).
  - `Review` com `itemReviewed`, `reviewRating`, `reviewBody`, `positiveNotes`, `negativeNotes` ([Review](https://schema.org/Review)).
  - `Question` e `Answer` (subtipos de `Comment`).
  - Arestas genéricas de `CreativeWork`: `isBasedOn`, `isPartOf`, `hasPart`, `citation`, `mentions`, `about`, `version`, `creativeWorkStatus` (em `pending`: "Incomplete, Draft, Published, Obsolete"), `expires`.
  - `Role` para atributos de aresta (`startDate`, `endDate`, `roleName`) ([Role](https://schema.org/Role)).
- **O que falta.** Não existe `Decision`, `Assumption`/premissa, `Evidence`, `Rationale` ou `Hypothesis`. O único `evidence*` do vocabulário é médico (`evidenceLevel`, `evidenceOrigin`, `MedicalEvidenceLevel`).
- **Substituição de uma decisão por outra.** `supersededBy` existe, mas o domínio e o intervalo são `Class`, `Enumeration` e `Property` ([supersededBy](https://schema.org/supersededBy)): serve para aposentar termos do vocabulário, não instâncias. Para instâncias restaria `isBasedOn` (derivação, sem semântica de "invalida") ou `creativeWorkStatus: Obsolete`, ambos fracos. O motor já tem `supersedes`/`revokes` próprios no Hexlog atual; schema.org não acrescenta nada aqui.
- **Escopo declarado.** "schema.org não pretende ser uma ontologia universal" e deve ser usado ao lado de outros vocabulários que compartilham o modelo de dados ([datamodel](https://schema.org/docs/datamodel.html)).

## (c) Curadoria e estabilidade: 3

- **Quem mantém.** Projeto colaborativo fundado por Google, Microsoft, Yahoo e Yandex; o fórum principal é o W3C Schema.org Community Group desde abril de 2015; Steering Group aprova releases por acordo unânime; o webmaster (Dan Brickley) publica ([about](https://schema.org/docs/about.html), [howwework](https://schema.org/docs/howwework.html)). O projeto declara que não é governado pelo W3C nem pelo W3C Process.
- **Versionamento.** Releases numeradas, com planejamento público no GitHub. Ritmo recente: 29.0 (2025-03-24), 29.1 (2025-04-25), 29.2 (2025-05-13), 29.3 (2025-09-04), 29.4 (2025-12-08), 30.0 (2026-03-19), 30.1 (2026-09-16) ([releases](https://schema.org/docs/releases.html)). O repositório recebeu push em 2026-11-06 (API do GitHub, 6298 estrelas).
- **Estabilidade.** Termos aposentados vão para o "attic" (`schemaorg-all-https` os inclui) e ganham `supersededBy` ([developers](https://schema.org/docs/developers.html)). Termos em `pending` (184 em 30.1) podem ser promovidos ou descartados ([extension](https://schema.org/docs/extension.html)); portanto não ancorar nada neles. A numeração major sobe a cada poucos meses, mas isso é só contador de release (a 15.0 é de 2022-10, a 30.1 é de 2026-09), não quebra de compatibilidade documentada.

## (d) Mecanismo de extensão: 2

- **Hosted extensions.** O mecanismo de subdomínios (`bib.`, `auto.`, `health-lifesci.`) foi absorvido: desde abril de 2019 seus termos são "totalmente parte" do schema.org e só mantêm um rótulo ([extension](https://schema.org/docs/extension.html)). Não serve como caminho para termos do projeto.
- **`pending`.** Rótulo reservado para termos experimentais; entrada de novos termos agora passa por ele. Exige proposta no GitHub e consenso do Steering Group, ou seja, não é caminho para termos privados do plugin ([howwework](https://schema.org/docs/howwework.html)).
- **Extensões externas.** São o caminho para o nosso caso: terceiros publicam vocabulário em outro namespace e o combinam com schema.org. O exemplo oficial é o GS1 Web Vocabulary, para o qual "nenhuma permissão ou consenso foi necessário" ([howwework](https://schema.org/docs/howwework.html)). `additionalType` liga uma coisa a uma classe externa ([additionalType](https://schema.org/additionalType)); `@vocab`/prefixos do JSON-LD permitem misturar. O `jsonldcontext.json` oficial já traz prefixos de outros vocabulários (`bibo`, `brick`, `cmns-cls`, entre outros).
- **Limitação.** Não há registro de extensões de terceiros, nem política de versão para elas. O modelo é "publique em seu namespace". Para o `when.vocabulary` isso significa que cada host (Claude Code, git, CI) viraria um namespace próprio nosso, sem nada oficial do schema.org por trás.

## (e) Forma legível por máquina e licença: 2

- **Formatos.** Dump completo em JSON-LD, Turtle, N-Triples, N-Quads, RDF/XML e CSV (tipos e propriedades separados), em `https` e `http`, com e sem o "attic" ([developers](https://schema.org/docs/developers.html)). Medido aqui: `schemaorg-current-https.jsonld` com 1,5 MB e `jsonldcontext.json` com 210 KB e 3103 entradas. Não há JSON Schema oficial.
- **Cuidado com o contexto.** O contexto JSON-LD oficial usa `http://schema.org/` como `@vocab`, e o projeto diz que não vai mudar isso sem aviso longo ([developers](https://schema.org/docs/developers.html)). Termos podem aparecer com `http` ou `https`; consumidores devem aceitar os dois.
- **Licença.** Os direitos autorais do esquema são licenciados sob CC BY-SA 3.0, com compromisso de patentes nos termos do W3C Patent Policy (RF) ([terms](https://schema.org/docs/terms.html)). O código do site é Apache-2.0 ([repositório](https://github.com/schemaorg/schemaorg)). Atribuição e share-alike valem se o binário embutir cópia do vocabulário; copiar só nomes de termos provavelmente não traz obrigação, mas não verifiquei isso juridicamente (ver Lacunas). A nota é 2 por causa dessa licença e do contexto em `http`; o formato em si merece 3.

## (f) Suporte em Rust: 2

- **Oficial:** nenhum crate oficial do schema.org.
- **Geração de tipos, comunidade:** `schemy` 0.1.1 e `schemaorg-rs` 0.1.0 (ambos MIT, abril de 2025, repositório [frectonz/schemy](https://github.com/frectonz/schemy)) geram structs do JSON-LD do vocabulário; `schemaorg` 0.1.0 (2023, codegen) está parado. `schemaorg-validate` 0.3.0 (MIT, 2026-05-29) valida dados estruturados contra o vocabulário oficial. Todos têm baixa adoção (centenas a pouco mais de mil downloads), e os de geração têm cerca de 18 meses sem release, ou seja, defasados perante a 30.1. Fonte: [crates.io API](https://crates.io/api/v1/crates?q=schemaorg).
- **JSON-LD em geral:** `json-ld` 0.22.0 (MIT/Apache-2.0, 2026-10-30, mais de 690 mil downloads, [repositório](https://github.com/timothee-haudebourg/json-ld)), `oxjsonld` 0.2.7 (MIT/Apache-2.0, 2026-10-18, mais de 1 milhão de downloads, parte do Oxigraph, [repositório](https://github.com/oxigraph/oxigraph/tree/master/lib/oxjsonld)) e `sophia_jsonld` 0.10.0. Maduros e mantidos.
- **Sem crate.** Para só emprestar nomes bastam um `serde_json` e uma tabela embutida.

## (g) Custo de adoção: 2 (só nomes) / 1 (semântica completa)

- **Só emprestar nomes e IRIs (custo baixo).** O motor guarda, no campo `vocabulary`, um identificador de namespace e, no `event`, o nome do termo. Uma tabela estática com poucos termos (ver abaixo) cobre o caso. Nada de parser RDF.
- **Adotar o modelo semântico (custo alto).** Exigiria: carregar e embutir o contexto (offline, para não depender de rede), tratar `http` vs `https`, expandir/compactar JSON-LD, resolver `subClassOf` e `domainIncludes` e validar valores. Nada disso é necessário para texto de instrução na v1.
- **Descasamento de forma.** `Action` descreve uma ação com agente, objeto e resultado, enquanto o `when` do plano pede "em qual momento do ciclo isto vale". Teríamos de inventar (e hospedar) de qualquer jeito os termos `before-commit`, `after-push` etc. num namespace nosso.

## Veredito

**Só emprestar nomes**, com um subconjunto pequeno de `Action`/`ActionStatusType` como base do campo `event` e do estado de execução (`PotentialActionStatus`, `ActiveActionStatus`, `CompletedActionStatus`, `FailedActionStatus`). O schema.org não cobre eventos do ciclo de desenvolvimento nem o modelo de decisão (premissa, evidência, substituição), e não tem lugar para vocabulário de hosts. Adotar inteiro não se justifica: o custo (JSON-LD, contexto em `http`, CC BY-SA) é maior que o ganho. Rejeitar também não: os termos de `Action` são curados, estáveis, e dão nomes prontos para o que o motor vai querer dizer em gates (verificar, instalar, revisar, cancelar).

## Termos concretos úteis

| Termo | URI ou nome oficial | Para que usaríamos |
|---|---|---|
| Action | `https://schema.org/Action` | Tipo-raiz de "alguma coisa feita" no `when` e `how` de um gate. |
| CheckAction | `https://schema.org/CheckAction` | Nome base para a etapa de verificação mecânica (rodar teste, lint). |
| ReviewAction | `https://schema.org/ReviewAction` | Evento de revisão (code review) no `when`. |
| InstallAction | `https://schema.org/InstallAction` | Evento de instalação de dependência ou plugin. |
| UpdateAction / ReplaceAction | `https://schema.org/UpdateAction`, `https://schema.org/ReplaceAction` | Nome para "substituir uma revisão por outra" (apenas como nome). |
| AcceptAction / RejectAction / AuthorizeAction | `https://schema.org/AcceptAction`, `https://schema.org/RejectAction`, `https://schema.org/AuthorizeAction` | Resultado de gate bloqueante (aprovar, rejeitar, autorizar). |
| ScheduleAction / CancelAction | `https://schema.org/ScheduleAction`, `https://schema.org/CancelAction` | Gate agendado ou desligado (marco futuro). |
| ActionStatusType | `https://schema.org/ActionStatusType` | Estado de execução de um gate: Potential, Active, Completed, Failed. |
| error | `https://schema.org/error` | Causa de falha em uma ação `Failed`. |
| actionProcess | `https://schema.org/actionProcess` | Ponteiro de `Action` para o `how` (um `HowTo`). |
| HowTo / HowToStep | `https://schema.org/HowTo`, `https://schema.org/HowToStep` | Forma do campo `how` quando for lista de passos. |
| Claim | `https://schema.org/Claim` | Candidato a nome da premissa, só como nome (não traz semântica de decisão). |
| isBasedOn | `https://schema.org/isBasedOn` | Aresta de derivação entre nós (ex.: decisão derivada de outra). |
| Role | `https://schema.org/Role` | Padrão para atributos de aresta (período, nome do papel). |
| InstantaneousEvent (pending) | `https://schema.org/InstantaneousEvent` | Nome para evento pontual de host; **não usar** enquanto estiver em `pending`. |

## Lacunas

- **Licença CC BY-SA 3.0 e embutir o vocabulário.** Não verifiquei se embutir um subconjunto do dump (IRIs e comentários) no binário ou no repositório obriga a atribuição e a licença share-alike no plugin. Precisa de parecer jurídico ou do dono. Nomes de termos isolados provavelmente não têm obrigação, mas isto é suposição.
- **Política formal de compatibilidade.** Achei o "attic" e `supersededBy` ([developers](https://schema.org/docs/developers.html)), mas não achei texto que prometa que termos publicados nunca mudam de significado. Não verifiquei o histórico de mudanças semânticas de `Action`.
- **Uso real de `Action` em desenvolvimento.** Não procurei se alguma ferramenta de CI, git ou IDE publica `Action` do schema.org para eventos de desenvolvimento; não encontrei nada nas fontes primárias consultadas, mas não fiz busca ampla.
- **`pending` em 30.1.** Contei 184 termos pelo dump (`isPartOf pending.schema.org`), mas não li as propostas `InstantaneousEvent` e `Error` nas issues do GitHub (#4527, #4579), então não sei se vão ser promovidos.
- **Qualidade dos crates de geração.** Verifiquei apenas data, licença e downloads de `schemy`/`schemaorg-rs`; não compilei nem comparei a saída com a versão 30.1.
- **Teste de `json-ld`/`oxjsonld` com o contexto do schema.org.** Não executei nenhum crate Rust; a avaliação de (f) vem do README e do crates.io. Não confirmei conformidade JSON-LD 1.1 do `json-ld`.
- **Relação com outros candidatos.** `InstantaneousEvent` é equivalente a `prov:InstantaneousEvent` e `Error` herda de `sarif:Result`: vale cruzar com as notas de PROV-O, SARIF e OpenTelemetry/CloudEvents, que outros pesquisadores avaliam.
