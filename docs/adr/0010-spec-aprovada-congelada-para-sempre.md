---
status: accepted
---

# Spec aprovada congelada para sempre

## Contexto

O hook `spec-lock` barrava a edição da spec só enquanto a branch do work item existia. Quem apagava a branch (merge, limpeza da worktree) reabria a spec aprovada, e o agente podia reescrevê-la sem emenda. Duas falhas vizinhas apareciam na revisão da [issue #9](https://github.com/gabrielh-silvestre/oh-my-hexflow/issues/9) (M1, M2, M3, M4):

- O hook comparava o caminho só no texto, então um `Edit` por um link simbólico para `.omhl/specs` passava.
- O hook consultava o `cwd` da sessão: com o `cwd` no repositório A, um `Write` na spec do repositório B caía em `spec-in-linked-worktree`.

A entrega faz parte da umbrella [#42](https://github.com/gabrielh-silvestre/oh-my-hexflow/issues/42): uma spec aprovada nunca é editada, com ou sem branch.

## Decisão

**Congelada é branch local ou marca `.frozen`.** Um work item está congelado quando tem uma branch local `tipo/wi-<id>__<slug>` ou um arquivo vazio `.frozen` em qualquer pasta `wi-<id>__*` dele. A marca é por work item e sobrevive à branch. O `spec-lock` decide só pelo disco e pelo git, nunca pelo banco.

**Uma gravação só.** `SpecStore::write_freeze_mark` (`ports/specs.rs`) é a única função que grava a marca. Ela recusa pasta que é link simbólico, não segue link no nome da marca (`create_new`) e trata qualquer coisa que já ocupe o nome como "já marcada". Dois chamadores:

- a **cura do `spec-lock`**: ao decidir um evento de spec, o hook grava a marca de todo work item que tem branch local e ainda não a tem;
- o comando **`omhl hook wt-post-start --branch <nome>`**, chamado pelo `post-start` do `wt` (`.config/wt.toml`) quando a worktree nasce.

O gate de worktree (`omhl gate-worktree`) continua só leitura. As cinco specs que já existiam sem branch (`agt1`, `dir1`, `dir2`, `out1`, `out2`) receberam a marca à mão, uma vez, antes desta entrega.

**Resolução de caminho.** O hook avalia o caminho lexical (`.` e `..` normalizados no texto) e o físico (`resolve_physical`, `ports/paths.rs`: links resolvidos, a parte que ainda não existe anexada no texto, link pendente seguido, até 40 saltos). A negação de qualquer um dos dois vence. Se o caminho físico não se resolve, vale o lexical, com a linha `path-unresolvable` no stderr. O `cwd` da sessão não entra mais na decisão.

**Subpasta passa (regra do M6).** O nome de uma emenda é conferido só no nível da pasta do work item. Um arquivo novo dentro de uma subpasta, mesmo chamado `parte-01.md` ou `emenda-07.md`, passa numa spec congelada, e o `spec validate` não o lê. Um teste fixa esse comportamento.

**Teto de 99 emendas (D7).** O padrão `emenda-NN.md` tem dois dígitos. Com `emenda-99.md` na pasta, o hook sugere e libera `emenda-100.md`, e o `spec validate` a recusa com `amendment-name-invalid`. O teto de 99 emendas por spec é um limite aceito: só registro, sem código novo.

**Dois contratos no grupo `hook`.** Os eventos do Claude Code (`session-start`, `spec-lock`, `agent-role`) leem o stdin e nunca saem com `2`; em erro interno `session-start` e `agent-role` liberam, e o `spec-lock` libera só antes de reconhecer a spec (sem marcador válido, sem pasta `wi-<id>__<slug>`, outra ferramenta, stdin que não é JSON, payload acima de 1 MiB). Depois do reconhecimento, uma falha de git ou de leitura nega com `spec-lock-unavailable`, dentro de um orçamento de 6 segundos de git menor que o `timeout` do hook, e o `hooks.json` envolve o `spec-lock` em `|| exit 2` (o `omhl` ausente, o pânico e a saída `1` bloqueiam todo `Write`, `Edit` e `MultiEdit`, em qualquer repositório com o plugin habilitado), ao contrário do `agent-role`, que segue com `|| exit 1`. O marcador inválido no caminho de subida é pulado, e na pasta congelada `.omhl.toml` e `.omhl/` são negados com `spec-frozen`, para um marcador plantado não soltar a trava. O hook avalia também o `.omhl/workflow.toml` principal: `Write`, `Edit` e `MultiEdit` que mudariam o `specs.dir` efetivo enquanto algum work item tem spec congelada são negados com `spec-dir-frozen`, para mover a pasta de specs não soltar a trava. O `specs.dir` antigo e o novo passam pela mesma definição da política, e as trocas de `Edit` e `MultiEdit` são testadas em três versões do texto, por causa do CRLF. Num projeto omhl, `Write`, `Edit` e `MultiEdit` de caminho com componente `.git` abaixo da raiz do marcador são negados com `git-dir-protected` (regra lexical, sem git), para o agente não redirecionar o git (`.git/commondir`, um `.omhl/.git/` falso) e furar as travas; e as de um `.omhl.toml` que hoje é marcador válido e deixaria de ser são negadas com `marker-protected`, a última regra a rodar, porque esse marcador ancora as três travas. O `wt-post-start` não lê stdin, é silencioso em segundo plano e sai `1` com `spec-store-unusable` em pasta de spec que é link simbólico. O comando é lido por máquina, então entra nos "seis adiados" de formato de saída do [ADR 0009](0009-formato-de-saida-agent-human-json-e-registro-de-renderizadores.md).

## Alternativas descartadas

- **O gate grava a marca.** O `pre-switch` roda antes de a branch existir, e o agente roda o gate só para conferir; um comando de conferência que escreve no disco surpreende.
- **Um fato no hexlog que o hook lê.** O hook teria de abrir o banco, o que ele evita de propósito.
- **Heurística de git** (branch já mesclada, histórico). Não é "para sempre": depende de o histórico existir.
- **Flag `--record` no gate.** Mistura decisão com efeito.
- **Folha de ação `omhl spec freeze`.** Exigiria página de skill e duas listas fixas de cobertura, e convidaria o agente a congelar um rascunho sem existir comando para descongelar.
- **Marca só pela cura do `spec-lock`.** O dono pediu o `post-start` para marcar no nascimento; a cura ficou como rede.
- **Reusar `spec-frozen` para o `workflow.toml`.** O remédio dele (registrar divergência e criar emenda) é o errado para mover a pasta de specs; o código próprio `spec-dir-frozen` deixa o agente distinguir "editar esta spec" de "mover a pasta de specs".
- **Exigir que o `.git` seja pasta para valer como principal.** Custaria uma leitura a mais e um caso a mais no hook sem fechar porta que as ferramentas de arquivo alcancem, porque a negação do `Write` em `.git` já a fecha.

## Consequências

- O `spec-lock` passa a gravar a marca. É o único efeito colateral do hook: ele deixa de ser só leitura do disco.
- O `post-start` roda uma vez, em segundo plano, e falha em silêncio. Se falhar (`omhl` velho, marca não gravável), a cura do `spec-lock` grava a marca no primeiro evento de spec.
- O dono precisa reinstalar o `omhl` no `PATH` e rodar `wt config approvals add` antes de qualquer `wt switch`. Sem a aprovação e sem terminal interativo, o `wt` aborta todo `wt switch`.
- Descongelar é só o dono, apagando o `.frozen` à mão. Não existe comando.
- Depois do primeiro work item, o `specs.dir` fica fixo pelo Claude Code: a branch, ou a marca com a branch apagada, congela a pasta de specs antiga, e só o dono a muda, fora do Claude Code. O `workflow.toml` de uma worktree ligada não é avaliado; a mudança feita nela vale quando o PR é mergeado na `main`, sob o review do diff.
- O agente não muda nada sob `.git` por `Write`, `Edit` ou `MultiEdit` num projeto omhl (também não cria `.git/hooks/*` nem edita `.git/info/exclude`); o dono e o `Bash` seguem livres. O mesmo vale para um `.omhl.toml` que deixaria de ser marcador válido; o marcador quebrado pelo dono fora do Claude Code solta as travas enquanto estiver quebrado.
- As travas protegem contra deslize, não contra agente hostil: o `Bash` fica fora do `spec-lock`; `.claude/settings*.json` com `disableAllHooks`, um `git` falso no `PATH` e um repositório bare com worktrees são limites conhecidos, e também o payload do hook acima de 1 MiB, que o hook recusa com `input-too-large` no stderr e a edição passa (a trava é freio, não cofre).
- `docs/directives-flow.md`, seções 5, 7 e 10, descrevem o comportamento; esta decisão não o repete.

## Limites

- Uma branch criada fora do `wt` (`git branch`, `git switch -c`, `wt switch --no-hooks`) não dispara o `post-start`. Se for apagada sem nenhum evento de spec no meio, não deixa marca e a spec volta a ser editável.
- Há uma janela entre a checagem do hook e a escrita (TOCTOU): se o alvo de um link mudar nesse intervalo, o hook não detecta.
- Um `specs.dir` que é link simbólico continua congelando, porque a decisão vale também pelo caminho lexical.
- Qualquer branch local com o id do work item o congela, mesmo criada por outra pessoa; a marca só sai à mão.
- As regras do `spec-dir-frozen` e do `git-dir-protected` não alcançam um projeto cujo marcador fica numa subpasta do repositório: o `workflow.toml` dele não é o de `<pasta principal do git>/.omhl/`, e o `.git` da pasta principal fica acima do marcador.
- Num sistema de arquivos sem distinção de caixa (macOS; pasta do Windows no WSL), `.OMHL/workflow.toml`, `.GIT` e `.OMHL.TOML` são os mesmos arquivos e os filtros não os reconhecem.

## Follow-ups

- [Issue #9](https://github.com/gabrielh-silvestre/oh-my-hexflow/issues/9) M13 (testes copiados entre arquivos) vai para o PR de testes.
- `AGENTS.md`, passo 5 da Fase 0: usa `wt switch -c --no-cd -y`. Avaliar tirar o `-y` depois que o `post-start` estiver aprovado (decisão do dono).
- A [issue #7](https://github.com/gabrielh-silvestre/oh-my-hexflow/issues/7) reaproveita `resolve_physical` (`db-inside-project`) e a primitiva de abertura não bloqueante (FIFO de `blobs/`).
- G04 (issue [#32](https://github.com/gabrielh-silvestre/oh-my-hexflow/issues/32), M9, M10, M13, M15, N8 e N3) e G06 (#32, M12, N1, N12 e N4) seguem em espera, em PRs próprios.
