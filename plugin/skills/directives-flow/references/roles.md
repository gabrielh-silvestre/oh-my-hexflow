# Os papéis dos agentes

Quando consultar: antes de chamar um agente do plugin (o que passar a ele), ao ler a resposta dele, ou quando uma guarda recusar uma gravação com `role-record-forbidden`, `role-revision-forbidden`, `role-answer-forbidden` ou `role-retirement-forbidden`. A ordem em que os agentes entram está em [cycle.md](cycle.md).

## A matriz

A matriz de comandos, tipos de node, relações e retiradas de cada papel está numa tabela só, na seção "A matriz por papel" de [directives-flow.md](https://github.com/gabrielh-silvestre/oh-my-hexflow/blob/main/docs/directives-flow.md#a-matriz-por-papel). Esta página não a copia. A matriz da versão instalada está no frontmatter (`omhl-allow`) de cada agente em `plugin/agents/`, por exemplo [critic.md](../../../agents/critic.md). Leitura (`omhl list`, `omhl get` e os demais) vale para os sete papéis; `attest`, `load`, `close`, `gate-pr` e `gate-worktree` não valem para nenhum.

## O que a sessão principal passa a cada agente

A chamada é `oh-my-hexflow:<papel>`. Todo agente recebe o id do work item (`work-items.<id>`): sem ele, ele não grava.

| Agente | Passe também |
|---|---|
| `oh-my-hexflow:analyst` | Nada além do id; o pedido do humano, se houver |
| `oh-my-hexflow:planner` | O slug da spec na primeira chamada, a pasta absoluta da spec nas seguintes; o modo consenso (RALPLAN-DR), SHORT (DELIBERATE só a pedido do humano ou item de alto risco); as perguntas abertas do analyst |
| `oh-my-hexflow:architect` | O resumo RALPLAN-DR e o ADR do planner; o modo "revisão de consenso (ralplan), SHORT" (ou DELIBERATE); a objeção do critic, nas voltas seguintes |
| `oh-my-hexflow:critic` | O caminho absoluto da pasta da spec; o modo "revisão de consenso (ralplan), SHORT" (ou DELIBERATE) e o pedido da linha-resumo ralplan |
| `oh-my-hexflow:code-reviewer` | A pasta da spec e a mudança a revisar |
| `oh-my-hexflow:security-reviewer` | A pasta da spec e a mudança a revisar |
| `oh-my-hexflow:verifier` | A pasta da spec; o id da divergência a conferir; o par `from`/`to` de cada `contradicts` a retirar |

A spec mora no checkout principal, fora da worktree do agente. Passe o caminho **absoluto** da pasta, no checkout principal: o `dir` do `omhl spec init`, que o planner repete em "Spec written to". Um caminho relativo (`.omhl/specs/wi-<id>__<slug>/`) não existe dentro da worktree, e sem a pasta os leitores da spec não a encontram.

O par `from`/`to` de cada `contradicts` vem das linhas `contradicts: <from> -> <to>` que o critic e os revisores listam na resposta (peça no pedido). Para conferir o que está vigente, ou se a resposta não trouxe o par, rode `omhl project` e leia a seção `## Relations` de `.omhl/projection/work-items.<id>.md`: a projeção só lista relações não retiradas. Nenhum comando de leitura do banco (`omhl list`, `omhl get`, `omhl render`) devolve relações, e o verifier não roda `project`: sem o par no pedido, ele o devolve.

## O que fazer com a resposta

Toda resposta termina com dois campos fixos:

- **Next role:** o papel a chamar em seguida, ou `none`. A sessão principal decide; chamar o papel indicado é o padrão.
- **Open questions for the main session:** perguntas que o agente não pode gravar. A sessão principal grava cada uma como `gap` com `omhl append`, com o `kind` sugerido (`technical` ou `business`). Sem `kind`, o gap vai ao humano.

## Quem confirma o quê

- O critic confirma qualquer premissa `technical`, seja qual for o autor, e só premissa: grava `status` `confirmed` e `confirmed_by` com `by` igual a `critic`, mais `question` e `answer` literais. Premissa de negócio, diretiva e gap `business` ou sem `kind` continuam com o humano. O critic responde (`answers`) só gap `technical`.
- Um `confirmed_by` com `by` igual a `critic` gravado por outro autor, ou numa premissa de negócio, é `confirmation-invalid`.

## A âncora

Toda `decision` (do architect, do critic ou de um revisor) leva `rests-on` para uma diretiva ou premissa confirmada, inclusive a premissa que o critic confirmou. Sem a âncora, o `omhl gate-pr` reprova `decisions-anchored`. O revisor que acusa uma violação grava a `decision` dele com `rests-on` e liga um `contradicts` ao alvo violado.

## A retirada

- Só o `oh-my-hexflow:verifier` (isolado, sem o contexto do autor) e o dono retiram um `contradicts`. Ninguém retira o que ele mesmo gravou. A sessão principal (`claude`) não retira: a guarda recusa com `role-retirement-forbidden`.
- O verifier retira em duas chamadas ao `omhl append` e dá a razão na mensagem dele, porque o grafo só guarda a retirada. Não repasse a ele um par que ele não deva retirar.
- O dono (`user`) é a CLI fora do Claude Code, então o dono retira um `contradicts` só num terminal externo, fora do Claude Code. `! omhl ...` dentro do Claude Code grava `claude` e é recusado. A receita (corpo `retirement` e o `base` em duas chamadas) está em [append.md](append.md#entradas). Como a sessão principal não sabe os ids, passe ao dono o par `from`/`to` copiado das linhas `contradicts: <from> -> <to>` ou da seção `## Relations` da projeção.
- Uma `divergence` de revisor sai de `open` só pelo verifier, para `code-fixed`, com evidência nova. `accepted` e `spec-stale` são respostas do humano.
