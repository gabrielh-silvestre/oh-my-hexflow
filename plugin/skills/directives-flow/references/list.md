# omhl list

- [Quem usa](#quem-usa-em-qual-etapa-e-com-qual-filtro)
- [Quando usar](#quando-usar)
- [Entradas](#entradas)
- [Saída](#saída)
- [Erros](#erros)
- [Exemplos](#exemplos)

## Quem usa, em qual etapa e com qual filtro

| Papel | Etapa | Comando |
|---|---|---|
| analyst | 2 | `omhl list --type gap --where work_item=work-items.<id>` |
| analyst | 2 | `omhl list --type guideline` |
| analyst | 2 | `omhl list --type premise --where work_item=work-items.<id>` |
| architect | 5, 7 | `omhl list --type decision --where work_item=work-items.<id>` |
| architect | 5, 7 | `omhl list --type guideline` |
| architect | 5, 7 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| code-reviewer | 9 | `omhl list --type decision --where work_item=work-items.<id>` |
| code-reviewer | 9 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| code-reviewer | 9 | `omhl list --type guideline` |
| code-reviewer | 9 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| critic | 6, 7 | `omhl list --type decision --where work_item=work-items.<id>` |
| critic | 3 | `omhl list --type gap --where work_item=work-items.<id> --where kind=technical` |
| critic | 6, 7 | `omhl list --type guideline` |
| critic | 3 | `omhl list --type premise --where work_item=work-items.<id> --where kind=technical --where status=assumed` |
| critic | 6, 7 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| planner | 4 | `omhl list --type gap --where work_item=work-items.<id>` |
| planner | 4 | `omhl list --type guideline` |
| planner | 4 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| planner | 4 | `omhl list --type work-item` |
| security-reviewer | 9 | `omhl list --type decision --where work_item=work-items.<id>` |
| security-reviewer | 9 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| security-reviewer | 9 | `omhl list --type guideline` |
| security-reviewer | 9 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| sessao | review 2 | `omhl list` |
| sessao | plan-consensus 5, review 6 | `omhl list --type decision --where work_item=work-items.<id>` |
| sessao | review 6 | `omhl list --type decision --where work_item=work-items.<id> --format json` |
| sessao | review 1, review 2, review 3, review 5 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| sessao | plan-consensus 1 | `omhl list --type gap --where work_item=work-items.<id>` |
| sessao | review 6 | `omhl list --type premise --where work_item=work-items.<id> --format json` |
| sessao | interview 3, plan-consensus 2, review 4 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |
| verifier | 9 | `omhl list --type divergence --where work_item=work-items.<id> --where resolution=open` |
| verifier | 8, 9 | `omhl list --type guideline` |
| verifier | 9 | `omhl list --type premise --where work_item=work-items.<id> --where status=confirmed` |

## Quando usar

Para achar nós no banco quando você não sabe o id: as premissas de um work item, as divergências abertas, os checks, as diretivas. O `list` devolve a revisão vigente de cada nó, em ordem de gravação, página a página. Se você já sabe o id, use `omhl get`. É somente leitura.

O `list` **respeita o pedido ou o recusa**: um filtro que nunca casaria nada (tipo no plural, campo que não existe, valor fora dos permitidos) vem como erro com a regra, nunca como `✔ 0 nodes`. Por isso um `✔ 0 nodes` (`{"items":[]}` no `json`) é resposta vazia verdadeira: nenhum nó casa. Para consultar as divergências abertas de um work item:

```omhl-example fixture=project
$ omhl list --type divergence --where work_item=work-items.w1 --where resolution=open
✔ 0 nodes
```

## Entradas

Todas as opções são opcionais. Sem nenhuma, lista os nós de todos os tipos.

- `--type <tipo>`: um tipo do catálogo, **no singular**: `check`, `guideline`, `premise`, `decision`, `gap`, `divergence`, `attestation`, `work-item` ou `pull-request`. Um tipo que não existe no catálogo é recusado (`premises` no plural recusa).
- `--id-prefix <prefixo>`: filtra pelo começo do id. Regras:
  - é uma sequência de nomes separados por `.`; cada nome tem de 1 a 63 caracteres, só com letras minúsculas, dígitos e hífens, e não começa por hífen; o prefixo todo tem até 200 caracteres;
  - **casa por segmento inteiro**: `premises.p1` casa `premises.p1`, mas `premises.p` não casa `premises.p1` (e `premises.cli1` não casa `premises.cli1-b1`);
  - **começa por um namespace de tipo**: `premises`, `guidelines`, `checks`, `decisions`, `gaps`, `divergences`, `attestations` ou `work-items` (`premise` no singular recusa). Com `--type`, o namespace é o do tipo (`work-items` serve a `work-item` e `pull-request`).
- `--where <CAMPO=VALOR>`: filtra por igualdade de um campo, repetível; todos os pares precisam bater. Regras:
  - o comando divide no **primeiro** `=`, então o valor pode conter `=`; um par sem `=` ou com o campo vazio é recusado;
  - o campo precisa ser de **texto**: o schema do campo tem `type: "string"`, ou não tem `type` e todo valor do `enum` é string (é o caso de `status`, `kind`, `resolution`, `concern` e `outcome`). Campo `boolean`, `integer`, `array` ou `object` não vale: `--where blocking=true` recusa;
  - se o campo tem `enum`, o valor precisa ser um dos permitidos (`status=confimed` recusa);
  - **um valor por campo**: o mesmo campo com valores diferentes recusa, e essa checagem vem antes da do `enum`: `--where status=confimed --where status=open` recusa o segundo par por repetição, não o primeiro por valor fora do `enum`; o mesmo par repetido segue aceito;
  - o campo precisa existir no tipo (com `--type`) ou em algum tipo candidato (veja abaixo);
  - no máximo 8 pares, e o valor tem até 256 caracteres.
- **Tipos candidatos (sem `--type`).** Os candidatos são os tipos que o namespace do `--id-prefix` admite, ou os nove do catálogo se não há prefixo. O filtro vale se **algum candidato** declara todos os campos do `--where` como texto e aceita os valores; senão é recusado em `/where/<índice>`. Por isso `--where status=open` funciona sem `--type`, e `--where title=x --where kind=missing` recusa (nenhum tipo declara os dois). Como nenhum tipo tem mais de seis campos de texto, mais de seis campos distintos nunca passam.
- `--limit <n>`: o tamanho da página, de **1 a 200**; sem a opção, 50. `0` e acima de 200 são recusados.
- `--after-seq <n>`: o cursor da página: devolve os nós com `seq` maior que `n`. Use o `next_after_seq` da página anterior. Um valor além da cabeça do banco devolve a página vazia, que é resposta verdadeira.
- `--db <caminho>`: o arquivo do banco. Um caminho novo **cria o banco**; o caminho precisa ficar fora da pasta do projeto.
- `--project-dir <pasta>`: onde procurar o marcador `.omhl.toml`. A pasta precisa existir.
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. O padrão é `agent`; `human` mostra os mesmos fatos em blocos; `json` é a saída para máquinas.

## Saída

Texto no stdout, saída 0. A primeira linha resume a página: `✔ N nodes · seq A–B` (a faixa de `seq` da página; `✔ 0 nodes` quando nenhum nó casa). Depois vem uma linha por nó, com o id inteiro (nunca encurtado), o tipo, o `seq` e os campos do tipo:

- `check`: o evento, `blocking` ou `non-blocking` e `concern/variant`;
- `attestation`: o desfecho, o check e o work item;
- `premise`: o `status`, o `kind`, o work item e o texto;
- `work-item` e `pull-request`: o `status` e o título;
- `guideline`: o `kind` e o texto;
- `divergence`: a `resolution` e o texto;
- `decision`: o texto; `gap`: a pergunta.

Um nó sobreposto por um check nativo de mesmo id leva a palavra `shadowed` depois do `seq`. O resto de `data` (evidências, `confirmed_by`, `run`, `expect`) e o autor, o `at` e o `record_id` ficam de fora: leia o nó inteiro com `omhl get <id>`. Quando há mais nós depois da página, a última linha é `Next: omhl list --after-seq N with the same filters for more. One node in full: omhl get <id>.`; repita o comando com `--after-seq N` **e os mesmos filtros**. Sem `Next:`, a página é a última. Com `--format human` os mesmos fatos saem em blocos, um por nó: o id numa linha e, abaixo, `type`, `seq`, os campos do tipo e `of` (o work item) com os rótulos alinhados.

Com `--format json` a saída é uma linha: `{"items": [...]}`. Cada item traz `id`, `type`, `record_id`, `seq`, `at`, `data` (os campos do nó), `author` (`agent` e `client`) e `shadowed` (`true` quando um check nativo de mesmo id o sobrepõe), mais `snapshot` e `external_id` quando o nó os tem. Quando há mais nós depois da página, a resposta traz `next_after_seq`; repita o comando com `--after-seq` igual a esse valor. Sem `next_after_seq`, a página é a última.

## Erros

Um erro sai no stderr, em texto (`✘ código · mensagem`; com `--format json`, numa linha JSON), com saída 1. Uma opção desconhecida ou um valor que o clap não lê (`--limit abc`) é erro de uso do clap, com saída 2 e sem código. A mensagem de cada recusa diz a regra e não repete o valor recebido. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

- `invalid-arguments` em `/limit` (saída 1): o `--limit` é `0` ou passa de 200. Use um valor de 1 a 200.
- `invalid-arguments` em `/type` (saída 1): o `--type` não existe no catálogo. Use o nome no singular, da lista acima.
- `invalid-arguments` em `/id_prefix` (saída 1): o prefixo tem um segmento vazio, com caractere fora de letra minúscula, dígito e hífen, começando por `-` ou acima de 63 caracteres, passa de 200 caracteres no total, não começa por um namespace de tipo, ou está fora do namespace do `--type`. Corrija o prefixo.
- `invalid-arguments` em `/where/<índice>` (saída 1): o par `--where` de posição `<índice>` viola uma regra: sem `=` ou com campo vazio; campo que o tipo não declara; campo que não é de texto; valor fora do `enum`; campo repetido com valores diferentes (o ponteiro é o do segundo par); ou, sem `--type`, nenhum tipo candidato o satisfaz. Corrija o par.
- `where-eq-too-many` (saída 1): o `--where` tem mais de 8 pares (só passam pares idênticos repetidos, pois um tipo não declara tantos campos de texto). Reduza os pares.
- `where-eq-value-too-long` (saída 1): o valor de um `--where` passa de 256 caracteres. Use um valor menor.
- `no-marker` (saída 1): não há `.omhl.toml` na pasta nem acima dela. Rode dentro do projeto; numa branch anterior ao marcador, rebaseie sobre a `main`.
- `db-inside-project` (saída 1): o `--db` aponta para dentro do projeto. Use um caminho fora dele.
- `invalid-project-dir` (saída 1): o `--project-dir` não existe. A CLI não sobe até um projeto de um diretório acima: corrija o caminho.

## Exemplos

Lista tudo. A fixture tem um work item e uma premissa; com `--format json` a mesma resposta sai para máquinas:

```omhl-example fixture=project
$ omhl list
✔ 2 nodes · seq 1–2
  work-items.w1 · work-item · seq 1 · open · First item
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --format json
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"status":"open","title":"First item"},"id":"work-items.w1","record_id":"<record-id>","seq":1,"shadowed":false,"type":"work-item"},{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
```

Filtra por tipo e por um campo de texto, e filtra só por um campo sem `--type`:

```omhl-example fixture=project
$ omhl list --type premise --where status=confirmed
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --where status=open
✔ 1 node · seq 1
  work-items.w1 · work-item · seq 1 · open · First item
```

Pagina: a primeira página traz `next_after_seq`; a segunda, não. Um `--after-seq` além da cabeça é a página vazia verdadeira:

```omhl-example fixture=project
$ omhl list --limit 1
✔ 1 node · seq 1
  work-items.w1 · work-item · seq 1 · open · First item
Next: omhl list --after-seq 1 with the same filters for more. One node in full: omhl get <id>.
$ omhl list --limit 1 --after-seq 1
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --after-seq 99
✔ 0 nodes
```

Com `--format human` os mesmos fatos saem em blocos, um por nó:

```omhl-example fixture=project
$ omhl list --format human
✔ 2 nodes · seq 1–2

  work-items.w1
    type:   work-item
    seq:    1
    status: open
    title:  First item

  premises.p1
    type:   premise
    seq:    2
    status: confirmed
    kind:   business
    of:     work-items.w1
    text:   Money is stored in cents.
```

O prefixo casa por segmento inteiro: `premises.p1` casa, `premises.p` não:

```omhl-example fixture=project
$ omhl list --id-prefix premises.p1
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
$ omhl list --id-prefix premises.p
✔ 0 nodes
```

Recusa: `--limit` fora de 1 a 200:

```omhl-example fixture=project
$ omhl list --limit 0
2> ✘ invalid-arguments · limit must be between 1 and 200
2>   - /limit
# exit 1
$ omhl list --limit 201
2> ✘ invalid-arguments · limit must be between 1 and 200
2>   - /limit
# exit 1
```

Recusa: `--type` que não existe no catálogo (o plural recusa):

```omhl-example fixture=project
$ omhl list --type premises
2> ✘ invalid-arguments · type is not defined in the catalog
2>   - /type
# exit 1
```

Recusa: `--id-prefix` que não começa por um namespace de tipo, ou que sai do namespace do `--type`:

```omhl-example fixture=project
$ omhl list --id-prefix premise
2> ✘ invalid-arguments · id prefix must start with the namespace of a type
2>   - /id_prefix
# exit 1
$ omhl list --type premise --id-prefix work-items
2> ✘ invalid-arguments · id prefix is outside the namespace of the type
2>   - /id_prefix
# exit 1
```

Recusa: `--id-prefix` com segmento inválido (vazio, começando por `-`, acima de 63 caracteres):

```omhl-example fixture=project
$ omhl list --id-prefix premises..x
2> ✘ invalid-arguments · id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total
2>   - /id_prefix
# exit 1
$ omhl list --id-prefix premises.-x
2> ✘ invalid-arguments · id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total
2>   - /id_prefix
# exit 1
$ omhl list --id-prefix premises.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
2> ✘ invalid-arguments · id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total
2>   - /id_prefix
# exit 1
```

Recusa: com `--type`, campo que o tipo não declara, campo que não é de texto e valor fora do `enum`:

```omhl-example fixture=project
$ omhl list --type premise --where blocking=true
2> ✘ invalid-arguments · field is not declared by the type
2>   - /where/0
# exit 1
$ omhl list --type check --where blocking=true
2> ✘ invalid-arguments · field is not a text field; where compares text only
2>   - /where/0
# exit 1
$ omhl list --type premise --where status=confimed
2> ✘ invalid-arguments · value is not one of the values the field allows
2>   - /where/0
# exit 1
```

Recusa: o mesmo campo com valores diferentes. O mesmo par repetido segue aceito:

```omhl-example fixture=project
$ omhl list --type premise --where status=confirmed --where status=assumed
2> ✘ invalid-arguments · field repeated in where with different values
2>   - /where/1
# exit 1
$ omhl list --type premise --where status=confirmed --where status=confirmed
✔ 1 node · seq 2
  premises.p1 · premise · seq 2 · confirmed · business · work-items.w1 · Money is stored in cents.
```

Recusa: sem `--type`, nenhum tipo declara todos os campos do `--where`:

```omhl-example fixture=project
$ omhl list --where title=x --where kind=missing
2> ✘ invalid-arguments · no type declares every where field
2>   - /where/1
# exit 1
```

Recusa: `--where` malformado (sem `=` ou com o campo vazio):

```omhl-example fixture=project
$ omhl list --where foo
2> ✘ invalid-arguments · where must be FIELD=VALUE with a non-empty FIELD
2>   - /where/0
# exit 1
$ omhl list --where =x
2> ✘ invalid-arguments · where must be FIELD=VALUE with a non-empty FIELD
2>   - /where/0
# exit 1
```

Recusa: `--db` dentro da pasta do projeto:

```omhl-example fixture=project
$ omhl list --db novo.db
2> ✘ db-inside-project · the database folder is inside the project; its blobs/ folder would dirty the attested tree
# exit 1
```
