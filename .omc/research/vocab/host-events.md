# Vocabulários de eventos dos hosts (Git, GitHub, Claude Code, Codex, Gemini CLI, Cursor, opencode)

- **Data da consulta:** 2026-11-06. Nenhuma das páginas traz data de revisão; as versões estão na seção de cada fonte.
- **Contexto:** insumo para o debate sobre o `when: {vocabulary, event}` do nó de gate em [omhl-v2-graph-engine.md](../../plans/omhl-v2-graph-engine.md) (pontos abertos, "Campos do nó de gate").
- **Regra de leitura:** cada afirmação factual traz a URL da fonte primária. Nomes de evento ficam na grafia oficial (maiúsculas e minúsculas variam entre hosts). Inferências minhas estão marcadas como "inferência".
- **Verificação executada:** a ordem dos hooks do `git commit` e o bloqueio por `pre-commit` foram testados num repositório descartável (git 2.43.0). O resto vem só da documentação.

## 1. Git hooks

- **Fonte:** <https://git-scm.com/docs/githooks> (tag mais recente do repositório na consulta: v2.56.0; texto-fonte lido em <https://github.com/git/git/blob/master/Documentation/githooks.adoc>, branch `master`).
- **Quantidade:** 29 hooks documentados. Os de ciclo de commit, push, rebase e checkout são os que interessam ao motor.

### Eventos, bloqueio e semântica

| Hook | Comando que dispara | Momento | Bloqueia? | Fonte do detalhe |
|---|---|---|---|---|
| `pre-commit` | `git commit` | antes de obter a mensagem e criar o commit | sim, saída não-zero aborta; `--no-verify` ignora | githooks, seção `pre-commit` |
| `prepare-commit-msg` | `git commit` | após preparar a mensagem padrão, antes do editor | sim, saída não-zero aborta; `--no-verify` **não** ignora | githooks, seção `prepare-commit-msg` |
| `commit-msg` | `git commit`, `git merge` | com a mensagem proposta | sim; `--no-verify` ignora | githooks, seção `commit-msg` |
| `post-commit` | `git commit` | depois do commit | não ("cannot affect the outcome") | githooks, seção `post-commit` |
| `pre-merge-commit` | `git merge` | merge feito com sucesso, antes de criar o commit | sim; `--no-verify` ignora | githooks, seção `pre-merge-commit` |
| `post-merge` | `git merge` (inclui `git pull`) | depois do merge; não roda se houve conflito | não | githooks, seção `post-merge` |
| `pre-rebase` | `git rebase` | antes do rebase | sim ("can be used to prevent a branch from getting rebased") | githooks, seção `pre-rebase` |
| `post-rewrite` | `git commit --amend`, `git rebase` | depois de reescrever commits; argumento 1 é `amend` ou `rebase` | não documentado como bloqueante | githooks, seção `post-rewrite` |
| `post-checkout` | `git checkout`, `git switch`, `git clone`, `git worktree add` | depois de atualizar a worktree | não (o status de saída vira o do comando) | githooks, seção `post-checkout` |
| `pre-push` | `git push` | antes de enviar | sim, saída não-zero aborta sem enviar nada | githooks, seção `pre-push` |
| `pre-receive` | lado remoto, `git push` | antes de atualizar qualquer ref | sim, nenhuma ref é atualizada | githooks, seção `pre-receive` |
| `update` | lado remoto, `git push` | uma vez por ref, antes de atualizá-la | sim, impede aquela ref | githooks, seção `update` |
| `post-receive`, `post-update` | lado remoto, `git push` | depois de atualizar as refs | não | githooks, seções homônimas |
| `reference-transaction` | qualquer comando que atualize refs | estados `preparing`, `prepared`, `committed`, `aborted` | sim nos estados `preparing` e `prepared` | githooks, seção `reference-transaction` |
| `applypatch-msg`, `pre-applypatch`, `post-applypatch` | `git am` | antes da mensagem, antes e depois do commit do patch | os dois primeiros bloqueiam, `post-applypatch` não | githooks, seções homônimas |
| `pre-auto-gc` | `git gc --auto` | antes do gc automático | sim | githooks, seção `pre-auto-gc` |
| `push-to-checkout`, `proc-receive`, `receive-report`, `sendemail-validate`, `fsmonitor-watchman`, `post-index-change`, `p4-changelist`, `p4-prepare-changelist` | casos especiais | ver githooks | não detalhados aqui | githooks |

Teste local (git 2.43.0), ordem observada num `git commit`: `pre-commit` (0 args), `prepare-commit-msg` (2 args), `commit-msg` (1 arg), `post-commit` (0 args). Com `pre-commit` retornando 1, nenhum commit foi criado. Com `--no-verify`, `pre-commit` e `commit-msg` não rodaram e `prepare-commit-msg` e `post-commit` rodaram.

### O que o filtro expressa

- Git **não tem matcher**. O "filtro" é implícito: o comando que dispara o hook (`git commit`, `git push`) e os dados recebidos. `pre-push` recebe o nome e a URL do remoto como argumentos e, no stdin, linhas `<local-ref> SP <local-object-name> SP <remote-ref> SP <remote-object-name>` (https://git-scm.com/docs/githooks, seção `pre-push`). `pre-rebase` recebe o upstream e a branch.
- Quem quer filtrar por branch ou caminho faz isso dentro do script.
- Bloqueio é por **status de saída não-zero** do script, sem JSON.

## 2. GitHub

### 2.1 Eventos de webhook

- **Fonte:** <https://docs.github.com/en/webhooks/webhook-events-and-payloads> (sem versão; página do GitHub.com consultada em 2026-11-06).
- **Quantidade:** 76 eventos listados, entre eles `push`, `pull_request`, `pull_request_review`, `pull_request_review_comment`, `pull_request_review_thread`, `issues`, `issue_comment`, `create`, `delete`, `check_run`, `check_suite`, `status`, `deployment`, `deployment_status`, `release`, `workflow_run`, `workflow_job`, `workflow_dispatch`, `repository_dispatch`, `merge_group`, `ping`, `meta`.
- **Forma:** cada evento tem um nome e, em muitos, um campo `action` com o tipo de atividade (ex.: `pull_request` com `assigned`, `opened`, `closed`...). O nome do evento é o primeiro nível, o `action` é o segundo.
- **Semântica antes/depois:** são notificações de algo que **já aconteceu** ("This event occurs when there is activity on a pull request"). Nenhum webhook bloqueia a ação que o originou. Inferência a partir do texto da página: não há campo de resposta com decisão.

### 2.2 Eventos `on:` do GitHub Actions

- **Fonte:** <https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows> (texto-fonte: <https://github.com/github/docs/blob/main/content/actions/reference/workflows-and-actions/events-that-trigger-workflows.md>, branch `main`).
- **Quantidade:** 33 eventos: `branch_protection_rule`, `check_run`, `check_suite`, `create`, `delete`, `deployment`, `deployment_status`, `discussion`, `discussion_comment`, `fork`, `gollum`, `image_version`, `issue_comment`, `issues`, `label`, `merge_group`, `milestone`, `page_build`, `public`, `pull_request`, `pull_request_review`, `pull_request_review_comment`, `pull_request_target`, `push`, `registry_package`, `release`, `repository_dispatch`, `schedule`, `status`, `watch`, `workflow_call`, `workflow_dispatch`, `workflow_run`. Comentários em pull request usam `issue_comment` (a própria página orienta isso).
- **Tipos de atividade (`types`)**, extraídos da mesma página:

| Evento | `types` |
|---|---|
| `pull_request` | `assigned`, `unassigned`, `labeled`, `unlabeled`, `opened`, `edited`, `closed`, `reopened`, `synchronize`, `converted_to_draft`, `locked`, `unlocked`, `enqueued`, `dequeued`, `milestoned`, `demilestoned`, `ready_for_review`, `review_requested`, `review_request_removed`, `auto_merge_enabled`, `auto_merge_disabled` |
| `check_run` | `created`, `rerequested`, `completed`, `requested_action` |
| `workflow_run` | `completed`, `requested`, `in_progress` |
| `release` | `published`, `unpublished`, `created`, `edited`, `deleted`, `prereleased`, `released` |
| `pull_request_review` | `submitted`, `edited`, `dismissed` |
| `push`, `create`, `delete`, `status`, `fork` | não se aplica |

- **Padrão de tipos:** por padrão `pull_request` só dispara em `opened`, `synchronize` e `reopened`; outros tipos exigem `types` (mesma página, seção `pull_request`).
- **O que o filtro expressa** (https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax): `types` (tipo de atividade), `branches`/`branches-ignore` (nome da ref de destino, com globs), `tags`/`tags-ignore` (apenas `push`), `paths`/`paths-ignore` (arquivos alterados). Se `branches` e `paths` aparecem juntos, ambos precisam casar.
- **Bloqueio:** workflow falho **não bloqueia por si**. Bloqueia o merge quando o check é marcado como obrigatório na proteção de branch: "all required status checks must pass before collaborators can merge" (https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches). Portanto o bloqueio é configuração de repositório, não propriedade do evento.
- **Contexto que o evento carrega:** cada evento define `GITHUB_SHA` e `GITHUB_REF` próprios (ex.: `push` usa o commit da ponta; `pull_request` usa o commit de merge e `refs/pull/<n>/merge`).

## 3. Claude Code

- **Fonte:** <https://code.claude.com/docs/en/hooks> (a página não traz número de versão próprio; menciona recursos até v2.1.319, ex.: `scratchpad_dir` exige v2.1.257).
- **Quantidade:** 33 eventos, em três cadências: por sessão (`SessionStart`, `SessionEnd`), por turno (`UserPromptSubmit`, `Stop`, `StopFailure`) e a cada chamada de ferramenta (`PreToolUse`, `PostToolUse`) (seção "Hook lifecycle").

### Eventos, bloqueio e matcher

| Evento | Quando dispara | Bloqueia? (exit 2) | O `matcher` filtra |
|---|---|---|---|
| `SessionStart` | sessão começa ou retoma | não (stderr só ao usuário) | origem: `startup`, `resume`, `clear`, `compact`, `fork` |
| `Setup` | `--init-only`, `--init` ou `--maintenance` | não | flag: `init`, `maintenance` |
| `UserPromptSubmit` | prompt enviado, antes de o Claude processar | sim, o prompt não chega ao Claude | sem matcher |
| `UserPromptExpansion` | comando digitado vira prompt | sim | nome do comando |
| `PreToolUse` | antes da chamada de ferramenta | sim | nome da ferramenta (`Bash`, `Edit\|Write`, `mcp__.*`) |
| `PermissionRequest` | chamada precisa de decisão de permissão | não (nega por objeto `decision`) | nome da ferramenta |
| `PermissionDenied` | modo auto nega a chamada | não | nome da ferramenta |
| `PostToolUse` | após sucesso | não ("the tool already ran"), stderr vai ao Claude | nome da ferramenta |
| `PostToolUseFailure` | após falha | não | nome da ferramenta |
| `PostToolBatch` | após lote de chamadas paralelas | sim, interrompe o laço antes da próxima chamada ao modelo | sem matcher |
| `Stop` | Claude termina de responder | sim, impede parar e continua a conversa | sem matcher |
| `StopFailure` | turno termina por erro de API | não | tipo de erro (`rate_limit`, `server_error`...) |
| `SubagentStart` / `SubagentStop` | subagente criado / termina | não / sim | tipo de agente |
| `TaskCreated` / `TaskCompleted` | tarefa criada / marcada concluída | sim / sim | sem matcher |
| `TeammateIdle` | colega de time vai ficar ocioso | sim | sem matcher |
| `Notification` | Claude Code envia notificação | não | tipo (`permission_prompt`, `idle_prompt`...) |
| `PreCompact` / `PostCompact` | antes / depois da compactação | sim / não | `manual`, `auto` |
| `PreModelSwitch` / `PostModelSwitch` | antes / depois de trocar o modelo | sim / não | nome canônico do modelo |
| `ConfigChange` | arquivo de configuração mudou | sim (exceto `policy_settings`) | fonte da configuração |
| `InstructionsLoaded` | CLAUDE.md ou regra carregada | não | motivo do carregamento |
| `FileChanged` | arquivo observado mudou | não | nomes literais de arquivo a observar |
| `CwdChanged`, `DirectoryAdded` | diretório mudou / foi adicionado | não | `DirectoryAdded`: `slash_command`, `register_repo_root` |
| `WorktreeCreate` / `WorktreeRemove` | worktree criada / removida | sim (qualquer saída não-zero) | sem matcher |
| `Elicitation` / `ElicitationResult` | servidor MCP pede entrada / usuário responde | sim | nome do servidor MCP |
| `MessageDisplay` | texto do assistente exibido | não | sem matcher |
| `SessionEnd` | sessão termina | não | motivo (`clear`, `logout`, `other`...) |

Fonte das colunas "Bloqueia?" e "matcher": seções "Exit code 2 behavior per event" e "Matcher patterns" da página acima.

### Como um hook bloqueia

- **Exit code 2:** "stop, don't do this"; o efeito depende do evento (coluna acima).
- **JSON em stdout (exit 0):** `PreToolUse` usa `hookSpecificOutput.permissionDecision` (`allow`, `deny`, `ask`, `defer`) e `permissionDecisionReason`, mais `updatedInput` para reescrever a entrada. `UserPromptSubmit`, `PostToolUse`, `Stop`, `SubagentStop`, `ConfigChange` e `PreCompact` usam `decision: "block"` com `reason`. `PermissionRequest` usa `decision.behavior`. Seção "Decision control".
- **Entrada comum (stdin, JSON):** `session_id`, `prompt_id`, `transcript_path`, `cwd`, `permission_mode`, entre outros (seção "Common input fields").

### O que o matcher expressa

- Cada evento filtra **um campo diferente** (tabela "Each event type matches on a different field").
- Avaliação: vazio, `*` ou ausente casa tudo; só letras, dígitos, `_`, `-`, espaço, `,` e `|` é **comparação exata** ou lista de valores exatos; qualquer outro caractere vira **regex JavaScript sem âncora** (seção "Matcher patterns").
- Campo `if` (um por handler): sintaxe de regra de permissão, ex. `Bash(git *)` ou `Edit(*.ts)`. **Só vale em eventos de ferramenta** (`PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `PermissionDenied`); em outros eventos o hook com `if` nunca roda. É filtro de melhor esforço: a página recomenda o sistema de permissões para impor regra rígida (seção "How `if` patterns match Bash commands").
- Por isso "antes de `git commit`" no Claude Code é composição: evento `PreToolUse` + matcher `Bash` + `if: "Bash(git commit *)"`. Os subcomandos de uma linha composta (`npm test && git commit`) também são conferidos (mesma seção, tabela).

## 4. Outros agentes de código

### 4.1 Codex CLI

- **Fonte:** <https://developers.openai.com/codex/hooks> (lida em Markdown via `.md`; sem número de versão na página).
- **Eventos (14):** `SessionStart`, `SessionEnd`, `SubagentStart`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreModelRequest`, `PostModelRequest`, `PreCompact`, `PostCompact`, `UserPromptSubmit`, `SubagentStop`, `Stop`, `Interrupt`.
- **Matcher:** string regex; `*`, `""` ou omitido casa tudo. Filtra nome da ferramenta (`PreToolUse`, `PostToolUse`, `PermissionRequest`), origem (`SessionStart`: `startup`, `resume`, `clear`, `compact`, `fork`), gatilho de compactação (`manual`, `auto`) e tipo de subagente. `UserPromptSubmit`, `Stop`, `Interrupt`, `PreModelRequest` e `PostModelRequest` ignoram o matcher (seção "Matcher patterns"). Shell é casado como `Bash`; `apply_patch` aceita também `Edit` e `Write`.
- **Bloqueio:** `PreToolUse` com `hookSpecificOutput.permissionDecision: "deny"`, com `{"decision":"block","reason":...}` ou com exit code 2 e motivo em stderr. `UserPromptSubmit` bloqueia o prompt por `decision: "block"` ou exit 2. Em `Stop`, `decision: "block"` **não rejeita o turno**: faz o Codex continuar, com `reason` como novo prompt (seções `PreToolUse`, `UserPromptSubmit`, `Stop`).
- **Sem campo `if`:** busca no texto da página não achou equivalente. O filtro por comando (ex.: só `git commit`) fica no script.

### 4.2 Gemini CLI

- **Fonte:** <https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/reference.md> (branch `main`, sem versão).
- **Eventos (11):** `BeforeTool`, `AfterTool`, `BeforeAgent`, `AfterAgent`, `BeforeModel`, `BeforeToolSelection`, `AfterModel`, `SessionStart`, `SessionEnd`, `Notification`, `PreCompress`.
- **Matcher:** "regex (for tools) or exact string (for lifecycle)". Em `BeforeTool` e `AfterTool` casa o nome da ferramenta (`read_file`, `run_shell_command`, `mcp_<servidor>_<ferramenta>`).
- **Bloqueio:** exit 2 ("System Block", stderr vira motivo) ou JSON com `decision: "deny"`/`"block"`. Efeito por evento: `BeforeTool` impede a ferramenta e o turno continua; `BeforeAgent` descarta o prompt; `AfterAgent` com deny **rejeita a resposta e força nova tentativa**; `BeforeModel` cancela a chamada ao LLM. `SessionStart`, `SessionEnd`, `Notification` e `PreCompress` são consultivos e ignoram `continue` e `decision`.
- **Campo `continue: false`:** encerra o laço do agente inteiro.

### 4.3 Cursor

- **Fonte:** <https://cursor.com/docs/hooks> (lida em Markdown; sem versão na página).
- **Eventos (21)**, em três categorias: agente (`sessionStart`, `sessionEnd`, `preToolUse`, `postToolUse`, `postToolUseFailure`, `subagentStart`, `subagentStop`, `beforeShellExecution`, `afterShellExecution`, `beforeMCPExecution`, `afterMCPExecution`, `beforeReadFile`, `afterFileEdit`, `beforeSubmitPrompt`, `preCompact`, `stop`, `afterAgentResponse`, `afterAgentThought`), Tab (`beforeTabFileRead`, `afterTabFileEdit`) e ciclo do app (`workspaceOpen`).
- **Matcher:** string regex; vazio ou `*` casa tudo. O valor testado muda por evento: nome da ferramenta em `preToolUse`/`postToolUse` (`Shell`, `Read`, `Write`, `Grep`, `Delete`, `Task`, `MCP:<nome>`), **texto completo do comando** em `beforeShellExecution`/`afterShellExecution`, tipo de subagente em `subagentStart`/`subagentStop`; nos demais é um valor fixo (`beforeSubmitPrompt` casa `UserPromptSubmit`, `stop` casa `Stop`) (seção "Matcher Configuration").
- **Bloqueio:** saída `{"permission": "allow" | "deny" | "ask"}` (com `user_message` e `agent_message`) em `preToolUse`, `beforeShellExecution` e `beforeMCPExecution`; `beforeSubmitPrompt` usa `{"continue": false}`. A página diz que saídas inválidas bloqueiam e que "non-zero exit codes other than `2` fail open", exceto com `failClosed: true`. Inferência: exit 2 bloqueia. Em `preToolUse`, `"ask"` é aceito pelo schema mas não é aplicado hoje.
- **`stop`:** pode devolver `followup_message`, que o Cursor submete como próxima mensagem do usuário (limite padrão de 5 repetições).

### 4.4 opencode

- **Fonte:** <https://opencode.ai/docs/plugins/> (texto-fonte lido em <https://github.com/sst/opencode/blob/dev/packages/web/src/content/docs/plugins.mdx>, branch `dev`).
- **Modelo diferente:** não há hooks de comando. São **plugins JavaScript/TypeScript em processo**. O plugin é uma função que devolve um objeto de manipuladores, por exemplo `"tool.execute.before"`, `"shell.env"` e `event`.
- **Eventos assináveis (28):** `command.executed`; `file.edited`, `file.watcher.updated`; `installation.updated`; `lsp.client.diagnostics`, `lsp.updated`; `message.part.removed`, `message.part.updated`, `message.removed`, `message.updated`; `permission.asked`, `permission.replied`; `server.connected`; `session.created`, `session.compacted`, `session.deleted`, `session.diff`, `session.error`, `session.idle`, `session.status`, `session.updated`; `todo.updated`; `shell.env`; `tool.execute.after`, `tool.execute.before`; `tui.prompt.append`, `tui.command.execute`, `tui.toast.show`.
- **Bloqueio:** o único exemplo documentado de bloqueio é lançar um `Error` dentro de `tool.execute.before` (exemplo "`.env` protection"). A página não descreve contrato de bloqueio para os outros eventos.
- **Matcher:** não existe. O filtro é `if` no código (ex.: `input.tool === "read"`).

## 5. (a) Tabela de equivalência

"-" significa que a fonte não documenta evento equivalente. Células com "(filtro no script)" indicam que o host não tem filtro declarativo para aquela condição.

| Conceito | Git | GitHub (Actions `on:`) | Claude Code | Codex CLI | Gemini CLI | Cursor | opencode |
|---|---|---|---|---|---|---|---|
| Sessão começa | - | - | `SessionStart` (matcher `startup\|resume\|clear\|compact\|fork`) | `SessionStart` (mesmos valores) | `SessionStart` (`startup`, `resume`, `clear`) | `sessionStart` | `session.created` |
| Prompt do usuário, antes de processar | - | - | `UserPromptSubmit` (bloqueia) | `UserPromptSubmit` (bloqueia) | `BeforeAgent` (bloqueia) | `beforeSubmitPrompt` (bloqueia) | - |
| Antes de uma ferramenta | - | - | `PreToolUse` (matcher = nome da ferramenta) | `PreToolUse` | `BeforeTool` | `preToolUse` | `tool.execute.before` |
| Depois de uma ferramenta | - | - | `PostToolUse`, `PostToolUseFailure` | `PostToolUse` | `AfterTool` | `postToolUse`, `postToolUseFailure` | `tool.execute.after` |
| **Antes do commit** | `pre-commit`, `commit-msg` | - | `PreToolUse` + matcher `Bash` + `if: "Bash(git commit *)"` | `PreToolUse` + matcher `Bash` (filtro no script) | `BeforeTool` + matcher `run_shell_command` (filtro no script) | `beforeShellExecution` + matcher `git commit` (regex sobre o comando) | `tool.execute.before` (filtro no código) |
| Depois do commit | `post-commit` | `push` (se enviado) | `PostToolUse` + `Bash` + `if` | `PostToolUse` + `Bash` | `AfterTool` + `run_shell_command` | `afterShellExecution` | `tool.execute.after` |
| **Antes do push** | `pre-push` (e `pre-receive`/`update` no servidor) | - | `PreToolUse` + `Bash` + `if: "Bash(git push *)"` | `PreToolUse` + `Bash` | `BeforeTool` + `run_shell_command` | `beforeShellExecution` + `git push` | `tool.execute.before` |
| Push já feito | `post-receive` (servidor) | `push` | - | - | - | - | - |
| Antes do rebase | `pre-rebase` | - | idem Bash + `if` | idem | idem | idem | idem |
| Editar arquivo (depois) | - | - | `PostToolUse` + `Edit\|Write`; `FileChanged` | `PostToolUse` + `Edit\|Write` | `AfterTool` | `afterFileEdit` | `file.edited` |
| Agente termina o turno | - | - | `Stop` (pode forçar continuar) | `Stop` (pode forçar continuar) | `AfterAgent` (pode forçar retry) | `stop` (`followup_message`) | `session.idle` (só observa) |
| Antes da compactação | - | - | `PreCompact` (bloqueia) | `PreCompact` | `PreCompress` (não bloqueia) | `preCompact` (observa) | - (`session.compacted` é depois) |
| Subagente inicia / termina | - | - | `SubagentStart` / `SubagentStop` | `SubagentStart` / `SubagentStop` | - | `subagentStart` / `subagentStop` | - |
| Pedido de permissão | - | - | `PermissionRequest` | `PermissionRequest` | `Notification` (`ToolPermission`, só observa) | - | `permission.asked` / `permission.replied` |
| Sessão termina | - | - | `SessionEnd` | `SessionEnd` | `SessionEnd` | `sessionEnd` | - (`session.deleted` é outro fato) |
| Worktree criada / removida | `post-checkout` (também em `git worktree add`) | - | `WorktreeCreate` / `WorktreeRemove` | - | - | - | - |
| PR aberto, revisado, mesclado | - | `pull_request` (`types`), `pull_request_review`, `merge_group` | - | - | - | - | - |
| Resultado de CI | - | `check_run`, `check_suite`, `status`, `workflow_run` (`completed`) | - | - | - | - | - |

Notas:

- "Antes do commit" no Git é um evento nativo. Nos agentes é sempre **uma chamada de ferramenta Shell/Bash cujo texto casa `git commit`**. Só o Claude Code tem filtro declarativo por comando (`if`); no Cursor o próprio matcher é o filtro sobre o comando; nos demais o script decide.
- Um commit feito pelo agente passa pelos dois mundos: o hook do agente (se configurado) e o `pre-commit` do Git (que roda quando o `git commit` é executado de fato).

## 6. (b) Conceitos que se repetem em todas as fontes

Candidatos a núcleo comum do `when`:

1. **Fase relativa ao fato: antes ou depois.** Git (`pre-*`/`post-*`), Claude Code (`Pre*`/`Post*`), Codex, Gemini (`Before*`/`After*`), Cursor (`before*`/`after*`, `pre*`/`post*`) e opencode (`*.before`/`*.after`) usam isso no próprio nome. GitHub Actions é sempre "depois" (o evento já ocorreu). Os nomes são regra de leitura, mas o conceito é o mesmo.
2. **"Antes" pode bloquear, "depois" não.** Git: `post-commit`, `post-merge`, `post-receive` "cannot affect the outcome". Claude Code: `PostToolUse` e `PostToolUseFailure` "the tool already ran/failed". Gemini e Cursor seguem o mesmo padrão. Exceções documentadas: `Stop` (Claude, Codex) e `AfterAgent` (Gemini) bloqueiam "o ato de parar", que é um "antes de encerrar".
3. **Sujeito do evento: o que está acontecendo.** Em todas as fontes, o evento recorta um objeto: sessão, prompt, chamada de ferramenta, commit, push, compactação, subagente, PR, run de CI. As categorias reaparecem de um host a outro.
4. **Discriminador secundário dentro do evento.** Git: argumentos e stdin. GitHub: `action`/`types`. Claude, Codex, Gemini e Cursor: `matcher`. É sempre "qual variante deste evento".
5. **Protocolo de bloqueio.** Git: saída não-zero. Claude, Codex, Gemini e Cursor: exit code 2, ou JSON com decisão. Padrão comum: o hook devolve um motivo que o agente (ou usuário) lê.
6. **Evento de ferramenta por nome de ferramenta.** Claude (`Bash`), Codex (`Bash`), Gemini (`run_shell_command`), Cursor (`Shell`), opencode (`bash`): o nome varia, o recorte "ferramenta de shell" existe em todos.
7. **Cadência de ciclo de vida: início e fim de sessão, antes da compactação.** Presentes em Claude, Codex, Gemini e Cursor.

## 7. (c) O que é específico de um host e não generaliza

| Host | Específico | Por que não generaliza |
|---|---|---|
| Git | Hooks do lado servidor (`pre-receive`, `update`, `post-receive`), `reference-transaction`, `post-index-change`, hooks de `git-p4` e `git-am` | Dependem de operações do Git; outros hosts não têm o conceito |
| GitHub | `schedule`, `workflow_dispatch`, `repository_dispatch`, `workflow_call`, eventos de issue, projeto, discussão, release, pacote | Fatos de plataforma, sem análogo local |
| GitHub | `types`/`action` como segundo nível, filtros `branches`, `tags`, `paths` | Lista de valores varia por evento (21 `types` em `pull_request`, 1 em `watch`) |
| GitHub | Bloqueio só via proteção de branch (check obrigatório) | O evento nunca carrega decisão |
| Claude Code | `PermissionRequest`, `PermissionDenied`, `PostToolBatch`, `PreModelSwitch`, `Elicitation`, `MessageDisplay`, `TeammateIdle`, `TaskCreated` | Ligados a recursos do produto |
| Claude Code | Campo `if` com regra de permissão (`Bash(git *)`), só em eventos de ferramenta | Sintaxe própria, filtro de melhor esforço |
| Claude Code | `permissionDecision: "defer"`, `updatedInput`, tipos de handler `http`, `mcp_tool`, `prompt`, `agent` | Contrato próprio de saída |
| Codex | `Interrupt`, `PreModelRequest`, `PostModelRequest`; hooks exigem revisão e confiança antes de rodar | Recursos do produto |
| Gemini CLI | `BeforeModel`, `AfterModel`, `BeforeToolSelection` (altera o pedido ao LLM, resposta sintética, lista de ferramentas permitidas) | Opera no nível da chamada ao modelo, não no nível de ação do agente |
| Cursor | Hooks de Tab (`beforeTabFileRead`, `afterTabFileEdit`), `workspaceOpen`, `afterAgentThought`, `beforeReadFile` | Recursos do editor |
| opencode | Plugins em processo (JS/TS), eventos de bus (`message.part.updated`, `lsp.*`, `tui.*`) e sem matcher | Modelo de extensão diferente: código, não configuração |

## 8. (d) Formato sugerido para `when`

```yaml
when:
  vocabulary: <id do vocabulário>   # ex.: git, claude-code, github-actions, codex, gemini-cli, cursor, opencode
  event: <nome oficial do evento, na grafia do host>
  matcher: <opcional>               # interpretado conforme o vocabulário
```

Pontos de desenho (sugestões, para o debate):

- `event` guarda a **grafia oficial do host**, sem normalizar. É o que cada fonte publica e o que um leitor humano reconhece. A normalização entre hosts fica no vocabulário, não no nó.
- `matcher` é **opcional** e **opaco para o motor**: cada vocabulário declara como o interpreta. Em Claude Code, Codex, Gemini e Cursor é uma string (exata, lista ou regex, conforme o host). No GitHub Actions é um objeto com `types`, `branches`, `tags`, `paths`. No Git é ausente ou um argumento (ex.: nome da branch).
- Para o Claude Code, o filtro `if` precisa de lugar: sugestão `matcher: {value: "Bash", if: "Bash(git commit *)"}`.
- Na v1 o conteúdo do gate é só texto de instrução e nada dispara (decisão 4 do plano). Por isso `when` precisa ser **legível para o agente** ("antes de `git commit`, no host Claude Code"), não executável. Hooks reais ficam para os marcos futuros.

### Quatro exemplos reais

1. **Git, antes do commit:** o nativo do Git.

   ```yaml
   when:
     vocabulary: git
     event: pre-commit
   ```

2. **Claude Code, antes de `git commit` pelo agente:** `PreToolUse` com matcher `Bash` e `if` filtrando o comando (campos documentados em https://code.claude.com/docs/en/hooks, seções "Matcher patterns" e "Common fields"). Não executei este hook num Claude Code real.

   ```yaml
   when:
     vocabulary: claude-code
     event: PreToolUse
     matcher: {value: Bash, if: "Bash(git commit *)"}
   ```

3. **GitHub Actions, PR aberto ou atualizado em `main`:** equivale a `on: pull_request: {types: [opened, synchronize], branches: [main]}`.

   ```yaml
   when:
     vocabulary: github-actions
     event: pull_request
     matcher: {types: [opened, synchronize], branches: [main]}
   ```

4. **Claude Code, início de sessão (retoma ou nova):** é o momento em que a v1 injeta a base nativa (decisão 5 do plano).

   ```yaml
   when:
     vocabulary: claude-code
     event: SessionStart
     matcher: "startup|resume"
   ```

## 9. Lacunas

- **Sem data nem versão:** nenhuma das páginas de Claude Code, Codex, Gemini, Cursor e opencode traz data de revisão. Registro só a data da consulta (2026-11-06) e, no Claude Code, o maior número de versão citado no texto (v2.1.319). A página de Git é a do site e o texto-fonte vem do branch `master`; a tag mais recente do repositório na consulta era v2.56.0, mas não confirmei que a página publicada corresponde a ela.
- **Gemini e opencode** foram lidos nos repositórios oficiais (branches `main` e `dev`), não nas páginas renderizadas. As páginas dos sites não foram comparadas linha a linha com o repositório. No opencode, `sst/opencode` pode ter sido renomeado; a URL funcionou na consulta.
- **Codex:** li a página inteira de eventos de `PreToolUse`, `UserPromptSubmit`, `Stop` e `SessionStart`. Não li os detalhes de bloqueio de `PostToolUse`, `PermissionRequest`, `PreCompact` e `SubagentStop`. A ausência de campo `if` vem de busca textual na página.
- **Cursor:** que exit code 2 bloqueia é inferência a partir de "non-zero exit codes other than `2` fail open". Não li o contrato de bloqueio de `beforeReadFile`, `preCompact` e dos hooks de Tab. Não verifiquei se a regex do matcher é ancorada.
- **opencode:** só um exemplo documentado de bloqueio (`tool.execute.before` com `throw`). Não há evidência na página de que `session.*`, `message.*` ou `permission.*` possam bloquear. Prompt do usuário não tem evento bloqueante documentado.
- **GitHub webhooks:** a página renderizada mostra apenas um tipo de `action` por evento (seletor dinâmico); por isso os `types` completos vêm da página de Actions, que não cobre os 76 eventos. Cabeçalhos de entrega (`X-GitHub-Event`) não foram verificados.
- **GitHub bloqueio:** a afirmação "webhook não carrega decisão" é inferência da leitura das páginas. Verifiquei a regra de checks obrigatórios apenas pela página de branches protegidas, sem testar num repositório.
- **Git:** não verifiquei se `--no-verify` ignora `pre-push` (a página do `githooks` para `pre-push` que li não o menciona; a opção existe em `git push` segundo meu conhecimento prévio, não conferido aqui). Não detalhei `p4-*` além de `p4-changelist` e `p4-prepare-changelist`.
- **Claude Code:** o exemplo 2 (`PreToolUse` + `if: "Bash(git commit *)"`) usa a sintaxe documentada, mas não foi executado.
- **Fora do escopo:** não pesquisei outros hosts (Aider, Windsurf, Copilot, GitLab CI, Jenkins) nem os vocabulários curados (schema.org, CloudEvents, OpenTelemetry, PROV-O), que cabem a outros pesquisadores.
