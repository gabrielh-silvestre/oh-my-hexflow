# oh-my-hexflow

Plugin de desenvolvimento opinativo, construído sobre grafos, e o motor `hexlog` que os guarda. Termo em inglês (o mesmo identificador do código), definição em pt-BR.

## Language

**Engine**:
O motor agnóstico que guarda os records append-only de cada tenant e valida cada um contra o catalog. Guarda o grafo e não o executa. Os products conhecem o engine, mas o engine não conhece nenhum product, graph ou host. Editar e apagar são feitos por revision e retirement, nunca alterando ou removendo um record. Chama-se HexCore (o crate ainda se chama `hexlog`). Não confundir com o `hexcore-legacy`, um motor de execução de workflows abandonado.
_Avoid_: core, framework, runtime

**Product**:
Um conjunto de graphs, com seus gates, construído sobre o engine e ligado a um host por um plugin. O primeiro é o HexLog (decisões, checks e work items); cada graph de um product tem fronteira própria.
_Avoid_: app, module, extension (extension é outra coisa)

**Record**:
Toda gravação imutável no histórico de um tenant: a criação ou a revision de um node ou de uma relation, ou uma retirement. Node e relation não são records: são aquilo sobre o que os records falam. Cada record ocupa uma posição na sequência e é encadeado ao anterior.
_Avoid_: link, entry, elo, registro (na conversa, use o termo em inglês)

**Tenant**:
O espaço isolado que guarda todo o histórico de records de um projeto. Cada tenant tem a sua própria cadeia, e vários tenants convivem no mesmo banco global do usuário.
_Avoid_: project, workspace

**Batch**:
O grupo de 1 a 50 records enviados juntos e gravados de forma tudo-ou-nada: ou todos entram na chain, ou nenhum.
_Avoid_: transaction, commit

**Host**:
O ambiente de onde vêm os eventos e onde o agente roda: Claude Code, git, GitHub, Codex e afins. O programa que usa o motor não é um host: é o `omhl`.
_Avoid_: hospedeiro, platform, environment

**Chain**:
A sequência ordenada de records de um tenant, em que cada record carrega o hash do anterior, de modo que qualquer adulteração é detectável.
_Avoid_: ledger, log

**Node**:
Uma coisa nomeada que existe no grafo de um tenant (uma decisão, uma premissa, um check) e acumula revisões ao longo do tempo. É identificada pelo seu semantic id. Aproxima-se da `prov:Entity` geral do PROV-O, como referência e não como equivalência formal.
_Avoid_: entity, concept

**Relation**:
A ligação gravada entre dois nodes, de um relation type dado (por exemplo `after`), identificada pelo trio relation type, origem e destino. Pode carregar dados (por exemplo a condição de um ramo) só quando o relation type define um schema para eles, e tem revisões e retirement como um node: mudar os dados é uma revision, nunca uma alteração.
_Avoid_: edge, link, aresta (na conversa, use o termo em inglês)

**Relation type**:
O nome do tipo de uma relation (`after`, `supports`, `contradicts`), definido como dado e não no código.
_Avoid_: edge type, link type, kind

**Check**:
Um node de instrução que diz ao agente o que fazer e quando: tem `when` (antes de qual evento), `how` (como executar) e `blocking` (se barra ou só informa). Na v1 é só texto.
_Avoid_: rule, hook, step

**Gate**:
Uma pergunta avaliável sobre o histórico de um tenant que responde se algo pode prosseguir. Não é uma instrução: é a consulta que confere se os checks foram cumpridos.
_Avoid_: check, validation

**Attestation**:
Um node, de um tipo do graph de checks, cuja revision grava o desfecho de um check em um snapshot específico: passou, passou com alerta ou falhou. Refazer o check no mesmo snapshot deve dar o mesmo desfecho. Aproxima-se da attestation do in-toto.
_Avoid_: approval (no GitHub é aprovação humana), result, verdict, evidence

**Snapshot**:
O instante de algo externo ao qual um record fica amarrado, identificado por um kind e um id (no HexLog, um commit do git amarra cada attestation). O engine valida o formato do kind e do id, mas não sabe quais kinds existem: isso é o product que define. Não é o estado aberto ou fechado de um work item.
_Avoid_: state, revision

**Work item**:
Um node que representa uma unidade de trabalho em qualquer escopo: commit, pull request, issue, plano ou feature inteira. Cada escopo é um tipo de node especializado de work item; o pull request é uma dessas especializações.
_Avoid_: unit (não tem relação com unit test nem com o unit of work de Fowler), deliverable, change

**Native**:
O conteúdo de fábrica embutido no binário e comum a todos os projetos (checks e diretrizes). Cada projeto só o estende com nodes próprios e nunca o altera.
_Avoid_: built-in, base, default

**Extension**:
Um arquivo TOML escrito à mão em `.omhl/<pasta do tipo>/<slug>.toml` (`checks/` ou `guidelines/`), com o texto longo opcional num `.md` de mesmo nome ao lado, que acrescenta nodes do projeto (checks e diretrizes) e é carregado no banco por `omhl load`. É versionado com o projeto e estritamente validado. Só adiciona: nunca altera o native.
_Avoid_: overlay, manifest, override

**Projection**:
Uma vista gerada sob demanda a partir do banco por `omhl project`, em arquivos só de leitura em `.omhl/projection/`, que nunca são editados à mão e não são versionados (a pasta traz o próprio `.gitignore`). A primeira projeção é o estado atual por work item, com tudo que está vigente para ele, um arquivo por work item. Não inclui o TOML nem o Markdown de uma extension, que são escritos à mão.
_Avoid_: export, mirror

**Graph**:
Uma área do modelo, com seus próprios tipos de node e relation types (por exemplo o graph de decisões ou o de checks). Um tenant tem nodes de vários graphs.
_Avoid_: domain, module

**Premise**:
Um node do graph de decisões que afirma algo tomado como verdadeiro para poder decidir. Em pt-BR, premissa. Quando pertence a um work item (a frente de trabalho), vale enquanto ele estiver aberto. Tem um `kind` opcional: `technical` (a afirmação tomada como verdadeira para decidir) ou `business` (a expectativa da frente de trabalho).
_Avoid_: assumption, fact

**Guideline**:
Um node do graph de decisões que orienta como agir em um contexto, de forma atemporal, sem ser uma escolha pontual. Em pt-BR, diretiva: é o mesmo node, não um conceito novo.
_Avoid_: rule, policy

**Decision**:
Um node do graph de decisões que registra uma escolha tomada, com a evidência que a sustenta embutida nela.
_Avoid_: ADR, resolution

**Gap**:
Um node do graph de decisões que registra uma dúvida aberta de um work item (a pergunta e o seu contexto). Fecha quando uma premise a responde.
_Avoid_: question, doubt

**Divergence**:
Um node do graph de decisões que registra a diferença entre a spec e o que foi feito, com uma `resolution` que diz como ela foi tratada (`open`, `spec-stale`, `code-fixed` ou `accepted`).
_Avoid_: deviation

**Confirmação**:
A aprovação humana de uma premise, de uma guideline ou da emenda de uma divergence, dada dentro da sessão e registrada em `confirmed_by`, com a pergunta e a resposta literais como evidência. É autodeclarada: o `Author` do record continua sendo quem o digitou, não quem confirmou.
_Avoid_: approval

**Catalog**:
O conjunto das definições dos graphs, isto é, os tipos de node e os relation types de cada um, definido como dado e não no código.
_Avoid_: schema, vocabulary

**Definition**:
Um tipo de node ou um relation type do catalog, gravado no histórico do tenant com versão `major.minor` imutável. Todo objeto de uma definition é fechado: não aceita chave que não declarou. Mudança que só acrescenta (propriedade opcional, valor novo de enum, alargar as pontas de um relation type) ou que só muda texto descritivo sobe a minor; qualquer outra sobe a major. Um record validado numa minor continua válido nas minors seguintes da mesma major. O número da versão é declarado pelo product que define o tipo; o engine só confere se a mudança podia ser minor e recusa a mesma versão com outro conteúdo. Cada record guarda a versão contra a qual foi validado; sem pedido explícito de major, vale a minor mais recente da major mais alta.
_Avoid_: schema, model, versão (sozinho)

**Lineage**:
Toda a história de um node ou de uma relation: seus records, do primeiro ao latest record, ligados por revision. No máximo um deles está vigente.
_Avoid_: subject, history, linhagem (na conversa, use o termo em inglês)

**Latest record**:
O último record da história de um node ou de uma relation, vigente ou não. É a base da próxima revision. Se for uma retirement, o node não tem current revision.
_Avoid_: tip, head

**Current revision**:
A revision que vale hoje para um node: a mais recente, ainda não substituída nem retirada. Cada node tem no máximo uma.
_Avoid_: tip, latest, head

**Chain head**:
O último record da chain de um tenant. Não é o `HEAD` do git, que aponta para um snapshot.
_Avoid_: head (sozinho), tip, tail, last record

**Retirement**:
Um record que tira um node ou uma relation de vigência, mantendo todo o histórico na chain. Nada é apagado.
_Avoid_: delete, revocation, tombstone

**Revision**:
Um record que muda o conteúdo de um node ou de uma relation, apontando para a revision anterior e mantendo-a no histórico. Aproxima-se da ligação `prov:wasRevisionOf` do PROV-O, como referência e não como equivalência formal.
_Avoid_: version, amendment, update

**Origin**:
De onde vem um node: native (de fábrica, embutido no binário) ou do tenant (gravado ou carregado de uma extension).
_Avoid_: source, provenance

**Author**:
Quem gravou um record: o agente (com o modelo usado) ou uma pessoa.
_Avoid_: creator, actor

**External id**:
Reservado, sem uso na v1: o id de origem externa de um node, guardado para uma futura substituição do node.
_Avoid_: original id, source id

**Semantic id**:
O nome estável que identifica um node em todas as suas revisões (por exemplo `checks.run-tests`). Escolhido por sentido, e não gerado, para que uma colisão revele um node que já existe: gravar um node novo com um semantic id em uso é recusado e aponta para o record vigente, e quem gravou decide se cabe uma revision, uma relation ou um node diferente. Aproxima-se do identificador da `prov:Entity` geral do PROV-O, à qual cada revision se liga por `prov:specializationOf`; é referência, não dependência.
_Avoid_: id (sozinho), target, name

**Record id**:
O identificador técnico de um record específico, gerado na gravação. Cada revisão de um node tem o seu.
_Avoid_: id (sozinho), uuid
