# Veredito do júri: vocabulários do `hexlog` e do `oh-my-hexflow`

Referência para qualquer agente que precise de termos de vocabulário no motor ou no plugin: leia este arquivo antes de pesquisar de novo. Enquadramento: [jury/frame.md](jury/frame.md). Posições da rodada 1: [jury/round1-anonymized.md](jury/round1-anonymized.md). Plano de origem: [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md).

## Veredito

**Decisão 1, forma do `when` dos gates: W-A.** `when = {vocabulary, event, matcher?}`, com o vocabulário do host como está. Na v1 só existe o vocabulário `free`. O `event` guarda a grafia oficial do host. O `matcher` é opaco para o motor e é interpretado pelo vocabulário. Não há núcleo comum entre hosts na v1.

**Decisão 2, nomes de tipos, relações e revisões do grafo de decisões: N-A.** Nomes próprios do domínio, mais o campo opcional `equivalent: <URI>` como metadado informativo. O motor guarda a URI, só valida a sintaxe e nunca a consulta para decidir comportamento. Nenhuma dependência de código de padrão externo.

**Confiança:** MEDIUM nas duas. **Placar da rodada 1:** W-A 274 contra W-B 70; N-A 292 contra N-C 52. **Rodada 2:** 5 de 5 em W-A e N-A, com dois jurados mudando de voto e citando argumento novo. Contagem sem sinal de bandwagon. A rodada 1 isolada leva ao mesmo resultado.

## Por que

1. **Nenhum padrão curado define "antes de" nem bloqueante.** CDEvents, OpenTelemetry, schema.org, PROV-O e Activity Streams falham no mesmo ponto; só o Git tem "antes do commit" nativo. A grafia do host é a única fonte de verdade para `pre-commit` e `PreToolUse`.
2. **W-A é reversível e W-B e W-C não são.** Como `vocabulary` é campo aberto, um núcleo `core` ou um `cdevents` entra depois como mais um vocabulário, sem migração. Uma base nativa escrita num núcleo vira porta de mão única para todo projeto que sobrepôs um nó nativo.
3. **Na v1 o núcleo comum não tem consumidor.** O plugin só injeta texto no `SessionStart` e o agente lê pelo `how`. O risco de ler errado só aparece com mais de um host real.
4. **N-B e N-C criam duas fontes de verdade.** O PROV-O não tem `supersedes`, premissa nem evidência, e `wasInvalidatedBy` não equivale a "substituída". O `status` do MADR ("superseded by") duplicaria a vigência calculada pelo motor. O OKF duplicaria autoria e instante, que o Hexlog já grava em cada registro, e é um envelope de arquivo markdown, não de registro SQLite com relações tipadas.
5. **Os nomes ficam gravados em cadeia append-only.** Trocar um nome depois é criar revisão nova. Por isso os nomes próprios da v1 precisam de cuidado, e as pontes (`equivalent`) são trocáveis sem migração.

## O que é dado, o que é referência

**Adotar como dado (sem dependência de crate):** as grafias oficiais dos hosts (hooks do Git, eventos do Claude Code, `on:` do GitHub Actions, e os de Codex, Gemini, Cursor e opencode quando absorvidos); o formato `{vocabulary, event, matcher?}`; o campo `equivalent`.

**Emprestar nomes (referência e `equivalent`):**

| Necessidade | Termo | Origem | Observação |
|---|---|---|---|
| id de `vocabulary` | prefixo por organização no `type` | CloudEvents | convenção de nome, sem envelope nem SDK |
| `event` de VCS e CI | `sujeito.predicado` (`change.merged`, `build.finished`, `testcaserun.finished`) | CDEvents | só como nome; predicados são passado, não "antes de"; se um dia for absorvido, a versão vai no id (`cdevents@0.5`) |
| revisão e id semântico | `specializationOf`, `wasRevisionOf`, `wasInvalidatedBy` | PROV-O | `externalId` não tem termo PROV; continua campo do motor |
| substituição | `replaces`, `isReplacedBy` | Dublin Core Terms | nome padrão para o `supersedes` |
| campos da decisão | `status` (`proposed`, `accepted`, `rejected`, `deprecated`), `decision-makers`, `consulted`, `informed`, Decision Drivers, Considered Options, Decision Outcome, Consequences, Confirmation | MADR 4.0.0 | `status` só como ciclo de vida; nunca "superseded by" |
| premissa, evidência e contradição | `Claim`, `assumed`, `AssertedEvidence`, `isCounter` | SACM 2.3 | nomes conferidos no XMI da 2.4 beta, não na 2.3 formal |
| frescor do nó | `status` (`draft`, `stable`, `deprecated`), `stale_after` | OKF v0.2 | só ideia de campo; OKF tem 3 meses e nenhum release |
| resultado de gate (depois da v1) | `PASSED`, `WARNED`, `FAILED`; `kind`, `level` | in-toto `test-result`; SARIF | só quando os gates executarem |
| execução de gate registrada (depois da v1) | `Activity`, `used`, `wasAssociatedWith` | PROV-O | PROV é retrospectivo; serve para registrar, não para declarar o `when` |

**Rejeitados ou fora:** Activity Streams 2.0, DMN e BPMN, SKOS (por ora). O schema.org fica só como fonte de nomes: embutir um subconjunto do dump pode exigir licença share-alike (CC BY-SA 3.0), ponto que ninguém verificou.

## Voto dissidente

Sem núcleo comum, a base nativa pode virar várias cópias do mesmo gate, uma por host, e elas divergem. Se a base ganhar um segundo host real antes de existir um vocabulário `core`, W-A vira dívida e W-B passa a compensar. O `equivalent` sem consumidor nem teste também pode apodrecer; a mitigação é validar só a sintaxe da URI, documentar como "aproximação, não equivalência formal" e ter um teste de arquitetura que impeça o motor de importar RDF. A tese de interoperabilidade do OKF e do MADR ficou sem consumidor externo comprovado.

## Premissa mais arriscada e teste

**Premissa:** o agente entende o `when` bruto do host (por exemplo `{claude-code, PreToolUse, matcher Bash + if}`) sem ambiguidade. Ninguém mediu.
**Teste:** injetar no `SessionStart` de um projeto real um gate em `{free, before-commit}` e outro em `{claude-code, PreToolUse, ...}` e observar, em 3 a 5 tarefas, se o agente aplica a instrução no momento certo.

## Quando reabrir a decisão

- A base nativa da v1 precisar do mesmo gate em 2 ou mais hosts: reavaliar W-B como vocabulário `core`.
- O teste acima mostrar leitura errada do `when` bruto.
- Surgir um consumidor externo real de nós OKF ou MADR que o dono queira alimentar, com `status` e autoria estritamente derivados do motor.

## Achados do relatório do Hexlog que cruzam com esta decisão

Fonte: [hexlog-current-model.md](../hexlog-current-model.md).

- `supersedes` e `revokes` não cruzam processos no Hexlog atual. A sobreposição de um nó nativo por um nó de projeto pelo mesmo id precisa que os dois fiquem na mesma cadeia, ou que o motor novo defina outra regra. Ponto aberto para o dono.
- "Gate" no Hexlog atual é uma consulta avaliável (passou ou falhou). No v2 é um nó de texto de instrução. O nome colide e precisa de decisão.
- A regra de no máximo um registro vigente por linhagem (`FORK_REJECTED`) continua necessária com revisões por id e roda dentro da transação.

## Notas de origem

[schema-org.md](schema-org.md), [cloudevents-cdevents.md](cloudevents-cdevents.md), [otel-ecs.md](otel-ecs.md), [prov-activitystreams.md](prov-activitystreams.md), [host-events.md](host-events.md), [others.md](others.md).

## Limites

O painel usou um só modelo (sonnet), com personas e lentes diferentes; a independência real é média, então o consenso é evidência fraca por si só. Nenhum jurado leu o relatório do Hexlog por inteiro (77 KB). O exemplo do Claude Code (`PreToolUse` com `if`) não foi executado por ninguém.
