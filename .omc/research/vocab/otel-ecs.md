# Avaliação: OpenTelemetry Semantic Conventions e Elastic Common Schema (ECS)

- **Contexto:** [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md) (nó de gate com `when` estruturado `{vocabulary, event}`).
- **Data da pesquisa:** 2026-11-06.
- **Versões avaliadas:** OTel Semantic Conventions `v1.45.0` (release 2026-11-02); ECS `v9.5.0` (release 2026-08-04); Weaver `v0.28.0` (2026-10-14).
- **Método:** li os YAML dos registros nas tags acima (`model/` do semconv e `schemas/` do ECS) e a documentação normativa. Nada foi instalado nem executado além de leituras HTTP e de parse dos YAML.

## Resumo em uma frase

OTel cobre bem o **vocabulário de atributos** do ciclo de desenvolvimento (repositório, ref, pipeline, teste, deployment), mas **não define eventos nomeados** para commit, push, PR, review nem a noção de "antes/depois/bloqueante"; ECS dá só categorias genéricas (`event.category/type/action/outcome`), sem VCS nem CI; nenhum dos dois tem ontologia de decisão.

## Notas por critério

| Critério | Nota | Justificativa |
|---|---|---|
| (a) Eventos e ações do ciclo de desenvolvimento | **1** | OTel tem entidades e atributos `vcs.*`, `cicd.*`, `test.*`, `deployment.*`, mas nenhum grupo `event` para commit/push/PR/review; só há spans de pipeline e de task. ECS tem `event.category`, com 20 valores, nenhum de VCS/CI/teste, e `event.action` é texto livre. Nada modela "antes de" nem "bloqueante". |
| (b) Grafo de decisões (decisão, premissa, evidência, substituição) | **0** | Nenhum dos registros tem esses conceitos. OTel tem `entity` e `entity_associations`, que são estrutura de telemetria, e não relação semântica entre decisões. Span Links existem na API de trace, mas só ligam spans; não encontrei relação tipada (não verificado, ver Lacunas). |
| (c) Curadoria e estabilidade | **2** | OTel: mantido pela SIG Semantic Conventions (mantenedores de Dynatrace, Google, Microsoft, Dash0), versões semver mensais, grupos com `development/alpha/beta/release_candidate/stable`, e grupo `stable` não pode regredir. Mas o que nos interessa está imaturo: `vcs` e `cicd` em `release_candidate`, `test` e `deployment` (exceto `deployment.environment.name`) em `development`. ECS: RFC process, semver, `alpha/beta/GA` por campo. |
| (d) Extensão para termos do projeto e hosts | **3** | OTel recomenda prefixar nomes próprios com domínio reverso (`com.acme.x`) e proíbe estender namespaces existentes. Weaver permite registro próprio com `manifest.yaml` + `dependencies` e `imports`. ECS aceita campos custom via `--include` com o mesmo formato YAML. O Claude Code já faz isso: emite `claude_code.*`. |
| (e) Forma legível por máquina e licença | **3** | OTel: YAML do registro (`model/`), JSON Schema da sintaxe (`semconv.schema.json`, `semconv.schema.v2.json` no Weaver), Apache-2.0. ECS: `schemas/*.yml` e `generated/ecs/ecs_flat.yml` (2614 campos), Apache-2.0. |
| (f) Suporte em Rust | **2** | `opentelemetry-semantic-conventions` 0.33.1 (Apache-2.0) só traz constantes geradas; o que é experimental fica atrás da feature `semconv_experimental`. Weaver é escrito em Rust, mas **todos os crates têm `publish = false`**: só entram como dependência git. ECS não tem crate (não encontrei; ver Lacunas) e o gerador é Python. |
| (g) Custo de adoção | **2** | Emprestar nomes é grátis. Adotar o formato de registro exige: parser YAML dos grupos, validação de nomenclatura e de stability, resolução de dependências entre registros (`schema_url` com versão) e, se quisermos o mesmo comportamento do Weaver, depender dele via git. Como o motor é agnóstico e o `when` é `{vocabulary, event}`, o custo real é decidir qual subconjunto vira dado do pacote nativo. |

## Evidências por afirmação

### (a) Cobertura do ciclo de desenvolvimento

- `vcs` (registro): atributos `vcs.repository.url.full`, `vcs.repository.name`, `vcs.ref.{base,head}.{name,type,revision}`, `vcs.change.{id,title,state}` (`open`, `wip`, `closed`, `merged`), `vcs.provider.name` (`github`, `gitlab`, `gittea`, `gitea`, `bitbucket`), `vcs.owner.name`; todos `release_candidate`. Há `entities.yaml`, `metrics.yaml` e `registry.yaml`, **sem `events.yaml` nem `spans.yaml`**. Fonte: <https://github.com/open-telemetry/semantic-conventions/tree/v1.45.0/model/vcs> (v1.45.0, 2026-11-02).
- `cicd`: `cicd.pipeline.{name,result,action.name}`, `cicd.pipeline.run.{id,state,url.full}` (estado `pending`, `executing`, `finalizing`), `cicd.pipeline.task.{name,type}` com tipo `build`, `test`, `deploy`, e `cicd.pipeline.task.run.result` (`success`, `failure`, `error`, `timeout`, `cancellation`, `skip`). Spans `span.cicd.pipeline.run.server` e `span.cicd.pipeline.task.internal`, ambos `release_candidate`. Fonte: <https://github.com/open-telemetry/semantic-conventions/tree/v1.45.0/model/cicd>.
- `test`: `test.suite.name`, `test.suite.run.status`, `test.case.name`, `test.case.result.status`; `development`. Fonte: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/model/test/registry.yaml>.
- `deployment`: `deployment.name`, `deployment.id`, `deployment.status` (`failed`, `succeeded`) em `development`; `deployment.environment.name` (`production`, `staging`, `test`, `development`) é `stable`. Fonte: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/model/deployment/registry.yaml>.
- Eventos em OTel: a convenção geral de eventos é `Status: Development`; evento tem nome que "identifica de forma única a estrutura do evento", sem valores dinâmicos; deve ter timestamp e severidade padrão. Fonte: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/docs/general/events.md>. O único grupo `event` que encontrei nos domínios do ciclo de dev é `feature_flag.evaluation` (`release_candidate`): <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/model/feature-flags/events.yaml>.
- ECS `event.*` (`kind`, `category`, `action`, `outcome`, `type`, `sequence`, `reason`, entre outros): `event.category` tem `api`, `authentication`, `configuration`, `database`, `driver`, `email`, `file`, `host`, `iam`, `intrusion_detection`, `library`, `malware`, `network`, `package`, `process`, `registry`, `session`, `threat`, `vulnerability`, `web`. `event.action` é "normalmente definido pelo implementador". `event.outcome`: `success`, `failure`, `unknown`. `event.type` inclui `start`, `end`, `allowed`, `denied`, `change`, `creation`, `deletion`. Fonte: <https://github.com/elastic/ecs/blob/v9.5.0/schemas/event.yml> (v9.5.0, 2026-08-04).
- ECS não tem fieldset `vcs`, `ci`, `cicd`, `test` ou `deployment` (conferido contra `generated/ecs/ecs_flat.yml`): <https://github.com/elastic/ecs/blob/v9.5.0/generated/ecs/ecs_flat.yml>.

### (b) Grafo de decisões

- OTel define `entity` e `entity_associations` (`one_of`, `all_of`) para dizer quais entidades um sinal deve acompanhar: <https://github.com/open-telemetry/weaver/blob/main/schemas/semconv-syntax.v2.md>. Isso descreve telemetria, e não tipos de nó nem relações de um grafo.
- ECS tem fieldset `entity` ("fields to describe various types of entities across IT environments") com `name`, `id`, `type`, `sub_type`, `source`: <https://github.com/elastic/ecs/blob/v9.5.0/schemas/entity.yml>. Também não modela relações.
- A API de trace permite registrar links entre spans na criação: <https://github.com/open-telemetry/opentelemetry-specification/blob/v1.62.0/specification/trace/api.md>. Não verifiquei se o Link carrega tipo de relação (ver Lacunas).

### (c) Curadoria e estabilidade

- Mantenedores e aprovadores do semconv: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/README.md>. Releases recentes: v1.43.0 (2026-07-03), v1.44.0 (2026-08-04), v1.45.0 (2026-11-02): <https://github.com/open-telemetry/semantic-conventions/releases>.
- Níveis de estabilidade de grupo: `development`, `alpha`, `beta`, `release_candidate`, `stable`; ausente = `development`; `stable` não regride; nenhum grupo é removido, só deprecado: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/docs/general/semantic-convention-groups.md>.
- O próprio registro raiz declara `stability: development` no `manifest.yaml`: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/model/manifest.yaml>. Nomes e chaves de atributo, nomes de spans e de eventos entram no contrato de estabilidade: <https://github.com/open-telemetry/opentelemetry-specification/blob/v1.62.0/specification/versioning-and-stability.md>.
- GenAI saiu do repositório principal e foi para <https://github.com/open-telemetry/semantic-conventions-genai>, cujo `schema_url` é `https://opentelemetry.io/schemas/gen-ai-dev/1.42.0-dev` e o README ainda tem "Schema URL: TODO" (<https://github.com/open-telemetry/semantic-conventions-genai/blob/main/model/manifest.yaml>). Por isso `gen_ai` não é dependência estável.
- ECS: níveis `alpha`, `beta`, `GA` por campo, GA é o padrão; mudanças grandes passam por RFC; semver, majors alinhados à Elastic Stack: <https://github.com/elastic/ecs/blob/v9.5.0/CONTRIBUTING.md> e <https://github.com/elastic/ecs/blob/v9.5.0/README.md>. 324 de 2614 campos estão marcados `beta`.
- ECS acompanha o OTel de longe: `otel-semconv-version` = `v1.38.0` na tag 9.5.0, contra v1.45.0 do OTel (<https://github.com/elastic/ecs/blob/v9.5.0/otel-semconv-version>). 195 campos têm bloco `otel:` com relação `match`, `equivalent`, `related`, `conflict`, `metric`, `otlp` ou `na`: <https://github.com/elastic/ecs/blob/v9.5.0/CONTRIBUTING.md>.

### (d) Extensão

- Regra de nomes próprios: prefixo de domínio reverso (`com.acme.shopname`) ou nome único da aplicação; não usar namespace existente como prefixo. Fonte: <https://github.com/open-telemetry/semantic-conventions/blob/v1.45.0/docs/general/naming.md>.
- Registro próprio com `manifest.yaml` (`name`, `schema_url`, `dependencies[].{schema_url, registry_path}`), reaproveitamento por `ref:` e `imports` de métricas, eventos e entidades; máximo de 10 níveis de registros: <https://github.com/open-telemetry/weaver/blob/main/docs/define-your-own-telemetry-schema.md>.
- ECS: `generator.py --include <dir>` com campos no mesmo formato YAML, e `--subset` para gerar só o necessário: <https://github.com/elastic/ecs/blob/v9.5.0/USAGE.md>.
- Exemplo real de vocabulário de host próprio: o Claude Code emite eventos `claude_code.user_prompt`, `claude_code.tool_result`, `claude_code.tool_decision`, eventos de hook (registrado, início e fim de execução) e métricas `claude_code.commit.count` e `claude_code.pull_request.count`, sem usar `vcs.*`: <https://code.claude.com/docs/en/monitoring-usage.md>. Ou seja, o host já segue a regra "namespace próprio" e não o semconv.

### (e) Forma legível por máquina e licença

- Sintaxe YAML dos grupos (v1 e v2) e JSON Schema: `semconv.schema.json` (draft 2020-12) e `semconv.schema.v2.json` em <https://github.com/open-telemetry/weaver/tree/main/schemas>.
- Weaver (`registry check`, `resolve`, `generate`, `diff`, `live-check`): Apache-2.0, Rust, <https://github.com/open-telemetry/weaver>. Registro semconv: Apache-2.0 (campo `license` da API do GitHub para <https://github.com/open-telemetry/semantic-conventions>; também verifiquei o mesmo para <https://github.com/elastic/ecs>).
- ECS: `schemas/*.yml`, `generated/ecs/ecs_flat.yml`: <https://github.com/elastic/ecs/tree/v9.5.0/schemas>.

### (f) Rust

- `opentelemetry-semantic-conventions` 0.33.1, Apache-2.0, rust-version 1.75, features `default = []` e `semconv_experimental`: <https://crates.io/crates/opentelemetry-semantic-conventions> e <https://github.com/open-telemetry/opentelemetry-rust/blob/main/opentelemetry-semantic-conventions/Cargo.toml>.
- Weaver: workspace com `publish = false` e `rust-version = "1.89.0"`; crates úteis para nós (`weaver_semconv`, `weaver_resolver`, `weaver_forge`) respondem 404 no crates.io: <https://github.com/open-telemetry/weaver/blob/main/Cargo.toml>.

## Veredito

**Adotar subconjunto** (OTel), com ECS reduzido a **só emprestar nomes**.

- **OTel, adotar como vocabulário de atributos do `when`/payload** os termos da tabela abaixo (namespaces `vcs`, `cicd`, `test`, `deployment`, `code`), registrando em cada nó a versão do semconv (`v1.45.0`) e o nível de estabilidade herdado (`release_candidate` ou `development`). Emprestar também as convenções de nome (minúsculas, namespace com ponto, prefixo de domínio reverso para o nosso) e os níveis de estabilidade de grupo.
- **Não** adotar o formato de registro nem o Weaver no v1: o custo é alto e o motor é agnóstico. Reavaliar quando o motor precisar validar vocabulários de terceiros.
- **ECS: rejeitar como base**, por não ter VCS/CI e ter `event.action` livre; emprestar só a ideia de `event.outcome` (`success`, `failure`, `unknown`).
- **Nenhum dos dois resolve o grafo de decisões** (critério b = 0). Esse vocabulário precisa vir de outro candidato (por exemplo, um vocabulário de ontologia/proveniência).
- **Atenção ao `{vocabulary, event}`:** OTel não nomeia eventos de commit/push/PR/review. O `event` de cada host (por exemplo `PreToolUse` do Claude Code, hooks do git, jobs de CI) continua definido pelo host; o OTel só ajuda a descrever os atributos desses eventos.

## Termos concretos úteis

| Termo | Nome oficial | Para que usaríamos |
|---|---|---|
| Repositório | `vcs.repository.url.full`, `vcs.repository.name` | Identificar o repositório em que o gate se aplica |
| Ref de origem | `vcs.ref.head.name`, `vcs.ref.head.revision`, `vcs.ref.head.type` (`branch`, `tag`) | Branch/commit de onde parte uma mudança |
| Ref de destino | `vcs.ref.base.name`, `vcs.ref.base.revision` | Branch-alvo de um PR |
| Mudança (PR) | `vcs.change.id`, `vcs.change.title`, `vcs.change.state` (`open`, `wip`, `closed`, `merged`) | Estado do PR, para condicionar um gate a "PR aberto" |
| Provedor | `vcs.provider.name` (`github`, `gitlab`, `gitea`, `bitbucket`) | Distinguir hosts de VCS |
| Pipeline | `cicd.pipeline.name`, `cicd.pipeline.run.id` | Identificar a pipeline que roda um check |
| Estado da execução | `cicd.pipeline.run.state` (`pending`, `executing`, `finalizing`) | Aproximação de "antes/durante/depois" |
| Resultado | `cicd.pipeline.result`, `cicd.pipeline.task.run.result` (`success`, `failure`, `error`, `timeout`, `cancellation`, `skip`) | Resultado de um check ou gate |
| Tipo de tarefa | `cicd.pipeline.task.type` (`build`, `test`, `deploy`) | Classificar o `how` de um gate |
| Teste | `test.suite.name`, `test.suite.run.status`, `test.case.name`, `test.case.result.status` | Descrever o resultado do check "rodar testes" (`development`) |
| Ambiente | `deployment.environment.name` (`production`, `staging`, `test`, `development`) | Escopo de um gate de deploy (único `stable` do grupo) |
| Estado do deploy | `deployment.status` (`failed`, `succeeded`) | Resultado de um deploy (`development`) |
| Local no código | `code.file.path`, `code.function.name`, `code.line.number` | Apontar onde um achado ocorre |
| Nome de evento | `event.name` (regras em `events.md`) | Regra de nomenclatura: estável, sem valores dinâmicos |
| Resultado genérico | `event.outcome` (ECS: `success`, `failure`, `unknown`) | Só como inspiração do desfecho de um gate |
| Permitido/negado | `event.type` ECS `allowed`, `denied` | Só emprestar o nome para "gate liberou/barrou"; vale apenas para as categorias `api`, `intrusion_detection`, `network` |
| Estabilidade | `development`, `alpha`, `beta`, `release_candidate`, `stable` | Estado de maturidade dos nós nativos |
| Identidade de registro | `schema_url` (ex.: `https://opentelemetry.io/schemas/1.45.0`) | Modelo de versionamento: identifica registro e versão |
| Prefixo próprio | domínio reverso (`com.acme.x`) | Regra para termos do nosso vocabulário sem colidir |
| Host: Claude Code | `claude_code.user_prompt`, `claude_code.tool_result`, `claude_code.tool_decision` | Exemplo do vocabulário nativo do host (não é OTel semconv) |

## Lacunas

- **Propostas em aberto no OTel:** não consegui consultar issues e PRs do repositório (a API do GitHub devolveu 403 por limite de taxa), então não sei se há trabalho em andamento para eventos de VCS (commit, push, review) ou para `test`/`deployment`. Verificar antes da decisão final.
- **Documentação humana de `vcs`, `test`, `deployment`:** li os YAML (campo `stability`), mas as páginas de docs desses namespaces não existem em `docs/<ns>/` nesta tag; o status exibido no site pode divergir do YAML.
- **Relação tipada em Span Links:** confirmei que a API de trace tem links, mas não li a definição de `Link` até o fim; não sei se aceita tipo de relação além de atributos.
- **Módulos `vcs`/`cicd` no crate Rust:** não abri o docs.rs para confirmar quais constantes existem sem `semconv_experimental`.
- **Weaver como dependência:** não testei compilar `weaver_semconv` via git nem medi o tamanho da árvore de dependências.
- **ECS em Rust:** não procurei crates do ECS no crates.io de forma sistemática; afirmo apenas que o gerador oficial é Python.
- **Mapeamento hook a evento:** não mapeei quais eventos do Claude Code (hooks) correspondem a "antes/depois"; os nomes `claude_code.*` vêm da página de monitoramento, não da lista de hooks.
- **Licença de conteúdo da documentação publicada** (sites opentelemetry.io e elastic.co): verifiquei só a licença dos repositórios.
