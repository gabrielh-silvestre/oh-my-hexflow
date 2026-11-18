# omhl load

## Quando usar

Depois de criar, editar ou apagar um arquivo em `.omhl/` (check ou diretiva), para o banco refletir os arquivos. Rode-o antes de qualquer `omhl check run` que venha depois de uma mudança num check: o `check run` recusa com `spec-stale` se o TOML e o banco divergem. `--dry-run` mostra o plano sem gravar nada.

## Entradas

- Sem argumentos posicionais. Lê os arquivos do projeto em `.omhl/`.
- `--dry-run`: informa o que faria, sem gravar.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).

## Saída

Uma linha JSON no stdout, saída 0: `tenant`, `dry_run`, e as listas de ids `created`, `revised`, `unchanged`, `skipped`, `orphans`, `orphan_relations`, `relations_created`, mais `warnings`. Um arquivo apagado da árvore não some do banco: o `load` só lista o id em `orphans`. Para descartá-lo de vez, use `omhl check retire` (check) ou `omhl guideline retire` (diretiva). Um id já retirado volta em `skipped` e não é recriado. O comando é idempotente: sem mudança nos arquivos, tudo sai em `unchanged`.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um argumento que falta é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `extensions-invalid` (saída 1): algum arquivo de `.omhl/` é inválido. Os `details` trazem o `path` e o `code` do problema (por exemplo `event-unsupported`, para um evento fora de `before-commit`, `before-pr` e `before-merge`, ou `rule-two-streams`, para uma regra com padrão em `stdout` e em `stderr`). Corrija o TOML e rode de novo.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Carrega os arquivos do projeto. Sem mudança nenhuma, os checks saem em `unchanged`; o `--dry-run` só informa:

```omhl-example fixture=checks
$ omhl load
{"created":[],"dry_run":false,"orphan_relations":[],"orphans":[],"relations_created":[],"revised":[],"skipped":[],"tenant":"acme","unchanged":["checks.ok","checks.review"],"warnings":[]}
$ omhl load --dry-run
{"created":[],"dry_run":true,"orphan_relations":[],"orphans":[],"relations_created":[],"revised":[],"skipped":[],"tenant":"acme","unchanged":["checks.ok","checks.review"],"warnings":[]}
```
