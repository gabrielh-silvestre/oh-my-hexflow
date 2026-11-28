# oh-my-hexflow

O vocabulário (record, tenant, node, gate, extension, projection) está em
[GLOSSARY.md](GLOSSARY.md). Quem desenvolve o plugin encontra o que precisa em
[CONTRIBUTING.md](CONTRIBUTING.md).

## Requisitos

### Obrigatório

- `git` 2.31 ou mais novo no `PATH`, porque `attest`, `gate-delivery` e `close` leem o commit atual. O `omhl` usa `git rev-parse --path-format=absolute`, que nasceu no 2.31.
- Um `sh` POSIX, que roda o script dos hooks do plugin.
- `HOME` ou `XDG_DATA_HOME` definido, para o `omhl` achar o banco.
- Linux, para os checks mecânicos.
- Claude Code, para o plugin.

### Só para compilar do fonte

- Um compilador C.
- A toolchain Rust fixada no `rust-toolchain.toml`.

### Opcional

- Worktrunk e GitHub: o `omhl` não depende de nenhum dos dois para rodar.

O plugin foi testado só no Linux e no WSL.

## Instalação

1. Instale o binário pronto (Linux x86_64) em `~/.local/bin`:

   ```bash
   [ "$(uname -sm)" = "Linux x86_64" ] && mkdir -p ~/.local/bin && (cd "$(mktemp -d)" && curl -fsSLO https://github.com/gabrielh-silvestre/oh-my-hexflow/releases/latest/download/omhl-x86_64-unknown-linux-musl.tar.gz && curl -fsSLO https://github.com/gabrielh-silvestre/oh-my-hexflow/releases/latest/download/omhl-x86_64-unknown-linux-musl.tar.gz.sha256 && sha256sum -c omhl-x86_64-unknown-linux-musl.tar.gz.sha256 && tar -xzf omhl-x86_64-unknown-linux-musl.tar.gz -C ~/.local/bin omhl)
   ```

   Em outra plataforma, compile do fonte e copie o binário, fora de qualquer worktree:

   ```bash
   cargo build --release
   cp target/release/omhl ~/.local/bin/omhl
   ```

   Para atualizar, rode o mesmo comando de novo.
   O `~/.local/bin` precisa estar no `PATH` do processo do Claude Code; reinicie a sessão depois de instalar.

2. Instale o plugin pelo marketplace do GitHub, numa sessão do Claude Code:

   ```text
   /plugin marketplace add gabrielh-silvestre/oh-my-hexflow
   /plugin install oh-my-hexflow@oh-my-hexflow
   ```

   O marketplace instala só o plugin, não o `omhl`: o plugin lê o `omhl` do `PATH` (passo 1).

### Avisos de início de sessão

O plugin avisa no início da sessão, com o comando para corrigir, nos casos abaixo:

- O `omhl` não está no `PATH`: os hooks ficam inativos. Num projeto marcado com `.omhl.toml`, a edição de arquivos é bloqueada até você instalar o binário.
- O `omhl` é mais velho que a versão em `plugin/hooks/min-omhl-version`: atualize com o mesmo comando.
- Num projeto marcado com `.omhl.toml`, o `git` não está instalado: instale o `git` 2.31 ou mais novo.
- Num projeto marcado com `.omhl.toml`, o `git` é anterior ao 2.31: atualize o `git`.

Skills de orquestração dos agentes, em `plugin/skills/`:

| Skill | O que cobre |
|---|---|
| `interview` | A entrevista de negócio com o dono, antes do work item: placar de ambiguidade, rodadas de perguntas e a gravação do work item e das premissas confirmadas. |
| `plan-consensus` | O consenso do plano: chama o analyst, o planner, o architect e o critic em sequência, limita as voltas, escala o impasse ao dono e retira os `contradicts` resolvidos antes do `spec validate`. |
| `review` | A revisão da entrega: chama o code-reviewer, o security-reviewer e o verifier, grava os achados, monta a tabela do PR e confere o `gate-delivery`. Traz também a referência dos checks mecânicos e do fechamento do work item. |

As skills `role-*` não são chamadas pela sessão: são as vistas por papel, pré-carregadas pelos agentes do plugin.

O ciclo passo a passo, com o diagrama, está em
[docs/directives-flow.md](docs/directives-flow.md), seção 12.

## Marcar um projeto

Crie o marcador `.omhl.toml` na raiz do repositório.

A opção `--db` troca o caminho do banco, mas o
`omhl` recusa (`db-inside-project`) um banco dentro da raiz do projeto: a pasta
`blobs/` ao lado dele sujaria a árvore que o `check run` atesta.

## Primeiros passos

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
| `check retire <check-id>` | É definitivo e vale para todos os clones e branches: o id não volta. Se a branch ou a worktree é anterior ao check, faça rebase sobre a `main` (ou restaure o arquivo) em vez de aposentar |
| `guideline retire <id> [--by <id-novo>]` | Com `--by`, grava também que a diretiva nova a supera. É definitivo e vale para todos os clones e branches |

A interface do agente é a CLI.

## Extensions e projeção

O formato dos arquivos e as regras estão em
[docs/extensions-and-projection.md](docs/extensions-and-projection.md).
Arquivos prontos para copiar para o `.omhl/` do projeto estão em
[plugin/examples/README.md](plugin/examples/README.md); o que o plugin impõe, o
que é opcional e o que é só deste repositório está em
[docs/plugin-and-project-boundary.md](docs/plugin-and-project-boundary.md).

## Fluxo por diretivas e premissas

O humano define diretivas e premissas; o agente escreve a spec a partir delas.
O ciclo vai da premissa confirmada ao gate de criação da worktree, à spec
congelada, à emenda e à retro do `close`. Os comandos, o `workflow.toml`, os
códigos de erro e os limites estão em
[docs/directives-flow.md](docs/directives-flow.md).

## Checks mecânicos

Um check com o campo `run` é mecânico. A tabela de falhas, o escopo
dos hooks, a interrupção por sinal e o ajuste de timeouts estão em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

### Escrever um check

- Um comando por check. Não junte comandos num `run` (`sh -c "a && b"`, `;`,
  `2>&1`): crie um check por comando e ordene com `after`.

### Instalar os hooks

```bash
omhl hooks install
```

Rode-o uma vez em cada clone.

- Rode `omhl hooks install` com um binário fora de
  qualquer worktree e fora de `target/` (`cargo install` ou uma cópia em
  `~/.local/bin`).

### Branches

- A `main` nunca recebe alteração direta.

### O que faz um hook falhar

- **Escopo do `pre-push`.** Não se empurra outra branch sem trocar de
  checkout.
- **Arquivo fora do stage.** Faça
  o stage, o commit, ponha no `.gitignore` ou apague.
- **Arquivo rastreado alterado pelo hook.** Restaure o arquivo com `git checkout -- <arquivo>` e repita,
  ou faça commit da mudança, se quiser mantê-la.

### Recuperar uma attestation

Com a árvore
limpa, rode o evento à mão na branch do work item:

```bash
omhl check run --event before-commit
```

### Autoria

O hook é um freio, não uma parede: um agente que escapa dele
continua indistinguível da sessão principal. O hook protege contra o erro do agente, não contra a má-fé: o
[ADR 0008](docs/adr/0008-papel-do-agente-imposto-por-hook-e-matriz.md) tem os
limites.

Se o TOML do disco diverge do banco ou ficou inválido, corrija o arquivo e
rode `omhl load`. Detalhes em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

## Avisos de atualização

Uma entrada por mudança que exige ação de quem atualiza, a mais nova primeiro.

### A `develop` sai do binário: cada projeto declara as suas branches

O binário não trata mais a `develop` como branch embutida. Cada projeto declara a integração e as exceções em `[branch]` do `.omhl/workflow.toml` do diretório principal. O que muda para quem atualiza:

1. Declare `[branch]` para manter o comportamento de hoje: sem a declaração, um commit na `develop` é recusado com `branch-name-invalid`. No exemplo, `poc/*` não roda checks e a `develop` pede a marca:

   ```toml
   version = 1

   [branch]
   integration = "develop"

   [[branch.exception]]
   pattern = "poc/*"
   mode = "skip"

   [[branch.exception]]
   pattern = "develop"
   mode = "dry-run-with-mark"
   ```

2. A marca e os códigos têm nomes novos, sem alias: a primeira linha da marca é `omhl-commit-mark v1`, no arquivo `omhl-commit-mark` da pasta git da worktree, e os códigos são `commit-mark-missing` e `commit-mark-unavailable`. A marca antiga `omhl-develop-mark` é ignorada.
3. Até a release do plugin na `main`, o command instalado ainda grava o nome antigo: grave a marca nova à mão, com a linha de `plugin/commands/omhl-commit-mark.md`, no caminho que a dica do `commit-mark-missing` mostra.
4. O formato do nome da branch pode ser declarado em `branch.format`; trocar um formato já declarado é só do dono.
5. A base da revisão do `gate-delivery` e do `close` vem de `branch.integration`.
6. Um binário anterior recusa o `workflow.toml` com `[branch]`: instale o binário novo antes de o `[branch]` chegar ao diretório principal.

### Os checks viram a interface do projeto

O evento, o comando e o gate agora se chamam `before-push`, `gate-delivery` e `delivery`. O que muda para quem atualiza:

1. Rode `omhl hooks install` uma vez por usuário: o `pre-push` instalado precisa ser reescrito. Os scripts dos hooks ficam numa pasta por usuário que todo clone usa, então uma rodada conserta todos.
2. Rode `omhl load` depois de atualizar, para os checks guardados tomarem os nomes novos de evento.
3. As 2 guidelines `one-decision-per-turn` e `commit-policy` saem do plugin. Quem as cita as declara em `.omhl/guidelines/`.
4. As 5 variáveis de toolchain do Rust (`RUSTFLAGS`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`) ficam no ambiente do check. Um check `cargo` que dependia de elas serem removidas põe `env -u` no seu `run`.
5. O limiar e os pesos da entrevista vivem em `[interview]`.
6. O comando do nativo `checks.run-tests` vive em `[native.run]`. Depois do merge de uma mudança desse comando, ou de uma ativação com comando, as outras worktrees abertas falham no `spec-digest-match` do nativo até o próximo commit ali rodar `check run`.
7. O binário anterior recusa `[interview]`, `[native.run]` e `before-push`.
8. Plugin e binário se atualizam juntos: o `plugin/hooks/min-omhl-version` agora é `0.3.0`, igual à versão do workspace.

### O `close` recusa mais e o `require` acrescenta

O que cada comando recusa e com qual código está na página do comando, em `plugin/skills/directives-flow/references/`. O que muda para quem atualiza:

1. `close` agora recusa onde fechava: um work item que falha na checagem do gate não fecha mais, e o mesmo vale nos caminhos `--force --note` e `--failed-delivery`.
2. `require` agora acrescenta, em `[prefix.<tipo>]`, aos padrões do binário em vez de substituí-los. Quem repetia os padrões ali pode tirá-los.
3. O check `checks.config-loosening-review` pede, antes do PR, a revisão do diff de `.omhl/workflow.toml` e de `.omhl/checks/`. Ele continua desligado até o id entrar em `[native] checks`.
4. As opções novas são `--base`, para escolher a base do diff, e `--owner-override`, para o dono liberar uma recusa.
5. Um `workflow.toml` inválido na `main` continua recusando todo PR que não seja o PR de reparo contra a `main`; o `--abandon` passa, porque não avalia o gate.
6. Plugin e binário se atualizam juntos.

### Os checks nativos vêm desligados

O que cada check nativo pede e como ligá-lo está em [native-checks.md](plugin/skills/directives-flow/references/native-checks.md). O que muda para quem atualiza:

1. Um projeto sem `[native]` no `.omhl/workflow.toml` passa a ter os checks nativos desligados: o `render` não os imprime e o `gate-delivery` e o `close` não perguntam por eles.
2. Quem quiser manter algum declara o id em `[native] checks`.
3. `checks.run-tests` e `checks.ci-green-before-merge` declarados são cobrados mesmo com um check próprio no mesmo momento: o check do projeto deixa de substituir o nativo.
4. Um projeto com `.omhl.toml` e sem repositório git mostra a linha `main-dir-unknown` a cada início de sessão, e `gate-delivery` e `close` recusam ali com `probe-failed`. O `git init` tira a linha da sessão, mas `gate-delivery` e `close` precisam também de um primeiro commit com a árvore limpa, porque a sonda recusa um repositório sem commits.
5. O binário anterior recusa um `workflow.toml` com `[native]`. Com o binário novo, um arquivo que ele não aceita faz `gate-delivery` e `close` recusarem com `invalid-workflow-config`, salvo o `--abandon` e o PR de reparo.
6. Plugin e binário se atualizam juntos.

### A pasta `.omhl/notes/` nasce com um `.gitignore` próprio

O hook cria a pasta `.omhl/notes/` na primeira nota escrita por uma ferramenta de edição de arquivo do Claude Code (uma nota escrita pelo `Bash` passa sem o hook), agora com um `.gitignore` próprio que ignora tudo (`*`). O `omhl` nunca recria o arquivo se a pasta já existe. Para versionar as notas, edite ou apague esse `.gitignore`.

Se a `.omhl/notes/` do seu projeto nasceu sem o arquivo, porque é anterior a esta mudança, porque um binário velho criou a pasta ou porque a criação falhou no meio, crie o `.omhl/notes/.gitignore` à mão com a linha `*`, se quiser. Não há migração, e o `omhl` não o recria.

## Créditos

Os agentes de `plugin/agents/` derivam do oh-my-claudecode 5.7.19, de Yeachan Heo, distribuído sob a licença MIT. O copyright, o texto da licença e a lista do que deriva dele estão em [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
