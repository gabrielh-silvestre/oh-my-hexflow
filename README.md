# oh-my-hexflow

Plugin do Claude Code que guarda as decisões, os checks e os work items de um
projeto num histórico encadeado por hash. A cada sessão nova, ele injeta esse
contexto no Claude Code. O workspace tem dois crates:

- `hexlog`: o motor agnóstico. Guarda records por tenant num SQLite em modo
  WAL, valida cada escrita contra um catálogo de tipos e verifica a cadeia de
  hash.
- `omhl`: o plugin. Monta os grafos de decisões, checks e work items, avalia o
  gate de pull request e expõe tudo por CLI e pelo hook `SessionStart`.

O vocabulário (record, tenant, node, gate, extension, projection) está em
[GLOSSARY.md](GLOSSARY.md).

## Requisitos

- Rust 1.97.0, fixado em `rust-toolchain.toml`.
- `git` 2.31 ou mais novo no `PATH`, porque `attest`, `gate-pr` e `close` leem o commit atual. O `omhl` usa `git rev-parse --path-format=absolute`, que nasceu no 2.31; num git mais velho, todo `check run` falha com `probe-failed`.
- Claude Code, se você for usar o plugin.

O plugin foi testado só no Linux e no WSL. Não há binário por plataforma.

## Instalação

1. Compile o binário de release:

   ```bash
   cargo build --release
   ```

2. Instale esse binário no `PATH`, fora de qualquer worktree. É o único `omhl`:
   o plugin (hooks) e o hook `pre-switch` do `wt`
   (`.config/wt.toml`) chamam o `omhl` do `PATH`, sem cópia por worktree.

   ```bash
   cp target/release/omhl ~/.local/bin/omhl
   ```

   Depois de puxar mudanças, recompile e copie de novo.

3. Abra o Claude Code com o plugin:

   ```bash
   claude --plugin-dir plugin/
   ```

O plugin liga os hooks `SessionStart` e `PreToolUse` (`hook spec-lock` para
`Edit`, `Write` e `MultiEdit`, e `hook agent-role` para o `Bash`, em
`plugin/hooks/hooks.json`). O agente grava e consulta pela CLI, guiado por seis
skills em `plugin/skills/`:

| Skill | O que cobre |
|---|---|
| `directives-flow` | O fluxo por diretivas e premissas: `append`, `attest`, `spec init`, `spec validate`, `gate-worktree` e `guideline retire`. Traz também o ciclo com os agentes, os papéis e o template da tabela do PR. |
| `mechanical-checks` | Os checks mecânicos: `load`, `check run`, `check retire`, `check sweep`, `byproduct`, `hooks install` e `hooks post-commit`. |
| `work-item-closing` | O fechamento do work item: `gate-pr`, `close` e `report retro`. |
| `query-and-integrity` | A consulta e a integridade do banco: `get`, `list`, `verify`, `project`, `render`, `report answers` e `report recurrences`. |
| `plan-consensus` | O consenso do plano: chama o analyst, o planner, o architect e o critic em sequência, limita a 3 voltas, escala o impasse ao dono e retira os `contradicts` resolvidos antes do `spec validate`. |
| `review` | A revisão da entrega: chama o code-reviewer, o security-reviewer e o verifier, grava os achados, monta a tabela do PR e confere o `gate-pr`. |

As quatro primeiras skills têm um `SKILL.md` curto e, por subcomando, um
`references/<subcomando>.md` com cinco títulos fixos (`Quando usar`, `Entradas`,
`Saída`, `Erros`, `Exemplos`). `plan-consensus` e `review` não têm `references/`
próprio: usam as páginas de apoio da `directives-flow` (`cycle.md`, `roles.md` e
`tracking-table.md`), que não seguem os cinco títulos. Um teste de cobertura
exige que todo subcomando folha da CLI, exceto o grupo `hook`, seja citado por
uma skill e tenha a sua `references/`. Outro teste roda os exemplos (`omhl-example`) contra o binário
real e reprova a skill que descreve uma saída que o `omhl` não dá. Os detalhes
estão em [docs/architecture.md](docs/architecture.md).

O plugin traz também sete agentes em `plugin/agents/` (`analyst`, `planner`,
`architect`, `critic`, `code-reviewer`, `security-reviewer` e `verifier`), que a
sessão principal chama como `oh-my-hexflow:<papel>`. Cada um grava só o que a
matriz do seu papel permite, e o hook `agent-role` impõe essa matriz no `Bash`.
O ciclo de dez passos, com o diagrama, está em
[docs/directives-flow.md](docs/directives-flow.md), seção 12.

## Marcar um projeto

O `omhl` só age num diretório que tem um `.omhl.toml`, ou num diretório abaixo
dele. Crie o marcador na raiz do repositório:

```toml
version = 1
tenant = "meu-projeto"
```

O nome do tenant tem até 63 caracteres: letras minúsculas, dígitos e hífen,
começando por letra ou dígito. Todos os tenants do usuário dividem um único
banco em `$XDG_DATA_HOME/omhl/omhl.db`, ou em `~/.local/share/omhl/omhl.db`
quando a variável não está definida. A opção `--db` troca esse caminho, mas o
`omhl` recusa (`db-inside-project`) um banco dentro da raiz do projeto: a pasta
`blobs/` ao lado dele sujaria a árvore que o `check run` atesta.

Fora de um projeto marcado, o hook não imprime nada e não cria o banco.

## Primeiros passos

Os comandos abaixo rodam dentro de um projeto marcado e imprimem texto para o
agente (`--format agent`, o padrão). `--format human` mostra os mesmos fatos em
blocos, e `--format json` devolve a linha de JSON, que é a forma de ler a saída
por código. Os comandos lidos por máquina (`hook session-start`,
`hook spec-lock`, `hook agent-role`, `hooks post-commit` e `gate-worktree`)
seguem em JSON e recusam `--format agent|human` com erro de uso (saída 2). A
decisão está no
[ADR 0009](docs/adr/0009-formato-de-saida-agent-human-json-e-registro-de-renderizadores.md).

1. Grave uma guideline:

   ```bash
   echo '{"items":[{"body":{"kind":"node","id":"guidelines.tone","type":"guideline","data":{"kind":"architecture","text":"Write short commit subjects.","confirmed_by":{"by":"person:me","question":"Adopt short commit subjects?","answer":"yes"}}}}]}' \
     | omhl append
   ```

2. Leia o node de volta:

   ```bash
   omhl get guidelines.tone
   ```

3. Veja o contexto que a próxima sessão recebe:

   ```bash
   omhl render
   ```

4. Verifique a cadeia de hash do tenant:

   ```bash
   omhl verify
   ```

## Comandos

| Comando | O que faz |
|---|---|
| `list` | Lista os nodes vigentes, com filtros `--type`, `--id-prefix`, `--limit`, `--after-seq` e `--where <campo>=<valor>` (repetível, igualdade de campo). Recusa a opção que violaria uma regra (`--limit` fora de 1 a 200, `--type` ou prefixo inexistente, `--where` em campo que não é de texto) em vez de devolver `[]` |
| `get <id>` | Imprime um node, ou `null` quando ele não existe |
| `append` | Grava os records de um pedido JSON lido do stdin |
| `attest <check> <work-item> --outcome <outcome>` | Registra o resultado de um check no commit atual |
| `render` | Imprime o contexto da sessão |
| `gate-pr <work-item>` | Avalia o gate de pull request |
| `gate-worktree --branch <nome>` | Decide se a worktree da branch pode ser criada. Fica em silêncio fora de um projeto com marcador; recusa um `--project-dir` que não existe |
| `close <work-item>` | Fecha um work item e grava o resultado do gate. Devolve também o campo `retro` |
| `verify` | Verifica a cadeia de hash |
| `load [--dry-run]` | Carrega as extensions de `.omhl/` no tenant |
| `project` | Gera a projeção em Markdown em `.omhl/projection/` |
| `hook session-start` | Roda o hook `SessionStart` do Claude Code |
| `hook spec-lock` | Roda o hook `PreToolUse` do Claude Code: lê o evento no stdin e nega a edição de uma spec congelada |
| `hook agent-role` | Roda o hook `PreToolUse` do Claude Code para o `Bash` e o `Monitor`: aplica a matriz do papel ao agente portado |
| `spec init <work-item> <slug>` | Cria a pasta da spec de um work item aberto, com a primeira parte, no diretório principal |
| `spec validate <work-item>` | Valida a estrutura e o grafo da pasta de spec do work item |
| `report answers [--work-item <id>]` | Estado de cada gap, quem o respondeu e se uma diretiva sustenta a resposta. Recusa um work item que não existe |
| `report recurrences [--min <n>]` | Premissas repetidas em work items suficientes para sugerir a promoção a guideline |
| `report retro <work-item>` | A retro de um work item, aberto ou fechado: divergências, emendas e gaps sem resposta |
| `check run --event <evento>` ou `check run --check <id>` | Roda os checks mecânicos de um evento (ou um só) e grava as attestations do work item da branch atual. `--pre-push` é do hook `pre-push` |
| `check sweep` | Encerra os processos que fugiram de um `check run` anterior |
| `check retire <check-id>` | Aposenta o check órfão, cujo arquivo `.omhl/checks/<slug>.toml` não existe na árvore, e some o aviso `check-orphaned`. É definitivo e vale para todos os clones e branches: o id não volta. Se a branch ou a worktree é anterior ao check, faça rebase sobre a `main` (ou restaure o arquivo) em vez de aposentar |
| `guideline retire <id> [--by <id-novo>]` | Retira a diretiva órfã, cujo arquivo `.omhl/guidelines/<slug>.toml` não existe na árvore. Com `--by`, grava também que a diretiva nova a supera. É definitivo e vale para todos os clones e branches |
| `byproduct <check> <work-item> [--tail <n>]` | Imprime as últimas linhas da saída guardada de um check. Recusa `--tail` fora de 1 a 500 |
| `hooks install [--force]` | Instala os hooks do git deste clone |
| `hooks post-commit` | Apaga a marca da `develop` depois de um commit (o hook `post-commit` o chama) |

`attest` e `close` exigem a árvore git limpa. Um erro sai no stderr, com o
código de saída 1, em texto (`✘ código · mensagem`) nos comandos que falam
`agent` e `human`; com `--format json`, e nos comandos lidos por máquina, sai
como uma linha de JSON.

A interface do agente é a CLI.

## Extensions e projeção

Um projeto declara os próprios checks e guidelines em arquivos TOML
versionados, em `.omhl/checks/` e `.omhl/guidelines/`. O `omhl load` os grava
no tenant. O hook nunca grava: quando os arquivos divergem do banco, ele só
acrescenta uma linha `NOTE` ao contexto.

O `omhl project` gera um arquivo Markdown por work item em
`.omhl/projection/`, fora do git. O mesmo estado do banco sempre gera os
mesmos bytes.

O formato dos arquivos e as regras estão em
[docs/extensions-and-projection.md](docs/extensions-and-projection.md).

## Fluxo por diretivas e premissas

O humano define diretivas e premissas; o agente escreve a spec a partir delas.
O ciclo vai da premissa confirmada ao gate de criação da worktree, à spec
congelada, à emenda e à retro do `close`. Os comandos, o `workflow.toml`, os
códigos de erro e os limites estão em
[docs/directives-flow.md](docs/directives-flow.md).

## Checks mecânicos

Um check com o campo `run` é mecânico. O `omhl check run` executa o comando,
compara o resultado com as regras de `expect`, grava uma attestation com o
desfecho e guarda a saída para o `omhl byproduct`. A tabela de falhas, o escopo
dos hooks, a interrupção por sinal e o ajuste de timeouts estão em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

### Escrever um check

Este é o `.omhl/checks/fmt.toml` do próprio repositório. O `how` fica no
`fmt.md`, ao lado:

```toml
concern = "style"
variant = "format"
blocking = true
run = ["cargo", "fmt", "--all", "--", "--check"]
timeout_secs = 10
expect = [{ exit = 0, outcome = "passed" }]
when = { event = "before-commit" }
```

- `concern` (`style`, `test` ou `compile`) e `variant` (`format` e `lint`,
  `suite` e `coverage`, `validate` e `build`) são campos do TOML. O `variant`
  tem de pertencer ao `concern`.
- `when.event` é `before-commit`, `before-pr` ou `before-merge`. Outro valor
  falha no `omhl load`.
- Um comando por check. Não junte comandos num `run` (`sh -c "a && b"`, `;`,
  `2>&1`): crie um check por comando e ordene com `after`. Cada regra do
  `expect` olha um fluxo, `stdout` ou `stderr`, nunca os dois.
- Um `gh` ausente atrás de `["sh", "-c", "gh ..."]` não é erro de spawn: o `sh`
  existe e sai com 127. O `omhl` grava um `failed` com `exit_code` 127, que fica
  imutável para aquele snapshot.

### Instalar os hooks

```bash
omhl hooks install
```

O comando grava `pre-commit`, `pre-merge-commit`, `pre-push` e `post-commit` em
`$XDG_DATA_HOME/omhl/hooks/` e aponta o `core.hooksPath` do clone para essa
pasta. Rode-o uma vez em cada clone.

- `omhl hooks install` trata dos hooks do **git**. O `omhl hook session-start`
  é outra coisa: é o hook do Claude Code.
- Se o clone já tem hooks próprios em `.git/hooks`, o comando recusa com
  `hooks-existing`. Com `--force` ele segue, e você perde esses hooks.
- Os scripts gravam o caminho do binário que rodou a instalação, e o hook sai
  com 1 se esse binário sumir. Rode `omhl hooks install` com um binário fora de
  qualquer worktree e fora de `target/` (`cargo install` ou uma cópia em
  `~/.local/bin`); um binário dentro de uma worktree ligada some com ela e a
  saída avisa (`binary-in-worktree`); um binário na worktree principal do clone não
  avisa, porque o `wt remove` não remove a worktree principal. A pasta de hooks é uma só por usuário: reinstalar em
  outro clone troca o binário de todos.

### Branches

O `check run` descobre o work item pelo nome da branch, que segue o padrão
`tipo/wi-<id>__<slug>` (por exemplo `feat/wi-w1__x`). O work item
`work-items.<id>` tem de existir e estar aberto.

- A `main` nunca recebe alteração direta. O `pre-commit` barra o commit nela e
  o `pre-push` barra qualquer push para ela.
- Na `develop`, os checks rodam e não gravam nada. O commit exige a marca que o
  command `/oh-my-hexflow:omhl-develop-mark` cria.
- Essas regras valem também para a branch de um plano: com os hooks instalados,
  `feat/mechanical-gates-v0` é barrada como fora do padrão.

### O que faz um hook falhar

- **Escopo do `pre-push`.** Ele barra a `main`, pula a remoção de ref e a tag, e
  atesta só o `HEAD` do checkout. Não se empurra outra branch sem trocar de
  checkout.
- **Arquivo fora do stage.** Uma alteração de arquivo rastreado que não está
  no stage, ou um arquivo não rastreado e não ignorado, faz o `pre-commit`
  falhar. O `pre-push` falha com qualquer arquivo sujo ou não rastreado. Faça
  o stage, o commit, ponha no `.gitignore` ou apague.
- **Arquivo rastreado alterado pelo hook.** Se um check altera um arquivo
  rastreado, a execução se perde: o `omhl` não grava nada e sai com
  `check-not-run`. Restaure o arquivo com `git checkout -- <arquivo>` e repita,
  ou faça commit da mudança, se quiser mantê-la. O
  `chain_properties.txt`, em `crates/hexlog/tests/proptest-regressions/`, que o
  `proptest` reescreve quando um teste falha, só muda numa rodada manual de
  `crates/hexlog/tests`: o gate local não roda esse arquivo.

### Recuperar uma attestation

Um commit feito sem os hooks (`--no-verify`, ou um clone sem
`omhl hooks install`) não tem attestation, e o `gate-pr` reprova. Com a árvore
limpa, rode o evento à mão na branch do work item:

```bash
omhl check run --event before-commit
```

O comando atesta a árvore do `HEAD`.

### Autoria

O `omhl attest` à mão é recusado para um check com `run` (`check-run-author`).
Só o `omhl check run` grava a attestation desse check, e só o `omhl load` grava
o check. É um freio, não uma parede: um agente que escapa do hook
continua indistinguível da sessão principal. Só os agentes portados se
distinguem: o hook `agent-role` define `OMHL_ROLE=omhl:<papel>` a partir do
`agent_type` do evento, e o record sai com `agent` `omhl:<papel>` e com a matriz
do papel aplicada. O hook protege contra o erro do agente, não contra a má-fé: o
[ADR 0008](docs/adr/0008-papel-do-agente-imposto-por-hook-e-matriz.md) tem os
limites. Fora do Claude Code a CLI grava `user`, então o dono retira um
`contradicts` só de um terminal externo.

Cada attestation do `omhl check run` leva o `spec_digest` (sha256 de `run`,
`timeout_secs` e `expect`), e o `gate-pr` só aceita a que bate com a definição
vigente do check. Se o TOML do disco diverge do banco ou ficou inválido, o
`check run` recusa com `spec-stale` em qualquer evento (corrija o arquivo e
rode `omhl load`). Se o TOML foi apagado, o check é órfão: o `check run` não
roda o `run` gravado e avisa com `check-orphaned` a cada execução, sem
bloquear o commit. Detalhes em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

## Desenvolvimento

O `pre-commit` roda fmt e clippy; o `pre-push` roda a validação e a compilação
do workspace e os testes unitários (`cargo test --workspace --lib --locked`).
Os testes de `crates/*/tests` e os
doctests rodam no CI (`.github/workflows/ci.yml`), que bloqueia o merge com:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked --no-fail-fast
```

`cargo test` sem filtro roda a suíte inteira, que é pesada. Ao mexer em área
coberta por `tests/`, rode só o módulo afetado
(`cargo test -p <crate> --test integration <modulo>::`; no omhl, `--test e2e`
para o e2e; `--lib <modulo>::` para o unitário). Detalhes em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

O workspace proíbe `unsafe` e os tipos de memória compartilhada listados em
`clippy.toml`. Os testes de arquitetura, em
`crates/*/tests/integration/architecture.rs`, travam as regras de dependência
entre os módulos. O mapa dos módulos e dessas
regras está em [docs/architecture.md](docs/architecture.md).

## Créditos

Os sete agentes de `plugin/agents/` (analyst, planner, architect, critic, code-reviewer, security-reviewer e verifier) derivam do oh-my-claudecode 5.7.19, de Yeachan Heo, distribuído sob a licença MIT. O copyright, o texto da licença e a lista do que deriva dele estão em [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
