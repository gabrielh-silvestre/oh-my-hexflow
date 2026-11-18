---
status: accepted
---

# Papel do agente imposto por hook e matriz

## Contexto

A guarda do `omhl` só separava os autores `user`, `claude`, `loader`, `executor` e `guideline-retire` ([ADR 0004](0004-executor-e-loader-como-fronteira-da-guarda.md)). Todo agente que chamava a CLI pelo Bash gravava como `claude`, igual à sessão principal. O ciclo com os agentes do plugin (analyst, planner, architect, critic, code-reviewer, security-reviewer e verifier) precisa que cada papel grave só o que é dele: o critic não grava diretiva, o architect não confirma premissa, só o verifier retira um `contradicts`.

O Claude Code entrega o `agent_type` do subagente no evento do hook `PreToolUse`; para um subagente de plugin o valor é o identificador com escopo `<plugin>:<agente>`, aqui `oh-my-hexflow:<papel>` ([Hooks reference, Common input fields e SubagentStart](https://code.claude.com/docs/en/hooks#common-input-fields), consultada em 2026-11-18). O Bash do agente não recebe nenhuma variável de ambiente do plugin ([Plugins reference, Where each variable resolves](https://code.claude.com/docs/en/plugins-reference#where-each-variable-resolves)), e o subagente escreve o comando que quiser. A fonte do papel tem de ser algo que o agente não digita.

## Decisão

O papel do agente portado vem de um hook e de um dado que mora em três lugares, mantidos iguais por teste:

- **O hook `omhl hook agent-role`** (`PreToolUse` do `Bash` e do `Monitor`, `app/agent_role.rs`). É o mecanismo, não uma cópia do dado. Para um `agent_type` `oh-my-hexflow:<papel>`, ele lê o `omhl-allow` do arquivo `plugin/agents/<papel>.md`, confere o comando contra a lista e, quando está dentro, responde `allow` com o comando reescrito: `OMHL_ROLE=omhl:<papel>` na frente. O papel é a variável `OMHL_ROLE`, definida pelo hook; o hook nega a atribuição dela escrita à mão, de qualquer origem. A resposta `allow` com `updatedInput` troca o comando e dispensa o prompt, e só a saída `2` bloqueia a chamada; qualquer outro código não zero deixa o comando rodar ([PreToolUse decision control](https://code.claude.com/docs/en/hooks#pretooluse-decision-control) e [Exit code 2 behavior per event](https://code.claude.com/docs/en/hooks#exit-code-2-behavior-per-event)).
- **A matriz compilada no binário** (`guard/role.rs`), dona de `nodes`, `relations` e `retire`. Com `OMHL_ROLE` válido sob o Claude Code, a CLI grava `agent: "omhl:<papel>"` e a guarda aplica a linha do papel e as condições finas do `append`. Um valor fora dos sete papéis, ou a variável fora do Claude Code, é recusado com `role-unknown`.
- **O `omhl-allow` no arquivo de cada agente.** Diz ao hook quais folhas da CLI o papel roda, antes de o binário existir no caminho. O Bash do agente não recebe `CLAUDE_PLUGIN_ROOT`, então o `append` não lê o frontmatter. Em produção, o `omhl-allow` é o único dono de `commands` e a matriz compilada é a única dona de `nodes`, `relations` e `retire`; a lista `append` do `omhl-allow` repete a matriz só para o teste de paridade, e a CLI não a lê.

A tabela única de `docs/directives-flow.md` (seção 8, "A matriz por papel") é a terceira cópia dos quatro campos, também travada por teste. Para os testes, a referência é `guard/role.rs`: `ROLE_REFERENCE_COMMANDS` (campo `commands`, só de teste) e `ROLE_REFERENCE_MATRIX` (`nodes`, `relations`, `retire`, lida de `MATRICES`). Quando um teste de paridade falha, a referência é essa; o `omhl-allow` e a tabela são corrigidos até igualar, e quem quer mudar um papel muda a referência e as duas cópias no mesmo commit. O `omhl-allow` é lido com a crate `serde-saphyr =2.1.1`, só com o recurso `deserialize`. O dono a escolheu pela popularidade e pela manutenção; a `serde_yaml` está arquivada.

**O que o leitor de comando cobre.** O hook não usa biblioteca de shell: um leitor próprio (`workflow/shell/`) entende aspas, heredocs, separadores, redirecionamentos e os programas que embrulham o `omhl` (`env`, `time`, `timeout`, `nice`, `nohup`, `command`, `exec`, `stdbuf`, `xargs` e `rtk proxy`, com as opções que sabe pular). Na dúvida, o hook nega, se o comando for de agente portado e mencionar `omhl`. A lista do que ele nega está em [directives-flow.md](../directives-flow.md), seção 12. Fora dela, também nega `env -i`, `env -S` e `exec -c`, quando alguma palavra do comando contém `omhl`.

## Consequências

- **O que muda na consequência do ADR 0004.** O ADR 0004 diz que "o agente que chama a CLI pelo Bash continua indistinguível do usuário". Para o agente portado, isso deixa de valer: o record sai com `agent: "omhl:<papel>"`, distinto de `claude` e de `user`, e a guarda aplica a matriz do papel. Contra a má-fé nada muda: o agente que escapa do hook continua gravando como `claude`. O ADR 0004 não é reescrito; a frase do `README.md` ("Autoria") que repetia o texto antigo passa a dizer isto.
- **`user` é o `omhl` fora do Claude Code**, sem a variável `CLAUDECODE`. O dono retira um `contradicts` (a retirada é de `user` e do verifier, nunca de `claude`) só de um terminal externo. Terminais de IDE e `! omhl ...` dentro do Claude Code gravam `claude`, e a retirada é recusada com `role-retirement-forbidden`.
- **A retirada do verifier tem duas chamadas.** Nenhum comando lê o id de record de uma relação, e a retirada o pede como `base`. O verifier envia a retirada sem `base`, recebe `FORK_REJECTED` com o id vigente e repete o pedido com ele. Um comando de leitura de relação fica para outro work item.
- **A razão da retirada não fica no grafo.** A `retirement` não tem `data`, e o verifier não grava `decision`. A razão vai na mensagem do verifier e na coluna de evidência da tabela do PR. O piloto decide se um tipo novo se justifica.
- **O `allow` do hook dispensa o prompt do dono para o comando reescrito.** O comando `omhl` de um agente portado que está na lista do papel roda sem confirmação, nos modos `default` e `auto`. Regra `deny` ou `ask` das configurações do dono continua valendo depois do `allow` do hook, e as ações que nenhum modo aprova também. A fonte é a doc de hooks do Claude Code ("PreToolUse decision control" e "PreToolUse updatedInput in auto mode"), conferida em 2026-11-18, e não um teste deste repositório.
- Os testes `agent_role`, `agent_role_hook`, `agents` e `agent_examples` travam a matriz, o hook, o frontmatter e cada exemplo de comando dos agentes pelo hook e pela guarda.

## Limites

O hook protege contra o **erro** do agente, não contra a **má-fé**. O limite foi assumido pelo dono e vale para a v1. Ficam fora do hook; o comando roda sem prefixo e grava como `claude`, sem a matriz do papel:

- `bash -c`, `sh -c`, `eval`, `source`, `sudo` e `doas`, e os programas que executam outro comando sem estarem na lista de embrulhos (`setsid`, `chrt`, `flock`, `coproc`, `find -exec`, `script -c`);
- `cargo run -p omhl -- append`, comum neste repositório, e qualquer caminho que o leitor não reconheça como `omhl`;
- um nome do programa escondido por escape ANSI-C (`$'o\x6dhl'`) ou montado por aspas dentro do corpo de um heredoc com substituição, que o leitor não decodifica;
- as outras ferramentas: o hook vale só para o `Bash` e o `Monitor` (matcher `Bash|Monitor` em `plugin/hooks/hooks.json`);
- um evento que o hook não consegue ler (stdin acima de 1 MiB, JSON inválido, UTF-8 inválido ou aninhamento fundo demais): o hook escreve o erro no stderr, sai com `0` e deixa o comando passar sem prefixo, como o `spec-lock`. Comando de agente portado que passa de 256 KiB já é negado pelo leitor; o corte do evento (1 MiB) vem antes e libera;
- um `agent_type` fora do formato `oh-my-hexflow:<papel>` (por exemplo, se o Claude Code mudar o escopo do nome do subagente de plugin): o hook devolve `Pass` para todos os agentes, em silêncio, e o `omhl` deles grava como `claude`. A defesa é `tests/plugin.rs`, que fixa o `name` do manifesto, e reconferir o formato na doc de hooks a cada versão do Claude Code que o dono adotar;
- um erro de leitura na sessão principal, que deixa passar um `OMHL_ROLE=` escrito à mão, e um `CLAUDE_PLUGIN_ROOT` vazio ou relativo, que o hook não recusa.

**Fail-open do `|| exit 1`.** O Claude Code só bloqueia a chamada de ferramenta quando o hook sai com `2`. Um `omhl` velho no `PATH` (sem o subcomando `hook agent-role`, o clap sai com `2`) bloquearia todo `Bash`. O envoltório `|| exit 1` converte essa saída em `1` (erro não bloqueante), e o comando roda sem prefixo: o `omhl` do agente grava como `claude`, sem recusa e sem o `OMHL_ROLE`. O Claude Code mostra o aviso `PreToolUse:Bash hook error` na transcrição, mas nada impede o comando. O binário ausente (`127`) e um pânico do hook (`101`) já não bloqueiam, com ou sem o envoltório, e caem no mesmo resultado. O custo está aceito, porque é erro de ambiente e não má-fé, da mesma família dos caminhos de fuga acima. A defesa é reinstalar o binário depois de cada merge que o altere, e um teste do hook roda o comando reescrito no `bash` e confere que o `OMHL_ROLE` chega ao `omhl`. A negação legítima sai por JSON com saída `0` e não depende do envoltório.

**O que o leitor de comando custa.** O leitor decide sobre o texto cru do comando. Um comando de agente portado que menciona `omhl` em qualquer palavra e contém uma substituição (`$(...)`, crase, `<(...)`) é negado como composto, porque a substituição pode produzir o programa ou a folha (`$(command -v omhl) list`, `P=$(command -v omhl); $P list`). O custo conhecido é que `echo $(date) omhl-docs` também é negado, mesmo sem chamar o `omhl`: o leitor prefere negar a adivinhar o que a substituição produz. Sem a menção a `omhl`, `echo $(date)` passa. O comando reescrito roda no shell da ferramenta, que pode ser `bash` ou `zsh`; o teste do hook o roda nos dois, e o `time -p` fica fora do `zsh`, onde `time` é palavra reservada sem opções.

**O que o dono revisa na v1.** Uma `divergence` aberta e um `contradicts` contra uma diretiva não bloqueiam o PR (`docs/directives-flow.md`, seção 10); a prova de comportamento dos agentes é o piloto, que é outro work item.
