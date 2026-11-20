# omhl append

- [Quem usa](#quem-usa-em-qual-etapa-e-com-qual-filtro)
- [Quando usar](#quando-usar)
- [Entradas](#entradas)
- [Saída](#saída)
- [Erros](#erros)
- [Exemplos](#exemplos)

## Quando usar

Para gravar no banco do projeto: work item, premissa, gap, divergência, diretiva e relation. É o único comando de escrita genérica; os registros que têm comando próprio (`omhl attest`, `omhl guideline retire`) não passam por aqui. Sob o Claude Code a CLI grava `agent: "claude"`. A guarda recusa o pedido inteiro se um item viola uma regra: nada é gravado pela metade.

## Entradas

- O pedido é um JSON no stdin: `{"key": "<texto>", "items": [...]}`. `items` tem de um a cinquenta itens. Cada item tem `body` e, opcionalmente, `base` (o `record_id` que ele revisa), `snapshot` e `external_id`. Qualquer outra chave no topo do pedido é `append-invalid`.
- `body` é `{"kind":"node","id":"<namespace>.<nome>","type":"<tipo>","data":{...}}`, `{"kind":"relation","relation_type":"<nome>","from":"<id>","to":"<id>"}` ou `{"kind":"retirement","lineage":{"kind":"relation","relation_type":"contradicts","from":"<id>","to":"<id>"}}`, que retira uma relação vigente. A retirada exige o `base` da relação (o `record_id` vigente) e nenhum comando `omhl` o lê: mande o pedido sem `base`, a resposta é `FORK_REJECTED` com `not-current` e `details[0].current` é o `base`; repita o pedido com esse valor em `base` no item. Outro erro na primeira chamada quer dizer que a relação não existe, já foi retirada ou é sua: não repita. Só o verifier e o dono retiram.
- Premissa `confirmed` leva `confirmed_by` com `by` (`person:<nome>`), `question` e `answer` literais, copiados da resposta real do humano.
- Diretiva nova ou revisada leva `kind`: `business` ou `architecture`.
- Opções: `--db <caminho>` (um caminho novo cria o banco) e `--project-dir <pasta>` (a pasta onde a busca do marcador `.omhl.toml` começa; precisa existir).
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.
- `key` (opcional, no topo do JSON, ao lado de `items`; não existe flag `--key`): um texto de 1 a 200 caracteres que você escolhe para identificar este pedido, por exemplo `gaps-limit-1`. Ela existe para o retry. Se o `append` falha sem você saber se gravou (`BUSY`, `STORE_ERROR`, timeout), repita o pedido idêntico com a mesma `key`:
  - Se o primeiro não tinha gravado, o segundo grava e devolve `replayed: false`.
  - Se o primeiro já tinha gravado, nada é gravado de novo e a saída traz os mesmos `records` com `replayed: true`. Nos dois casos o pedido está feito: siga em frente.
  - Mesma `key` com `items` diferentes é `IDEMPOTENCY_CONFLICT` (`path` `/key`): a `key` já pertence a outro pedido. Use uma `key` nova se o conteúdo mudou de propósito.
  - Só `items` conta na comparação: o autor não. `key` vazia ou com mais de 200 caracteres é `INVALID_INPUT`.
  - A guarda do omhl roda antes da `key`: se o estado mudou e a guarda passou a recusar o pedido (por exemplo, `work-item-closed`), o retry sai com o erro da guarda, não com `replayed: true`.
  - Sem `key`, o retry é um pedido novo. Uma revisão (com `base`) que já tinha gravado volta como `FORK_REJECTED` (`not-current`) em vez de `replayed`. Por isso: pedido que você pode ter de repetir leva `key`; um pedido de leitura rápida e sem retry pode ficar sem.

### Campos de data por tipo

Campo extra é recusado. Os textos têm de 1 a 2000 caracteres (o `title`, de 1 a 200).

| Tipo | Obrigatórios | Opcionais |
|---|---|---|
| `work-item` | `title`, `status` (`open`; `closed` só pelo `omhl close`) | |
| `premise` | `text`, `status` (`assumed` ou `confirmed`) | `kind` (`technical`, `business`), `work_item`, `confirmed_by` |
| `guideline` | `text` | `kind` (`business`, `architecture`), `confirmed_by` |
| `gap` | `question`, `context` | `kind` (`technical`, `business`), `work_item` |
| `divergence` | `source_ref`, `kind` (`missing`, `partial`, `contradicts`, `unrequested`), `text`, `evidence`, `resolution` (`open`, `spec-stale`, `code-fixed`, `accepted`), `work_item` | `confirmed_by`, `amendment` |
| `decision` | `text`, `evidence` | `work_item` |

`evidence` é uma lista de 1 a 20 itens `{"source": "...", "summary": "..."}`. `confirmed_by` é `{"by": "person:<nome>", "question": "...", "answer": "..."}`.

## Saída

Texto no stdout, saída 0: `✔ append · N records` (`1 record` no singular) e uma linha `- <record-id> · seq S` por item gravado, na ordem do pedido. O `<record-id>` inteiro é o `base` de uma revisão futura. Quando a `key` do pedido já tinha gravado exatamente estes `items`, o cabeçalho ganha `· replayed` (veja a seção Entradas). A cabeça da cadeia não aparece no texto. Com `--format json` é uma linha JSON: `chain_head` (`hash` e `seq` da cabeça da cadeia), `records` (o `id` e o `seq` de cada item gravado) e `replayed` (`true` ou `false`). Um erro sai no stderr, com saída 1, em texto (`✘ código · mensagem`, uma linha `- caminho · código · mensagem` por detalhe e `Next:` quando há `next_step`; com `--format json`, numa linha JSON).

## Erros

Cada erro traz o código (`error.code` no JSON) e, nos detalhes, o `path` do campo (nunca o conteúdo digitado). Um erro de uso do clap (opção desconhecida) sai com saída 2, sem código. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `input-invalid-json` (saída 1): o stdin não é um JSON válido. A mensagem diz a linha e a coluna. Corrija o JSON.
- `input-not-utf8` (saída 1): o stdin não é UTF-8. Reenvie em UTF-8.
- `input-too-large` (saída 1): o pedido passou do teto de tamanho. Divida em pedidos menores.
- `input-too-deep` (saída 1): o JSON tem aninhamento demais. Achate a estrutura.
- `append-invalid` (saída 1): uma chave desconhecida ou um campo com a forma errada. O `path` aponta o campo (por exemplo, `base` mora no item, não no `body`).
- `confirmation-required` (saída 1): premissa `confirmed`, diretiva ou divergência `spec-stale` sem `confirmed_by`. Pergunte ao humano e grave a pergunta e a resposta literais.
- `confirmation-invalid` (saída 1): o `by` não começa com `person:` ou não tem nome (salvo `"by": "critic"` gravado pelo `omhl:critic` numa premissa `technical`), ou o `confirmed_by` veio num registro que não o admite (por exemplo, `confirmed_by` ou `amendment` numa divergência que não está em `spec-stale`). Corrija o campo.
- `kind-missing` (saída 1): diretiva nova ou revisada sem `kind`. Acrescente `business` ou `architecture`.
- `kind-invalid` (saída 1): `kind` de diretiva com outro valor. Use `business` ou `architecture`.
- `work-item-not-found` (saída 1): o `work_item` da premissa, do gap ou da divergência não existe. Grave o work item antes.
- `work-item-closed` (saída 1): o work item já foi fechado. Grave num work item aberto.
- `use-close-work-item` (saída 1): o `status` do work item foi trocado para `closed` por aqui. Feche com `omhl close`.
- `rests-on-architecture` (saída 1): uma diretiva se apoia numa diretiva de `kind` `architecture`. Apoie-a numa de negócio.
- `rests-on-target-type` (saída 1): uma diretiva se apoia numa premissa. A relation `rests-on` de diretiva só aceita diretiva.
- `guideline-retire-by-command` (saída 1): a retirada de diretiva veio pelo `append`. Use `omhl guideline retire`.
- `role-unknown` (saída 1): o `OMHL_ROLE` tem um valor fora dos sete papéis, ou foi definido fora do Claude Code. Corrija ou remova a variável.
- `role-record-forbidden` (saída 1): o papel grava um tipo de node ou relação que não é dele, uma divergência criada fora de `open`, ou uma relação a partir de node de outro ou inexistente. Grave só o que a linha do papel permite.
- `role-revision-forbidden` (saída 1): o papel revisa node de outro, premissa já confirmada ou divergência já resolvida, muda o `kind` de premissa ou gap, ou tira uma divergência de `open` fora do fechamento do verifier. Grave um node seu ou peça ao dono.
- `role-answer-forbidden` (saída 1): o critic liga `answers` a um destino que não é gap `technical`. Deixe o gap ao dono.
- `role-retirement-forbidden` (saída 1): quem chama não pode retirar esse node ou relação, ou o `contradicts` é seu. Peça ao verifier ou ao dono.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.
- `INVALID_INPUT` (saída 1): um valor do pedido tem forma inválida para o banco (por exemplo, id semântico mal formado, `batch-size` com lote vazio ou de mais de 50 itens, ou `idempotency-key-format`). O `path` e o `code` do detalhe dizem qual.
- `INVALID_RECORD` (saída 1): o `data` não bate com o schema da tabela acima, ou a relation aponta para um nó inexistente. Cada violação vem em `details`, com `code` (`required`, `enum`, `additional-properties`, `min-length`, `endpoint-not-found`, entre outros) e o `path` do campo; o `message` não traz os valores válidos, olhe a tabela. Corrija todos os campos listados de uma vez.
- `FORK_REJECTED` (saída 1): `not-current` quer dizer que o `base` não é o registro vigente do id; use o `current` do detalhe como novo `base` (se `retired` for `true`, o registro foi retirado: pare e avise). `lineage-in-batch`: dois itens do mesmo id no lote; junte-os.
- `IDEMPOTENCY_CONFLICT` (saída 1): a `key` já gravou um pedido diferente. O detalhe é `key-reused`, no `path` `/key`. Repita o mesmo pedido com a mesma `key`, ou use uma `key` nova para um pedido novo.
- `CYCLE_REJECTED` (saída 1): a relation fecharia um ciclo (`rests-on`, `supersedes`). O `next_step` do detalhe diz o que fazer.
- `BUSY` (saída 1): outro `omhl` está gravando. O `omhl` já tentou 3 vezes e nada foi gravado: repita o mesmo comando.
- `STORE_ERROR` (saída 1): falha do banco; o resultado é incerto. Repita o pedido com a mesma `key`: se já tinha gravado, volta `replayed: true`.
- `TYPE_NOT_FOUND` (saída 1): o `type` não existe. Use um da tabela acima.
- `RELATION_TYPE_NOT_FOUND` (saída 1): o `relation_type` não existe (`answers`, `rests-on`, `supports`, `contradicts`, `supersedes`).
- `RESERVED_ID` (saída 1): o id pertence a um nó nativo. Escolha outro id.

## Exemplos

Grava um gap e a relation `answers` de uma premissa para ele, no mesmo pedido:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}},{"body":{"kind":"relation","relation_type":"answers","from":"premises.p1","to":"gaps.limit"}}]}
EOF
✔ append · 2 records
- <record-id> · seq 3
- <record-id> · seq 4
```

Com `--format json` a resposta traz também a cabeça da cadeia:

```omhl-example fixture=project
$ omhl append --format json <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w8","type":"work-item","data":{"title":"Export PDF","status":"open"}}}]}
EOF
{"chain_head":{"hash":"<hash>","seq":3},"records":[{"id":"<record-id>","seq":3}],"replayed":false}
```

Cria o work item (o passo 2 do protocolo):

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w9","type":"work-item","data":{"title":"Export CSV","status":"open"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
```

Recusa pelo schema: o mesmo work item sem `title`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"work-items.w9","type":"work-item","data":{"status":"open"}}}]}
EOF
2> ✘ INVALID_RECORD · record content is invalid
2>   - /items/0/body/data/title · required · required
# exit 1
```

O mesmo pedido, com a mesma `key`, duas vezes: o segundo devolve `replayed: true` e não grava de novo:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record · replayed
- <record-id> · seq 3
```

A mesma `key` com outro conteúdo é recusada com `IDEMPOTENCY_CONFLICT`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}
EOF
✔ append · 1 record
- <record-id> · seq 3
$ omhl append <<'EOF'
{"key":"gap-limit-1","items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"Excel export.","work_item":"work-items.w1"}}}]}
EOF
2> ✘ IDEMPOTENCY_CONFLICT · idempotency key was used with different items
2>   - /key · key-reused
# exit 1
```

Recusa: `confirmed_by` numa divergência que não é `spec-stale` (ao fechar como `code-fixed` ou `accepted`, omita `confirmed_by` e `amendment`):

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"divergences.limit","type":"divergence","data":{"source_ref":"CMP-001","kind":"missing","text":"The export has no row limit.","evidence":[{"source":"src/export.rs","summary":"No limit is applied."}],"resolution":"code-fixed","work_item":"work-items.w1","confirmed_by":{"by":"person:ana","question":"q","answer":"a"}}}}]}
EOF
2> ✘ confirmation-invalid · confirmation-invalid at /items/0/body/data/confirmed_by
2>   - /items/0/body/data/confirmed_by · confirmation-invalid
# exit 1
```

Recusa: premissa `confirmed` sem `confirmed_by`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p2","type":"premise","data":{"text":"Dates are UTC.","status":"confirmed","kind":"business","work_item":"work-items.w1"}}}]}
EOF
2> ✘ confirmation-required · confirmation-required at /items/0/body/data/confirmed_by
2>   - /items/0/body/data/confirmed_by · confirmation-required
# exit 1
```

Recusa: o `by` sem o prefixo `person:`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p4","type":"premise","data":{"text":"Dates are UTC.","status":"confirmed","kind":"business","work_item":"work-items.w1","confirmed_by":{"by":"ana","question":"q","answer":"a"}}}}]}
EOF
2> ✘ confirmation-invalid · confirmation-invalid at /items/0/body/data/confirmed_by/by
2>   - /items/0/body/data/confirmed_by/by · confirmation-invalid
# exit 1
```

Recusa: premissa de um work item que não existe:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"premises.p3","type":"premise","data":{"text":"Dates are UTC.","status":"assumed","kind":"technical","work_item":"work-items.zz"}}}]}
EOF
2> ✘ work-item-not-found · work-item-not-found at /items/0/body/data/work_item
2>   - /items/0/body/data/work_item · work-item-not-found
# exit 1
```

Recusa: diretiva sem `kind`:

```omhl-example fixture=project
$ omhl append <<'EOF'
{"items":[{"body":{"kind":"node","id":"guidelines.g1","type":"guideline","data":{"text":"Be brief."}}}]}
EOF
2> ✘ kind-missing · the guideline has no kind
2>   - /items/0/body/data/kind
2> Next: set kind to business or architecture in the item data and append again.
# exit 1
```

Recusa: o stdin não é JSON:

```omhl-example fixture=project
$ omhl append <<'EOF'
not json
EOF
2> ✘ input-invalid-json · input is not valid JSON at line 1 column 2
# exit 1
```
