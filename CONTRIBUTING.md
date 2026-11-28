# Contribuindo

Este arquivo é a casa de quem desenvolve o plugin e do que vale só para este repositório. Quem só usa o plugin encontra o que precisa no [README](README.md).

## Por onde começar

- [docs/plugin-and-project-boundary.md](docs/plugin-and-project-boundary.md): o que o plugin impõe, o que é opcional e o que é só deste repositório, e onde cada tipo de texto mora.
- [plugin/examples/README.md](plugin/examples/README.md): arquivos de check e de diretiva prontos para copiar.
- [docs/architecture.md](docs/architecture.md): o mapa dos módulos.
- [AGENTS.md](AGENTS.md): as instruções do agente que trabalha neste repositório.

## Convenções

As convenções do repositório moram em `.omhl/guidelines/`, como diretivas carregadas no grafo, inclusive a do código em inglês e sem emoji. Leia-as antes de mexer no código.

## Desenvolvimento

`cargo test` sem filtro roda a suíte inteira, que é pesada. Ao mexer em área coberta por `tests/`, rode só o módulo afetado (`cargo test -p <crate> --test integration <modulo>::`; no omhl, `--test e2e` para o e2e; `--lib <modulo>::` para o unitário). O critério que decide o grupo de cada teste está em [Testes unitários no gate, integração no CI](#testes-unitários-no-gate-integração-no-ci).

Para desenvolver o plugin, abra o Claude Code com `claude --plugin-dir plugin/`.

Depois de puxar mudanças, recompile e copie o binário de novo, como no passo de compilar do fonte da [instalação](README.md#instalação).

## Grafos do código

Este repositório usa o [GitNexus](https://github.com/abhigyanpatwari/GitNexus) e o Graphify para quem desenvolve nele. É configuração só do repositório: o plugin não depende dela. Os grafos não são versionados.

| Grafo | O que indexa | Quando usar |
| --- | --- | --- |
| GitNexus `oh-my-hexflow` | produção, testes e documentação | achar os testes que cobrem um símbolo, antes de rodar só o módulo afetado |
| GitNexus `oh-my-hexflow-prod` | o mesmo, sem os testes | impacto em produção: no grafo completo, o filtro de testes do GitNexus deixa passar testes unitários |
| Graphify (`graphify-out/`) | código e documentos versionados, sem os testes | ter uma visão geral, descobrir onde algo está e navegar entre documentos |

Ao consultar:

- Quem chama o quê, e o impacto de uma mudança, são perguntas para o GitNexus. Ele também erra: já ligou chamada da biblioteca padrão ou de dependência a uma função do projeto com o mesmo nome. Confira no código o que for decisivo.
- Não filtre o GitNexus pela confiança da ligação: na conferência feita aqui, quase todas as ligações por coincidência de nome, de confiança baixa, estavam certas.
- No Graphify, a falta de uma ligação não prova que a chamada não existe. Ele perde chamadas entre arquivos e liga por engano nomes iguais, inclusive entre crates que não dependem um do outro.
- Não conte com o Graphify para saber qual documento explica um trecho de código: ele liga pouco os documentos ao código, e muitos documentos viram um nó só.
- Quando o código muda, o `graphify update` pode trocar o nome de uma comunidade pelo de um símbolo ou arquivo. Para nomear de novo com o Claude, rode no checkout principal `flock .ignore/graph-sync/lock graphify label . --backend=claude-cli --model=sonnet`.

Instale com `npm install -g gitnexus` e `uv tool install graphifyy`. O script também usa o `jq` para ler JSON, o `rsync` para a cópia e o `flock`, do util-linux, para a trava. Na primeira vez, e quando quiser atualizar tudo, inclusive os documentos, rode no checkout principal:

```bash
bash scripts/graph-sync.sh sync --docs
```

A extração dos documentos usa o Claude pela CLI `claude` e gasta do seu limite de uso. Sem `--docs`, o comando atualiza o GitNexus e o código no Graphify sem modelo, e avisa quando há documento pendente.

O hook do [`.claude/settings.json`](.claude/settings.json) roda a sincronização em segundo plano depois que o agente executa um `omhl close` ou um `git pull` que mude a `main` ou a `develop`. Com uma tag `v*` ainda não processada no clone, a mesma rodada extrai os documentos. O resultado chega ao agente no turno seguinte.

- Um `git pull` no seu terminal não dispara o hook, nem um comando que o agente rode fora da ferramenta Bash. Rode o script à mão.
- A leitura é sempre do checkout principal como está, inclusive o que não tem commit, mesmo que o comando saia de uma worktree. Edições feitas depois da última sincronização não aparecem.
- O grafo de produção vem de uma cópia sem testes em `.ignore/graph-prod/`, que o script mantém. Não edite a cópia.
- O GitNexus não indexa pastas e arquivos que começam com ponto, como o `.omhl/`.
- O GitNexus também não lê `.gitignore` de subpasta. Pasta comum ignorada assim precisa entrar no [`.gitnexusignore`](.gitnexusignore); o script avisa quando um arquivo ignorado é indexado.
- O recorte de testes está no [`.graphifyignore`](.graphifyignore) e no `TEST_PATH` do [script](scripts/graph-sync.sh); mude os dois juntos.
- Não rode `graphify hook install`: neste clone, a pasta de hooks do git é do `omhl`.

## ADRs

O projeto não escreve mais ADRs. Os arquivos de `docs/adr/` são histórico congelado: nenhum é editado, renomeado ou movido. A decisão nova mora no grafo, como uma `decision` ancorada por `rests-on`. Onde um ADR deixou de valer, o README e o `docs/` dizem o comportamento atual, e o ADR fica como está.

## Fluxo de PR filho e agregadora

Abra um PR filho quando o work item se divide em entregas revisadas em separado. O PR filho para depois de colar a tabela no corpo. Só o PR agregador recebe o `gate-delivery`, o merge e o `close`.

## Gate local, CI e merge

### A `main` com proteção do GitHub

As camadas que bloqueiam o merge estão na lista "O merge é bloqueado em camadas" de [Testes unitários no gate, integração no CI](#testes-unitários-no-gate-integração-no-ci).

### O custo do `pre-push`

Uma worktree nova nasce com o `target/` vazio, então o primeiro `before-commit` e o primeiro `before-push` dela são frios. Antes do primeiro `git push` numa worktree nova, rode:

```bash
cargo check --workspace --all-targets --profile checks --locked
cargo clippy --workspace --all-targets --profile checks --locked -- -D warnings
cargo build --workspace --profile checks --locked
cargo test --workspace --lib --profile checks --locked --no-run
```

O `cargo test --no-run` sozinho não aquece os outros comandos.

### Testes unitários no gate, integração no CI

O `checks.test-suite` (`.omhl/checks/test-suite.toml`) limita os testes simultâneos: sem o limite, o gate morno usava mais núcleos do que o pre-push comporta.

Três perguntas e um critério de módulo, de cima para baixo, decidem o grupo de um teste; a primeira resposta "sim" vale:

1. Executa o binário `omhl` compilado (`CARGO_BIN_EXE_omhl`) ou um hook git real? É e2e (`crates/omhl/tests/e2e/`).
2. Chama git, `sh` ou outro processo, ou espera o relógio (`sleep`, timeout)? É integração.
3. Passa pela fachada (`Hexlog::open`, `Omhl`, `App`)? É integração.

Se as três respostas são não e o teste não atravessa dois módulos, ele é unitário: fica num `tests.rs` ao lado da fonte (`src/<modulo>/tests.rs`) e roda no `--lib`. Um teste atravessa dois módulos quando o que o seu assert afirma depende da lógica de outro módulo: ele chama uma função de outro módulo e afirma sobre o resultado, ou afirma o comportamento combinado dos dois. Esse teste também não é unitário: vai para a integração. Módulo, aqui, é o módulo de primeiro nível de `src/` (arquivo ou pasta) e, dentro de `adapters/`, `app/` e `workflow/`, o de segundo nível (por exemplo `adapters::text` ou `workflow::gap`). O módulo do teste é o do arquivo de onde ele vem (`src/<m>/tests.rs` ou `tests/<grupo>/<m>.rs`). Não atravessa: chamar o próprio módulo (API pública ou submódulo dele), usar outro módulo só para montar a entrada ou o valor esperado (tipo, contrato, constante, tabela, catálogo embutido, hash), nem comparar entre si tabelas ou catálogos embutidos de outros módulos.

Os meta-testes são a exceção. Meta-teste lê a árvore do repositório, os docs, `plugin/*.json` ou o `Cargo.lock`, ou testa um helper de teste (o oráculo que outros testes usam), e não tem módulo de fonte. Vai para o binário onde moram os helpers do seu arquivo; com o helper em `tests/support/`, ou sem helper comum, vai para a integração.

O `support/` tem lugar diferente em cada crate. No `hexlog` fica dentro de `integration/`, porque a regra de API pública (`it_should_keep_integration_tests_on_the_public_api`) recusa `#[path]` em `tests/`. Se o `hexlog` ganhar um e2e, o `support/` muda de lugar ou a regra relaxa.

O merge é bloqueado em camadas (decisão `perf2-bloqueio-camada-dupla`):

1. O ruleset `main` do GitHub exige o check `ci` do app do GitHub Actions (`integration_id` 15368) e proíbe deletar a `main` ou reescrevê-la com force-push. O papel admin só contorna as regras no merge de um PR, marcando o bypass de forma explícita (`bypass_mode: pull_request`); um push direto na `main` fica bloqueado para todos.
2. A attestation manual de `checks.ci-green-before-merge` cita a execução do CI do mesmo commit. Selecione a execução com `gh run list --workflow ci.yml --branch <branch> --event pull_request --limit 1 --json databaseId,headSha,conclusion,url`. Exija `conclusion` igual a `success` e `headSha` igual a `git rev-parse HEAD`, e grave os dois na nota: `omhl attest checks.ci-green-before-merge work-items.<id> --outcome passed --note "<url> <headSha>"`. Depois de um rebase (por exemplo, no merge de pilha do `gh stack`), o SHA muda: espere o CI novo e ateste de novo.

Se o `proptest` gravar uma semente nova em `crates/hexlog/tests/proptest-regressions/chain_properties.txt` (arquivo rastreado) numa rodada local, restaure o arquivo antes do commit.

### Reajustar o `timeout_secs` do `test-suite`

O `test-suite` é exceção à regra fria (decisão `perf2-timeout-secs-incremental`): com o build frio proibido na máquina do dono, o prazo é `max(120, 2 x parede)` de uma rodada logo depois de `touch crates/hexlog/src/lib.rs`, que recompila `hexlog` e `omhl`, o pior caso incremental.

### O programa gravado na attestation

O `docs/mechanical-gates.md` descreve o [programa gravado na attestation](docs/mechanical-gates.md#o-programa-gravado-na-attestation). Aqui o nome engana: `cargo` é um link para `rustup`, e o `rustup` decide o que roda pelo nome com que foi chamado. Gravar o destino do link e o sha256 juntos, como campo novo da attestation, é um follow-up.

## Worktrunk neste repositório

O plugin não usa o Worktrunk: ele ensina `git worktree add -b` e `git switch -c`. Este repositório mantém uma configuração própria do Worktrunk em [`.config/wt.toml`](.config/wt.toml), que é a fonte dos comandos.

- O `pre-switch` roda o portão antes de a branch e a worktree existirem num `wt switch -c`; uma saída não zero aborta. O `post-start` chama `omhl hook branch-created --branch {{ branch }}` em segundo plano, quando a worktree nasce. O `{{ branch }}` vai sem aspas (o `wt` já o escapa) e o `case` libera `develop` e `main`.
- O `wt` guarda a aprovação de um comando de projeto pelo texto exato do comando. Depois de mudar qualquer comando do `.config/wt.toml`, o dono roda `wt config approvals add` e confere com `wt config approvals list`; sem a aprovação e sem terminal, todo `wt switch` aborta com `Cannot prompt for approval`. O agente nunca usa `-y` nem `--no-hooks` para contornar isso: ele para e pede ao dono.
- `wt switch -c` parte da branch padrão; para partir da `develop`, passe `--base develop`.

## Binário novo depois do merge

Depois do merge e antes de qualquer gravação no banco real, instale o binário novo em `~/.local/bin/omhl` (`cargo install --locked --path crates/omhl --root ~/.local`), o binário que os hooks de git executam; se os hooks apontam para outro caminho, rode `omhl hooks install` com o binário novo. Nunca rode o `target/debug` ou o `target/release` de uma worktree contra o banco real. O limite que explica a regra, um binário anterior sobre um banco com sessão, está em [docs/architecture.md](docs/architecture.md).
