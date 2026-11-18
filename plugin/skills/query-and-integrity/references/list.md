# omhl list

- [Quando usar](#quando-usar)
- [Entradas](#entradas)
- [Saída](#saída)
- [Erros](#erros)
- [Exemplos](#exemplos)

## Quando usar

Para achar nós no banco quando você não sabe o id: as premissas de um work item, as divergências abertas, os checks, as diretivas. O `list` devolve a revisão vigente de cada nó, em ordem de gravação, página a página. Se você já sabe o id, use `omhl get`. É somente leitura.

O `list` **respeita o pedido ou o recusa**: um filtro que nunca casaria nada (tipo no plural, campo que não existe, valor fora dos permitidos) vem como erro com a regra, nunca como `{"items":[]}`. Por isso um `{"items":[]}` é resposta vazia verdadeira: nenhum nó casa. Para consultar as divergências abertas de um work item:

```omhl-example fixture=project
$ omhl list --type divergence --where work_item=work-items.w1 --where resolution=open
{"items":[]}
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
- `--format <agent|human|json>`: opção global, vale antes ou depois do subcomando. Por ora este comando só imprime `json`, o padrão; `agent` e `human` saem com erro de uso (saída 2).

## Saída

Uma linha JSON no stdout, saída 0: `{"items": [...]}`. Cada item traz `id`, `type`, `record_id`, `seq`, `at`, `data` (os campos do nó), `author` (`agent` e `client`) e `shadowed` (`true` quando um check nativo de mesmo id o sobrepõe), mais `snapshot` e `external_id` quando o nó os tem. Quando há mais nós depois da página, a resposta traz `next_after_seq`; repita o comando com `--after-seq` igual a esse valor. Sem `next_after_seq`, a página é a última.

## Erros

Um erro sai no stderr, numa linha JSON, com saída 1. Uma opção desconhecida ou um valor que o clap não lê (`--limit abc`) é erro de uso do clap, com saída 2 e sem código. A mensagem de cada recusa diz a regra e não repete o valor recebido. Os erros comuns a todos os comandos estão em [errors.md](errors.md).

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

Lista tudo. A fixture tem um work item e uma premissa:

```omhl-example fixture=project
$ omhl list
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"status":"open","title":"First item"},"id":"work-items.w1","record_id":"<record-id>","seq":1,"shadowed":false,"type":"work-item"},{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
```

Filtra por tipo e por um campo de texto, e filtra só por um campo sem `--type`:

```omhl-example fixture=project
$ omhl list --type premise --where status=confirmed
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
$ omhl list --where status=open
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"status":"open","title":"First item"},"id":"work-items.w1","record_id":"<record-id>","seq":1,"shadowed":false,"type":"work-item"}]}
```

Pagina: a primeira página traz `next_after_seq`; a segunda, não. Um `--after-seq` além da cabeça é a página vazia verdadeira:

```omhl-example fixture=project
$ omhl list --limit 1
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"status":"open","title":"First item"},"id":"work-items.w1","record_id":"<record-id>","seq":1,"shadowed":false,"type":"work-item"}],"next_after_seq":1}
$ omhl list --limit 1 --after-seq 1
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
$ omhl list --after-seq 99
{"items":[]}
```

O prefixo casa por segmento inteiro: `premises.p1` casa, `premises.p` não:

```omhl-example fixture=project
$ omhl list --id-prefix premises.p1
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
$ omhl list --id-prefix premises.p
{"items":[]}
```

Recusa (I11): `--limit` fora de 1 a 200:

```omhl-example fixture=project
$ omhl list --limit 0
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"limit must be between 1 and 200","path":"/limit"}],"message":"limit must be between 1 and 200"}}
# exit 1
$ omhl list --limit 201
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"limit must be between 1 and 200","path":"/limit"}],"message":"limit must be between 1 and 200"}}
# exit 1
```

Recusa (I6): `--type` que não existe no catálogo (o plural recusa):

```omhl-example fixture=project
$ omhl list --type premises
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"type is not defined in the catalog","path":"/type"}],"message":"type is not defined in the catalog"}}
# exit 1
```

Recusa (I12): `--id-prefix` que não começa por um namespace de tipo, ou que sai do namespace do `--type`:

```omhl-example fixture=project
$ omhl list --id-prefix premise
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"id prefix must start with the namespace of a type","path":"/id_prefix"}],"message":"id prefix must start with the namespace of a type"}}
# exit 1
$ omhl list --type premise --id-prefix work-items
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"id prefix is outside the namespace of the type","path":"/id_prefix"}],"message":"id prefix is outside the namespace of the type"}}
# exit 1
```

Recusa (I3): `--id-prefix` com segmento inválido (vazio, começando por `-`, acima de 63 caracteres):

```omhl-example fixture=project
$ omhl list --id-prefix premises..x
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total","path":"/id_prefix"}],"message":"id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total"}}
# exit 1
$ omhl list --id-prefix premises.-x
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total","path":"/id_prefix"}],"message":"id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total"}}
# exit 1
$ omhl list --id-prefix premises.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total","path":"/id_prefix"}],"message":"id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total"}}
# exit 1
```

Recusa (I4): com `--type`, campo que o tipo não declara, campo que não é de texto e valor fora do `enum`:

```omhl-example fixture=project
$ omhl list --type premise --where blocking=true
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"field is not declared by the type","path":"/where/0"}],"message":"field is not declared by the type"}}
# exit 1
$ omhl list --type check --where blocking=true
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"field is not a text field; where compares text only","path":"/where/0"}],"message":"field is not a text field; where compares text only"}}
# exit 1
$ omhl list --type premise --where status=confimed
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"value is not one of the values the field allows","path":"/where/0"}],"message":"value is not one of the values the field allows"}}
# exit 1
```

Recusa (I5): o mesmo campo com valores diferentes. O mesmo par repetido segue aceito:

```omhl-example fixture=project
$ omhl list --type premise --where status=confirmed --where status=assumed
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"field repeated in where with different values","path":"/where/1"}],"message":"field repeated in where with different values"}}
# exit 1
$ omhl list --type premise --where status=confirmed --where status=confirmed
{"items":[{"at":"<at>","author":{"agent":"claude","client":"cli"},"data":{"confirmed_by":{"answer":"In cents.","by":"person:ana","question":"How is money stored?"},"kind":"business","status":"confirmed","text":"Money is stored in cents.","work_item":"work-items.w1"},"id":"premises.p1","record_id":"<record-id>","seq":2,"shadowed":false,"type":"premise"}]}
```

Recusa (I10): sem `--type`, nenhum tipo declara todos os campos do `--where`:

```omhl-example fixture=project
$ omhl list --where title=x --where kind=missing
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"no type declares every where field","path":"/where/1"}],"message":"no type declares every where field"}}
# exit 1
```

Recusa: `--where` malformado (sem `=` ou com o campo vazio):

```omhl-example fixture=project
$ omhl list --where foo
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"where must be FIELD=VALUE with a non-empty FIELD","path":"/where/0"}],"message":"where must be FIELD=VALUE with a non-empty FIELD"}}
# exit 1
$ omhl list --where =x
2> {"error":{"code":"invalid-arguments","details":[{"code":"invalid-arguments","message":"where must be FIELD=VALUE with a non-empty FIELD","path":"/where/0"}],"message":"where must be FIELD=VALUE with a non-empty FIELD"}}
# exit 1
```

Recusa: `--db` dentro da pasta do projeto:

```omhl-example fixture=project
$ omhl list --db novo.db
2> {"error":{"code":"db-inside-project","details":[],"message":"the database folder is inside the project; its blobs/ folder would dirty the attested tree"}}
# exit 1
```
