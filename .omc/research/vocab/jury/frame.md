# Júri: enquadramento da decisão (vocabulários do `hexlog` e do `oh-my-hexflow`)

Plano de origem: [omhl-v2-graph-engine.md](../../../plans/omhl-v2-graph-engine.md). Evidência que todos os jurados devem ler antes de opinar:

- [schema-org.md](../schema-org.md)
- [cloudevents-cdevents.md](../cloudevents-cdevents.md)
- [otel-ecs.md](../otel-ecs.md)
- [prov-activitystreams.md](../prov-activitystreams.md)
- [host-events.md](../host-events.md)
- [others.md](../others.md)
- [hexlog-current-model.md](../../hexlog-current-model.md)

## Contexto fixo (decisões do dono, não rediscutir)

- Motor `hexlog` em Rust, agnóstico; plugin `oh-my-hexflow`; binário `omhl`; CLI e MCP no mesmo binário; SQLite global com projetos como tenants.
- A v1 tem dois grafos: decisões e gates/checks. O conteúdo é só texto de instrução para o agente; nada executa nem bloqueia. O plugin injeta o texto no `SessionStart` de projetos que usam o plugin.
- Ids semânticos (nome estável do nó, ex.: `gates.before-commit.run-tests`); cada revisão tem id técnico próprio; `externalId` aponta para a primeira revisão (a nativa). O projeto estende a base nativa adicionando nós e sobrepondo pelo mesmo id.
- Grafo de decisões herda os tipos premissa, diretriz e decisão; as relações entre entidades são registros próprios. Hexlog atual tem 8 espécies de relação fixas em código (supports, contradicts, answers, derivesFrom, complements, reopens, supersedes, revokes).
- Nó de gate tem os campos `when` (quando executar), `how` (como executar) e `blocking` (informativo na v1). O `when` é estruturado `{vocabulary, event, matcher?}`; na v1 só existe o vocabulário `free`, mas cada vocabulário absorvido depois terá suporte nativo.
- O dono quer apoiar vocabulário e ontologia em padrões ricos, já definidos e curados, para não inventar termos; e deixar uma referência fácil para outros agentes não redescobrirem.

## Decisão 1: forma do `when` dos gates

- **W-A. Vocabulário do host como está.** `vocabulary` é o id do host (git, claude-code, github, codex...), `event` guarda a grafia oficial do host, `matcher` é opaco ao motor e interpretado pelo vocabulário. Sem núcleo comum entre hosts na v1. Empresta só convenções: id de vocabulário por organização (CloudEvents `type`) e pares `sujeito.predicado` (CDEvents, ex.: `change.merged`) para VCS e CI.
- **W-B. Núcleo próprio mínimo mais hosts.** Além dos hosts, o motor define um vocabulário mínimo `{fase: before|after, sujeito: commit|push|pr|test|session|...}` baseado na tabela de equivalência de host-events.md (seções 5 e 6); cada vocabulário de host mapeia para ele e a base nativa escreve os gates no núcleo.
- **W-C. Adotar o CDEvents como núcleo.** `vocabulary: cdevents` com os tipos do CDEvents como eventos canônicos (change.created, change.merged, build.finished, testcaserun.finished...), mais os vocabulários de host. Os pesquisadores registraram: CDEvents não tem commit nem push, nenhum predicado é "antes de", versão embutida no `type`, ainda 0.x.

## Decisão 2: nomes de tipos, relações e revisões do grafo de decisões

- **N-A. Nomes próprios com ponte declarativa.** Tipos e relações com nomes do domínio; cada termo pode declarar `equivalent: <URI>` como metadado (PROV-O specializationOf e wasRevisionOf para revisões, Dublin Core replaces/isReplacedBy para substituição, campos do MADR para a decisão, nomes do SACM para premissa e evidência). O funcionamento não depende de nenhum padrão.
- **N-B. Adotar o PROV-O como modelo.** Revisão como Entity específica (`specializationOf` do id semântico), `wasRevisionOf`, `wasInvalidatedBy`, `wasDerivedFrom`; os tipos de domínio viram subclasses. Os pesquisadores registraram: sem `supersedes`, sem premissa nem evidência, retrospectivo, nenhuma crate PROV.
- **N-C. OKF como envelope mais MADR como campos da decisão.** Nós seguem OKF v0.2 (type livre, sources, status, stale_after) e a decisão segue MADR 4.0.0 (status, decision-makers, drivers, options, outcome); as relações tipadas ficam do motor. Os pesquisadores registraram: OKF tem 3 meses e nenhum release, links sem tipo.

Fora desta decisão (para depois da v1): vocabulário de resultado de gate (in-toto test-result, SARIF) e PROV para registrar execuções de gate.

## Rubrica

1. **Encaixe semântico:** cobre "antes de" e bloqueante (decisão 1); cobre revisão estável, substituição e sobreposição nativa/projeto (decisão 2).
2. **Estabilidade e lock-in:** maturidade, versionamento e o quanto o motor fica preso a uma spec que muda.
3. **Custo na v1 só-texto em Rust:** o que o motor precisaria implementar agora, dependências, licença.
4. **Extensibilidade e legibilidade para o agente:** facilidade de absorver novos vocabulários e clareza do texto que o agente lê.

## Formato do voto

`POSITION: when=<W-A|W-B|W-C>; names=<N-A|N-B|N-C>`, seguido de uma lista curta de "adotar" (termos concretos que entram como dependência de fato) e "emprestar nomes" (termos usados só como referência).
