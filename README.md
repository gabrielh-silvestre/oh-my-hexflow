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
- `git` no `PATH`, porque `attest`, `gate-pr` e `close` leem o commit atual.
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
quando a variável não está definida. A opção `--db` troca esse caminho.

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

`attest` e `close` exigem a árvore git limpa. Um erro sai no stderr como JSON,
com o código de saída 1.

O servidor MCP expõe dez ferramentas com a mesma saída da CLI: `list`, `get`,
`append`, `attest`, `render`, `gate_pr`, `close`, `verify`, `load` e `project`.

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
