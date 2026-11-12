# oh-my-hexflow

Plugin do Claude Code que guarda as decisões, os checks e os work items de um
projeto num histórico encadeado por hash. A cada sessão nova, ele injeta esse
contexto no Claude Code. O workspace tem dois crates:

- `hexlog`: o motor agnóstico. Guarda records por tenant num SQLite em modo
  WAL, valida cada escrita contra um catálogo de tipos e verifica a cadeia de
  hash.
- `omhl`: o plugin. Monta os grafos de decisões, checks e work items, avalia o
  gate de pull request e expõe tudo por CLI, por servidor MCP e pelo hook
  `SessionStart`.

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

2. Copie o binário para a pasta do plugin, onde o git o ignora:

   ```bash
   cp target/release/omhl plugin/bin/omhl
   ```

3. Abra o Claude Code com o plugin:

   ```bash
   claude --plugin-dir plugin/
   ```

O plugin liga o hook `SessionStart` (`plugin/hooks/hooks.json`) e o servidor
MCP `omhl` (`plugin/.mcp.json`).

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

Os comandos abaixo rodam dentro de um projeto marcado e imprimem uma linha de
JSON.

1. Grave uma guideline:

   ```bash
   echo '{"items":[{"body":{"kind":"node","id":"guidelines.tone","type":"guideline","data":{"text":"Write short commit subjects."}}}]}' \
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
| `list` | Lista os nodes vigentes, com filtros `--type`, `--id-prefix`, `--limit` e `--after-seq` |
| `get <id>` | Imprime um node, ou `null` quando ele não existe |
| `append` | Grava os records de um pedido JSON lido do stdin |
| `attest <check> <work-item> --outcome <outcome>` | Registra o resultado de um check no commit atual |
| `render` | Imprime o contexto da sessão |
| `gate-pr <work-item>` | Avalia o gate de pull request |
| `close <work-item>` | Fecha um work item e grava o resultado do gate |
| `verify` | Verifica a cadeia de hash |
| `load [--dry-run]` | Carrega as extensions de `.omhl/` no tenant |
| `project` | Gera a projeção em Markdown em `.omhl/projection/` |
| `mcp` | Serve as ferramentas MCP por stdio |
| `hook session-start` | Roda o hook `SessionStart` do Claude Code |
| `check run --event <evento>` ou `check run --check <id>` | Roda os checks mecânicos de um evento (ou um só) e grava as attestations do work item da branch atual. `--pre-push` é do hook `pre-push` |
| `check sweep` | Encerra os processos que fugiram de um `check run` anterior |
| `byproduct <check> <work-item> [--tail <n>]` | Imprime as últimas linhas da saída guardada de um check |
| `hooks install [--force]` | Instala os hooks do git deste clone |
| `hooks post-commit` | Apaga a marca da `develop` depois de um commit (o hook `post-commit` o chama) |

`attest` e `close` exigem a árvore git limpa. Um erro sai no stderr como JSON,
com o código de saída 1.

O servidor MCP expõe dez ferramentas com a mesma saída da CLI: `list`, `get`,
`append`, `attest`, `render`, `gate_pr`, `close`, `verify`, `byproduct` e
`project`. O `load`, o `check run`, o `check sweep` e os comandos `hooks`
existem só na CLI.

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
  `~/.local/bin`); o `plugin/bin/omhl` some com a worktree e a saída avisa
  (`binary-in-worktree`). A pasta de hooks é uma só por usuário: reinstalar em
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
  `check-not-run`. Um exemplo é o `chain_properties.proptest-regressions`, que o
  `proptest` reescreve quando um teste falha. Restaure e repita:

  ```bash
  git checkout -- crates/hexlog/tests/chain_properties.proptest-regressions
  ```

  Ou faça commit da semente nova, se quiser mantê-la.

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
o check. É um freio, não uma parede: o agente que chama a CLI pelo Bash
continua indistinguível do usuário.

Cada attestation do `omhl check run` leva o `spec_digest` (sha256 de `run`,
`timeout_secs` e `expect`), e o `gate-pr` só aceita a que bate com a definição
vigente do check. Se o TOML do disco diverge do banco, foi apagado ou ficou inválido, o
`check run` recusa com `spec-stale` em qualquer evento (corrija o arquivo e
rode `omhl load`). Detalhes em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

## Desenvolvimento

Rode as três checagens antes de abrir um pull request:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

O workspace proíbe `unsafe` e os tipos de memória compartilhada listados em
`clippy.toml`. Os testes de arquitetura em `crates/*/tests/architecture.rs`
travam as regras de dependência entre os módulos. O mapa dos módulos e dessas
regras está em [docs/architecture.md](docs/architecture.md).
