# omhl report answers

## Quando usar

Para ver o estado de cada gap (dúvida técnica aberta durante a execução): se está aberto, quem o respondeu e se uma diretiva confirmada sustenta a resposta. Use antes de escrever a spec ou de fechar o work item, para saber que dúvidas ainda esperam resposta e quais respostas o humano ainda não viu. É somente leitura.

## Entradas

- `--work-item <id>`: restringe aos gaps de um work item, como `work-items.w1`. O work item precisa existir (aberto ou fechado). Sem a opção, o relatório cobre todos os gaps.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).

## Saída

Uma linha JSON no stdout, saída 0: `{"gaps": [...]}`, só com os gaps vivos, na ordem de criação. Um filtro por work item sem nenhum gap devolve `{"gaps":[]}`, que aqui é resposta verdadeira (o work item existe, só não tem gaps). Cada gap traz `id`, `work_item` (quando o gap tem) e `state`:

- `open`: nenhuma premissa o responde. Só estes três campos.
- `answered`: há resposta. O gap ganha `answered_by` (a premissa), `answerer`, `author` (quem gravou a resposta) e `no_criterion`.
- `needs-human`: a resposta contraria uma diretiva confirmada e não tem `confirmed_by`. O gap segue contando como aberto no validador e na retro, até o humano decidir.

`answerer` é `human` quando a premissa que responde está confirmada (`confirmed` com `confirmed_by`) e `autonomous` em qualquer outro caso. `no_criterion` é `true` quando nenhuma diretiva confirmada sustenta a resposta; essas respostas entram na retro. Com mais de uma resposta, vale a primeira premissa confirmada; sem nenhuma, a primeira em ordem de gravação.

Uma resposta é a relation `answers` de uma premissa para o gap, gravada com `omhl append`. Para sustentar a resposta, grave a relation `supports` de uma diretiva confirmada para a premissa.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `work-item-not-found` (saída 1): o `--work-item` não existe. O ponteiro é `/work_item`. Confira o id com `omhl get <id>`.
- `INVALID_INPUT` (saída 1): o `--work-item` tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Sem gap nenhum, o relatório é vazio:

```omhl-example fixture=project
$ omhl report answers
{"gaps":[]}
```

Dois gaps: um respondido pela premissa confirmada `premises.p1` (sem diretiva que a sustente) e outro aberto. O filtro por work item devolve o mesmo, porque os dois são dele:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.fmt","type":"gap","data":{"question":"Which date format?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":3},{"id":"<record-id>","seq":4},{"id":"<record-id>","seq":5}],"replayed":false}
$ omhl report answers
{"gaps":[{"answered_by":"premises.p1","answerer":"human","author":{"agent":"claude","client":"cli"},"id":"gaps.limit","no_criterion":true,"state":"answered","work_item":"work-items.w1"},{"id":"gaps.fmt","state":"open","work_item":"work-items.w1"}]}
$ omhl report answers --work-item work-items.w1
{"gaps":[{"answered_by":"premises.p1","answerer":"human","author":{"agent":"claude","client":"cli"},"id":"gaps.limit","no_criterion":true,"state":"answered","work_item":"work-items.w1"},{"id":"gaps.fmt","state":"open","work_item":"work-items.w1"}]}
```

Recusa: o work item não existe e o id está mal formado:

```omhl-example fixture=project
$ omhl report answers --work-item work-items.zz
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/work_item"}],"message":"work-item-not-found at /work_item"}}
# exit 1
$ omhl report answers --work-item Bad_Id
2> {"error":{"code":"INVALID_INPUT","details":[{"code":"semantic-id-format","message":"invalid semantic id","path":"/work_item"}],"message":"invalid semantic id"}}
# exit 1
```
