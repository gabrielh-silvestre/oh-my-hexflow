# Gravar o work item e as premissas da entrevista

Quando consultar: nos passos 2 e 3 da skill [interview](../SKILL.md), ao gravar o resultado da entrevista. A referência completa do `omhl append`, com as entradas e os erros, está em [append.md](../../directives-flow/references/append.md).

São dois lotes de `omhl append`, nesta ordem (a guarda recusa premissa sem um work item aberto):

- **Lote 1:** o work item (`work-items.<id>`, `title`, `status` `open`), gravado como `claude`.
- **Lote 2:** uma premissa de negócio por item. Cada uma leva `status` `confirmed`, `kind` `business`, o `work_item` e o `confirmed_by`, com `by` = `person:<nome>` e a `question` e a `answer` literais, copiadas da resposta real do dono, sem resumir, traduzir nem corrigir.

Mostre a premissa ao dono em português; o `text` gravado é em inglês.

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.i1","type":"work-item","data":{"title":"Export CSV","status":"open"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":3},"records":[{"id":"<record-id>","seq":3}],"replayed":false}
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.i1","type":"premise","data":{"text":"The export must include every column the user sees.","status":"confirmed","kind":"business","work_item":"work-items.i1","confirmed_by":{"by":"person:ana","question":"Quais colunas entram no export?","answer":"Todas as que o usuário vê na tela."}}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":4},"records":[{"id":"<record-id>","seq":4}],"replayed":false}
```
