# Gates mecânicos

Vocabulário em [GLOSSARY.md](../GLOSSARY.md). Visão geral e primeiros passos no [README](../README.md).

Um check com o campo `run` é mecânico. O `omhl check run` executa o comando, compara o resultado com as regras de `expect`, grava uma attestation com o desfecho (`passed`, `passed_with_warning` ou `failed`) e guarda a saída do comando num arquivo, o `byproduct`. O gate de pull request lê só o desfecho.

O `check run` roda em três eventos, todos do git:

| Evento | Quem dispara | Snapshot da attestation |
|---|---|---|
| `before-commit` | hook `pre-commit` (e `pre-merge-commit`) | `git-tree`: a árvore que o commit vai gravar |
| `before-pr` | hook `pre-push` | `git-commit`: o `HEAD`, com a árvore limpa |
| `before-merge` | você, à mão: `omhl check run --event before-merge` | `git-commit` |

O `omhl check run` só roda no Linux. Em outro sistema ele responde `unsupported-platform`.

Códigos de saída: `0` quando nenhum check bloqueante reprovou, `1` para qualquer erro (uma linha JSON no stderr, stdout vazio) e `2` para erro de uso da linha de comando.

**Evento sem checks.** Um `check run` cujo evento não tem nenhum check no banco sai com `0`, sem rodar nada. Para não passar por aprovação, a saída traz `checks_planned: 0` e `warnings: [{"code": "no-checks-for-event"}]`. O `recorded` continua significando o modo de gravação (work item), não que algo foi gravado.

## 1. Falhas sem veredito

Um erro sem veredito é um problema que impede o `omhl` de dizer se o check passou. Ele não é um `failed`. O `omhl` não grava attestation nesses casos, exceto onde a tabela diz o contrário, e termina com o código de saída 1.

O erro sai no stderr, numa linha:

```json
{"error":{"code":"check-not-run","message":"...","details":[{"path":"checks.fmt","code":"spawn-failed","message":"...","context":{"kind":"spawn-not-found","hint":"..."}}]}}
```

Cada item de `details` nomeia um check (`path`), o motivo (`code`) e, nas falhas sem veredito, um objeto `context` (veja "O `context` do erro", adiante). Nada do `context` vai para o banco.

Depois de uma falha, o `omhl` segue com os checks restantes do evento ou para tudo. Quando para, cada check restante sai com o detalhe `not-run`, sem ter rodado.

| # | Situação | O que o `omhl` faz | Código e `kind` | O que fica gravado | O que tentar |
|---|---|---|---|---|---|
| 1 | Erro de spawn: o binário do `run` não existe, não é executável ou não pôde ser lançado (por exemplo `cargo` fora do `PATH` que o git enxerga). Vale só para `run` com argv direto | Segue para os demais checks. No fim, sai com erro | `check-not-run`, detalhe `spawn-failed`; `kind` `spawn-not-found`, `spawn-permission-denied` ou `spawn-failed` | Nada | Instale o binário ou ponha o diretório dele no `PATH` dos hooks, ou corrija a permissão. Repita o mesmo `omhl check run` |
| 2 | Desfecho imutável diferente: o mesmo snapshot, check e work item já têm outro desfecho gravado | Não substitui o desfecho gravado. Segue para os demais. No fim, sai com erro | `attestation-refused`, detalhe `outcome-immutable`; `kind` `guard-refused` | O desfecho antigo permanece. O `context` traz o `stored` (desfecho gravado), o `expected` (o novo) e o hash do `byproduct` novo | Mude a árvore (um novo commit ou uma nova alteração) para obter um veredito novo |
| 3 | Árvore alterada pelo comando: o check mudou um arquivo rastreado, ou criou um arquivo não rastreado e não ignorado | Para tudo. O check afetado não é gravado e os restantes não rodam | `check-not-run`, detalhe `command-changed-worktree` (`before-commit`, inclui arquivo não rastreado) ou `command-changed-tree` (`before-pr` e `before-merge`); `kind` `tree-changed`; os restantes `not-run` | Nada. O `context` traz a cauda da saída (`output_tail`) | Mantenha o check somente leitura. Ponha arquivos gerados no `.gitignore` ou restaure os rastreados com `git checkout -- <caminho>` |
| 4 | `Busy` esgotado: o banco ficou travado por cerca de 74 segundos depois de o comando rodar. Vale também para outro erro do banco ao gravar a attestation | Segue para os demais. No fim, sai com erro. O erro nunca usa o código `busy` | `attestation-not-recorded`; `kind` `busy-exhausted` (ou `attestation-not-recorded`, quando o banco falha ao gravar por outro motivo) | Nada no banco. O `byproduct` já está no disco e o `context` traz o hash dele, o `expected` e o `output_tail` | Repita o mesmo comando quando o outro processo terminar |
| 5 | Recusa da guarda por snapshot: o índice mudou entre a sonda e a gravação, ou o comando fez `git add` | Para tudo. Os restantes não rodam | `attestation-refused`, com o código da guarda no detalhe: `snapshot-tree-mismatch`, `snapshot-dirty` ou `snapshot-not-head`; `kind` `guard-refused`; os restantes `not-run` | Nada no banco. O `context` traz o hash do `byproduct` (a saída já está no disco), o `expected` e o `snapshot` | Não faça stage durante o hook. Rode de novo |
| 6 | Outra recusa da guarda: o work item foi fechado durante a execução (`work-item-closed`) ou a sonda falhou dentro da guarda (`probe-failed`) | Segue para os demais. No fim, sai com erro | `attestation-refused`, com `work-item-closed` ou `probe-failed` no detalhe; `kind` `guard-refused` | Nada | `work-item-closed`: confira o work item com `omhl get <work-item>`. `probe-failed`: rode `git status` e corrija o estado do repositório |
| 7 | Falha do blob: o `omhl` não consegue gravar o `byproduct` | Para tudo. Sem o arquivo de saída o veredito não seria recuperável | `check-not-run`, detalhe `not-run`; `kind` `blob-failed`; os restantes `not-run` | Nada. O `context` traz o `expected` e o `output_tail` | Corrija a permissão da pasta de saída ou libere espaço em disco. A pasta fica ao lado do banco, em `blobs/` |
| 8 | Falha da sonda: o `omhl` não consegue ler a árvore ou o `HEAD` com o `git` | Para tudo | Antes de qualquer comando (leitura do `HEAD`, no começo ou antes do primeiro check de `before-pr` e `before-merge`): `probe-failed`, sem `context`. Antes de um check de `before-commit` (árvore do índice) ou depois de qualquer comando: `check-not-run` com detalhe `not-run` e `kind` `probe-failed`; os restantes `not-run` | Nada. Quando a sonda falha depois do comando, o `context` traz o `output_tail` | Rode `git status` e corrija o estado do repositório |
| 9 | Spec gravado inválido: o `data` do check no banco tem `run`, mas o `omhl` não consegue decodificá-lo | Não executa esse check. Segue para os demais. No fim, sai com erro | `check-not-run`, detalhe `spec-invalid` (com o `path` e o `code` do problema, sem eco do conteúdo); `kind` `spec-invalid` | Nada | Corrija o TOML do check e rode `omhl load` |
| 10 | Plataforma sem suporte: o sistema não é Linux | Para tudo. Nada executa | `unsupported-platform`; os restantes `not-run` | Nada | Rode o `check run` no Linux ou no WSL |
| 11 | Interrupção por sinal: Ctrl+C, `SIGTERM`, `SIGHUP` ou `kill` ao `omhl` ou só ao `git` | O `omhl` morre pela ação padrão do sistema. Nenhum erro sai. Veja a seção 4 | Sem código de erro. Se o `git` morreu antes de o `omhl` amarrar-se a ele, o erro é `check-not-run`, detalhe `not-run`, `kind` `parent-gone`, e nada rodou | Nada para o check interrompido nem para os que não rodaram. As attestations já gravadas ficam | Rode o mesmo comando de novo. Se sobrar processo de um check, rode `omhl check sweep` |
| 12 | Definição velha: o TOML do check na working tree diverge da definição gravada no banco (`run`, `timeout_secs` ou `expect`), porque alguém editou o arquivo e não rodou `omhl load` | Em qualquer evento (`before-commit`, `before-pr` e `before-merge`): não executa esse check, segue para os demais e, no fim, sai com erro | `check-not-run`, detalhe `spec-stale`; `kind` `spec-stale` | Nada | Rode `omhl load` e repita o `omhl check run` |

**Erro final.** Um problema sem veredito (`check-not-run`, `attestation-refused`, `attestation-not-recorded`) tem precedência sobre `checks-failed`. Entre eles vale o primeiro na ordem de execução. Os detalhes listam todos os checks afetados e os que falharam.

Quando só há checks bloqueantes reprovados, o erro é `checks-failed`. Um check não bloqueante reprovado aparece em `results` com `ok: false` e não muda o código de saída. Cada check bloqueante reprovado traz um detalhe `failed` cuja `message` diz como ler a saída:

```bash
omhl byproduct checks.lint work-items.w1 --tail 40
```

Na branch `develop` não há `byproduct`: o erro de um check reprovado leva a cauda da saída no `context.output_tail`.

### O `context` do erro

O `context` carrega só os campos que se aplicam ao caso: `kind`, `event`, `check`, `work_item`, `branch`, `argv`, `cwd`, `snapshot`, `expected`, `stored`, `byproduct`, `os_error`, `output_tail`, `hint` e, nos erros do `pre-push`, `lref` e `rref`. O `hint` é uma frase fixa em inglês que diz o que tentar.

O `output_tail` (as últimas 20 linhas de `stdout` e de `stderr`, cada uma cortada em 500 caracteres) aparece só quando o comando rodou e nada foi gravado: `Busy` esgotado, outro erro de gravação da attestation, árvore alterada, sonda que falha depois do comando, falha do blob e `develop`. Quando a attestation foi gravada, o erro não repete a saída. Use `omhl byproduct`.

O `output_tail` pode conter segredos que o comando imprimiu. É a mesma saída que você veria rodando o comando à mão.

| `kind` | Quando | `hint` |
|---|---|---|
| `spawn-not-found` | O binário do `run` não existe | `<argv0>` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again |
| `spawn-permission-denied` | O binário não é executável | `<argv0>` is not executable; fix its permissions |
| `spawn-failed` | Outro erro ao lançar o comando | Sem frase fixa. Veja o `os_error` |
| `busy-exhausted` | Banco travado depois de o comando rodar | the database stayed locked for about 74 seconds; the verdict above was computed but not recorded; run the same command again when the other process ends |
| `attestation-not-recorded` | O banco falhou ao gravar por outro motivo que não `Busy` | Sem frase fixa. A `message` do detalhe traz o código do erro do banco |
| `guard-refused` | A guarda recusou a gravação | `snapshot-*`: the index changed while the command ran; do not stage during a hook, then run again. `outcome-immutable`: this snapshot already has the stored outcome for this check; change the tree to get a new verdict |
| `blob-failed` | O `byproduct` não foi gravado | the output store is not writable; fix its permissions or free disk space |
| `probe-failed` | O `git` não leu a árvore | git could not read the tree; run git status and fix the repository state |
| `tree-changed` | O comando alterou a árvore | the command changed the working tree; keep checks read-only, add generated files to .gitignore, or restore tracked files with git checkout -- <path> |
| `push-to-main` | Push com destino `main` | main receives no direct pushes; open a pull request |
| `parent-gone` | O processo que lançou o `omhl` saiu antes dos checks | the process that started omhl exited before the checks ran; nothing ran; run the same command again |

O teste `crates/omhl/tests/repo_checks.rs` confere que cada código e cada `kind` desta seção aparece na página.

## 2. Um comando por check

Um check roda um comando e cada regra olha um fluxo. Não junte dois comandos num `run` (`sh -c "a && b"`, `;`, `2>&1`). Crie um check por comando e ordene com o campo `after`. Assim cada desfecho, `exit_code` e `byproduct` pertencem a um comando só.

Exemplo real do repositório, em `.omhl/checks/lint.toml`:

```toml
concern = "style"
variant = "lint"
blocking = true
after = ["checks.fmt"]
run = ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"]
timeout_secs = 21
expect = [{ exit = 0, outcome = "passed" }]
when = { event = "before-commit" }
```

O `how` vem do arquivo `lint.md`, ao lado do TOML. `concern` e `variant` são campos do TOML: `concern` vale `style`, `test` ou `compile`, e `variant` tem de pertencer ao `concern` (`style` com `format` ou `lint`, `test` com `suite` ou `coverage`, `compile` com `validate` ou `build`).

Cada regra do `expect` tem `exit` e, no máximo, um padrão em `stdout` ou em `stderr`. Uma regra com os dois falha no `omhl load` com `rule-two-streams`, e a mensagem repete a boa prática: `keep one command per check`.

A boa prática trata de juntar comandos, não de usar `sh -c`. Um `run` como `["sh", "-c", "gh ..."]` com um único `gh` é válido. Nesse caso, um `gh` ausente não é erro de spawn: o `sh` existe e sai com 127. O resultado é um `failed` com `exit_code` 127, gravado e imutável para aquele snapshot. Só um `run` com argv direto (`["gh", ...]`) produz o erro de spawn da linha 1 da tabela.

O evento do check é fechado: `before-commit`, `before-pr` ou `before-merge`. O `omhl load` recusa qualquer outro valor com `extensions-invalid` e o código `event-unsupported`. As chaves `blocking`, `how`, `after` e `when` estão em [extensions-and-projection.md](extensions-and-projection.md); as chaves mecânicas (`run`, `timeout_secs`, `expect`, `concern`, `variant`) são as desta seção.

## 3. Branch, marca da `develop` e `main`

O `check run` descobre o work item pelo nome da branch. O padrão é `tipo/wi-<id>__<slug>`:

- `tipo` é um segmento sem `/`.
- `<id>` fica entre `wi-` e o primeiro `__`. Só aceita minúsculas, dígitos e `-`. O work item é `work-items.<id>`.
- `<slug>` não é vazio e pode conter `/`.

`feat/wi-w1__x` e `fix/wi-pr-7__ajusta/parser` seguem o padrão. `feat/wi-w1`, `wi-w1__x`, `feat/wi-W1__x` e `a/b/wi-w1__x` não seguem.

| Branch | O que o `check run` faz |
|---|---|
| `tipo/wi-<id>__<slug>` | Exige que `work-items.<id>` exista e esteja aberto. Roda os checks e grava as attestations nesse work item |
| `develop` | Roda os checks, não grava nada e sai com `"recorded": false` e `"work_item": null`. No `before-commit`, exige a marca abaixo |
| `main` | Responde `branch-protected`. Nada roda |
| Qualquer outra | Responde `branch-name-invalid`. Nada roda |

Quando o `HEAD` está solto (durante um rebase, por exemplo), o `omhl` procura a branch no estado do rebase e, depois, a única branch local que contém o `HEAD`. Com zero ou várias, responde `head-ambiguous`.

Erros que barram antes de rodar qualquer comando:

| Código | Quando | O que fazer |
|---|---|---|
| `branch-name-invalid` | A branch não segue o padrão | `git branch -m` para o nome `tipo/wi-<id>__<slug>` |
| `head-ambiguous` | `HEAD` solto e nenhuma ou várias branches o contêm | `git switch` para a sua branch |
| `branch-protected` | Commit ou `check run` na `main`; ou push com destino `main` (`kind` `push-to-main`) | Crie uma branch `tipo/wi-<id>__<slug>`. Para a `main`, abra um pull request |
| `work-item-not-found`, `work-item-closed` | A branch segue o padrão, mas o work item não existe ou já foi fechado | Grave o work item ou use outra branch |
| `develop-mark-missing` | `before-commit` na `develop` sem a marca | Crie a marca (abaixo) |
| `worktree-differs-from-index` | `before-commit` com a working tree diferente do índice: alteração de arquivo rastreado que não está no stage (vale para um commit sem a opção `-a` e para `git commit <caminho>` com outro arquivo sujo), ou arquivo não rastreado e não ignorado | Faça o stage, ignore ou apague o arquivo |
| `worktree-dirty` | `before-pr` ou `before-merge` com a árvore suja, inclusive arquivo não rastreado e não ignorado | Faça commit, ignore ou apague o arquivo |
| `push-ref-not-checked-out`, `push-input-invalid` | Problemas do `pre-push`, explicados abaixo | Veja a regra `lsha == HEAD` |

### A marca da `develop`

A `develop` não tem work item. Para um commit direto nela, o `before-commit` exige uma marca: o arquivo `omhl-develop-mark` dentro da pasta do git da worktree (`git rev-parse --absolute-git-dir`), com a primeira linha `omhl-develop-mark v1`. Um arquivo vazio ou com outra primeira linha vale como ausente. Um `touch` não cria a marca.

No Claude Code, crie a marca com o command do plugin: `/oh-my-hexflow:omhl-develop-mark`. O texto dele está em [plugin/commands/omhl-develop-mark.md](../plugin/commands/omhl-develop-mark.md). No terminal, o equivalente é:

```sh
printf 'omhl-develop-mark v1\n' > "$(git rev-parse --absolute-git-dir)/omhl-develop-mark"
```

O `check run` nunca apaga a marca. Quem a apaga é o hook `post-commit`, que chama `omhl hooks post-commit` e remove a marca depois de um commit que saiu na `develop`. Um `omhl check run --event before-commit` à mão e um commit abortado não gastam a marca. Um commit com `--no-verify` gasta. Em outra branch, o `post-commit` não faz nada.

Só o `before-commit` exige a marca. O `before-pr` e o `before-merge` na `develop` rodam, barram em falha, não gravam e não pedem a marca.

A marca é um freio, não uma parede: um humano no terminal a cria à mão. O gate de pull request continua cobrando attestation de work item.

### A `main` protegida

A `main` nunca recebe alteração direta:

- O `pre-commit` barra o commit na `main` com `branch-protected`.
- O `pre-push` barra qualquer push cuja ref remota seja `refs/heads/main`, inclusive a remoção da `main` e o push cruzado `feat:main`.

### A regra `lsha == HEAD` do `pre-push`

O `pre-push` entrega ao `omhl` a stdin do git, uma linha por ref (`<ref local> <sha local> <ref remota> <sha remoto>`). O `omhl` lê todas as linhas antes de decidir e aplica, nesta ordem:

1. Qualquer linha bem formada com ref remota `refs/heads/main` responde `branch-protected` (`kind` `push-to-main`). Nada roda, mesmo que outra linha esteja malformada.
2. Qualquer linha sem quatro campos, inclusive uma linha em branco, responde `push-input-invalid`.
3. Remoção de ref (`lsha` só de zeros) e tag (`refs/tags/`) são puladas.
4. Cada linha restante precisa ter `lsha` igual ao `HEAD` do checkout. Senão o erro é `push-ref-not-checked-out`, com `lref` e `rref` no `context`. Uma `lref` `refs/heads/<b>` com `<b>` diferente da branch do checkout também recebe esse erro, mesmo com o sha igual ao `HEAD`.
5. Sem linha restante (inclusive stdin vazia), o resultado é `{"event":"before-pr","push":"nothing-to-attest","results":[]}`, sem rodar nada. Com linhas restantes, o `before-pr` roda uma vez sobre o `HEAD`.

Consequências:

- Você não empurra outra branch sem trocar de checkout (ou de worktree). Um `git push origin a b` com `b` fora do checkout é barrado inteiro.
- Um push da branch do checkout para a ref remota de outro work item (`feat/wi-w1__x:refs/heads/feat/wi-w9__z`) passa e atesta o work item do checkout. O work item de destino fica sem attestation, e o `gate-pr` dele falha.

## 4. Interrupção por sinal

Quando você aperta Ctrl+C, ou o `omhl` ou o `git` recebe `SIGTERM`, `SIGHUP` ou `SIGKILL`:

- O `omhl` termina. Nenhum processo do comando fica órfão e nenhum lock do `target/` fica preso.
- Um guardião manda `TERM` e `CONT` ao grupo do comando, espera 2 segundos e manda `KILL`. Um comando cooperativo usa esses 2 segundos para encerrar os próprios filhos.
- Nada é gravado para o check interrompido nem para os que ainda não rodaram. As attestations já gravadas ficam.
- Só pode sobrar um arquivo de saída sem uso na pasta `blobs/` e um temporário `.tmp-<pid>-<n>`. Os dois são inofensivos.
- Para o git, o hook falhou e o commit ou o push é abortado.

Condições e limites:

- O desenho pressupõe o `sh` do sistema com `trap`, `read` e `kill` embutidos.
- Um `omhl` iniciado com `SIGINT` ignorado (tarefa em segundo plano de um shell não interativo) não morre por Ctrl+C, como qualquer programa.
- **Ctrl+Z** para o `git` e o `omhl`. Ele não pausa o comando, que roda num grupo de fundo.
- **Resíduo.** Não são alcançados um descendente que mudou de grupo ou de sessão (`setsid`) e que ninguém limpa, e um processo mediado por daemon (`docker run`, `systemd-run`), que nem é descendente do comando. O `omhl` não tenta alcançá-los. Quando o descendente herdou o ambiente do comando, a varredura da seção 9 o encerra depois.
- Um `omhl` desanexado do próprio pai (`sh -c 'omhl ... & sleep 1; exit'`) morre com `SIGKILL`, sem mensagem: exit 137, stderr vazio, nada gravado. A forma suportada de desanexar é `nohup sh -c 'omhl check run ...' > arquivo 2>&1 &`.

## 5. Escopo dos hooks

O `omhl hooks install` grava quatro scripts `sh` curtos (modo `0755`) e define `core.hooksPath`:

| Hook | O que roda | Quando |
|---|---|---|
| `pre-commit` | `omhl check run --event before-commit` | `git commit` |
| `pre-merge-commit` | `omhl check run --event before-commit` | `git merge --no-ff` e `git pull` que cria merge |
| `pre-push` | `omhl check run --event before-pr --pre-push` | `git push` |
| `post-commit` | `omhl hooks post-commit` | Depois de um commit; apaga a marca da `develop` |

Os scripts ficam na pasta de usuário `$XDG_DATA_HOME/omhl/hooks/`, ou em `~/.local/share/omhl/hooks/` quando a variável não está definida (a mesma regra do banco). O `core.hooksPath` da config local do clone recebe esse caminho absoluto. Na saída, o `omhl` lista o caminho e os scripts:

```bash
omhl hooks install
```

Regras:

- **Uma vez por clone.** Um clone sem `omhl hooks install` não tem hook, e o `gate-pr` cobra depois.
- **Pasta única.** Instalar num clone reescreve os scripts de todos os outros, e o caminho do `omhl` gravado é o da última instalação.
- **`hooks-existing`.** Se `core.hooksPath` não está definido e a pasta de hooks do clone (`.git/hooks`) tem algum arquivo que não seja `*.sample`, a instalação recusa. Com `--force`, ela segue e você perde esses hooks (git-lfs e `commit-msg`, por exemplo). Encadeá-los não é suportado.
- **`hooks-path-conflict`.** Se `core.hooksPath` já aponta para outra pasta, local ou global, a instalação recusa, salvo `--force`. Apontando para a própria pasta de usuário, a reinstalação é idempotente.
- **`hooks-path-unreachable` e `hooks-dir-unwritable`.** O primeiro aparece quando o git não resolve a pasta de hooks para a pasta instalada. O segundo, quando a pasta de usuário não pode ser criada nem escrita.
- **`binary-in-build-dir`.** Se o `omhl` que roda a instalação está numa pasta `target/debug` ou `target/release`, a instalação grava mesmo assim e a saída ganha um aviso `binary-in-build-dir` com o caminho. Todos os clones compartilham esses scripts: instale um binário estável (o do plugin ou um `cargo install`) e rode `omhl hooks install` de novo.
- **`--no-verify`** pula `pre-commit`, `pre-merge-commit` e `pre-push`. O gate de pull request continua reprovando um commit sem attestation.
- **`wt merge`.** O `wt merge` faz o commit de squash na branch da frente e depois um `git push` para o destino. Com os hooks instalados, o `pre-push` roda na worktree da frente. Para um destino que não é a `main`, ele roda o `before-pr` inteiro, e a branch da frente precisa seguir `tipo/wi-<id>__<slug>` com work item aberto. Para a `main`, o `pre-push` barra o push.
- **`before-merge`** não tem hook. Rode `omhl check run --event before-merge` quando quiser registrar o status do CI.

Os hooks do git não têm relação com o hook `omhl hook session-start` do Claude Code.

## 6. Records antigos com evento livre

Um record de `check` gravado antes de o evento ser fechado, com um evento fora de `before-commit`, `before-pr` e `before-merge`, segue válido no banco. Nenhum hook e nenhum `--event` o roda, e o gate de pull request não o cobra. O `omhl load` recusa um arquivo novo com esse evento.

## 7. Comando que lê do terminal

O comando roda num grupo de fundo. Se ele lê do terminal (`/dev/tty`), o kernel o para com `SIGTTIN` e ele fica parado até `timeout_secs`. O `TERM` do prazo fica pendente num processo parado, e só o `KILL` da carência, 2 segundos depois, o encerra. Escrever no terminal não para o comando.

Um check não pede nada ao terminal. Use `ssh -o BatchMode=yes`, `sudo -n` e um `pinentry` fora do terminal.

## 8. Limites do `pre-push` e da `main`

- **Teto de 1 MiB na stdin.** O `omhl` lê a stdin do `pre-push` até 1 MiB. O `git push --mirror` manda uma linha por ref, e com cerca de 5.000 refs (a ~200 bytes por linha) passa do teto. O push inteiro é recusado com `push-input-invalid` e `context.kind` `input-too-large`, mesmo que as refs sejam só tags. A barreira falha fechada.
- **A barreira efetiva da `main` é o `pre-push`.** O `pre-commit` só pega `git commit`. Estes caminhos gravam na `main` local sem passar pelo `before-commit`: `git am`, `rebase --apply`, `cherry-pick`, `revert`, `rebase`, o merge fast-forward e o `wt merge --no-ff`. O push da `main` é barrado pelo `pre-push`.
- **A branch de um plano também segue o padrão.** Com os hooks instalados, uma branch como `feat/mechanical-gates-v0` é barrada como `branch-name-invalid`. Use `tipo/wi-<id>__<slug>`.
- **Comando parado que só trata `TERM`.** Se a interrupção deixa o grupo do comando órfão, o kernel manda `SIGHUP` antes do `TERM` do guardião. Um comando parado que só trata `TERM` e `INT` morre pelo `SIGHUP` sem limpar o subgrupo. Quem cria subgrupo trata também `HUP`.
- **Commits que não gastam a marca da `develop`.** `git merge --no-ff`, `git am` e `rebase --apply` não rodam o `post-commit`, e um commit em `HEAD` solto na ponta da `develop` não é reconhecido como da `develop` no `post-commit`. A marca sobrevive a todos eles.
- **Marca gasta sem `pre-commit`.** `cherry-pick`, `revert` e `rebase` na `develop` rodam o `post-commit` sem passar pelo `pre-commit` e gastam a marca.

## 9. Merges, varredura e timeouts

### O `pre-merge-commit`

O hook `pre-merge-commit` instalado roda `omhl check run --event before-commit`:

- Na `main`, barra o `git merge --no-ff` com `branch-protected`. O merge fica pendente, com `MERGE_HEAD` presente, até você rodar `git merge --abort`.
- Numa branch de work item, roda o `before-commit` sobre a árvore do merge, como num commit.
- Na `develop`, um merge limpo exige a marca e não a gasta. A marca sobrevive e autoriza o próximo commit direto. Um merge com conflito, fechado por `git commit` ou `git merge --continue`, passa pelo `pre-commit` e pelo `post-commit`: exige a marca e a gasta. Um `git pull` que cria merge exige a marca. Por isso, `git pull` sem fast-forward e `git merge main` na `develop` falham com `develop-mark-missing` quando não há marca. O fast-forward não roda hook.

Passam sem hook que barre: o merge fast-forward, `cherry-pick`, `revert`, `rebase`, `git am`, `rebase --apply`, o `wt merge --no-ff` e qualquer comando com `--no-verify`.

### A `main` sem proteção do GitHub

Enquanto o repositório for privado, o projeto não usa a proteção de branch do GitHub. A `main` fica protegida pelos hooks locais (`pre-push`, `pre-commit` e `pre-merge-commit`) e pelo `gate-pr`. Reveja essa escolha quando o repositório ficar público.

### A varredura de processos fugidos

O runner põe a variável `OMHL_RUN_ID` (formato `v1:<pid>:<início>:<pidns>:<raiz>`) no ambiente de cada comando. Ela identifica o `omhl` dono e o projeto: `<raiz>` são os 16 primeiros hex do sha256 do caminho da raiz do projeto. A varredura procura processos que carregam essa marca e cujo dono morreu, e os encerra com `TERM`, `CONT`, 2 segundos de carência e `KILL`. Ela poupa o próprio processo, quem tem a mesma marca que o varredor e o grupo que um guardião vivo ainda limpa.

Ela roda em silêncio no início de todo `omhl check run`, inclusive no `--pre-push`, e sob demanda. A varredura do `check run` só encerra processos cuja marca leva a raiz do próprio projeto: uma marca de outra raiz, mesmo com o dono morto, fica intacta. A sob demanda (`omhl check sweep`) vale para qualquer raiz:

```bash
omhl check sweep
```

A saída é uma linha JSON com `swept` (cada item tem `pid`, `comm` e `owner_pid`), `unreadable` e `scan_timed_out`, e o código de saída é 0, também sem alvo. Quando acha alvo, a varredura acrescenta uma linha a `$XDG_DATA_HOME/omhl/sweep.jsonl`. A leitura de `/proc` espera no máximo 1 segundo.

Limites:

- Só Linux, e só processos do mesmo usuário.
- Não acha o processo que sobe com o ambiente limpo (`env -i`, `sudo`, um daemon que reinicia o ambiente).
- Não acha o processo mediado por daemon (`docker run`, `systemd-run`), que não herda o ambiente.
- Um guardião vivo que não termina a limpeza mascara o grupo enquanto viver.
- Duas worktrees rodando `check run` ao mesmo tempo poupam uma à outra: o dono vivo tem o mesmo início.
- Um servidor que um check já terminado lançou fora do grupo (o `sccache` e o daemon do Gradle, por exemplo) carrega a marca de um dono morto. Os filhos dele, criados para outra execução viva, herdam essa marca. A varredura do próximo `check run` do mesmo projeto mata o servidor no meio do trabalho (um `check run` de outro projeto não o toca, mas o `omhl check sweep` sim). O check vivo falha e grava um `failed` imutável, e um cliente de fora do `omhl` que usava o servidor falha sem explicação. Este repositório não usa `sccache`. É um risco conhecido e aceito.

### Reajustar os `timeout_secs`

Os valores ficam fixos no TOML de cada check. O `omhl` não os calcula nem os reajusta. Os atuais, medidos em execução fria (`CARGO_TARGET_DIR` novo e vazio, quatro rodadas por check, máquina com outros processos ativos):

| Check | Pior rodada fria | `timeout_secs` |
|---|---|---|
| `checks.fmt` | 1,3 s | 10 (piso) |
| `checks.lint` | 114,3 s | 149 |
| `checks.compile-validate` | 68,2 s | 89 |
| `checks.compile-build` | 153,6 s | 200 |
| `checks.test-suite` | não remedido | 660 |

Sem carga, as mesmas rodadas ficam entre 17 s e 25 s (22 s no `lint`, 23 s no `build`). A pior rodada vem da disputa por disco e CPU, e é ela que dita o prazo. O `test-suite` ficou longo demais para remedir a frio; vale o valor já gravado.

Para reajustar:

1. Rode o comando do `run` pelo menos duas vezes, cada uma com o `target/` limpo (por exemplo, um `CARGO_TARGET_DIR` novo). Anote o tempo de parede da rodada mais lenta.
2. Multiplique por 1,3 e arredonde para cima.
3. Aplique o piso de 10 segundos.
4. Grave o número no `timeout_secs` do TOML do check e rode `omhl load`.

Se o timeout estourar muitas vezes, revise o valor. Um `timed_out` grava `failed`, imutável para aquela árvore naquele banco (linha 2 da tabela). Só uma árvore nova libera um novo veredito. A imutabilidade do `failed` por `timed_out` é mantida de propósito; uma issue futura pode permitir a substituição. Um `cargo test` travado segura o hook por cerca de 11 minutos antes de virar `timed_out`.

### O custo do `pre-push`

Cada `wt merge` para um destino que não é a `main` roda o `before-pr` inteiro. Medido neste repositório: cerca de 6 minutos a frio isolado (`cargo check`, `cargo build` e `cargo test` em sequência), 7 a 9 minutos sob carga, e menos de 1 minuto a quente.

Uma worktree nova nasce com o `target/` vazio, então o primeiro `before-commit` e o primeiro `before-pr` dela são frios. Antes do primeiro `git push` numa worktree nova, rode:

```bash
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked --no-run
```

Cada comando aquece um check com prazo: `compile-validate`, `lint`, `compile-build` e a compilação do `test-suite`. O `cargo test --no-run` sozinho não aquece os três primeiros.

Se um teste falhar por flaky e o `proptest` gravar uma semente nova em `crates/hexlog/tests/chain_properties.proptest-regressions` (arquivo rastreado), o `test-suite` termina em `check-not-run` com `command-changed-tree`. Restaure o arquivo e repita, como descreve o [README](../README.md).

## 10. Modelo de ameaça do veredito

O gate prende o veredito à árvore e à definição do check que o produziu. Cada attestation do `check run` leva o `spec_digest`, um sha256 de `run`, `timeout_secs` e `expect` do check vigente no banco. Outros campos (`blocking`, `how`, `concern`) não mudam o digest.

- **Definição velha.** A attestation foi gravada sob uma definição que o banco já trocou. O gate pergunta, para cada check com `run`, `spec-digest-match:<check>`: existe uma attestation `ok` neste snapshot com o `spec_digest` da definição vigente? Se não, o gate reprova.
- **`omhl attest` à mão.** Não grava `spec_digest`, e a guarda recusa um `spec_digest` de quem não é o `executor`. Uma attestation à mão num check sem `run` no banco não satisfaz o gate depois que um `omhl load` devolve o `run`.
- **TOML editado e não carregado (`spec-stale`).** O `check run` compara o digest do TOML da working tree com o do banco, sem gravar nada. Em qualquer evento, a divergência recusa o check (linha 12 da tabela), inclusive no `before-commit`: o comando do banco não roda e a saída é não zero.
- **Regra de trabalho.** Checks mecânicos devem estar sempre alinhados com o banco: evite alterar checks durante o desenvolvimento; se alterar, rode `omhl load` antes de qualquer `check run`; o `check run` recusa com `spec-stale` se TOML e banco divergem.
- **Limite aceito.** Quem edita o TOML, roda `omhl load`, roda o `check run` e restaura o arquivo com `git checkout` obtém uma attestation válida para a definição que ficou no banco. O `omhl` não prova quem chamou a CLI: é um freio, não uma parede (README, "Autoria"). O `spec_digest` fecha o caso sem má-fé e a attestation à mão, não o bypass deliberado pelo `omhl load`.
