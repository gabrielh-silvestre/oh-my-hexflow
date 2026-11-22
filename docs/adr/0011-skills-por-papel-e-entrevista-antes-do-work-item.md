---
status: accepted
---

# Skills por papel, ciclo como dado e entrevista antes do work item

## Contexto

O plugin entregava a orientação da CLI em três skills de comando (`query-and-integrity`, `mechanical-checks`, `work-item-closing`), que o agente só lia se a sessão principal as carregasse, e a base `directives-flow` com o ciclo em prosa. Os sete agentes portados ([ADR 0008](0008-papel-do-agente-imposto-por-hook-e-matriz.md)) não recebiam, por etapa do ciclo, a lista dos comandos que o papel roda, e a conferência da sessão depois de cada chamada de agente não era verificável. A entrevista de negócio com o dono não existia como etapa: o work item nascia de um pedido ainda ambíguo.

Dois canais de entrega da orientação foram medidos antes da decisão. O resumo abaixo é o registro durável: a evidência bruta (as transcrições dos subagentes e os logs dos hooks) fica fora do repositório, na máquina do dono, e nenhum teste a lê.

**Spike 1 (sandbox descartável, Claude Code `2.1.327`, headless, sem time).** Quatro execuções com um plugin de teste e uma skill canário: `skills:` do agente na grafia pura (`role-probe`) e na qualificada (`<plugin>:role-probe`), cada uma com e sem `name` na chamada. O canário chegou nas quatro respostas e o `agent_type` visto pelos hooks `SubagentStart` e `PreToolUse` foi `<plugin>:<agente>`, com e sem `name`. A leitura correta é mais fraca do que parece: os agentes tinham a tool `Skill` e não havia controle negativo, então o canário pode ter vindo de uma chamada à `Skill` e não de pré-carga; e o caminho interativo com time, que é o uso real, não foi exercitado.

**G1 (mesma versão, com controle negativo).** A parte A rodou em headless e a parte B numa sessão interativa real, com 12 chamadas. O canário é a marca que a skill de teste deixa na resposta; "pré-carregada" quer dizer que o texto da skill aparece no registro do subagente antes do primeiro `tool_use`.

| Chamada | Agente e modo | `agent_type` no hook | Skill pré-carregada? | Controle |
|---|---|---|---|---|
| A1 | `skills: [role-probe]` (pura), sem `name`, headless | `omhlprobe:probe-bare` | sim | A3 e A4 |
| A2 | `skills: [omhlprobe:role-probe]` (qualificada), sem `name`, headless | `omhlprobe:probe-qual` | sim | A3 e A4 |
| A3 | sem `skills:`, headless | `omhlprobe:probe-none` | não | controle negativo 1 |
| A4 | `skills: [omhlprobe:nao-existe]`, headless | `omhlprobe:probe-missing` | não; **sem erro de criação**, a skill inexistente é ignorada em silêncio | controle negativo 2 |
| A5 | qualificada, **com** `name`, headless | `omhlprobe:probe-qual` | sim | informativa |
| A6 | qualificada, tool `Skill` liberada, sem `name`, headless | `omhlprobe:probe-qual-skill` | sim, sem `tool_use` de `Skill` | A3 e A4 |
| B1 | qualificada, sem `name`, interativa | `omhlprobe:probe-qual` | sim | B3a |
| B2 | qualificada, **com** `name`, interativa | o **nome gerado**; `taskKind` `in_process_teammate`, `customAgentType` `omhlprobe:probe-qual` | **não** | B3b |
| B3a | sem `skills:`, sem `name`, interativa | `omhlprobe:probe-none` | não | controle de B1 |
| B3b | sem `skills:`, com `name`, interativa | o nome gerado (`in_process_teammate`) | não | controle de B2 |
| B4 | `oh-my-hexflow:planner`, sem `name`, interativa | `oh-my-hexflow:planner` | n/a | n/a |
| B5 | `oh-my-hexflow:planner`, com `name`, interativa | o nome gerado; `customAgentType` `oh-my-hexflow:planner` | n/a | n/a |

O que a tabela prova, pelo critério de cinco condições (controle negativo, mesmo modo de chamada, marcador antes do primeiro `tool_use`, nenhuma chamada à `Skill` e canário ausente nos controles): a pré-carga por `skills:` vale **sem `name`**, nas duas grafias em headless (A1, A2, A6) e na qualificada em interativo (B1). **Com `name`, numa sessão interativa, a skill não é pré-carregada** (B2, controle B3b) e o `agent_type` do hook é o nome gerado, não `oh-my-hexflow:<papel>` (B2, B5); o tipo real fica só em `customAgentType` do `meta.json` do subagente. Uma skill inexistente em `skills:` falha em silêncio (A4).

**O que o G1 não provou:** a grafia pura em sessão interativa (a parte B só usou a qualificada); a colisão de nomes de skill entre plugins no `skills:`; a pré-carga interativa **com a tool `Skill` disponível** (B1 e B2 usaram um agente que a bloqueia, e os sete agentes reais não a bloqueiam); o comportamento em outra versão do Claude Code; a pré-carga com `name`, que o G1 mostra ausente e que o desenho, por isso, não usa.

## Decisão

1. **Uma skill `role-<papel>` por papel, pré-carregada pelo campo `skills:` do agente, na grafia qualificada.** O agente `analyst` declara `skills: [oh-my-hexflow:role-analyst]`, e assim os outros seis. A grafia qualificada é a que o G1 provou em interativo; as duas valem em headless. Cada `role-<papel>/SKILL.md` leva um bloco YAML `omhl-role-view` como fonte única: por etapa do ciclo, os comandos `omhl` que o papel roda, o que ele grava (`grava`) e as etapas que repetem outra (`repete`). O canal só vale em chamadas **sem `name`** (decisão do dono em 2026-11-18, opção (a'), confirmada pelo G1): a sessão chama `oh-my-hexflow:<papel>` sem esse parâmetro, e um `PreToolUse` do plugin que nega o `name` nessas chamadas é um work item `fix` próprio, fora deste. Se uma versão futura do Claude Code deixar de pré-carregar, o mesmo bloco vai ao corpo do agente (plano B), trocando só as constantes `SKILLS_SPELLING` e `CHANNEL` do teste.
2. **O ciclo é dado.** O `cycle.md` ganha um bloco `omhl-cycle` com o número, o nome e os papéis de cada etapa, e a prosa de cada linha numerada acompanha o bloco por uma regra do teste.
3. **Um teste em Rust cruza tudo.** O módulo `role_view`, em `src/test_support/` e no gate local `--lib`, cruza o `omhl-allow` de cada agente, as vistas, o ciclo, as frases de conferência da sessão e as tabelas "quem usa" das páginas de comando. As regras 0 a 8 e a mensagem de cada uma estão no módulo.
4. **A entrevista de negócio roda antes do work item.** Em `feat` e `fix` ela é obrigatória; nos demais prefixos, só a pedido do dono. O estado das rodadas fica em `.omhl/notes/` e nunca vira registro; só o que o dono confirma é gravado, como premissa. A skill `interview` conduz as rodadas e a `plan-consensus` cobre só o técnico depois dela.
5. **As três skills de CLI deixam de existir.** As páginas de `list`, `get`, `render`, `verify`, `project`, `report answers` e `report recurrences` vão para `directives-flow/references/`; as de `load`, `check run`, `check sweep`, `check retire`, `byproduct`, `hooks install`, `hooks post-commit`, `attest`, `gate-pr`, `close` e `report retro`, mais `gates-operation.md`, vão para `review/references/`. Os quatro `errors.md` viram um, o da `directives-flow`. As regras que valiam o tempo todo ficam como uma linha no `SKILL.md` de destino, e o detalhe vai para os índices `consulta.md`, `checks.md` e `fechamento.md`.
6. **A ordem do fechamento mora no passo 8 da `review`**, travada por teste: a parada do PR filho ("pare depois de colar a tabela"), `omhl check run --event before-merge` e `omhl attest`, `omhl gate-pr`, o PR e o merge com `wt`, só então `omhl close`, e por fim `omhl report retro`. O fechamento é do PR da agregadora; um PR filho nunca roda o gate nem o `close`.

### Drivers

Falha com lista exata (cada regra do teste devolve os itens que erram, em ordem); plugin autossuficiente (nenhuma página depende de arquivo fora de `plugin/`); não quebrar o que os testes de `tests/e2e/skills/` já travam; respeitar a divisão dos testes em três grupos e o formato de saída `agent`, `human` e `json` ([ADR 0009](0009-formato-de-saida-agent-human-json-e-registro-de-renderizadores.md)).

### Alternativas consideradas

- **O ciclo como constante em Rust**, ou **sem dado de ciclo**: a primeira esconde o ciclo de quem edita a prosa; a segunda perde a lista exata de "etapas sem linha".
- **Injeção por `SubagentStart` com matcher**: refém do valor de `agent_type`, que o G1 mostrou ser o nome gerado quando há `name`.
- **Prompt de delegação da sessão principal**: depende de ela lembrar, e não é verificável.
- **O hook `subagent-standards` do harness do dono**: não reconhece nomes de plugin.
- **Um gerador das tabelas "quem usa"**: recusado pela spec; o teste nos dois sentidos faz o trabalho sem uma segunda cópia solta.
- **Manter os quatro `errors.md`**: as cópias já derivavam (só o da `directives-flow` tinha `role-unknown` e a ordem das checagens com o `OMHL_ROLE`); uma página só custa menos que quatro iguais.

### Por que esta

`skills:` não depende do `agent_type` nem de hook, e o G1 mediu a pré-carga no caminho real do dono, e não só em headless: ela se sustenta nas chamadas sem `name`, que é o modo imposto. O YAML único evita gerador; o ciclo como dado dá o `etapas sem linha` exato; o teste nos dois sentidos mantém as tabelas iguais ao YAML e às citações da sessão. **Antítese registrada:** o plano B (a vista no corpo do agente) dispensa o G1 e o risco do canal e não deixa sete skills visíveis à sessão; fica como alternativa pronta, porque a spec pede a pré-carga por `skills:`.

### Classificação do teste

A regra do grupo de testes é que um teste de meta vai para o binário onde moram os helpers do seu arquivo. Os helpers do `role_view` (leitor, `extract_block`, `check`, `cycle_prose`, `session_checks`) moram em `src/test_support/`, então o binário é o `--lib`: é a aplicação da regra, não um desvio. A maior parte dos testes exercita as regras sobre texto sintético; os que leem os arquivos reais falham no gate local antes do push. O módulo respeita a fronteira do teste unitário (sem processo, `Omhl` nem `App`) e declara `#[cfg(test)] mod tests;`.

### Convenção e escopo da regra 8

Citar `omhl <folha>` com o prefixo `omhl` é **ordem de rodar o comando**; quem só menciona um comando escreve sem o prefixo (`o gate-pr`, a varredura global (`check sweep`)), porque a regra mede citação, não uso. A regra 8 olha só as skills `interview`, `plan-consensus` e `review`, fora das cercas e da tabela "Comando para referência": a `directives-flow`, incluindo o `cycle.md`, fica fora do escopo. A projeção de uma folha junta as linhas dos YAML dos papéis e as citações da sessão (papel `sessao`, etapa `<skill> <n>` ou `<skill> regras`), e a tabela da página tem de ser igual a ela nos dois sentidos. Uma página sem linha diz "Nenhuma etapa do ciclo a roda.".

**Página migrada** é a página de uma folha da CLI que não está em `BASE_LEAVES` (as cinco da base: `append`, `spec init`, `spec validate`, `gate-worktree` e `guideline retire`); são 18, e cada uma abre com a seção "Quem usa, em qual etapa e com qual filtro". `gates-operation.md` e o `errors.md` fundido não são página migrada e não levam o bloco.

**Âncora `WHO_USES_ANCHOR`.** A comparação bidirecional só confere que página e projeção concordam: se a condensação perdesse um prefixo por engano, o conserto natural (trocar a tabela pela frase de vazio) deixaria o teste verde e uma orientação real sumiria. Por isso o `role_view` leva uma constante escrita à mão com a classe esperada de cada uma das 18 páginas: `NoRole` (projeção vazia: `render`, `verify`, `hooks post-commit` e `check sweep`), `RolesOnly` (só linhas de papel: `report recurrences`) e `Session` (as outras 13). Mudar uma classe é legítimo quando a escrita muda de propósito, e então o mesmo diff edita a constante, a página e a explicação no corpo do PR.

## Consequências

- A evidência bruta do spike 1 e do G1 fica fora do repositório, e o registro durável é este ADR. Nenhum teste lê evidência: a regra 5 compara o `skills:` de cada agente com as constantes `SKILLS_SPELLING` e `CHANNEL`.
- A regra 5 também confere que cada skill citada em `skills:` existe como `plugin/skills/<nome>/SKILL.md`, porque o Claude Code ignora em silêncio uma skill inexistente (A4).
- A âncora `WHO_USES_ANCHOR` é uma cópia a mais da classe de cada página e é editada a cada mudança legítima de citação: a tensão é aceita para que uma perda de orientação não passe verde.
- A escolha da grafia qualificada deixa a colisão de nomes entre plugins como risco não testado.
- O canal `skills:` só vale em chamadas sem `name`; com `name`, a vista não chega e a matriz `omhl-allow` do papel não se aplica. A vista e a matriz dependem, portanto, da regra de prosa e do hook.
- A transcrição e a mecânica do passo 0 (`append`, `spec init`, `spec validate`, `gate-worktree`) não verificam a identidade do humano em `confirmed_by`.
- Mais sete skills visíveis ao modelo, e o conteúdo da vista entra inteiro no contexto de cada agente. Trocar para o plano B muda só a constante `CHANNEL` do teste e o lugar do bloco.
- Qualquer edição de citação `omhl` nas três skills de workflow exige atualizar a tabela "quem usa" da página (regra 8). Um erro em `cycle.md`, em `plugin/agents/*.md`, num `role-*/SKILL.md`, num `SKILL.md` de workflow ou numa página migrada reprova o pre-push até em PR `docs`.
- As páginas migradas moram em uma só skill (`duplicate_owners`) e cada índice (`consulta.md`, `checks.md`, `fechamento.md`, `gates-operation.md`, `errors.md`) é linkado pelo `SKILL.md` da própria skill (`is_orphan_page`).

## Limites

- A entrevista é obrigatória só por prosa: nada no binário a impõe.
- O papel no hook depende do `agent_type` que o Claude Code entrega, que no interativo com `name` é o nome gerado. Enquanto o work item `fix` do `PreToolUse` não está mergeado e o `omhl` reinstalado, numa chamada com `name` por engano o hook devolve `Pass`, a matriz do papel não se aplica e a vista `role-<papel>` não é pré-carregada (B2): é a janela de risco. O passo 7 da `review` só pega `decision` gravada como `claude`; `premise`, `gap` e `divergence` passam ([ADR 0008](0008-papel-do-agente-imposto-por-hook-e-matriz.md) cobre o caso genérico).
- A âncora trava a classe da página, não cada linha: perder uma de várias linhas `sessao` da mesma folha só é pego pela comparação bidirecional.
- A conferência dos comandos contra a CLI real (flags existentes) não entra: o teste prova que a documentação concorda consigo, não que o agente a usa.
- O teste não prova a completude por etapa: as regras 3 e 4 olham a folha e a união dos `grava` do papel, então um comando que falta numa etapa mas existe em outra passa.
- `gates-operation.md` e o `errors.md` fundido ficam sem o bloco "quem usa". A spec diz "cada página de referência migrada" e o bloco é "quem usa, em qual etapa e com qual filtro"; este ADR lê isso como as 18 páginas de comando. Se a leitura do dono for outra, `gates-operation.md` ganha um bloco "a sessão, quando o hook ou o check recusa", e a mudança é de uma página.
- Repetir o G1 depende da máquina do dono: a sonda mora só na pasta de pesquisa local, e se ela se perder a sonda se perde com ela.

### Follow-ups

- Um teste que faça o `clap` analisar cada comando do YAML e das linhas de conferência (`Cli::command()`), cobrindo flags removidos.
- Reavaliar o canal `skills:` e o `agent_type` a cada versão do Claude Code, repetindo o G1 com os controles negativos.
- Testar a colisão de nomes de skill entre plugins e a grafia pura em interativo, e medir a pré-carga interativa com a tool `Skill` disponível (o piloto confere o caso real).
- A imposição por hook de "chamar sem `name`" é o work item `fix` próprio; avaliar a alternativa do hook (b) só com nova ordem do dono para o build frio.
