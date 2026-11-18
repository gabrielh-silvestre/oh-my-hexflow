# omhl report answers

## Quando usar

Para ver o estado de cada gap (dúvida técnica aberta durante a execução): se está aberto, quem o respondeu e se uma diretiva confirmada sustenta a resposta. Use antes de escrever a spec ou de fechar o work item, para saber que dúvidas ainda esperam resposta e quais respostas o humano ainda não viu. É somente leitura.

## Entradas

- `--work-item <id>`: restringe aos gaps de um work item, como `work-items.w1`. O work item precisa existir (aberto ou fechado). Sem a opção, o relatório cobre todos os gaps.
- `--db <caminho>` e `--project-dir <pasta>`: como nos demais comandos. Um `--db` com caminho novo cria o banco, fora da pasta do projeto; o `--project-dir` precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0: `✔ report answers · N gaps` e uma linha por gap vivo, na ordem de criação, `id · state · ...`. Sem gap nenhum (ou um filtro por work item sem gaps) sai `✔ report answers · none`, que aqui é resposta verdadeira (o work item existe, só não tem gaps). Com `--format json` é uma linha JSON, `{"gaps": [...]}`. Cada gap traz `id`, `work_item` (quando o gap tem; no texto, `of <work item>`) e `state`:

- `open`: nenhuma premissa o responde. Só estes três campos.
- `answered`: há resposta. O gap ganha `answered_by` (a premissa; no texto, `by <premissa>`), `answerer` (no texto, `<answerer> answer`), `author` (quem gravou a resposta; só no JSON) e `no_criterion` (no texto, `no criterion` ou `criterion found`).
- `needs-human`: a resposta contraria uma diretiva confirmada e não tem `confirmed_by`. O gap segue contando como aberto no validador e na retro, até o humano decidir.

`answerer` é `human` quando a premissa que responde está confirmada (`confirmed` com `confirmed_by`) e `autonomous` em qualquer outro caso. `no_criterion` é `true` quando nenhuma diretiva confirmada sustenta a resposta; essas respostas entram na retro. Com mais de uma resposta, vale a primeira premissa confirmada; sem nenhuma, a primeira em ordem de gravação.

Uma resposta é a relation `answers` de uma premissa para o gap, gravada com `omhl append`. Para sustentar a resposta, grave a relation `supports` de uma diretiva confirmada para a premissa.

## Erros

Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON). Uma opção desconhecida é erro de uso do clap, com saída 2 e sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `work-item-not-found` (saída 1): o `--work-item` não existe. O ponteiro é `/work_item`. Confira o id com `omhl get <id>`.
- `INVALID_INPUT` (saída 1): o `--work-item` tem forma inválida (`semantic-id-format`). O `path` do detalhe diz qual argumento.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Sem gap nenhum, o relatório é vazio:

```omhl-example fixture=project
$ omhl report answers
✔ report answers · none
```

Dois gaps: um respondido pela premissa confirmada `premises.p1` (sem diretiva que a sustente) e outro aberto. O filtro por work item devolve o mesmo, porque os dois são dele; com `--format json` o relatório sai para máquinas:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"node","id":"gaps.fmt","type":"gap","data":{"question":"Which date format?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":5},"records":[{"id":"<record-id>","seq":3},{"id":"<record-id>","seq":4},{"id":"<record-id>","seq":5}],"replayed":false}
$ omhl report answers
✔ report answers · 2 gaps
gaps.limit · answered · by premises.p1 · human answer · no criterion · of work-items.w1
gaps.fmt · open · of work-items.w1
$ omhl report answers --work-item work-items.w1
✔ report answers · 2 gaps
gaps.limit · answered · by premises.p1 · human answer · no criterion · of work-items.w1
gaps.fmt · open · of work-items.w1
$ omhl report answers --format json
{"gaps":[{"answered_by":"premises.p1","answerer":"human","author":{"agent":"claude","client":"cli"},"id":"gaps.limit","no_criterion":true,"state":"answered","work_item":"work-items.w1"},{"id":"gaps.fmt","state":"open","work_item":"work-items.w1"}]}
```

Recusa: o work item não existe e o id está mal formado:

```omhl-example fixture=project
$ omhl report answers --work-item work-items.zz
2> ✘ work-item-not-found · work-item-not-found at /work_item
2>   - /work_item · work-item-not-found
# exit 1
$ omhl report answers --work-item Bad_Id
2> ✘ INVALID_INPUT · invalid semantic id
2>   - /work_item · semantic-id-format
# exit 1
```
