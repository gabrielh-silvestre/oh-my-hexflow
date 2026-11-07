# Pesquisa de reuso: caixa cinza `catalog`

Pesquisa feita em 2026-11-07 para a caixa `catalog` (`catalog/types.rs` e `catalog/validate.rs`) do crate `hexlog`. Contexto: o plano em `.omc/plans/omhl-v2-core-step3.md` escolheu `jsonschema` com `default-features = false` e `boon` de reserva. Resultado em uma linha: com o critério do dono, só `url` é apto; a validação de `data` vira gap interno.

## Tabela de Conteúdos

- [Critérios de aptidão](#critérios-de-aptidão)
- [jsonschema](#jsonschema)
- [boon](#boon)
- [valico](#valico)
- [jsonschema-valid](#jsonschema-valid)
- [url](#url)
- [fluent-uri](#fluent-uri)
- [iri-string](#iri-string)
- [Gap interno: validar data](#gap-interno-validar-data)
- [Gap interno: recusar palavras-chave](#gap-interno-recusar-palavras-chave)
- [Gap interno: inteiro seguro](#gap-interno-inteiro-seguro)
- [Links Úteis](#links-úteis)

---

## Critérios de aptidão

### O que é?
A régua que o dono fixou para aceitar uma biblioteca no lugar de código próprio. Um candidato só é **apto** se cumprir todos os itens; senão recebe `não apto` e a lógica que ele cobriria vira gap interno.

### Como aplicar?
Os itens, na ordem em que foram dados.

- **Estrelas:** no mínimo 1000 no GitHub (critério acrescentado depois da primeira rodada).
- **Atualização e saúde:** atualizações relevantes recentes; documentação, testes (CI) e badges no repositório. Conferi a presença de workflows e badges, não o conteúdo dos testes.
- **Manutenção:** mais de 18 meses sem release conta como ruim, salvo crate estável e terminado.
- **Licença:** compatível com MIT ou Apache-2.0.
- **Toolchain:** compila em Rust 1.97, edition 2024, com API síncrona.
- **Fecho de dependências normais:** sem `git2`, `gix`, `reqwest`, `ureq`, `hyper`, `tokio`, `async-std`.
- **API:** não obriga o chamador a usar `Rc`, `Arc`, `RefCell` ou `'static`.

Como medi: `crates.io/api/v1/crates/<nome>` para versões, downloads e licença; `cargo tree -e normal` em projetos descartáveis (`rustc 1.97.0`, `cargo 1.97.0`); GitHub pelas tools MCP para estrelas, commits e workflows; spikes de código para o comportamento. A tabela de síntese:

| candidato | estrelas | última release | downloads recentes | licença | veredito |
|---|---|---|---|---|---|
| jsonschema 0.60.2 | 833 | 2026-10-26 | 38,7 mi | MIT | não apto (estrelas) |
| boon 0.6.1 | 120 | 2025-01-07 | 321 mil | MIT OR Apache-2.0 | não apto |
| valico 4.0.0 | 305 | 2023-05-13 | 1,2 mi | MIT | não apto |
| jsonschema-valid 0.5.2 | 25 | 2023-11-08 | 85 mil | MPL-2.0 | não apto |
| url 2.5.8 | 1585 | 2026-01-05 | 250 mi | MIT OR Apache-2.0 | apto |
| fluent-uri 0.4.1 | 125 | 2025-11-04 | 45,7 mi | MIT | não apto |
| iri-string 0.7.14 | 29 | 2026-07-26 | 55,7 mi | MIT OR Apache-2.0 | não apto |

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [jsonschema](https://github.com/Stranger6667/jsonschema)

### O que é e o que faz?
Validador de JSON Schema em Rust, drafts até 2020-12, com saída de todos os erros e API para registrar schemas e metaschemas. Veredito: **não apto**, só porque tem 833 estrelas (faltam 167). Nas outras frentes passa.

### Evidências
- **Manutenção:** release 0.60.2 em 2026-10-26, 141 versões publicadas, push em 2026-11-07. Um único dono no crates.io (Stranger6667); 19 issues abertas.
- **Saúde:** 14 workflows (`ci.yml`, `codspeed.yml`, `daily.yml` e outros), README com codecov, docs.rs e 10 badges shields, site de documentação em jsonschema.dygalo.dev.
- **Licença e toolchain:** MIT; MSRV 1.85; compila com Rust 1.97 e edition 2024.
- **Fecho com `default-features = false`:** 64 pacotes contando o raiz, sem nenhum crate banido. `regex` e `fancy-regex` entram mesmo assim, porque são dependências obrigatórias. Com as features padrão o fecho sobe para 182 linhas e traz `reqwest`, `hyper`, `tokio` e `aws-lc`.
- **API:** `jsonschema::Validator` é `Send + Sync` e `build(&schema)` aceita referência, sem `Arc` nem `'static`.

### O que o spike provou
`iter_errors` devolveu todas as violações com `instance_path()` já em formato de JSON Pointer (`/a`, `/b/0`, `/b/2`; raiz vazia). `$ref` remoto na compilação falha com erro explícito, sem rede. Um metaschema restrito, montado pelo padrão oficial de extensão (`$dynamicAnchor: "meta"` + `allOf [$ref 2020-12]` + `pattern`, `patternProperties`, `$id`, `$async` iguais a `false` + `$ref` com `pattern "^#"`) e registrado com `Registry` e `Resource`, recusou `pattern` aninhado, `patternProperties`, `$id` e `$ref` remoto com o ponteiro certo, aceitou `$ref` local e não deu falso positivo num `enum` com a chave `pattern`. Código em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/research-catalog/spike/src/main.rs` (descartável, fora do projeto).

### Para reabrir a decisão
Se o dono relaxar o limiar de estrelas, é a primeira opção: o spike cobre erros com ponteiro, corte em 50 (`iter_errors().take(50)`) e recusa de palavras-chave sem andar pelo schema. Não cobre inteiro seguro (ver o gap próprio).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [boon](https://github.com/santhosh-tekuri/boon)

### O que é e o que faz?
Validador de JSON Schema (drafts 4 a 2020-12) com uma arena `Schemas` e um `Compiler`. Veredito: **não apto**, com 120 estrelas e 22 meses sem release.

### Evidências
- **Manutenção:** release 0.6.1 em 2025-01-07, mais de 18 meses; o último commit (2026-11-01) é CI e `BufReader`, não funcionalidade. Um mantenedor (santhosh-tekuri); 11 issues abertas.
- **Saúde:** `rust.yml` e `release.yml`; README com badges de CI, codecov e docs.rs.
- **Licença e fecho:** MIT OR Apache-2.0; 57 pacotes, sem crates banidos, mas com `regex` e `url` (com a pilha ICU de `idna`).
- **API:** `Schemas` precisa de `&mut`; sem `Arc` nem `'static`.

### O que o spike provou
Aceita o mesmo metaschema restrito por `$dynamicRef` e recusa `pattern` e `$ref` remoto com o ponteiro certo. Os erros vêm em árvore (`causes`); para a lista plana de violações com corte em 50 é preciso percorrer as folhas. Código em `.../research-catalog/spike2/src/main.rs`.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [valico](https://github.com/s-panferov/valico)

### O que é e o que faz?
Validador de JSON Schema e "coercer" de JSON. Veredito: **não apto**: 305 estrelas, README declara só o draft 7, release 4.0.0 em 2023-05-13 (mais de 3 anos).

### Evidências
Dependências normais incluem `fancy-regex`, `url`, `chrono`, `uuid`, `addr` e `uritemplate-next`. Licença MIT. Os workflows e o conteúdo dos testes não foram verificados; o README não tem badges.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [jsonschema-valid](https://github.com/mdboom/jsonschema-valid)

### O que é e o que faz?
Validador simples de JSON Schema. Veredito: **não apto**: 25 estrelas, suporta só os drafts 4, 6 e 7 (README), licença MPL-2.0.

### Evidências
Release 0.5.2 em 2023-11-08; houve commits em julho de 2026 (push em 2026-07-28), mas sem release nova. Dependências: `regex`, `url`, `chrono`, `iri-string`, `itertools`. README sem badges; workflows não verificados.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [url](https://github.com/servo/rust-url)

### O que é e o que faz?
Parser de URL do projeto Servo, no modelo WHATWG. Veredito: **apto** pelo critério (única biblioteca da pesquisa que o cumpre). É o que o plano já usa para validar a sintaxe de `equivalent` sem resolver.

### Evidências
- **Estrelas e manutenção:** 1585 estrelas; release 2.5.8 em 2026-01-05; push em 2026-10-30; 214 issues abertas.
- **Saúde:** workflows `main.yml` e `codecov.yml`; homepage em docs.rs/url.
- **Licença e fecho:** MIT OR Apache-2.0; 34 pacotes (com `idna` e ICU), sem crates banidos; compila em Rust 1.97, edition 2024; 250 mi de downloads recentes.

### Limite que o dono precisa saber
Segue a especificação WHATWG, não a RFC 3986, e é frouxo. No spike (`spike3/src/main.rs`) aceitou `"  https://x.com"`, `http:\\x\y`, `https://münchen.de/ü`, `http://a/b c`, `http://a/%zz` e `http:/x`. Recusou `relative/path`, `foo`, `http://a b`, `http://[::1` e `http://a.b:99999`. Um pré-filtro (somente ASCII, sem byte menor ou igual a 0x20, sem 0x7f, sem barra invertida) barra espaço, tab e `ü`, mas não `%zz`. O teste `it_should_reject_a_malformed_equivalent_uri_and_accept_a_wellformed_one_without_resolving_it` do plano passa se "malformado" for grosseiro; para sintaxe estrita de RFC 3986 seria preciso um parser próprio (ver as fontes de gap).

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [fluent-uri](https://github.com/yescallop/fluent-uri-rs)

### O que é e o que faz?
Parser e builder de URI/IRI conforme RFC 3986/3987. Veredito: **não apto**, com 125 estrelas, apesar de melhor semântica que `url` para o caso.

### Evidências
Release 0.4.1 em 2025-11-04, último commit em 2026-05-20; 45,7 mi de downloads (já está no fecho do `jsonschema`); MIT; 9 pacotes (`borrow-or-share`, `ref-cast`). `ci.yml` e badges de CI e docs.rs; sem codecov. No spike recusou todos os casos frouxos de `url` acima (`http://a/%zz`, espaço, barra invertida, não-ASCII), mas aceitou `http://a.b:99999`, que não é erro de sintaxe de RFC 3986.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## [iri-string](https://github.com/lo48576/iri-string)

### O que é e o que faz?
Tipos de string para URI/IRI (`UriStr::new` valida a sintaxe). Veredito: **não apto**, com 29 estrelas.

### Evidências
Release 0.7.14 em 2026-07-26; MIT OR Apache-2.0; sem nenhuma dependência (2 pacotes na árvore contando o raiz). README com badge de CI e docs.rs; os workflows não foram listados. Mesmo comportamento estrito de `fluent-uri` no spike.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap interno: validar data

### O que é?
A lógica de validar `data` contra um JSON Schema, devolvendo todas as violações com JSON Pointer da instância e corte em 50, não tem biblioteca apta. Se o dono mantiver o critério de 1000 estrelas, fica para código próprio.

### Como aplicar?
Um validador do **subconjunto** que os schemas do hexlog usam. A proposta, a ser confirmada pelo dono (o plano não lista as palavras-chave em uso):

- **Tipo:** `type` com `object`, `string`, `integer`, `boolean`, `array`.
- **Objeto:** `properties`, `required`, `additionalProperties`.
- **Valor:** `enum`, `const`.
- **Lista:** `items`, `minItems`, `maxItems`.
- **Limites:** `minLength`, `maxLength`, `minimum`, `maximum`.
- **Composição:** `allOf`, `anyOf`, `oneOf`, `not`.
- **Interno:** `$defs` com `$ref` que começa por `#`.
- **Fora do subconjunto:** `pattern`, `format`, `$dynamicRef`, `unevaluated*`.

Custo estimado em algumas centenas de linhas de Rust, sem medição. O risco é de correção (arestas de `oneOf`, `$ref` com ciclo), controlável com subconjunto pequeno e teste de propriedade. A base normativa é a especificação de validação 2020-12 e o conjunto de testes oficial, listados abaixo. Os ponteiros de erro seguem o RFC 6901.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap interno: recusar palavras-chave

### O que é?
A regra do plano: recusar definição cujo schema use `pattern`, `patternProperties`, `$ref` que não comece por `#`, `$id`, `$async` ou raiz que não seja `type: "object"`.

### Como aplicar?
Com validador próprio, uma **lista branca**: qualquer palavra-chave fora do subconjunto acima vira `INVALID_SCHEMA`. Isso cobre as recusas do plano e evita que uma palavra desconhecida seja ignorada em silêncio, comportamento padrão dos validadores. Se `jsonschema` voltar à mesa, o caminho provado no spike é o metaschema restrito por `$dynamicAnchor`; a especificação core descreve a extensão de metaschemas, e o spike mostrou que ela pega `pattern` em qualquer posição de subschema sem percorrer o schema à mão.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Gap interno: inteiro seguro

### O que é?
Recusar em `data`, em qualquer nível, todo número que não seja inteiro seguro (`1.0`, `1e2`, `2^53`). Nenhum validador de JSON Schema faz isso, porque o JSON Schema trata `1.0` como `integer`.

### Como aplicar?
Um passo próprio que percorre `data` depois de parseado com `serde_json` (sem `arbitrary_precision`): aceitar só o que `Number::as_i64` ou `as_u64` represente e esteja em ±(2^53 − 1). No spike: `1.0` e `1e2` chegam como `f64` (`as_i64()` é `None`); `9007199254740993` ainda é `i64`, então o corte numérico é obrigatório. O limite é o `Number.MAX_SAFE_INTEGER` do ECMAScript e a regra de interoperabilidade da I-JSON (RFC 7493). Para o hash canônico, o RFC 8785 define a serialização de números.

<small>[Voltar ao topo](#tabela-de-conteúdos)</small>

---

## Links Úteis

Consulta: as páginas de crates.io, GitHub e os spikes foram consultados em 2026-11-07. As especificações abaixo são a base normativa dos gaps; as URLs vêm de conhecimento prévio e **não foram abertas nesta sessão**.

**Candidatos (consultados em 2026-11-07)**

- [crates.io: jsonschema](https://crates.io/crates/jsonschema) e [repositório](https://github.com/Stranger6667/jsonschema), [site](https://jsonschema.dygalo.dev/)
- [crates.io: boon](https://crates.io/crates/boon) e [repositório](https://github.com/santhosh-tekuri/boon)
- [crates.io: valico](https://crates.io/crates/valico) e [repositório](https://github.com/s-panferov/valico)
- [crates.io: jsonschema-valid](https://crates.io/crates/jsonschema-valid) e [repositório](https://github.com/mdboom/jsonschema-valid)
- [crates.io: url](https://crates.io/crates/url) e [repositório](https://github.com/servo/rust-url)
- [crates.io: fluent-uri](https://crates.io/crates/fluent-uri) e [repositório](https://github.com/yescallop/fluent-uri-rs)
- [crates.io: iri-string](https://crates.io/crates/iri-string) e [repositório](https://github.com/lo48576/iri-string)
- Plano: `.omc/plans/omhl-v2-core-step3.md` (números, `equivalent`, catálogo e tabela de testes do `catalog`)
- Spikes descartáveis: `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/research-catalog/`

**Fundamentação dos gaps (não abertas nesta sessão)**

- [JSON Schema 2020-12: Core](https://json-schema.org/draft/2020-12/json-schema-core), referências, `$dynamicRef` e extensão de metaschema
- [JSON Schema 2020-12: Validation](https://json-schema.org/draft/2020-12/json-schema-validation), semântica de `type`, `enum`, `minimum` e demais palavras-chave do subconjunto
- [JSON-Schema-Test-Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite), casos para testar o validador próprio
- [RFC 6901: JSON Pointer](https://www.rfc-editor.org/rfc/rfc6901), formato dos ponteiros de erro
- [RFC 3986: URI Generic Syntax](https://www.rfc-editor.org/rfc/rfc3986), gramática para uma validação estrita de `equivalent`
- [WHATWG URL Standard](https://url.spec.whatwg.org/), o modelo que explica a frouxidão do `url`
- [RFC 7493: I-JSON](https://www.rfc-editor.org/rfc/rfc7493), faixa de inteiros interoperáveis
- [ECMAScript: Number.MAX_SAFE_INTEGER](https://tc39.es/ecma262/#sec-number.max_safe_integer), origem de 2^53 − 1
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785), serialização canônica de números para o `def_hash`
