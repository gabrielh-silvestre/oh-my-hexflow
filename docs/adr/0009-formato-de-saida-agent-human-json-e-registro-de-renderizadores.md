---
status: accepted
---

# Formato de saída `agent`, `human` e `json` e registro de renderizadores

## Contexto

O [ADR 0006](0006-cli-como-unica-interface-e-utcp-descartado.md) fez da CLI a única interface do agente, e a saída de todos os comandos era uma linha de JSON. O out1 mostrou que o agente lê melhor um texto curto que diz o que passou, o que bloqueou e o próximo passo, e deu esse texto só ao `check run` (`--format text|json`). Os outros 20 comandos ativos seguiam em JSON cru, e o agente decifrava campos para saber se podia seguir.

O [ADR 0007](0007-opcoes-da-cli-sao-respeitadas-ou-recusadas.md) já registra uma exceção à regra de que a saída da CLI não muda. Esta é a segunda: a saída padrão de 21 comandos muda de formato.

## Decisão

`--format agent|human|json` é uma opção global do `omhl` (`Cli.format`, `adapters/cli.rs`). `agent` é o texto compacto para o agente, `human` mostra os mesmos fatos em blocos com os rótulos alinhados, e `json` é a linha de sempre. O `text` do out1 some, sem alias: `--format text` sai com erro de uso.

- **Uma chave por comando e um fato por comando.** `command_key(&Command)` devolve uma `CommandKey` (26 chaves) por `match` exaustivo. O padrão de cada comando sai de `renderer_for(key).is_some()`: `agent` quando há renderizador, `json` quando não há. Não existe uma segunda tabela de padrões.
- **Cinco comandos adiados ficam em JSON.** `hook session-start`, `hook spec-lock`, `hook agent-role`, `hooks post-commit` e `gate-worktree` são lidos por Claude Code, git ou `wt`. Eles recusam `--format agent|human` com erro de uso do clap (saída 2, `format_misuse`), antes de executar. `deferred(key)` é uma lista independente de `renderer_for`, e o teste de superfície (`tests/output_agent_surface.rs`) confronta as duas.
- **Registro de renderizadores com Template Method.** `renderer_for(key)` devolve um `Renderer` que monta um `Doc`: linhas tipadas (cabeçalho, item, campo, nota, nó, saída de comando, texto verbatim) com profundidade e com o *estado* (`Ok`, `Warning`, `Error`) no lugar do glifo. Dois serializadores de layout, `layout::agent` e `layout::human`, transformam o `Doc` em texto. O `BaseRenderer` dá o esqueleto (cabeçalho, corpo, fecho `Next:`, erro genérico) e cada comando entrega a sua parte; o `render` devolve o próprio texto, igual nos dois layouts.
- **`Profile` por comando.** A tabela do `Profile` descreve como o texto deve mostrar cada chave do `Value`: por região do nó (`Root`, `Snapshot`, `Author`, `Data`), tipo de nó, pai de um nível e chave, com a forma `Literal`, `Closed`, `Hash(n)`, `Derived` ou `Noise`. O `BaseRenderer` lê só o `label` do `Profile`; o resto da tabela é um oráculo que o teste de folhas confronta com o `Value` real. Cada chave nova custa duas edições (a tabela e o renderizador) e o descasamento só aparece no CI até existir o teste `--lib` equivalente. O limite é escrito: o `Profile` não carrega caminho de JSON.
- **Vocabulário central.** `adapters/text/vocab.rs` é o único lugar com os símbolos (`✔`, `⚠`, `✘`), o `Next:`, `Nothing blocked.`, o aviso de segredos, os separadores, o recuo e as palavras que mais de um comando repete. `AgentVocabulary` e `HumanVocabulary` implementam o trait `Vocabulary` e sobrescrevem só o que difere. Frases que um comando só usa ficam no comando. A regra de arquitetura `it_should_keep_the_output_vocabulary_literals_inside_the_vocab_module` reprova glifo e `Next:` escritos fora do módulo (emenda-03 da spec de `work-items.out2`).
- **O `agent` é compacto.** Um espaço onde havia dois, valores sem espaço repetido nem nas pontas, sem alinhamento de colunas; o recuo de dois espaços por nível continua. O `human` não mudou. A limpeza de string é a única diferença inicial entre os dois layouts, e os fatos são os mesmos.
- **Paridade provada por teste.** O teste de folhas (`tests/output_agent_leaves.rs`) roda cada comando de verdade, lê o `--format json` e renderiza `agent` e `human` desse mesmo `Value`, com `render_success` e `render_failure` (puras e públicas em `adapters::text`). Uma chave que some do texto, ou uma chave nova fora da tabela do `Profile`, reprova.

### Exceção às saídas JSON do ADR 0007

A exceção tem a conta fechada:

| Grupo | Comandos | Saída padrão antes | Saída padrão depois |
|---|---|---|---|
| Ativos | 21 | uma linha de JSON (o `check run`: texto `agent`) | texto `agent`; `human` e `json` por `--format` |
| Adiados | 5 | uma linha de JSON | uma linha de JSON; `--format agent\|human` é erro de uso (saída 2) |

O JSON não muda: `error.code` e as chaves do relatório do `load` (lidas pelo hook) ficam congelados, e o `json` imprime o `Value` de antes. Prova dentro do PR: o diff de `crates/omhl/src` fora de `adapters/` é vazio, salvo `workflow/omhl_leaf.rs` (emenda-02: `--format` entra nas opções globais), e o de `crates/hexlog` e do `Cargo.lock` é vazio. Depois do PR a guarda é o teste de folhas e o `error.code` literal por comando nos estados de erro.

## Drivers

- Consumo por agente sem perder o JSON de máquina.
- Raio pequeno nos contratos congelados e nos testes, e nenhuma renomeação silenciosa de chave.
- Custo local baixo: um work item, ondas sequenciais, um `cargo` por vez.
- Ajustar a saída (trocar um símbolo, reescrever o aviso de segredos) sem tocar seis arquivos de produto e cerca de 180 pontos de teste (emenda-03).

## Alternativas consideradas

- **Opção B: Template Method sem `Doc`**, com cada especialização escrevendo `agent` e `human` em `String`. Viável, mas duplica os fatos em dois formatadores por comando, deixa "mesmos fatos" a cargo só do teste de folhas e não concentra a convenção do out1. Perde por custo de manutenção.
- **Opção V: view models tipados**, um struct `serde` por comando em `app`. Uma chave renomeada viraria erro de compilação, mas toca `app/output.rs` e os construtores dos 21 comandos, contra "o JSON não muda sem registro". Adotada só onde há dois leitores (o `load`), e mesmo aí adiada (dissenso abaixo).
- **Opção C: conversor genérico JSON para texto**, sem renderizador por comando. Não escolhe campos por tipo de nó (o `list` precisa de resumo por tipo), não decide símbolo nem `Next:` e deixaria `chain_head` como ruído em todo comando.
- **Flag por comando** e **padrão único `agent` para todos** foram decisões do dono, não opções: a flag é global, o padrão é por comando.
- **Variável de ambiente para o formato** ficou fora; reavaliar se o JSON padrão fizer falta a algum leitor.

## Por que esta

O `Doc` concentra a convenção do out1 e reduz 12 comandos a dados; projetá-lo junto do `check run`, o caso mais rico, evita um modelo que só serve aos comandos simples. O vocabulário central torna uma troca de símbolo uma edição só e vale para os dois layouts. O teste de folhas sobre o `Value` de uma execução real dá à paridade e à fronteira com `app/output.rs` uma prova mecânica. A flag global com padrão por comando atende ao dono e ao [ADR 0007](0007-opcoes-da-cli-sao-respeitadas-ou-recusadas.md).

## Consequências

- Acrescentar uma ação passa de 4 para 10 pontos de toque: os 4 de antes mais a variante de `CommandKey`, os braços de `command_key` e `command_path` (`cli.rs`) e os de `renderer_for`, `profile_for` e `deferred` (`adapters/text/mod.rs`). Nenhum tem braço `_`, então a compilação cobra; a lista `CommandKey::ALL` é cobrada pelo teste de superfície. A lista está em [architecture.md](../architecture.md), seção 5, item 5.
- Os bytes do `agent` do `check run` mudaram por decisão do dono (emenda-03 da spec de `work-items.out2`, premissa `premises.out2-out1-agent-bytes-change-accepted`): a compactação tira um espaço dos separadores e limpa os valores. Os fatos são os mesmos, e `--format text` sai com erro de uso. O registro de impacto da spec não tem essa linha; ela está aqui e no corpo do PR.

| Comando | Campo | Antes | Depois | O que quebra |
|---|---|---|---|---|
| `check run` | bytes do `agent` | texto do out1, dois espaços entre as partes | texto compacto, um espaço, valores limpos; mesmos fatos | quem comparava os bytes do out1; `tests/check_run_text.rs` e os unitários migrados (emenda-03, EMD-007 e EMD-008) |

- `adapters::text` expõe `render_success`, `render_failure` e `profile_for`, que os testes de `tests/` usam: passam a ser um contrato interno a manter.
- O `human` do `spec validate` não abre linha em branco antes de uma emenda com violação (`layout.rs`): o golden fixa o comportamento atual.
- A guarda contra chave renomeada é um teste de `tests/`: o gate local `--lib` não a cobre, e depois do PR só o CI a roda.
- `NON_BLOCKING_PREFIX` está duplicado de `app/check_run.rs` (privado lá; o diff vazio de `app/` impede de compartilhar).

## Dissenso: `LoadReport` tipado

O Architect recomendou tipar o relatório do `load` agora (`Serialize + Deserialize` em `app/load.rs`, JSON inalterado), porque o gatilho de [architecture.md](../architecture.md), seção 5, item 6, disparou com o renderizador de `load`: há dois leitores do relatório por chave de string (o hook e o texto). O Critic aceitou a dívida: o dono congelou as chaves do `load`, o teste de chaves é a guarda e tipar o relatório mexe em `app/load.rs` e no hook, fora do diff vazio de `app/`.

**Decisão: dívida aceita nesta entrega e `LoadReport` tipado como follow-up.** A razão depende do teste de folhas ler o `Value` real do `load`: renomear `created` em `app/load.rs` reprova, porque a chave nova fica fora da tabela e `created`, declarada, some. Se o teste passar a usar `Value` de amostra, o dissenso se reabre.

## Follow-ups

- `LoadReport` tipado, quando o teste de folhas reprovar por renomeação de chave do `load` uma vez, ou quando aparecer um terceiro leitor.
- Frente dos adiados, se um dia houver leitor humano.
- Variável de ambiente para o formato.
- Revisar `detail_lines` e `result_line` depois do `Doc`.
- Um teste `--lib` equivalente ao de folhas, para o gate local pegar chave renomeada.
