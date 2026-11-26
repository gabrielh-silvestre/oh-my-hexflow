# omhl load

## Quem usa, em qual etapa e com qual filtro

| Papel | Etapa | Comando |
|---|---|---|
| sessao | review regras | `omhl load` |

## Quando usar

Depois de criar, editar ou apagar um arquivo em `.omhl/` (check ou diretiva), para o banco refletir os arquivos. Rode-o antes de qualquer `omhl check run` que venha depois de uma mudança num check: o `check run` recusa com `spec-stale` se o TOML e o banco divergem. `--dry-run` mostra o plano sem gravar nada.

## Entradas

- Sem argumentos posicionais. Lê os arquivos do projeto em `.omhl/`.
- `--dry-run`: informa o que faria, sem gravar.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ loaded` (ou `✔ dry run`, que não grava nada) com as contagens de `created`, `revised` e `unchanged`, e as de `skipped`, relações, órfãos e `warning` quando existem; depois uma linha por grupo não vazio com os ids (`created`, `revised`, `orphan`) ou uma linha por item (`skipped`, `relation`, `orphan relation`, `warning` com caminho, código e mensagem). Com `warning` o cabeçalho passa a `⚠`. Os ids de `unchanged` só entram na contagem. Com `--format json` é uma linha JSON: `tenant`, `dry_run`, e as listas de ids `created`, `revised`, `unchanged`, `skipped`, `orphans`, `orphan_relations`, `relations_created`, mais `warnings`. Um arquivo apagado da árvore não some do banco: o `load` só lista o id em `orphans` (`orphan` no texto). Para descartá-lo de vez, use `omhl check retire` (check) ou `omhl guideline retire` (diretiva). Um id já retirado volta em `skipped` e não é recriado. O comando é idempotente: sem mudança nos arquivos, tudo sai em `unchanged`.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](../../directives-flow/references/errors.md).

- `extensions-invalid` (saída 1): algum arquivo de `.omhl/` é inválido. Os `details` trazem o `path` e o `code` do problema (por exemplo `event-unsupported`, para um evento fora de `before-commit`, `before-pr` e `before-merge`, ou `rule-two-streams`, para uma regra com padrão em `stdout` e em `stderr`). Corrija o TOML e rode de novo.
- `invalid-session-id` (saída 1): `CLAUDE_CODE_SESSION_ID` está definida mas vazia, não é UTF-8, passa de 100 unidades UTF-16 ou é igual ao marcador `outside-claude-code`; nada é gravado e o erro não mostra o valor (`path` vazio). Remova a variável ou rode o `omhl` numa sessão do Claude Code.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Carrega os arquivos do projeto. Sem mudança nenhuma, os checks saem em `unchanged`; o `--dry-run` só informa:

```omhl-example fixture=checks
$ omhl load
✔ loaded · 0 created · 0 revised · 2 unchanged
$ omhl load --dry-run
✔ dry run · 0 created · 0 revised · 2 unchanged
```

Com `--format json` o relatório sai para máquinas, com as listas de ids por inteiro:

```omhl-example fixture=checks
$ omhl load --format json
{"created":[],"dry_run":false,"orphan_relations":[],"orphans":[],"relations_created":[],"revised":[],"skipped":[],"tenant":"acme","unchanged":["checks.ok","checks.review"],"warnings":[]}
```
