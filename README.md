# oh-my-hexflow

O vocabulário (record, tenant, node, gate, extension, projection) está em
[GLOSSARY.md](GLOSSARY.md).

## Requisitos

- `git` 2.31 ou mais novo no `PATH`, porque `attest`, `gate-pr` e `close` leem o commit atual. O `omhl` usa `git rev-parse --path-format=absolute`, que nasceu no 2.31.
- Claude Code, se você for usar o plugin.

O plugin foi testado só no Linux e no WSL.

## Instalação

1. Compile o binário de release:

   ```bash
   cargo build --release
   ```

2. Instale esse binário no `PATH`, fora de qualquer worktree.

   ```bash
   cp target/release/omhl ~/.local/bin/omhl
   ```

   Depois de puxar mudanças, recompile e copie de novo.

3. Abra o Claude Code com o plugin:

   ```bash
   claude --plugin-dir plugin/
   ```

Skills de orquestração dos agentes, em `plugin/skills/`:

| Skill | O que cobre |
|---|---|
| `interview` | A entrevista de negócio com o dono, antes do work item: placar de ambiguidade, rodadas de perguntas e a gravação do work item e das premissas confirmadas. |
| `plan-consensus` | O consenso do plano: chama o analyst, o planner, o architect e o critic em sequência, limita a 3 voltas, escala o impasse ao dono e retira os `contradicts` resolvidos antes do `spec validate`. |
| `review` | A revisão da entrega: chama o code-reviewer, o security-reviewer e o verifier, grava os achados, monta a tabela do PR e confere o `gate-pr`. Traz também a referência dos checks mecânicos e do fechamento do work item. |

As sete skills `role-*` não são chamadas pela sessão: são as vistas por papel, pré-carregadas pelos agentes do plugin.

O ciclo de dez passos, com o diagrama, está em
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

## Desenvolvimento

`cargo test` sem filtro roda a suíte inteira, que é pesada. Ao mexer em área
coberta por `tests/`, rode só o módulo afetado
(`cargo test -p <crate> --test integration <modulo>::`; no omhl, `--test e2e`
para o e2e; `--lib <modulo>::` para o unitário). Detalhes em
[docs/mechanical-gates.md](docs/mechanical-gates.md).

O mapa dos módulos está em [docs/architecture.md](docs/architecture.md).

## Créditos

Os agentes de `plugin/agents/` derivam do oh-my-claudecode 5.7.19, de Yeachan Heo, distribuído sob a licença MIT. O copyright, o texto da licença e a lista do que deriva dele estão em [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
