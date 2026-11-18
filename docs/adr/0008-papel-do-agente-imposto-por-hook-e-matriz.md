---
status: accepted
---

# Papel do agente imposto por hook e matriz

## Contexto

A guarda do `omhl` só separava os autores `user`, `claude`, `loader`, `executor` e `guideline-retire` ([ADR 0004](0004-executor-e-loader-como-fronteira-da-guarda.md)). Todo agente que chamava a CLI pelo Bash gravava como `claude`, igual à sessão principal. O ciclo com os agentes do plugin (analyst, planner, architect, critic, code-reviewer, security-reviewer e verifier) precisa que cada papel grave só o que é dele: o critic não grava diretiva, o architect não confirma premissa, só o verifier retira um `contradicts`.

O Claude Code entrega o `agent_type` do subagente no evento do hook `PreToolUse`. O Bash do agente não recebe nenhuma variável de ambiente do plugin, e o subagente escreve o comando que quiser. A fonte do papel tem de ser algo que o agente não digita.

## Decisão

O papel do agente portado vem de três peças que um teste mantém iguais:

- **O hook `omhl hook agent-role`** (`PreToolUse` do `Bash`, `app/agent_role.rs`). Para um `agent_type` `oh-my-hexflow:<papel>`, ele lê o `omhl-allow` do arquivo `plugin/agents/<papel>.md`, confere o comando contra a lista e, quando está dentro, responde `allow` com o comando reescrito: `OMHL_ROLE=omhl:<papel>` na frente. O papel é a variável `OMHL_ROLE`, definida pelo hook; o hook nega a atribuição dela escrita à mão, de qualquer origem.
- **A matriz compilada no binário** (`guard/role.rs`). Com `OMHL_ROLE` válido sob o Claude Code, a CLI grava `agent: "omhl:<papel>"` e a guarda aplica a linha do papel e as condições finas do `append`. Um valor fora dos sete papéis, ou a variável fora do Claude Code, é recusado com `role-unknown`.
- **O `omhl-allow` no arquivo de cada agente.** Diz ao hook quais folhas da CLI o papel roda, antes de o binário existir no caminho. O Bash do agente não recebe `CLAUDE_PLUGIN_ROOT`, então o `append` não lê o frontmatter: a matriz compilada e o `omhl-allow` são duas cópias, travadas por paridade nos quatro campos.

A tabela única de `docs/directives-flow.md` (seção 8, "A matriz por papel") é a terceira cópia, também travada por teste. O `omhl-allow` é lido com a crate `serde-saphyr =2.1.1`, só com o recurso `deserialize`. O dono a escolheu pela popularidade e pela manutenção; a `serde_yaml` está arquivada.

**O que o leitor de comando cobre.** O hook não usa biblioteca de shell: um leitor próprio (`workflow/shell/`) entende aspas, heredocs, separadores, redirecionamentos e os programas que embrulham o `omhl` (`env`, `time`, `timeout`, `nice`, `nohup`, `command`, `exec`, `stdbuf`, `xargs` e `rtk proxy`, com as opções que sabe pular). Na dúvida, o hook nega, se o comando for de agente portado e mencionar `omhl`. A lista do que ele nega está em [directives-flow.md](../directives-flow.md), seção 12. Fora dela, também nega `env -i`, `env -S` e `exec -c`, quando alguma palavra do comando contém `omhl`.

## Consequências

- **O que muda na consequência do ADR 0004.** O ADR 0004 diz que "o agente que chama a CLI pelo Bash continua indistinguível do usuário". Para o agente portado, isso deixa de valer: o record sai com `agent: "omhl:<papel>"`, distinto de `claude` e de `user`, e a guarda aplica a matriz do papel. Contra a má-fé nada muda: o agente que escapa do hook continua gravando como `claude`. O ADR 0004 não é reescrito; a frase do `README.md` ("Autoria") que repetia o texto antigo passa a dizer isto.
- **`user` é o `omhl` fora do Claude Code**, sem a variável `CLAUDECODE`. O dono retira um `contradicts` (a retirada é de `user` e do verifier, nunca de `claude`) só de um terminal externo. Terminais de IDE e `! omhl ...` dentro do Claude Code gravam `claude`, e a retirada é recusada com `role-retirement-forbidden`.
- **A retirada do verifier tem duas chamadas.** Nenhum comando lê o id de record de uma relação, e a retirada o pede como `base`. O verifier envia a retirada sem `base`, recebe `FORK_REJECTED` com o id vigente e repete o pedido com ele. Um comando de leitura de relação fica para outro work item.
- **A razão da retirada não fica no grafo.** A `retirement` não tem `data`, e o verifier não grava `decision`. A razão vai na mensagem do verifier e na coluna de evidência da tabela do PR. O piloto decide se um tipo novo se justifica.
- Os testes `agent_role`, `agent_role_hook`, `agents` e `agent_examples` travam a matriz, o hook, o frontmatter e cada exemplo de comando dos agentes pelo hook e pela guarda.

## Limites

O hook protege contra o **erro** do agente, não contra a **má-fé**. O limite foi assumido pelo dono e vale para a v1. Ficam fora do hook; o comando roda sem prefixo e grava como `claude`, sem a matriz do papel:

- `bash -c`, `sh -c`, `eval`, `source`, `sudo` e `doas`, e os programas que executam outro comando sem estarem na lista de embrulhos (`setsid`, `chrt`, `flock`, `coproc`, `find -exec`, `script -c`);
- `cargo run -p omhl -- append`, comum neste repositório, e qualquer caminho que o leitor não reconheça como `omhl`;
- um nome do programa escondido por escape ANSI-C (`$'o\x6dhl'`) ou montado por aspas dentro do corpo de um heredoc com substituição, que o leitor não decodifica;
- as outras ferramentas: o hook vale só para o `Bash`;
- um erro de leitura na sessão principal, que deixa passar um `OMHL_ROLE=` escrito à mão, e um `CLAUDE_PLUGIN_ROOT` vazio ou relativo, que o hook não recusa.

**Fail-open do `|| exit 1`.** O Claude Code só bloqueia a chamada de ferramenta quando o hook sai com `2`. Um `omhl` velho no `PATH` (sem o subcomando `hook agent-role`, o clap sai com `2`), o binário ausente (`127`) e um pânico do hook bloqueariam todo `Bash`. O envoltório `|| exit 1` converte essas saídas em `1`, e o comando roda sem prefixo, em silêncio: o `omhl` do agente grava como `claude`. O custo está aceito, porque é erro de ambiente e não má-fé, da mesma família dos caminhos de fuga acima. A defesa é reinstalar o binário depois de cada merge que o altere, e um teste do hook roda o comando reescrito no `bash` e confere que o `OMHL_ROLE` chega ao `omhl`. A negação legítima sai por JSON com saída `0` e não depende do envoltório.

**O que o dono revisa na v1.** Uma `divergence` aberta e um `contradicts` contra uma diretiva não bloqueiam o PR (`docs/directives-flow.md`, seção 10); a prova de comportamento dos agentes é o piloto, que é outro work item.
