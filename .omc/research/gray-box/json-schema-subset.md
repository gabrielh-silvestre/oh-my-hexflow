# Validador próprio de um subconjunto de JSON Schema 2020-12

Relatório para o `test-engineer` e o `executor` da T3 implementarem `catalog/validate.rs` sem refazer a pesquisa. Consulta em 2026-11-07. Contexto: `jsonschema` saiu do plano (833 estrelas, ver [catalog-schema.md](catalog-schema.md)); o `hexlog` valida `data` com código próprio, lista branca de palavras-chave e avaliação sem `Arc`, `Rc`, `RefCell` nem `Mutex`. Onde o texto diz "proposta", a decisão é minha e está listada em "Pontos em aberto"; onde diz "spec", há seção citada.

## Tabela de Conteúdos

- [Subconjunto e lista branca](#subconjunto-e-lista-branca)
- [Semântica de cada palavra-chave](#semântica-de-cada-palavra-chave)
- [Referências, defs e ponteiros JSON](#referências-defs-e-ponteiros-json)
- [Ciclos, profundidade e custo](#ciclos-profundidade-e-custo)
- [Saída de erro](#saída-de-erro)
- [Suíte oficial de testes](#suíte-oficial-de-testes)
- [Algoritmo proposto](#algoritmo-proposto)
- [Armadilhas e testes próprios](#armadilhas-e-testes-próprios)
- [Pontos em aberto](#pontos-em-aberto)
- [Links Úteis](#links-úteis)

---

## [Subconjunto e lista branca](https://json-schema.org/draft/2020-12/json-schema-core)

### O que é?
A lista fechada de palavras-chave que `Catalog::new` aceita num schema. Qualquer outra palavra num lugar onde cabe schema vira `INVALID_SCHEMA`. A spec manda tratar palavra desconhecida como anotação (Core §4.3.1 e §6.5); o `hexlog` desvia de propósito, para um erro de digitação (`"requred"`) não virar schema que aceita tudo em silêncio. Esse desvio vai para o ADR.

### Lista final (proposta)

| palavra-chave | decisão | spec |
|---|---|---|
| `type` | aceita (string ou array de strings únicas) | Validation §6.1.1 |
| `enum`, `const` | aceita | Validation §6.1.2, §6.1.3 |
| `minimum`, `maximum` | aceita (inteiro seguro) | Validation §6.2.4, §6.2.2 |
| `minLength`, `maxLength` | aceita | Validation §6.3.2, §6.3.1 |
| `minItems`, `maxItems` | aceita | Validation §6.4.2, §6.4.1 |
| `required` | aceita | Validation §6.5.3 |
| `properties`, `additionalProperties` | aceita | Core §10.3.2.1, §10.3.2.3 |
| `items` (sem `prefixItems`) | aceita | Core §10.3.1.2 |
| `allOf`, `anyOf`, `oneOf`, `not` | aceita | Core §10.2.1.1 a §10.2.1.4 |
| `$ref` (só `#` e `#/...`), `$defs` | aceita | Core §8.2.3.1, §8.2.4 |
| `$schema` | aceita só na raiz e só com o URI 2020-12 | Core §8.1.1 |
| `title`, `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly`, `$comment` | aceita e ignora (anotação); o tipo do valor é conferido | Validation §9.1 a §9.5; Core §8.3 |

Anotações: a spec diz que elas não influenciam a validação (Core §7.7) e que `$comment` não pode ser interpretado (Core §13). Conferir o tipo do valor (`title` string, `deprecated` boolean, `examples` array) imita o que o metaschema faria; `default` e `examples` não são validados contra o schema (a spec só recomenda, Validation §9.2 e §9.5).

### Fora do subconjunto, e por quê
- **`pattern`, `patternProperties`:** regra do dono (plano, linhas 563 e 676); detalhe `pattern-not-allowed`.
- **`$id`, `$anchor`, `$dynamicRef`, `$dynamicAnchor`, `$vocabulary`, `$async`, `$ref` não interno:** resolução de URI e escopo dinâmico. `$dynamicRef` é a origem da complexidade PSPACE da validação (Attouche et al., 2024). `$async` não é palavra do 2020-12.
- **`unevaluatedProperties`, `unevaluatedItems`:** exigem coleta de anotações entre subschemas (Core §11); custo e superfície de erro sem demanda.
- **`prefixItems`, `contains`, `minContains`, `maxContains`, `propertyNames`, `dependentRequired`, `dependentSchemas`, `if`/`then`/`else`, `uniqueItems`, `minProperties`, `maxProperties`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`:** sem demanda nos tipos do `hexlog`. Todos entram depois sem mudar o desenho (cada um é uma regra local), mas só com pedido do dono.
- **`format`, `content*`:** anotação por padrão no 2020-12 (Validation §7.2.1); recusar evita falsa sensação de validação.

Contagem na suíte oficial (ocorrências que derrubam grupo, não grupos): `$id` 70, `$ref` não interno 61, `unevaluated*` 78, `prefixItems` 38, `contains` 21, `if` 20, `format` 19, `$dynamicAnchor` 17, `patternProperties` 13.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Semântica de cada palavra-chave](https://json-schema.org/draft/2020-12/json-schema-validation)

### O que é?
O comportamento exato que o avaliador implementa, com a seção que o justifica. A regra geral: uma asserção sobre um tipo só restringe esse tipo; instância de outro tipo passa (Core §7.6.1).

### Como aplicar?

- **`type`** (Validation §6.1.1). Valor string ou array de strings únicas entre `null`, `boolean`, `object`, `array`, `number`, `string`, `integer`. Com a guarda de inteiro seguro rodando antes, todo número em `data` é inteiro; então `integer` e `number` casam os mesmos valores. A spec define `integer` como número "com parte fracionária zero" (Validation §6.1.1; Core §6.3 e §4.2.1); a guarda do plano recusa `1.0` e `1e2` antes da avaliação, e a suíte oficial tem testes que esperam `1.0` como `integer` (ver [Suíte oficial de testes](#suíte-oficial-de-testes)), que por isso ficam fora.
- **`enum`, `const`** (Validation §6.1.2, §6.1.3). Igualdade da spec (Core §4.2.2): mesmo tipo e mesmo valor; strings ponto de código a ponto de código; arrays item a item em ordem; objetos sem ordem, mesmas chaves. `serde_json::Value::eq` já faz isso (testado: objetos com chaves em ordem diferente são iguais). Atenção: `Number` do `serde_json` não é igual matematicamente entre inteiro e float (`1 == 1.0` dá `false`, testado); isso só é seguro porque a guarda e a regra de números no schema eliminam floats dos dois lados. `enum` vazio é aceito (Validation §6.1.2 só diz "SHOULD have at least one element") e nunca casa. `const` aceita qualquer JSON, `null` incluído.
- **`minimum`, `maximum`** (Validation §6.2.4, §6.2.2). Inclusivos. Valor no schema: inteiro seguro (proposta). Comparação em `i64`.
- **`minLength`, `maxLength`** (Validation §6.3). Valor: inteiro não negativo. Comprimento é o número de "caracteres conforme o RFC 8259", isto é, pontos de código Unicode: `s.chars().count()`. Não é `len()` (bytes) nem UTF-16: `"💩"` tem 1 caractere, 4 bytes e 2 unidades UTF-16 (testado). `serde_json` recusa surrogate solta (`"\ud800"` dá erro), então o caso do RFC 8259 §8.2 nem chega.
- **`minItems`, `maxItems`** (Validation §6.4). Inteiro não negativo; só arrays.
- **`required`** (Validation §6.5.3). Array de strings únicas; só objetos. Chave presente com valor `null` conta como presente.
- **`properties`** (Core §10.3.2.1). Para cada nome que aparece no schema e na instância, o valor filho valida contra o subschema. Os nomes são dados, não palavras-chave: `{"properties":{"pattern":{...},"$ref":{...}}}` é um schema legal e não pode ser recusado.
- **`additionalProperties`** (Core §10.3.2.3). Aplica-se às chaves da instância que não estão em `properties` **do mesmo objeto de schema**; sem `patternProperties` no subconjunto, a conta é só essa. Não olha para dentro de `allOf` nem `$ref` (a suíte tem um teste para isso). Quando o valor é o literal `false`, a violação sai no ponteiro da chave excedente (exemplo da spec: `/1/z`, Core §12.4.2).
- **`items`** (Core §10.3.1.2). Sem `prefixItems`, aplica-se a todos os elementos. `items: false` com array vazio passa.
- **`allOf`, `anyOf`, `oneOf`** (Core §10.2.1). Array não vazio de schemas. `oneOf` passa com exatamente um ramo válido (mais de um falha). Os ramos são independentes e podem ser avaliados em qualquer ordem (Core §10.2).
- **`not`** (Core §10.2.1.4). Falha quando o subschema passa.
- **Schemas booleanos** (Core §4.3.2). `true` equivale a `{}` e `false` a `{"not":{}}`. Valem em qualquer posição de subschema; a **raiz**, porém, tem de ser objeto com `type` igual à string `"object"` (plano, linha 563).
- **Palavra ausente** nunca falha (Core §7.3).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Referências, defs e ponteiros JSON](https://json-schema.org/draft/2020-12/json-schema-core)

### O que é?
`$ref` aponta para outro schema do mesmo documento por fragmento JSON Pointer; `$defs` é só um lugar para guardar schemas reutilizáveis e não valida nada (Core §8.2.4).

### Como aplicar?

- **Irmãos de `$ref` valem.** No 2020-12 `$ref` é um applicator comum: as palavras ao lado dele são avaliadas também (Core §8.2.3.1; a suíte tem `ref applies alongside sibling keywords`). Quem vem do draft-7 ignora os irmãos; aqui isso é bug.
- **Formas aceitas:** `#` (raiz) e `#/...`. Tudo que não começa por `#` dá `ref-not-internal` (plano, linha 677). `#nome` (âncora) dá erro: o Core §5 diz que fragmento que não casa a sintaxe de JSON Pointer é nome simples, e âncora está fora.
- **Decodificação do fragmento**, nesta ordem (RFC 6901 §6 e §4): (1) percent-decoding em UTF-8 (`foo%22bar` vira `foo"bar`; bytes que não formam UTF-8 válido são erro); (2) separar por `/`; (3) em cada token, `~1` vira `/` e depois `~0` vira `~`. A ordem importa: `~01` tem de virar `~1`, não `/` (RFC 6901 §4). `~` seguido de outro dígito que não 0 ou 1 é erro de sintaxe (`ref-invalid-pointer`).
- **Tokens vazios contam:** `#/$defs//$defs/` tem os tokens `$defs`, ``, `$defs`, `` (a suíte tem esse caso). Não aparar barras.
- **Índice de array** no ponteiro: `0` ou dígitos sem zero à esquerda (RFC 6901, ABNF `array-index`). Serve para `#/allOf/1`; `-` nunca resolve.
- **Alvo tem de ser um schema.** Apontar para dentro de `enum`, para o mapa `properties` inteiro ou para `$defs` inteiro é comportamento indefinido na spec (Core §9.4.2); o `hexlog` recusa (`ref-unresolved`). Na prática: durante o build registra-se cada posição de schema em `HashMap<String, NodeId>` com o ponteiro canônico (tokens re-escapados com `~0` e `~1`), e o `$ref` só resolve para uma chave desse mapa.
- **Referência para frente** funciona: dois passes (build, depois resolução).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Ciclos, profundidade e custo](https://json-schema.org/draft/2020-12/json-schema-core#name-guarding-against-infinite-r)

### O que é?
Três riscos distintos: laço que nunca termina, explosão exponencial de um schema que compartilha subschemas, e estouro de pilha por recursão funda.

### Laço de `$ref` (termina?)
A spec proíbe o laço, mas deixa o comportamento "indefinido" (Core §9.4.1; §13: "Validators MUST NOT fall into an infinite loop"). A formalização é a de **schema bem formado** (Pezoa et al., 2016; Bourhis et al., 2017, §5.3; Attouche et al., 2024, Def. 2): monta-se um grafo em que `A -> B` quando `B` é aplicado à **mesma posição da instância** que `A` (via `$ref`, `allOf`, `anyOf`, `oneOf`, `not`), e o schema é bem formado se esse grafo é acíclico. Ciclo que passa por `properties`, `additionalProperties` ou `items` é legítimo e termina, porque cada passo desce um nível da instância (que é finita).

| schema | resultado |
|---|---|
| `{"$ref":"#"}` | recusa `ref-cycle` |
| `{"allOf":[{"$ref":"#"}]}` | recusa `ref-cycle` |
| `{"not":{"$ref":"#"}}` | recusa `ref-cycle` (é `γ = ¬γ`, exemplo 3 de Bourhis et al.) |
| `{"$defs":{"a":{"$ref":"#/$defs/b"},"b":{"$ref":"#/$defs/a"}}}` | recusa `ref-cycle` |
| `{"properties":{"child":{"$ref":"#"}}}` | aceita (árvore) |

O critério é conservador: ciclo inalcançável a partir da raiz também é recusado (Attouche et al. observam o mesmo). Detecção sem estourar a pilha: DFS **iterativa** com três cores (branco, cinza, preto) e pilha explícita em `Vec`; achar um nó cinza é ciclo.

### Explosão exponencial (termina rápido?)
Validação de JSON Schema "clássico" (2019-09 em diante sem `$dynamicRef`) é PTIME-completa (Pezoa et al., 2016, e Bourhis et al., 2017, §5.2, citados por Attouche et al.); com `$dynamicRef` passa a PSPACE-completa (Attouche et al., 2024), e `$dynamicRef` está fora do subconjunto. Mas o resultado PTIME vale para um algoritmo com **memoização**; o ingênuo é exponencial. Attouche et al. (§10) mediram validadores existentes e todos ficaram exponenciais numa família de schemas só com `$ref` estático. Exemplo que quebra o ingênuo: `$defs` com `d0..dn`, cada `di = {"allOf":[{"$ref":"#/$defs/d(i-1)"},{"$ref":"#/$defs/d(i-1)"}]}` custa `2^n`. O caso com descida é pior: `a = {"properties":{"x":{"allOf":[A,A]}}}` onde `A` contém `a` multiplica por dois a cada nível da instância.

Cobertura: memoizar o resultado booleano por par `(NodeId, endereço do &Value)` dentro de uma única chamada. Como o subconjunto não depende de anotações (`additionalProperties` olha só os irmãos), o resultado de `(nó, posição)` não depende de contexto, e o total fica em O(nós × nós da instância).

### Profundidade (pilha)
- `serde_json::from_str` recusa aninhamento acima do limite: medido, array com 128 níveis dá `recursion limit exceeded`, então a instância e o schema têm no máximo 127 níveis quando vêm de texto (127 é dedução; só testei que 128 e 200 falham). Isso só vale para valores que passaram pelo parser; um `Value` construído por código não tem o limite.
- A cadeia "mesma posição" é finita (grafo acíclico), mas pode ser longa: 10.000 `$defs` encadeados por `$ref` têm pouca profundidade de JSON e muita de avaliação. Proposta: contador de profundidade em todo passo de `valid`/`report` com teto de 512 quadros (não medido; o `test-engineer` valida numa thread de 2 MiB); ao estourar, a violação é `max-depth-exceeded` no ponteiro corrente, sem descer mais. O teto de 512 cobre 127 níveis de dado vezes alguns níveis de schema.
- O `Drop` de `serde_json::Value` e a guarda de inteiro seguro também são recursivos ou deveriam ser iterativos; não escrever uma versão recursiva sem limite da guarda.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Saída de erro](https://json-schema.org/draft/2020-12/json-schema-core#name-output-formatting)

### O que é?
O formato "Basic" do Core §12.4.2 é uma lista plana de unidades com `keywordLocation`, `instanceLocation` e `error` (§12.3). O `hexlog` guarda só o necessário: `DataViolation { pointer, code }` (plano, linha 407).

### Mapeamento
- `pointer` = `instanceLocation` (§12.3.3): JSON Pointer da instância, não percent-encoded (só o fragmento de URI é; RFC 6901 §6). Raiz é a string vazia. Montagem: `~` vira `~0` **antes** de `/` virar `~1`; índice de array em decimal.
- `code` = último componente de `keywordLocation` (a palavra-chave que falhou), em kebab-case. `keywordLocation` inteiro, `absoluteKeywordLocation` e a mensagem em texto livre não são guardados. A mensagem nunca ecoa valor de `data` (plano, linha 579), então `code` não carrega valor.
- Sem violação, o resultado é `Ok(())`; o "valid" do Core §12.4 é o próprio `Result`.

### Códigos (proposta)

| palavra-chave | `code` | ponteiro |
|---|---|---|
| `type` | `type` | o valor |
| `enum` | `enum` | o valor |
| `const` | `const` | o valor |
| `minimum`, `maximum` | `minimum`, `maximum` | o valor |
| `minLength`, `maxLength` | `min-length`, `max-length` | a string |
| `minItems`, `maxItems` | `min-items`, `max-items` | o array |
| `required` | `required` | o objeto (spec, §12.4.2) |
| `additionalProperties: false` | `additional-properties` | a chave excedente |
| schema `false` | `false-schema` | o valor |
| `anyOf` | `any-of` | o valor |
| `oneOf` | `one-of` | o valor |
| `not` | `not` | o valor |
| profundidade | `max-depth-exceeded` | o valor |
| corte | `too-many-errors` | `""` |

### O que reportar quando `anyOf` ou `oneOf` falha
Uma única violação no ponteiro do valor, sem descer nos ramos: os erros dos ramos descrevem tentativas que não foram escolhidas, e listá-los é o ruído que o dono quer evitar. `allOf` e `$ref` são transparentes: o erro vem do ramo que falhou, sem violação própria. Os nós "A subschema had errors." do exemplo Basic da spec (§12.4.2) não são emitidos.

### Corte em 50
Ordem determinística (palavras-chave na ordem da tabela, `properties` na ordem do `Map` que, sem `preserve_order`, é alfabética, arrays por índice); deduplicar pares `(pointer, code)` iguais; parar de coletar ao chegar a 51 e, se passou de 50, devolver o corte mais `too-many-errors`. Quantos itens reais cabem junto do marcador está em "Pontos em aberto".

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [Suíte oficial de testes](https://github.com/json-schema-org/JSON-Schema-Test-Suite)

### O que é?
`json-schema-org/JSON-Schema-Test-Suite`, a suíte independente de linguagem que os validadores usam. Licença MIT (Copyright 2012 Julian Berman). Commit conferido em 2026-11-07 e proposto como pino: **`245a45abc90190c0bff04d97e6d2b541b4bb9f12`** (2026-11-03, `main`). O repositório tem 755 estrelas, mas é fixture e não dependência, então o critério de 1000 estrelas do dono não se aplica.

Caminho: `tests/draft2020-12/*.json` (46 arquivos mais `optional/` e `proposals/`, fora do escopo). Formato: lista de grupos `{description, schema, tests: [{description, data, valid}]}`. A suíte inteira do 2020-12 tem 385 grupos e 1300 testes.

### Quais casos sobram
Filtro (script abaixo): o grupo sobra se todo schema nele usa só o subconjunto, com `$ref` interno que resolve, `$schema` só na raiz e igual ao URI 2020-12, e todo número no schema é inteiro seguro. O **teste** sobra se o `data` não tem número fora da guarda de inteiro seguro (`1.0`, `1e2`, 2^53 ou mais), que `hexlog` recusa antes.

Resultado, medido no commit pino: **21 arquivos, 124 grupos, 392 testes, cerca de 54 KB** filtrados (os 21 arquivos inteiros têm 148 KB).

| arquivo | grupos | testes |
|---|---|---|
| `additionalProperties` | 5 de 9 | 8 de 21 |
| `allOf` | 11 de 12 | 22 de 30 |
| `anyOf` | 8 de 8 | 16 de 18 |
| `boolean_schema` | 2 de 2 | 18 de 18 |
| `const` | 15 de 17 | 37 de 54 |
| `default` | 3 de 3 | 7 de 7 |
| `enum` | 16 de 16 | 45 de 53 |
| `infinite-loop-detection` | 1 de 1 | 2 de 2 |
| `items` | 5 de 10 | 12 de 29 |
| `maxItems`, `minItems` | 1 de 2 cada | 4 de 6 cada |
| `maxLength`, `minLength` | 1 de 2 cada | 5 de 7 cada |
| `maximum`, `minimum` | 1 de 2 cada | 1 de 8 e 5 de 11 |
| `not` | 8 de 9 | 38 de 40 |
| `oneOf` | 11 de 11 | 25 de 27 |
| `properties` | 5 de 6 | 20 de 28 |
| `ref` | 12 de 36 | 30 de 79 |
| `required` | 5 de 5 | 18 de 18 |
| `type` | 11 de 11 | 70 de 80 |

Descartados por usar palavra fora da lista branca: o resto (`$id`, `unevaluated*`, `prefixItems`, `contains`, `if`, `format`, `patternProperties`, `$dynamicRef` e companhia). Descartados pela guarda de números: os testes com `1.0`, `1e2`, floats e inteiros grandes.

### Como vendorizar
1. Copiar `LICENSE` e os 21 arquivos filtrados para `crates/hexlog/tests/fixtures/json-schema-test-suite/` (caminho proposto), mais um `MANIFEST.md` com repositório, SHA pino, data e o filtro.
2. Um teste de integração lê os JSON, compila cada `schema` com `Catalog` e confere `valid` de cada `data`. **Sem skip automático:** um runner que pula grupo quando o `Catalog` recusa o schema esconderia bug (schema do subconjunto recusado por engano vira "pulado"). O fixture já vem filtrado, então qualquer recusa é falha.
3. Para atualizar a suíte: rodar o filtro de novo contra o SHA novo e olhar o diff dos arquivos.

### Script do filtro (Python, usado só para gerar o fixture)
```python
import json, glob, os, urllib.parse
SAFE = 2**53 - 1
ALLOWED = {"type","enum","const","minimum","maximum","minLength","maxLength","minItems",
  "maxItems","required","properties","additionalProperties","items","allOf","anyOf","oneOf",
  "not","$defs","$ref","title","description","default","examples","$comment",
  "deprecated","readOnly","writeOnly","$schema"}
def safe(x):
    if isinstance(x, bool): return True
    if isinstance(x, float): return False
    if isinstance(x, int): return abs(x) <= SAFE
    return True
def nums_ok(v):
    if isinstance(v, dict): return all(nums_ok(x) for x in v.values())
    if isinstance(v, list): return all(nums_ok(x) for x in v)
    return safe(v)
def resolve(root, frag):
    cur = root
    if urllib.parse.unquote(frag) == "": return cur
    for tok in urllib.parse.unquote(frag)[1:].split("/"):
        tok = tok.replace("~1", "/").replace("~0", "~")
        if isinstance(cur, dict) and tok in cur: cur = cur[tok]
        elif isinstance(cur, list) and tok.isdigit() and int(tok) < len(cur): cur = cur[int(tok)]
        else: return None
    return cur
def walk(s, root, top=False):
    if isinstance(s, bool): return True
    if not isinstance(s, dict): return False
    for k, v in s.items():
        if k not in ALLOWED: return False
        if k == "$schema" and (not top or v != "https://json-schema.org/draft/2020-12/schema"): return False
        if k in ("enum", "const") and not nums_ok(v): return False
        if k in ("minimum","maximum","minLength","maxLength","minItems","maxItems") and not safe(v): return False
        if k in ("properties", "$defs") and not all(walk(x, root) for x in v.values()): return False
        if k in ("additionalProperties", "items", "not") and not walk(v, root): return False
        if k in ("allOf", "anyOf", "oneOf") and not (v and all(walk(x, root) for x in v)): return False
        if k == "$ref" and not (isinstance(v, str) and (v == "#" or v.startswith("#/")) and resolve(root, v[1:]) is not None): return False
    return True
```
O script aproxima "alvo é schema" por "resolve para algo"; o validador de verdade recusa alvo que não é posição de schema (ver [Referências, defs e ponteiros JSON](#referências-defs-e-ponteiros-json)), e os grupos assim estão fora do fixture por serem raros. O `test-engineer` deve conferir à mão os 12 grupos de `ref`.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Algoritmo proposto

### O que é?
Duas fases. **Compilação**, que roda em `Catalog::new` e recusa schema inválido. **Avaliação**, que roda em `validate_data(&self, ...)` e só empresta do schema compilado.

### Compilação
1. **Raiz:** tem de ser objeto com `type` igual a `"object"` (string); senão `root-not-object`.
2. **Build (passe 1):** percorrer o schema por **posição**, não por chave. A função `build(value, pointer)` recebe um valor que é um schema (objeto ou booleano); para cada chave faz `match`: palavra da lista branca com checagem do tipo do valor; `properties`, `$defs` (mapas de nome para schema, cujas chaves são nomes e não palavras); `allOf`/`anyOf`/`oneOf` (array não vazio de schemas); `additionalProperties`, `items`, `not` (um schema); `enum`, `const`, `default`, `examples` (dado, nunca percorrido como schema). Chave fora da lista: `pattern`/`patternProperties` dão `pattern-not-allowed`; `$ref` que não é interno `ref-not-internal`; o resto `keyword-not-allowed`. Cada nó ganha um `NodeId` e o ponteiro canônico entra em `HashMap<String, NodeId>`. Coletar todos os problemas (com teto de 50) em vez de parar no primeiro.
3. **Números do schema:** `minimum`, `maximum`, os comprimentos e todo número dentro de `enum` e `const` têm de ser inteiros seguros (proposta); senão `number-not-safe-integer`.
4. **Resolução (passe 2):** cada `$ref` vira `NodeId` pela regra da seção de referências.
5. **Ciclos:** DFS iterativa nas arestas "mesma posição" (`$ref`, `allOf`, `anyOf`, `oneOf`, `not`); ciclo é `ref-cycle`.
6. **Hash:** `def_hash` continua sendo `sha256hex(JCS(definição))` sobre a definição original (plano, linha 563), independente do compilado.

### Estruturas (esboço; sem `Arc`, `Rc`, `RefCell`, `Mutex`)
```rust
type NodeId = u32;

pub struct CompiledSchema { nodes: Vec<Node>, root: NodeId } // Send + Sync por construção

enum Node { Bool(bool), Schema(SchemaNode) }

struct SchemaNode {
    types: Option<u8>,                 // bitmask de tipos
    enum_: Option<Vec<serde_json::Value>>,
    const_: Option<serde_json::Value>,
    minimum: Option<i64>, maximum: Option<i64>,
    min_length: Option<u64>, max_length: Option<u64>,
    min_items: Option<u64>, max_items: Option<u64>,
    required: Vec<String>,
    properties: Vec<(String, NodeId)>, // ordem do Map (alfabética)
    additional: Option<NodeId>,
    items: Option<NodeId>,
    all_of: Vec<NodeId>, any_of: Vec<NodeId>, one_of: Vec<NodeId>,
    not: Option<NodeId>,
    reference: Option<NodeId>,
}
```

### Avaliação
```rust
struct Run<'a> {
    schema: &'a CompiledSchema,
    memo: HashMap<(NodeId, usize), bool>,   // resultado booleano por (nó, endereço do &Value)
    seen: HashSet<(NodeId, usize)>,         // para o relatório não repetir
    out: Vec<DataViolation>,
}

fn valid(&mut self, id: NodeId, v: &Value, depth: u16) -> bool; // memoizada; sem erros
fn report(&mut self, id: NodeId, v: &Value, ptr: &mut String, depth: u16); // só desce no que falha
```
- A chave `usize` é `std::ptr::from_ref(v) as usize`: identidade do nó dentro da chamada. É código seguro (o workspace proíbe `unsafe`) e estável porque `data` está emprestado de forma imutável durante toda a chamada.
- `valid` é a fase A: avalia cada palavra-chave, com curto-circuito (`anyOf` para no primeiro ramo bom; `oneOf` para no segundo). Não produz erro.
- `report` é a fase B: só roda se `valid` deu `false`. Em cada nó repete a avaliação por palavra-chave, mas chama `valid` nos filhos e só desce nos que falham; `anyOf`, `oneOf` e `not` que falham emitem uma violação e **não** descem. Isso mantém o custo polinomial e evita emitir o erro de um ramo de `not` ou de `anyOf` que, no total, passou.
- `ptr` é um `String` com `push`/`truncate` em volta de cada descida (sem alocar um ponteiro novo por nível).
- Parar o relatório ao chegar a 51 violações.
- Precondição: `data` já passou pela guarda de inteiro seguro. A guarda é um passe à parte e iterativo (plano, linha 535).

### Ordem de cada nó
`type`, `enum`, `const`, `minimum`, `maximum`, `min-length`, `max-length`, `min-items`, `max-items`, `required`, `properties`, `additionalProperties`, `items`, `allOf`, `anyOf`, `oneOf`, `not`, `$ref`. A ordem é fixa para a saída ser determinística.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Armadilhas e testes próprios

### O que é?
Os erros mais prováveis, na ordem de perigo, e os testes que a suíte oficial não cobre porque o subconjunto é menor.

### As três mais perigosas
1. **Laço e explosão.** `{"$ref":"#"}` trava; schema com subschema compartilhado fica exponencial sem memoização. Resposta: recusar ciclo "mesma posição" na compilação e memoizar `(nó, posição)`.
2. **Palavra-chave ou nome de propriedade?** Percorrer o schema tratando toda chave como palavra recusa `{"properties":{"pattern":{...}}}` e `{"properties":{"$ref":{...}}}` (a suíte tem os dois) e deixa passar o que estiver numa posição que ninguém visita. `enum`, `const`, `default` e `examples` guardam dado, não schema. Resposta: despachar pela posição, como no passe 1.
3. **Número.** `serde_json` lê `-0` como float, `1E2` como float, e `1 != 1.0` no `PartialEq`; `9007199254740992` (2^53) ainda é `i64`; `u64` acima de `i64::MAX` não cabe em `as_i64()`. A guarda de dados e a regra de números do schema têm de ser a mesma função (dois lugares = dois critérios).

Outras: irmãos de `$ref` são avaliados (draft-7 ignorava); `additionalProperties` olha só os `properties` do mesmo objeto; ordem do decode do ponteiro (`~1` antes de `~0`, percent antes dos dois); token vazio em `$ref`; `minLength` conta pontos de código; pointer de saída não leva percent-encoding; chave duplicada no JSON do schema (`{"type":"string","type":"number"}`) é resolvida pelo `serde_json` com o último valor, silenciosamente (testado), em linha com o RFC 8259 §4 que chama o resultado de imprevisível; o `def_hash` é sobre o `Value` já parseado, então o duplicado não é detectável depois.

### Testes próprios (nomes em inglês)

| teste | afirma |
|---|---|
| `it_should_reject_a_ref_cycle_without_descending_into_the_instance` | `{"$ref":"#"}`, `allOf` e `not` com `#`: `ref-cycle` |
| `it_should_accept_a_recursive_ref_guarded_by_properties` | árvore com `properties.child = {"$ref":"#"}` |
| `it_should_validate_a_shared_ref_dag_in_polynomial_time` | `d0..d30` com `allOf` duplo termina em tempo curto (timeout do teste) |
| `it_should_not_overflow_the_stack_on_deeply_nested_data` | instância com 127 níveis e schema recursivo, em thread de 2 MiB |
| `it_should_report_max_depth_exceeded_on_a_very_long_ref_chain` | 10.000 `$defs` encadeados |
| `it_should_treat_property_names_as_names_not_keywords` | `properties` com `pattern` e `$ref` como nomes |
| `it_should_resolve_ref_pointers_with_escapes_and_empty_tokens` | `~0`, `~1`, `%22`, `#/$defs//$defs/` |
| `it_should_reject_a_ref_to_a_non_schema_location` | `#/enum/0`, `#/properties` |
| `it_should_evaluate_ref_siblings` | `{"$ref":..., "maxLength":2}` |
| `it_should_not_look_into_all_of_for_additional_properties` | só `properties` do mesmo objeto |
| `it_should_report_a_single_violation_for_a_failing_any_of` | sem erro dos ramos |
| `it_should_count_string_length_in_code_points` | `"💩"` com `maxLength: 1` |
| `it_should_use_a_pointer_with_tilde_escapes_in_violations` | chave `a/b~c` vira `/a~1b~0c` |
| `it_should_cut_at_the_limit_and_add_too_many_errors` | 60 violações |
| `it_should_reject_non_safe_integers_in_the_schema` | `minimum: 1.5`, `enum: [1.0]` |
| `it_should_pass_the_filtered_official_suite_without_skips` | os 392 testes |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Pontos em aberto

### O que é?
Decisões que tomei como proposta e que o dono pode querer mudar. Nenhuma bloqueia a implementação; todas custam uma linha de código.

### Lista
1. **Forma do corte.** O plano diz "no máximo 50 ... e `too-many-errors`" (linha 403) e o teste "devolve 50 e `too-many-errors`" (linha 681). Leitura A: 49 violações reais mais o marcador, total 50. Leitura B: 50 reais mais o marcador, total 51. Recomendo A, que respeita "no máximo 50"; B só se o dono disser que o limite conta violações reais.
2. **Ponteiro de `required`.** Segui a spec (objeto, §12.4.2): perde o nome da propriedade que falta. Alternativa: o ponteiro da propriedade ausente (`/obj/email`), que é sintaxe válida de JSON Pointer mesmo apontando para algo que não existe e diz mais ao usuário.
3. **Números no schema só inteiros seguros.** Proposta minha; evita comparar float com inteiro. Custa recusar `minimum: 0.5`, que para dados inteiros equivale a `minimum: 1`.
4. **Palavra desconhecida recusada**, contra a spec (Core §4.3.1). Documentar no ADR.
5. **`enum` vazio e com duplicatas aceitos**, como a spec (Validation §6.1.2 usa SHOULD).
6. **Teto de 512 quadros** de avaliação: não medido.
7. **Palavras baratas que podem entrar depois:** `exclusiveMinimum`, `exclusiveMaximum`, `minProperties`, `maxProperties`, `uniqueItems`, `if`/`then`/`else`, `dependentRequired`, `prefixItems`. Cada uma traz casos prontos na suíte oficial (os arquivos têm o mesmo nome da palavra).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Todas as consultas em 2026-11-07. "Lido" quer dizer que li o texto nesta sessão; "citado" quer dizer que o resultado vem de fonte secundária.

**Especificações**
- [JSON Schema 2020-12: Core](https://json-schema.org/draft/2020-12/json-schema-core), lido: §4.2.2, §4.3.1, §4.3.2, §5, §6.3, §6.5, §7.3, §7.6.1, §8.1.1, §8.2.3.1, §8.2.4, §8.3, §9.2, §9.4, §10, §12, §13.
- [JSON Schema 2020-12: Validation](https://json-schema.org/draft/2020-12/json-schema-validation), lido: §6.1 a §6.5, §7.2.1, §9.
- [RFC 6901: JSON Pointer](https://www.rfc-editor.org/rfc/rfc6901), lido: §3, §4, §6.
- [RFC 8259: JSON](https://www.rfc-editor.org/rfc/rfc8259), lido: §4, §6, §8.2, §8.3.
- [Metaschema 2020-12](https://json-schema.org/draft/2020-12/schema), referência para o que um schema válido exige de cada valor.

**Literatura**
- [Pezoa, Reutter, Suárez, Ugarte, Vrgoč. Foundations of JSON Schema. WWW 2016, pp. 263-273](https://archives.iw3c2.org/www2016/WWW2016-Proceedings/proceedings/p263.pdf) (DOI 10.1145/2872427.2883029). O PDF foi baixado mas o texto não pôde ser extraído nesta sessão (sem `pdftotext`); os resultados (validação PTIME-completa para o Draft-04, esquema bem formado) estão **citados** de Bourhis et al. §5.2 e Attouche et al. §1.
- [Bourhis, Reutter, Suárez, Vrgoč. JSON: data model, query languages and schema specification. PODS 2017, arXiv:1701.02221](https://arxiv.org/abs/1701.02221), lido via [ar5iv](https://ar5iv.labs.arxiv.org/html/1701.02221): §5.2 (validação com as mesmas cotas), §5.3 (bem formada: grafo de precedência acíclico), satisfatibilidade.
- [Attouche, Baazizi, Colazzo, Ghelli, Sartiani, Scherzinger. Validation of Modern JSON Schema: Formalization and Complexity. POPL 2024, arXiv:2307.10034](https://arxiv.org/abs/2307.10034), lido via [ar5iv](https://ar5iv.labs.arxiv.org/html/2307.10034): validação PSPACE-completa com `$dynamicRef`, P no fragmento estático, memoização, Definições 2 e 3 (bem formado), seção 10 (validadores exponenciais).

**Suíte de testes**
- [json-schema-org/JSON-Schema-Test-Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite), commit `245a45abc90190c0bff04d97e6d2b541b4bb9f12` (2026-11-03), licença MIT; diretório `tests/draft2020-12/`.

**Projeto**
- Plano: `.omc/plans/omhl-v2-core-step3.md`, linhas 403, 407, 535, 563, 579, 676 a 682 e 1195.
- Relatório anterior: `.omc/research/gray-box/catalog-schema.md` (candidatos de biblioteca e `url` para `equivalent`).
- Spikes e scripts descartáveis: `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/` (`jts/analyze.py` é o filtro; `research-catalog/spike4/` testa o `serde_json`).
