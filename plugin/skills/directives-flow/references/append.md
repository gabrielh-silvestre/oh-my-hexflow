# omhl append

## Quando usar

Para gravar no banco do projeto: work item, premissa, gap, divergência, diretiva e relation. É o único comando de escrita genérica; os registros que têm comando próprio (`omhl attest`, `omhl guideline retire`) não passam por aqui. Sob o Claude Code a CLI grava `agent: "claude"`. A guarda recusa o pedido inteiro se um item viola uma regra: nada é gravado pela metade.

## Entradas

- O pedido é um JSON no stdin: `{"items": [...]}`, com um a cinquenta itens. Cada item tem `body` e, opcionalmente, `base` (o `record_id` que ele revisa), `snapshot` e `external_id`.
- `body` é `{"kind":"node","id":"<namespace>.<nome>","type":"<tipo>","data":{...}}` ou `{"kind":"relation","relation_type":"<nome>","from":"<id>","to":"<id>"}`.
- Premissa `confirmed` leva `confirmed_by` com `by` (`person:<nome>`), `question` e `answer` literais, copiados da resposta real do humano.
- Diretiva nova ou revisada leva `kind`: `business` ou `architecture`.
- Opções: `--db <caminho>` (um caminho novo cria o banco) e `--project-dir <pasta>` (a pasta onde a busca do marcador `.omhl.toml` começa; precisa existir).

## Saída

Uma linha JSON no stdout, saída 0: `chain_head` (`hash` e `seq` da cabeça da cadeia), `records` (o `id` e o `seq` de cada item gravado) e `replayed` (`true` quando a mesma `key` já tinha gravado o pedido). Um erro sai no stderr, numa linha JSON, com saída 1.

## Erros

Cada erro traz `error.code` e, em `details`, o `path` do campo (nunca o conteúdo digitado). Um erro de uso do clap (opção desconhecida) sai com saída 2, sem código.

- `input-invalid-json` (saída 1): o stdin não é um JSON válido. A mensagem diz a linha e a coluna. Corrija o JSON.
- `input-not-utf8` (saída 1): o stdin não é UTF-8. Reenvie em UTF-8.
- `input-too-large` (saída 1): o pedido passou do teto de tamanho. Divida em pedidos menores.
- `input-too-deep` (saída 1): o JSON tem aninhamento demais. Achate a estrutura.
- `append-invalid` (saída 1): uma chave desconhecida ou um campo com a forma errada. O `path` aponta o campo (por exemplo, `base` mora no item, não no `body`).
- `confirmation-required` (saída 1): premissa `confirmed`, diretiva ou divergência `spec-stale` sem `confirmed_by`. Pergunte ao humano e grave a pergunta e a resposta literais.
- `confirmation-invalid` (saída 1): o `by` não começa com `person:` ou não tem nome, ou o `confirmed_by` veio num registro que não o admite. Corrija o campo.
- `kind-missing` (saída 1): diretiva nova ou revisada sem `kind`. Acrescente `business` ou `architecture`.
- `kind-invalid` (saída 1): `kind` de diretiva com outro valor. Use `business` ou `architecture`.
- `work-item-not-found` (saída 1): o `work_item` da premissa, do gap ou da divergência não existe. Grave o work item antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Grave num work item aberto.
- `use-close-work-item` (saída 1): o `status` do work item foi trocado para `closed` por aqui. Feche com `omhl close`.
- `rests-on-architecture` (saída 1): uma diretiva se apoia numa diretiva de `kind` `architecture`. Apoie-a numa de negócio.
- `rests-on-target-type` (saída 1): uma diretiva se apoia numa premissa. A relation `rests-on` de diretiva só aceita diretiva.
- `guideline-retire-by-command` (saída 1): a retirada de diretiva veio pelo `append`. Use `omhl guideline retire`.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.
- `INVALID_INPUT` (saída 1): um valor do pedido tem forma inválida para o banco (por exemplo, id semântico mal formado, ou lote vazio). O `path` e o `code` do detalhe dizem qual.

## Exemplos

Grava um gap e a relation `answers` de uma premissa para ele, no mesmo pedido:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":4},"records":[{"id":"<record-id>","seq":3},{"id":"<record-id>","seq":4}],"replayed":false}
```

Recusa: premissa `confirmed` sem `confirmed_by`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p2","type":"premise","data":{"text":"Dates are UTC.","status":"confirmed","kind":"business","work_item":"work-items.w1"}}}]}
EOF
2> {"error":{"code":"confirmation-required","details":[{"code":"confirmation-required","message":"confirmation-required","path":"/items/0/body/data/confirmed_by"}],"message":"confirmation-required at /items/0/body/data/confirmed_by"}}
# exit 1
```

Recusa: o `by` sem o prefixo `person:`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p4","type":"premise","data":{"text":"Dates are UTC.","status":"confirmed","kind":"business","work_item":"work-items.w1","confirmed_by":{"by":"ana","question":"q","answer":"a"}}}}]}
EOF
2> {"error":{"code":"confirmation-invalid","details":[{"code":"confirmation-invalid","message":"confirmation-invalid","path":"/items/0/body/data/confirmed_by/by"}],"message":"confirmation-invalid at /items/0/body/data/confirmed_by/by"}}
# exit 1
```

Recusa: premissa de um work item que não existe:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p3","type":"premise","data":{"text":"Dates are UTC.","status":"assumed","kind":"technical","work_item":"work-items.zz"}}}]}
EOF
2> {"error":{"code":"work-item-not-found","details":[{"code":"work-item-not-found","message":"work-item-not-found","path":"/items/0/body/data/work_item"}],"message":"work-item-not-found at /items/0/body/data/work_item"}}
# exit 1
```

Recusa: diretiva sem `kind`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"guidelines.g1","type":"guideline","data":{"text":"Be brief."}}}]}
EOF
2> {"error":{"code":"kind-missing","details":[{"code":"kind-missing","message":"the guideline has no kind","next_step":"set kind to business or architecture in the item data and append again","path":"/items/0/body/data/kind"}],"message":"the guideline has no kind"}}
# exit 1
```

Recusa: o stdin não é JSON:

```omhl-example fixture=project
$ omhl append <<'EOF'
not json
EOF
2> {"error":{"code":"input-invalid-json","details":[],"message":"input is not valid JSON at line 1 column 2"}}
# exit 1
```

Recusa: `--project-dir` que não existe (a CLI não procura o projeto num diretório acima):

```omhl-example fixture=project
$ omhl append --project-dir /nonexistent <<'EOF'
{"items":[]}
EOF
2> {"error":{"code":"invalid-project-dir","details":[],"message":"the project directory does not exist"}}
# exit 1
```
