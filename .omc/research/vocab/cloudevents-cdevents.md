# Avaliação: CloudEvents (CNCF) e CDEvents (CD Foundation)

- **Contexto:** vocabulário do `when` dos nós de gate do motor v2, conforme [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md).
- **Data da consulta:** 2026-11-06. Todas as fontes são primárias (repositórios e especificações oficiais). O que não foi verificado está em "Lacunas".
- **Versões avaliadas:** CloudEvents spec v1.0.2 (release de 2022-02-06; `main` está em 1.0.3-wip). CDEvents spec v0.5.1 (release de 2026-04-15; `main` está em 0.6.0-draft).

## Resumo em uma linha por critério

| Critério | Nota (0-3) | Em uma frase |
|---|---|---|
| (a) Eventos do ciclo de desenvolvimento | 2 | CDEvents cobre repositório, PR, build, teste e deploy; não tem commit/push nem "antes de" nem bloqueio. |
| (b) Grafo de decisões | 1 | Só `links` do CDEvents dá uma ideia de relação entre eventos; nada de decisão, premissa, evidência ou substituição. |
| (c) Curadoria e estabilidade | 2 | CloudEvents é graduado no CNCF e estável, mas a spec não tem release desde 2022; CDEvents é incubado, ativo e ainda 0.x. |
| (d) Extensão | 3 | Prefixo reverse-DNS no `type`, atributos de extensão e `dev.cdeventsx.` com registro de eventos customizados. |
| (e) Forma legível por máquina e licença | 3 | JSON Schema para os dois, cerca de 50 schemas de eventos no CDEvents, Apache-2.0. |
| (f) Rust | 2 | Existem crates oficiais, mas o `cdevents-sdk` é tipado por versão da spec e tem pouca adoção. Adotar só os nomes dispensa os crates. |
| (g) Custo de adoção | 2 | Emprestar nomes custa quase nada; adotar o envelope e a semântica de evento ocorrido exige implementar o que o `when` não é. |

## (a) Cobre eventos e ações do ciclo de desenvolvimento: nota 2

- CDEvents cobre os estágios Core, Source Code Version Control, Continuous Integration, Testing, Continuous Deployment e Continuous Operations. Fonte: [spec.md, seção Vocabulary Stages](https://github.com/cdevents/spec/blob/main/spec.md), spec 0.6.0-draft em `main`.
- Tipos de VCS: `repository.{created,modified,deleted}`, `branch.{created,deleted}`, `change.{created,updated,reviewed,merged,abandoned}`. Fonte: [source-code-version-control.md](https://github.com/cdevents/spec/blob/main/source-code-version-control.md). Outros estágios: [continuous-integration.md](https://github.com/cdevents/spec/blob/main/continuous-integration.md), [testing-events.md](https://github.com/cdevents/spec/blob/main/testing-events.md), [continuous-deployment.md](https://github.com/cdevents/spec/blob/main/continuous-deployment.md), [continuous-operations.md](https://github.com/cdevents/spec/blob/main/continuous-operations.md).
- **Lacunas de cobertura:** não existe evento de commit nem de push (o texto só menciona "commits" dentro de `change.updated`; `grep -i push` no arquivo de VCS retorna 0). Todos os predicados são passado ou estado (`created`, `started`, `finished`, `merged`); não há "antes de" e não há noção de bloqueante. O próprio `change.reviewed` descreve aprovação/rejeição na prosa, mas o conteúdo da tabela só tem `id`, `source` e `repository`, sem campo de veredito.
- Também não há eventos de sessão de agente (prompt, uso de ferramenta, parada); isso é vocabulário de host, fora do escopo do CDEvents.

## (b) Serve para tipos e relações de um grafo de decisões: nota 1

- Nenhum dos dois define decisão, premissa, evidência ou "substitui". O modelo é de evento ocorrido, não de entidade versionada.
- O mais próximo é `links` do CDEvents: `linkType` `PATH`, `RELATION` (com `linkKind`, o exemplo documentado é `TRIGGER`) e `END`, mais `chainId`. Fonte: [links.md](https://github.com/cdevents/spec/blob/main/links.md) e [spec.md, seção links](https://github.com/cdevents/spec/blob/main/spec.md), 0.6.0-draft. Serve de inspiração para modelar relação entre eventos, não para as relações do grafo de decisões.
- O atributo `subject` do CloudEvents identifica o objeto do evento no contexto do produtor. Fonte: [cloudevents/spec.md, seção subject](https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md). Não carrega relação.

## (c) Curadoria e estabilidade: nota 2

- **CloudEvents:** projeto CNCF, sandbox em 2018-05-15, incubação em 2019-10-24, graduado em 2024-01-25. Fonte: [README do cloudevents/spec](https://github.com/cloudevents/spec/blob/main/README.md). Releases da spec: v1.0 (2019-10-24), v1.0.1 (2020-12-12), v1.0.2 (2022-02-06); `main` em 1.0.3-wip. Fonte: [releases do cloudevents/spec](https://github.com/cloudevents/spec/tags). O CloudEvents SQL (`cesql`) saiu em v1.0.0 em 2024-06-17. Estável e quase parado; ótimo para base, lento para mudar.
- **CDEvents:** projeto incubado da CD Foundation. Fonte: [README do cdevents/spec](https://github.com/cdevents/spec/blob/main/README.md). Releases: v0.4.0 (2024-04-11), v0.4.1 (2024-05-16), v0.5.0 (2025-12-22), v0.5.1 (2026-04-15); `main` em 0.6.0-draft com último push em 2026-11-02. Fonte: [releases do cdevents/spec](https://github.com/cdevents/spec/releases). Ativo, mas pré-1.0: cada tipo de evento tem versão própria e ela muda (ex.: `change.created` está em 0.4.0 enquanto `change.merged` está em 0.3.0).
- Governança do CDEvents vive no repositório da comunidade. Fonte: [governance.md](https://github.com/cdevents/spec/blob/main/governance.md).

## (d) Mecanismo de extensão: nota 3

- **CloudEvents `type`:** SHOULD ter prefixo reverse-DNS "que dita a organização que define a semântica"; exemplos `com.github.pull_request.opened`, `com.example.object.deleted.v2`. Fonte: [cloudevents/spec.md, atributo type](https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md). Isso mapeia direto para `vocabulary` no `when`: cada host ou projeto define o próprio prefixo sem colidir.
- **CloudEvents atributos de extensão:** qualquer número de atributos extras, nomes `[a-z0-9]`, até 20 caracteres (SHOULD), `data` reservado. Fonte: [cloudevents/spec.md, Naming Conventions e Extension Context Attributes](https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md). Lista de extensões documentadas (`sequence`, `dataref`, `severity`, `correlation`, `deprecation` e outras) em [extensions/README.md](https://github.com/cloudevents/spec/blob/main/cloudevents/extensions/README.md); entrar nessa lista exige apoio de duas organizações votantes.
- **CDEvents:** tipos `dev.cdevents.` são fechados pelo vocabulário; `dev.cdeventsx.<tool>-<subject>.<predicate>.M.m.p` são "custom events" definidos fora da spec, com registro em `custom/registry.md`, mais `customData` e `customDataContentType` em todo evento. Fontes: [spec.md, atributo type e custom data](https://github.com/cdevents/spec/blob/main/spec.md) e [custom/README.md](https://github.com/cdevents/spec/blob/main/custom/README.md). O README de custom descreve como promover um evento custom a padrão.

## (e) Forma legível por máquina e licença: nota 3

- **CloudEvents:** JSON Schema do envelope (draft-07), mais Avro e Protobuf. Fonte: [cloudevents/formats/cloudevents.json](https://github.com/cloudevents/spec/blob/main/cloudevents/formats/cloudevents.json). Licença Apache-2.0. Fonte: [LICENSE](https://github.com/cloudevents/spec/blob/main/LICENSE).
- **CDEvents:** 52 entradas em `schemas/` (inclui o diretório `_defs`; o resto é um arquivo por tipo de evento), JSON Schema 2020-12, com `$id` do tipo `https://cdevents.dev/0.6.0-draft/schema/change-reviewed-event` (o `$id` ainda diz `0.6.0-draft` em `main`). Fonte: [schemas/](https://github.com/cdevents/spec/tree/main/schemas). Exemplos de conformidade em [conformance/](https://github.com/cdevents/spec/tree/main/conformance) e registro de `provider` em `schemas/registry/providers.json` ([PROVIDERS.md](https://github.com/cdevents/spec/blob/main/PROVIDERS.md)). Licença Apache-2.0 (API do GitHub, verificada em 2026-11-06).

## (f) Suporte em Rust: nota 2

- `cloudevents-sdk` 0.9.0 (2025-11-30), ~1,77 milhão de downloads, Apache-2.0. O README oficial avisa "WIP... all APIs are considered unstable", com suporte à spec v1.0, formato JSON e bindings HTTP, Kafka e NATS (sem AMQP, MQTT, Avro ou webhook). Fontes: [crates.io/cloudevents-sdk](https://crates.io/crates/cloudevents-sdk) e [README do sdk-rust](https://github.com/cloudevents/sdk-rust/blob/master/README.md).
- `cdevents-sdk` 0.4.2 (2026-11-01), ~8 mil downloads, Apache-2.0. Gera structs tipadas por versão da spec (ex.: `spec_0_5_1::pipelinerun_finished`) e depende de `cloudevents-sdk` de forma opcional (feature `cloudevents`, ligada por padrão). Fontes: [crates.io/cdevents-sdk](https://crates.io/crates/cdevents-sdk), [README](https://github.com/cdevents/sdk-rust/blob/main/README.md) e [Cargo.toml](https://github.com/cdevents/sdk-rust/blob/main/cdevents-sdk/Cargo.toml).
- **Custo sem crates:** o `when` guarda só `{vocabulary, event}` como texto, então não precisa de envelope nem de structs tipadas. Os crates só pagam o custo se o motor passar a consumir ou emitir eventos CloudEvents de verdade, o que a v1 exclui.

## (g) Custo de adoção: nota 2

- **Emprestar nomes e convenções (baixo):** reaproveitar o par `sujeito.predicado` do CDEvents (ex.: `change.merged`) e a regra de prefixo por organização do CloudEvents para o campo `vocabulary`. Nada para implementar além de uma tabela de nomes.
- **Adotar o envelope e a semântica (alto):** exigiria `id`, `source`, `specversion`, `type`, `time` por ocorrência e a distinção produtor/consumidor. Isso descreve um evento que já ocorreu, e o `when` do gate descreve uma condição futura. A ponte natural é o modelo de subscription do CloudEvents, que filtra por `exact`, `prefix` e outros dialetos sobre atributos (ex.: `{"prefix": {"type": "com.example."}}`), mas a spec está em `0.1-wip`. Fonte: [subscriptions/spec.md](https://github.com/cloudevents/spec/blob/main/subscriptions/spec.md), seção 3.2.4.1.
- **Versão dentro do `type` (atrito):** o CDEvents embute `M.m.p` no `type` (`dev.cdevents.change.merged.0.3.0`). Isso conflita com ter `vocabulary` e `event` separados; a versão teria de ir para o `vocabulary` (ex.: `cdevents@0.5`). Fonte: [spec.md, atributo type](https://github.com/cdevents/spec/blob/main/spec.md).

## Veredito

**Só emprestar nomes.** Adotar a convenção de nomes do CloudEvents (prefixo por organização como separador de `vocabulary`) e os termos `sujeito.predicado` do CDEvents para o vocabulário de VCS/CI/deploy, sem adotar envelope, SDK nem versão no `type`. Motivo: nenhum dos dois tem "antes de", bloqueio, commit/push, sessão de agente ou grafo de decisões, e o `when` da v1 é só texto. Se o motor um dia emitir ou receber eventos reais, reavaliar `cloudevents-sdk` e `cdevents-sdk`.

## Termos concretos úteis

| Termo | Nome oficial | Para que usaríamos |
|---|---|---|
| Convenção de namespace do evento | `type` com prefixo reverse-DNS (CloudEvents) | Modelo para o campo `vocabulary` de cada host (ex.: git, ci, claude-code), evitando colisão entre vocabulários |
| Padrão de nome | `<subject>.<predicate>` (CDEvents) | Formato do campo `event` dentro de um vocabulário (ex.: `change.merged`) |
| PR criado | `dev.cdevents.change.created` | Evento `change.created` no vocabulário de VCS |
| PR revisado | `dev.cdevents.change.reviewed` | Evento `change.reviewed` (sem campo de veredito no conteúdo) |
| PR integrado | `dev.cdevents.change.merged` | Evento `change.merged` |
| PR atualizado | `dev.cdevents.change.updated` | Evento `change.updated` (novo commit no PR) |
| PR abandonado | `dev.cdevents.change.abandoned` | Evento `change.abandoned` |
| Branch criada/removida | `dev.cdevents.branch.created`, `dev.cdevents.branch.deleted` | Eventos `branch.created` e `branch.deleted` |
| Repositório | `dev.cdevents.repository.{created,modified,deleted}` | Eventos de ciclo de vida do repositório |
| Build | `dev.cdevents.build.{queued,started,finished}` | Eventos `build.*` do vocabulário de CI |
| Execução de teste | `dev.cdevents.testcaserun.{queued,started,finished,skipped}`, `dev.cdevents.testsuiterun.{queued,started,finished}` | Eventos `testcaserun.*` e `testsuiterun.*` |
| Artefato | `dev.cdevents.artifact.{packaged,published,signed,downloaded,deleted}` | Eventos de artefato |
| Deploy | `dev.cdevents.service.{deployed,upgraded,rolledback,removed,published}` | Eventos `service.*` de deploy |
| Ambiente | `dev.cdevents.environment.{created,modified,deleted}` | Eventos de ambiente |
| Pipeline e tarefa | `dev.cdevents.pipelinerun.*`, `dev.cdevents.taskrun.*` | Eventos de orquestração do CI |
| Tipo customizado | prefixo `dev.cdeventsx.<tool>-<subject>.<predicate>` | Modelo de como registrar um vocabulário de terceiro sem mexer no padrão |
| Relação entre eventos | `links` (`linkType` `PATH`/`RELATION`/`END`, `linkKind` `TRIGGER`) e `chainId` | Inspiração para relações, não para decisões |
| Filtro por atributo | dialetos `exact` e `prefix` da Subscriptions API (CloudEvents) | Semântica para casar o `when` com um evento (`prefix` cobre todo um vocabulário) |

## Lacunas

- **Não verificado:** o conjunto completo de valores de `linkKind` no CDEvents (a spec mostra só `TRIGGER` em exemplos).
- **Não verificado:** se os eventos `approval.*` (existem schemas `approvalcreated`, `approvalupdated` e `approvalclosed` em `main`, `$id` 0.6.0-draft) já têm prosa na spec ou entram no release 0.6.0; os docs de CI/CD/Core/Operations não os mencionam. A verificação por GitHub API foi interrompida por limite de requisições.
- **Não verificado:** a lista completa de arquivos em `schemas/` por nome e o conteúdo de `custom/registry.md` além do cabeçalho (a tabela de registro estava vazia no trecho lido).
- **Não verificado:** os dialetos `suffix`, `all`, `any`, `not` e `sql` da Subscriptions API; só `exact` e `prefix` foram lidos.
- **Não verificado:** se o CDEvents define uma noção de "antes de" em algum documento fora do repositório da spec (primer em cdevents.dev não foi lido).
- **Não verificado:** a lista de extensões documentadas foi lida só pelos nomes dos arquivos, não o conteúdo de cada uma.
- **Fora do escopo do candidato:** nenhum dos dois cobre eventos de sessão de agente (Claude Code) nem commit/push; precisa de outro vocabulário para isso.
- **Pontos de risco registrados:** o README do `cloudevents-sdk` se declara em desenvolvimento ativo com APIs instáveis; o CDEvents está em 0.x e muda versão por evento.
