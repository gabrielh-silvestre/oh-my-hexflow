# omhl report sessions

## Quem usa, em qual etapa e com qual filtro

Nenhuma etapa do ciclo a roda.

## Quando usar

Para listar as sessões reais do Claude Code que gravaram no banco do projeto, por exemplo para escolher as sessões de uma mineração que olha vários work items. Cada gravação do `omhl` guarda o id da sessão que a fez, e este é o único comando que o mostra. É somente leitura.

## Entradas

- `--work-item <id>`: restringe às sessões que gravaram ao menos um registro do work item, como `work-items.w1`. É repetível: com mais de um, vale a união, sem repetir sessão. Cada work item precisa existir (aberto ou fechado). A linha de uma sessão filtrada continua mostrando os totais e os work items da sessão inteira. Sem a opção, o comando lista todas as sessões.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ report sessions · N sessions` e uma linha por sessão, na ordem do primeiro registro de cada uma. Sem sessão real nenhuma sai `✔ report sessions · none`. Registros gravados fora do Claude Code (marcador `outside-claude-code`) e registros antigos, sem sessão, não têm linha nem somam em nenhuma. Com `--format json` é uma linha JSON, `{"sessions": [...]}`, e cada sessão traz:

- `id`: o id da sessão. No texto ele sai numa linha só, sem caracteres de controle; o JSON traz o valor exato, que é o que um programa deve consumir.
- `first` e `last`: a data do primeiro e a do último registro que a sessão gravou, no formato de `at` do `omhl get`.
- `nodes` e `edges`: quantos nós e quantas arestas a sessão gravou. Toda gravação conta, inclusive revisões e retiradas.
- `work_items`: os work items que a sessão tocou (no texto, `work items ...` ou `no work items`). O de um nó é o `work_item` dos dados; o de um nó de work item ou de pull request é o próprio id; diretiva, check e nó nativo não têm. Uma retirada herda o do registro anterior do mesmo nó ou da mesma aresta. Uma aresta pertence aos work items que as duas pontas tinham quando ela foi gravada; a retirada dela não relê as pontas, mesmo que elas tenham mudado de work item depois.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `work-item-not-found` (saída 1): um `--work-item` não existe. O ponteiro é `/work_item`. Confira o id com `omhl get <id>`.
- `INVALID_INPUT` (saída 1): um `--work-item` tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Num banco gravado só fora do Claude Code, a lista é vazia:

```omhl-example fixture=project
$ omhl report sessions
✔ report sessions · none
$ omhl report sessions --work-item work-items.w1 --format json
{"sessions":[]}
```

Recusa: o work item não existe:

```omhl-example fixture=project
$ omhl report sessions --work-item work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
```

Duas sessões: a primeira gravou uma decisão e a aresta que a liga à premissa de `work-items.w1`, a segunda gravou um gap do mesmo work item. A decisão não tem `work_item`; o da primeira sessão vem da ponta da aresta. O work item e a premissa foram gravados fora do Claude Code e não têm linha:

```omhl-example fixture=sessions
$ omhl report sessions
✔ report sessions · 2 sessions
5b1c0f3e-77a2-4c1e-9d0b-2f6a8e41c9aa · first 2026-11-24T22:32:22.926Z · last 2026-11-24T22:32:22.926Z · 1 node · 1 edge · work items work-items.w1
0e4d2a96-3c58-4f0b-8a17-d6b9c1e75f02 · first 2026-11-24T22:32:23.114Z · last 2026-11-24T22:32:23.114Z · 1 node · 0 edges · work items work-items.w1
$ omhl report sessions --work-item work-items.w1 --format json
{"sessions":[{"edges":1,"first":"2026-11-24T22:32:22.926Z","id":"5b1c0f3e-77a2-4c1e-9d0b-2f6a8e41c9aa","last":"2026-11-24T22:32:22.926Z","nodes":1,"work_items":["work-items.w1"]},{"edges":0,"first":"2026-11-24T22:32:23.114Z","id":"0e4d2a96-3c58-4f0b-8a17-d6b9c1e75f02","last":"2026-11-24T22:32:23.114Z","nodes":1,"work_items":["work-items.w1"]}]}
```
