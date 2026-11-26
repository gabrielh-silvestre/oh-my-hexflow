# omhl report recurrences

## Who uses it, at which stage and with which filter

| Role | Stage | Command |
|---|---|---|
| analyst | 2 | `omhl report recurrences --min 2` |

A sessão também o roda antes de gravar premissa: [directives-flow](../SKILL.md), seção Regras.

## Quando usar

Em dois momentos. Antes de registrar uma premissa, com `--min 2`, para achar uma premissa anterior que significa a mesma coisa e copiar o texto exato dela (assim a promoção agrupa as duas). E no fim de um work item, sem `--min`, para ver as premissas repetidas em work items suficientes para sugerir a promoção a diretiva. O relatório só lê: sugerir a promoção é com você, a confirmação é do humano, e só então a `guideline` é gravada, com `confirmed_by`. É somente leitura.

## Entradas

- `--min <n>`: o número mínimo de work items distintos para o grupo entrar. Sem a opção, vale `promotion.min_work_items` do `.omhl/workflow.toml`, e na falta dele o padrão 3. A opção vence o arquivo. O comando não valida o valor: `--min 0` e `--min 1` listam todos os grupos.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir. O `workflow.toml` é lido do projeto mesmo com `--db`.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ report recurrences · min N · G groups` (o `min` é o valor usado) e, por grupo, a linha `- kind · text` (o texto normalizado) seguida de `work items:` e `premises:` com os ids inteiros. Com `--format json` é uma linha JSON: `min` e `recurrences`, a lista de grupos, cada um com `kind`, `text`, `work_items` e `premises`. O agrupamento é pela chave `kind` e texto normalizado: o texto passa por `trim`, minúsculas, espaços colapsados e remoção da pontuação final. O grupo conta os work items distintos, inclusive os já fechados. Premissa sem `kind` ou sem `work_item` não entra, e um grupo some quando já existe uma diretiva com o mesmo texto normalizado. `✔ report recurrences · min 3 · none` (no JSON, `{"min":3,"recurrences":[]}`) é a resposta quando nada se repete o bastante.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Um `--min` que não é um número inteiro sem sinal é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `invalid-workflow-config` (saída 1): sem `--min`, o `.omhl/workflow.toml` é inválido. Corrija o arquivo, ou passe `--min`.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Uma premissa só não se repete: com o padrão 3, o relatório vem vazio. Com `--min 1`, o grupo da premissa aparece; com `--format json` o mesmo relatório sai para máquinas:

```omhl-example fixture=project
$ omhl report recurrences
✔ report recurrences · min 3 · none
$ omhl report recurrences --min 1
✔ report recurrences · min 1 · 1 group
- business · money is stored in cents
  work items: work-items.w1
  premises: premises.p1
$ omhl report recurrences --min 1 --format json
{"min":1,"recurrences":[{"kind":"business","premises":["premises.p1"],"text":"money is stored in cents","work_items":["work-items.w1"]}]}
```

Três work items com a mesma premissa, escrita com caixa, espaços e pontuação diferentes, formam um grupo no padrão 3. Com `--min 4`, o grupo some:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w2","type":"work-item","data":{"title":"Second item","status":"open"}}},{"body":{"kind":"node","id":"work-items.w3","type":"work-item","data":{"title":"Third item","status":"open"}}}]}
EOF
✔ append · 2 records
- <record-id> · seq 3
- <record-id> · seq 4
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p2","type":"premise","data":{"text":"money is stored in cents.","status":"assumed","kind":"business","work_item":"work-items.w2"}}},{"body":{"kind":"node","id":"premises.p3","type":"premise","data":{"text":"Money  is stored in CENTS","status":"assumed","kind":"business","work_item":"work-items.w3"}}}]}
EOF
✔ append · 2 records
- <record-id> · seq 5
- <record-id> · seq 6
$ omhl report recurrences
✔ report recurrences · min 3 · 1 group
- business · money is stored in cents
  work items: work-items.w1, work-items.w2, work-items.w3
  premises: premises.p1, premises.p2, premises.p3
$ omhl report recurrences --min 4
✔ report recurrences · min 4 · none
```
