# Gravar o work item e as premissas da entrevista

Quando consultar: nos passos 2 e 3 da skill [interview](../SKILL.md), ao gravar, no fim da entrevista, o work item e as premissas que as rodadas já confirmaram. A referência completa do `omhl append`, com as entradas e os erros, está em [append.md](../../directives-flow/references/append.md).

São dois lotes de `omhl append`, nesta ordem (a guarda recusa premissa sem um work item aberto):

- **Lote 1:** o work item (`work-items.<id>`, `title`, `status` `open`), gravado como `claude`.
- **Lote 2:** uma premissa de negócio por item, todas já confirmadas nas rodadas, sem nova confirmação em lote. Cada uma leva `status` `confirmed`, `kind` `business`, o `work_item` e o `confirmed_by`, com `by` = `person:<nome>`, a `question` (a premissa como você a redigiu) e a `answer` (a confirmação ou a correção literal do dono, sem resumir, traduzir nem corrigir), cada texto com até 2000 caracteres. A pergunta da rodada e a resposta inteiras ficam na nota.

Cada lote leva uma `key`. Se o lote 2 falhar, regrave só ele; o lote 1 já está no banco.

Mostre a premissa ao dono em português; o `text` gravado é em inglês.

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.i1","type":"work-item","data":{"title":"Export CSV","status":"open"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.i1","type":"premise","data":{"text":"The export must include every column the user sees.","status":"confirmed","kind":"business","work_item":"work-items.i1","confirmed_by":{"by":"person:ana","question":"O export inclui todas as colunas que o usuário vê na tela.","answer":"Confirmo."}}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 4
```
