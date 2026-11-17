# omhl report recurrences

## Quando usar

Em dois momentos. Antes de registrar uma premissa, com `--min 2`, para achar uma premissa anterior que significa a mesma coisa e copiar o texto exato dela (assim a promoção agrupa as duas). E no fim de um work item, sem `--min`, para ver as premissas repetidas em work items suficientes para sugerir a promoção a diretiva. O relatório só lê: sugerir a promoção é com você, a confirmação é do humano, e só então a `guideline` é gravada, com `confirmed_by`. É somente leitura.

## Entradas

- `--min <n>`: o número mínimo de work items distintos para o grupo entrar. Sem a opção, vale `promotion.min_work_items` do `.omhl/workflow.toml`, e na falta dele o padrão 3. A opção vence o arquivo. O comando não valida o valor: `--min 0` e `--min 1` listam todos os grupos.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir. O `workflow.toml` é lido do projeto mesmo com `--db`.

## Saída

Uma linha JSON no stdout, saída 0: `min` (o valor usado) e `recurrences`, a lista de grupos. Cada grupo traz `kind`, `text` (o texto normalizado), `work_items` e `premises`. O agrupamento é pela chave `kind` e texto normalizado: o texto passa por `trim`, minúsculas, espaços colapsados e remoção da pontuação final. O grupo conta os work items distintos, inclusive os já fechados. Premissa sem `kind` ou sem `work_item` não entra, e um grupo some quando já existe uma diretiva com o mesmo texto normalizado. `{"min":3,"recurrences":[]}` é a resposta quando nada se repete o bastante.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Um `--min` que não é um número inteiro sem sinal é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `invalid-workflow-config` (saída 1): sem `--min`, o `.omhl/workflow.toml` é inválido. Corrija o arquivo, ou passe `--min`.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Uma premissa só não se repete: com o padrão 3, o relatório vem vazio. Com `--min 1`, o grupo da premissa aparece:

```omhl-example fixture=project
$ omhl report recurrences
{"min":3,"recurrences":[]}
$ omhl report recurrences --min 1
{"min":1,"recurrences":[{"kind":"business","premises":["premises.p1"],"text":"money is stored in cents","work_items":["work-items.w1"]}]}
```

Três work items com a mesma premissa, escrita com caixa, espaços e pontuação diferentes, formam um grupo no padrão 3. Com `--min 4`, o grupo some:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w2","type":"work-item","data":{"title":"Second item","status":"open"}}},{"body":{"kind":"node","id":"work-items.w3","type":"work-item","data":{"title":"Third item","status":"open"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":4},"records":[{"id":"<record-id>","seq":3},{"id":"<record-id>","seq":4}],"replayed":false}
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p2","type":"premise","data":{"text":"money is stored in cents.","status":"assumed","kind":"business","work_item":"work-items.w2"}}},{"body":{"kind":"node","id":"premises.p3","type":"premise","data":{"text":"Money  is stored in CENTS","status":"assumed","kind":"business","work_item":"work-items.w3"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":6},"records":[{"id":"<record-id>","seq":5},{"id":"<record-id>","seq":6}],"replayed":false}
$ omhl report recurrences
{"min":3,"recurrences":[{"kind":"business","premises":["premises.p1","premises.p2","premises.p3"],"text":"money is stored in cents","work_items":["work-items.w1","work-items.w2","work-items.w3"]}]}
$ omhl report recurrences --min 4
{"min":4,"recurrences":[]}
```
