# Contrato de costuras do passo 3 (TDD com agentes isolados)

O `test-engineer` escreve os testes contra este contrato, sem ler a implementação; o `executor` implementa este contrato e não edita teste. Eles não conversam: tudo que um precisa saber do outro está aqui, em [omhl-v2-core-step3.md](omhl-v2-core-step3.md) (seções 4.4, 4.5, 5 e 16) e nas decisões de [omhl-v2-core-step3-critic-log.md](omhl-v2-core-step3-critic-log.md). A sessão principal escreve a costura de cada tarefa antes do primeiro teste. O valor esperado de um teste vem deste contrato, do plano ou de fonte independente, nunca da implementação.

Classe da fronteira: todas as costuras do passo 3 são em processo, sem rede; o banco é SQLite real em arquivo temporário, nunca falso.

## T2. Primitivas e cadeia pura

### Costura 1: tipos de id e erros (API pública `hexlog::*`)

Pega: formato aceito e recusado de cada id. Deixa passar: uso errado do id dentro do `decide` (T5a).

```rust
impl SemanticId { pub fn parse(text: &str) -> Result<SemanticId, HexlogError>; pub fn as_str(&self) -> &str; }
// Mesma forma em Name, TenantId, TypeName, RelationTypeName, RecordId, Hash, IdempotencyKey, ExternalId.
// Todos: Debug, Clone, PartialEq, Eq; Serialize como string simples.

pub struct HexlogError { pub code: ErrorCode, pub message: String, pub details: Vec<Detail> } // thiserror
impl HexlogError { pub fn at(self, prefix: &str) -> HexlogError; } // troca cada `path` vazio por `prefix` e cada `path` não vazio por `prefix` mais o `path`
pub fn pointer_segment(segment: &str) -> String;                   // RFC 6901: `~` vira `~0`, `/` vira `~1`
pub struct Detail { pub path: String, pub code: String, pub message: String, pub current: Option<RecordId>, pub retired: Option<bool> }
pub enum ErrorCode { InvalidInput, InvalidRecord, InvalidSchema, TypeNotFound, RelationTypeNotFound, TenantNotFound, ReservedId, ForkRejected, CycleRejected, IdempotencyConflict, ChainBroken, Busy, StoreError, Internal }
impl ErrorCode { pub fn as_str(self) -> &'static str; } // "INVALID_INPUT" e assim por diante; ErrorCode: Copy, PartialEq, Debug
```

Formatos e recusas. Toda recusa de `parse` e de `Snapshot::new` devolve exatamente um `Detail`, com `path == ""`; `message` nunca ecoa o valor.

| Tipo | Aceita | `code` | `details[0].code` |
|---|---|---|---|
| `SemanticId` | segmentos `^[a-z0-9][a-z0-9-]{0,62}$` separados por `.`, até 200 caracteres, sem ponto inicial, final ou duplo | `InvalidInput` | `semantic-id-format` |
| `Name`, `TenantId`, `TypeName`, `RelationTypeName` | `^[a-z0-9][a-z0-9-]{0,62}$` (um segmento) | `InvalidInput` | `name-format` |
| `RecordId` | `<TenantId>:<uuid>`, uuid minúsculo com hífens, versão 7, variante 8, 9, a ou b | `InvalidInput` | `record-id-format` |
| `Hash` | 64 hexadecimais minúsculos | `InvalidInput` | `hash-format` |
| `IdempotencyKey` | 1 a 200 unidades UTF-16 | `InvalidInput` | `idempotency-key-format` |
| `ExternalId` | 1 a 200 unidades UTF-16 | `InvalidRecord` | `external-id-format` |

Comprimento é sempre em unidades UTF-16 (`str::encode_utf16().count()`), como no Hexlog TS e na ordenação do JCS; vale também para `Author` (1 a 100) e `data_max_chars`. O ponteiro de erros de `data` é `/items/<i>/body/data/<chave escapada>...`.

### Costura 2: snapshot (API pública)

Pega: formato de 40 ou 64 hexadecimais minúsculos e a serialização. Deixa passar: as regras de uso da tabela S1 a S9 (T5a).

```rust
#[non_exhaustive] pub enum SnapshotKind { GitCommit } // Copy, PartialEq, Debug; serializa como "git-commit"
impl Snapshot { pub fn new(kind: SnapshotKind, id: &str) -> Result<Snapshot, HexlogError>; pub fn kind(&self) -> SnapshotKind; pub fn id(&self) -> &str; }
```

Recusa: `code == ErrorCode::InvalidRecord`, `details[0].code == "snapshot-format"`. Serializa como `{"kind":"git-commit","id":"<hex>"}`.

### Costura 3: cadeia pura (`hexlog::chain`)

Pega: JCS contra vetores independentes e hash contra o Hexlog TS. Deixa passar: quais campos entram no corpo (costura 5).

```rust
pub fn canonical(value: &serde_json::Value) -> Result<String, HexlogError>; // RFC 8785
// Número: `Number` que é i64 ou u64 sai em dígitos se |n| <= 2^53, senão Err; `Number` que é f64 sai pela regra ES (Number::toString; -0.0 sai "0"). A política "só inteiro seguro" (±(2^53 - 1)) NÃO é do canonical: é do passo 4 da escrita. Err: ErrorCode::InvalidInput, details[0].code == "number-out-of-range", details[0].path == "".
pub fn sha256hex(text: &str) -> Hash;
pub fn hash_record(prev_hash: &str, body: &str) -> Hash;                    // sha256hex(prev_hash + body); body já é JCS
pub fn anchor_doc(tenant: &TenantId, created_at: &str) -> String;           // JCS de {"created_at":created_at,"format":"hexlog-chain/1","tenant":tenant}, sem validar created_at
pub fn anchor(anchor_doc: &str) -> Hash;                                    // sha256hex(anchor_doc)
pub fn fingerprint(items: &[AppendItem]) -> Result<Hash, HexlogError>;      // sha256hex(JCS(array de itens, costura 5))
// Err só quando algum número de data é inteiro fora de ±2^53. A escrita (T5a) trata o Err como "não há replay possível" e segue ao passo 4. Em f64 inteiro (1.0) a impressão é a do inteiro (1).
```

Fontes independentes dos valores esperados. Regra de todas as fixtures douradas: geradas com o Hexlog TS em `/home/gabriel/personal/hexlog`, commit `5d1e22b`, sem editar aquele repositório (`canonicalize` do npm e `sha256`, a mesma conta de `src/domain/chain.ts:64-67`); o comando e o commit ficam anotados na própria fixture; nenhum valor nasce do Rust.

- `tests/fixtures/rfc8785_vectors.json`: conjunto baixado no spike da T1 (`/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/t1-spikes/rfc8785_vectors.json`), com a origem de cada vetor. Os 2 vetores com `input: null` (NaN e Infinity, que não existem em `Value`) saem; os 24 de número entram por `ieee754` convertido com `f64::from_bits` e `Number::from_f64`; os 9 de texto, por `input` lido com `serde_json::from_str`. Fundamento: [jcs-rfc8785.md](../research/gray-box/jcs-rfc8785.md).
- `tests/fixtures/golden_records.json`: 3 documentos (simples, Unicode, inteiros nos limites ±(2^53 − 1)), mais o `anchor_doc` e a âncora de um tenant.
- `tests/fixtures/golden_fingerprints.json`: impressões sobre o JSON da costura 5, por forma de item e por ordem.
- `tests/fixtures/golden_bodies.json`: as cinco formas da costura 5, com JCS e hash.

### Costura 4: portas públicas e tipos de entrada

Pega: que os fakes de `tests/support` compilam contra as portas. Deixa passar: o comportamento delas sob o `decide`.

`Clock`, `IdGenerator`, `NativeNodes`, `NativeNode` e os tipos de entrada como na seção 4.4 do plano, reexportados em `hexlog::*`. `tests/support/mod.rs` guarda só donos (plano, seção 16.5):

- `FixedClock` devolve um instante fixo (`2026-11-07T12:00:00.000Z` por padrão).
- `SeqIds(AtomicU64)`: `next` devolve `Uuid::from_u128(0x0192a000_0000_7000_8000_0000_0000_0000 | n)`, um uuid v7 válido.
- `FakeNative::new(version: &str)`, `reserve(&mut self, id: SemanticId, type_name: TypeName)`, `reserve_relation(&mut self, relation_type: RelationTypeName, from: SemanticId, to: SemanticId)`; `catalog_version()` devolve a `version` recebida.
- `tests/support/{catalog,tamper,mutations}.rs` nascem vazios e declarados.

O `decide` valida o formato de 24 caracteres de `Clock::now()` (`YYYY-MM-DDTHH:MM:SS.mmmZ`) e devolve `Internal` se divergir (T5a).

### Costura 5: forma do JSON que congela o formato

Pega: chave a chave, o que entra no hash. Deixa passar: a montagem pelo `decide` (T5a e T5b, que têm um teste de ponta a ponta lendo a coluna `body` crua).

Item de entrada (`AppendItem`), base da impressão; campo opcional ausente é omitido, nunca `null`:

```json
{"body":{"kind":"node","id":"a.b","type":"note","data":{}},"base":"acme:<uuid>","snapshot":{"kind":"git-commit","id":"<hex>"},"external_id":"x"}
{"body":{"kind":"relation","relation_type":"after","from":"a.b","to":"c.d"}}
{"body":{"kind":"retirement","lineage":{"kind":"node","id":"a.b"}},"base":"acme:<uuid>"}
{"body":{"kind":"retirement","lineage":{"kind":"relation","relation_type":"after","from":"a.b","to":"c.d"}},"base":"acme:<uuid>"}
```

Formato aprovado pelo dono no fim da T2 (2026-11-07), com as três escolhas desta tabela; mudar depois exige tenant novo. Corpo do record. Tipo interno, em `engine/model.rs`: `pub(crate) struct RecordBody<'a>`, um só para escrever e ler (`Serialize` + `Deserialize`, `#[serde(borrow)]`, `#[serde(deny_unknown_fields)]`, campos de texto `Cow<'a, str>`, `data: Option<Cow<'a, serde_json::Value>>`, opcionais com `skip_serializing_if = "Option::is_none"`). Campos e nomes JSON: `v` (`u8`, sempre 1), `tenant`, `seq` (`u64`), `id`, `at` (texto de `Clock::now()` sem alteração), `type` (Rust `type_name`), `lineage` (enum interno por `kind`: node com `id`; relation com `relation_type`, `from`, `to`, `from_origin`, `to_origin`), `revises`, `retirement` (`bool`), `data`, `author` (`agent`, `model`, `client`), `snapshot` (`kind`, `id`), `external_id`, `native_catalog`, `def_hash`, `batch` (`fingerprint`, `key`, `count`). Tipos auxiliares `BodyLineage<'a>`, `BodyAuthor<'a>`, `BodySnapshot<'a>`, `BodyBatch<'a>`. `RecordBody::canonical(&self) -> Result<String, HexlogError>` é `chain::canonical(&serde_json::to_value(self))`.

Presença de cada campo por forma (sem `null`, nunca; ausente é omitido):

| Campo | Node (raiz ou revisão) | Relation (raiz ou revisão) | Retirement de node | Retirement de relation |
|---|---|---|---|---|
| `v`, `tenant`, `seq`, `id`, `at`, `author`, `def_hash` | sempre | sempre | sempre | sempre |
| `type` | nome do tipo | nome do relation type (igual a `lineage.relation_type`) | copiado do latest | copiado do latest |
| `lineage` | `kind` node, `id` | `kind` relation, as cinco chaves | como node | como relation |
| `revises` | se há record anterior na lineage | idem | sempre | sempre |
| `retirement` | sempre, `false` | sempre, `false` | `true` | `true` |
| `data` | sempre (objeto; `{}` se vazio) | ausente | ausente | ausente |
| `snapshot` | regras S1 a S9 | ausente | ausente | ausente |
| `external_id` | opcional | opcional | ausente | ausente |
| `native_catalog` | ausente | só se algum extremo é native | ausente | copiado do latest, se havia |
| `batch` | só no primeiro record de todo lote, com ou sem `key`: `fingerprint`, `count` (número de itens) e `key` se houver | | | |

`from_origin` e `to_origin` valem `"tenant"` ou `"native"`.

### Testes da T2

Os do plano, mais os que o crítico acrescentou (D-2 a D-5):

| Arquivo | Testes |
|---|---|
| `tests/semantic_id.rs` | os 2 do plano |
| `tests/snapshot.rs` | os 2 do plano, mais `it_should_serialize_a_snapshot_as_kind_and_id` |
| `tests/jcs_vectors.rs` | os 3 do plano, mais `it_should_match_the_ts_fingerprint_for_each_item_shape`, `it_should_match_the_hexlog_ts_anchor_for_a_tenant`, `it_should_refuse_an_integer_beyond_2_to_the_53_in_canonical`, `it_should_fail_the_fingerprint_for_an_integer_beyond_2_to_the_53` e `it_should_give_the_same_fingerprint_to_one_and_one_point_zero` |
| `tests/ids.rs` (novo) | `it_should_validate_names_per_the_contract_table`, `it_should_validate_record_ids_per_the_contract_table`, `it_should_validate_hashes_per_the_contract_table`, `it_should_validate_idempotency_keys_in_utf16_units`, `it_should_validate_external_ids_in_utf16_units` |
| `tests/errors.rs` (novo) | `it_should_prefix_detail_paths_with_at`, `it_should_keep_the_path_when_prefixing_with_an_empty_string`, `it_should_escape_pointer_segments_per_rfc_6901`, `it_should_map_every_error_code_to_its_wire_name` |
| `src/engine/model/tests.rs` (unidade, declarado por `#[cfg(test)] mod tests;` em `model.rs`) | `it_should_serialize_each_record_shape_like_the_golden_body`: constrói `RecordBody` por campo e afirma `canonical()` igual ao JCS dourado e `hash_record` igual ao hash; `it_should_read_each_golden_body_back_to_the_same_text`: lê o JCS com `from_str::<RecordBody>` e afirma que reserializa para o mesmo texto; `it_should_refuse_a_body_with_an_unknown_field`: o `Deserialize` recusa |

O arquivo de unidade está em `src/` e passa pelo varredor de vocabulário: nada de `check`, `gate`, `unit`, `work`, `review` e afins em identificador ou literal.

## T2b. Emenda da T2: profundidade de JSON (D-10)

Decisão em D-10 do [log do crítico](omhl-v2-core-step3-critic-log.md). Chave duplicada e noncharacters ficam aceitos, como no Hexlog TS: a duplicada é resolvida por quem converte texto em `Value` (a última vence), e o `hexlog` recebe `Value`. A doc de `AppendItem.data` diz isso em uma linha.

### Costura 9: limites de JSON (`engine/limits.rs`, interno)

Pega: `Value` aninhado demais e número fora de inteiro seguro, sem recursão e antes de qualquer outra passada pelo valor. Deixa passar: onde cada chamada entra no `append` (T5a) e no catálogo (T3).

```rust
pub(crate) const JSON_NESTING_MAX: usize = 126; // contêineres (array ou objeto); escalar não conta
pub(crate) fn json_nesting_exceeds(value: &Value, max: usize) -> bool;                 // iterativa
pub(crate) fn json_issues(value: &Value, limit: usize) -> Vec<(String, &'static str)>; // iterativa
```

- Profundidade conta contêineres: `{}` tem 1, `{"a":[1]}` tem 2, `7` tem 0. O corpo do record gasta um dos 127 níveis que o `from_str` aceita, por isso o teto de `data` é 126.
- `json_issues` devolve `(ponteiro, código)` em ordem de documento (chaves na ordem do `Map`, que é alfabética; arrays por índice), com ponteiro RFC 6901 já escapado (raiz `""`), e para ao chegar a `limit` achados. Códigos: `number-not-safe-integer` (número que não é `i64` nem `u64` dentro de ±(2^53 − 1): caem `1.0`, `1e2`, `0.5`, `-0`, `-0.0`, ±2^53 e `18446744073709551615`; passam `0` e ±(2^53 − 1)) e `max-depth-exceeded` (no ponteiro do contêiner que passa de `JSON_NESTING_MAX`, sem descer nele; o resto do documento continua sendo percorrido, então `{"a": <126 arrays>, "b": 0.5}` dá o `max-depth-exceeded` em `/a/0/…/0` e depois `/b`).

### Mudanças em `hexlog::chain`

- `canonical`: teto de pilha de 256 níveis de contêiner; acima, `Err` `InvalidInput` com detalhe `max-depth-exceeded` e `path` `""`. Não é regra de produto: só impede o estouro para quem chama direto.
- `fingerprint`: antes do `to_value`, `items[i].data` com mais de `JSON_NESTING_MAX` níveis dá `Err` `InvalidInput` com `max-depth-exceeded` em `/<i>/data`. Pela D-3, `Err` significa "sem replay possível"; o passo 1 da escrita recusa o mesmo item com o próprio código (T5a).

### Testes da T2b

| Arquivo | Testes |
|---|---|
| `tests/jcs_vectors.rs` | `it_should_emit_noncharacters_raw_like_the_ts_oracle`: `{"k":"a\u{FFFE}\u{FFFF}b\u{FDD0}\u{FDEF}\u{1FFFE}\u{10FFFF}z"}` sai sem escape (RFC 8785, seção 3.2.2.2), e o sha256 do texto canônico é `cccc4a026451b00f922cc2986c70a03ad855b8a0aabb65156dd148e8a610e6dd`, do oráculo TS (D-10) |
| `tests/jcs_vectors.rs` | `it_should_refuse_a_value_nested_beyond_the_canonical_stack_limit`: 256 níveis passam; 257 e 5.000 dão `max-depth-exceeded`; roda em thread de 2 MiB |
| `tests/jcs_vectors.rs` | `it_should_fail_the_fingerprint_for_data_nested_beyond_126_levels_without_overflowing`: 126 passa; 127 dá `Err` em `/0/data`; 5.000 dá `Err` sem abortar; thread de 2 MiB |
| `src/engine/limits/tests.rs` (o executor declara `#[cfg(test)] mod tests;` em `limits.rs`) | `it_should_count_nesting_in_containers_and_not_in_scalars`, `it_should_flag_unsafe_numbers_and_depth_in_document_order` |

Pronto: os 3 testes novos de `jcs_vectors` (de 8 para 11) e os 2 de unidade verdes, mais os três comandos.

## T3. Catálogo e validação (caixa cinza `catalog`)

Decisões em D-7 do log do crítico; fundamento em [json-schema-subset.md](../research/gray-box/json-schema-subset.md) e [catalog-schema.md](../research/gray-box/catalog-schema.md).

### Costura 6: catálogo (API pública `hexlog::*`)

Pega: definições lidas de JSON, recusa de schema fora do subconjunto, hash de definição estável, violações de `data` com ponteiro. Deixa passar: o uso da porta `Definitions` pelo `decide` (T5a).

```rust
pub struct TypeDef { pub name: TypeName, pub schema: serde_json::Value, pub requires_snapshot: bool, pub equivalent: Option<String> }
pub struct RelationTypeDef { pub name: RelationTypeName, pub from: Option<Vec<TypeName>>, pub to: Option<Vec<TypeName>>, pub acyclic: bool, pub equivalent: Option<String> }
// Serialize + Deserialize + Debug + Clone + PartialEq nos dois. deny_unknown_fields. requires_snapshot e acyclic: default false na leitura, sempre escritos.
// equivalent, from, to: ausente na leitura = None; None é omitido na escrita (nunca null); null na leitura = None. Nomes: string simples, nome inválido falha na leitura.
// DataViolation { pub pointer: String, pub code: String } já existe em engine/ports.rs: a T3 o torna pub, com Debug, Clone e PartialEq, e o reexporta em lib.rs. Não há segundo tipo.
impl Catalog {
    pub fn new(types: Vec<TypeDef>, relations: Vec<RelationTypeDef>) -> Result<Catalog, HexlogError>;
    pub fn type_def(&self, name: &TypeName) -> Option<&TypeDef>;
    pub fn relation_type_def(&self, name: &RelationTypeName) -> Option<&RelationTypeDef>;
    pub fn type_hash(&self, name: &TypeName) -> Option<&Hash>;                 // def_hash = sha256hex(canonical(to_value(def)))
    pub fn relation_type_hash(&self, name: &RelationTypeName) -> Option<&Hash>;
    pub fn validate_data(&self, name: &TypeName, data: &serde_json::Value) -> Result<(), Vec<DataViolation>>; // tipo ausente: Err com um achado "type-not-found" em ""
}
// Catalog: Send + Sync. Porta interna: node_type(&self, &TypeName) -> Option<&NodeTypeInfo>, relation_type(&self, &RelationTypeName) -> Option<&RelationTypeInfo>; a T3 troca as duas assinaturas em engine/ports.rs, que hoje devolvem por valor, e o impl Definitions só delega.
```

Recusa de `Catalog::new`: `ErrorCode::InvalidSchema`, um detalhe por problema, `details[i].path` em `/types/<nome>/schema<ponteiro>`, `/types/<nome>/equivalent` ou `/relations/<nome>/<campo>`, e `code` entre:

| `code` | Quando |
|---|---|
| `root-not-object` | raiz sem `type: "object"` |
| `keyword-not-allowed` | palavra fora da lista branca, inclusive erro de digitação, `$id`, `$dynamicRef`, `$async`, `uniqueItems` |
| `pattern-not-allowed` | `pattern`, `patternProperties` |
| `ref-not-internal` | `$ref` que não é `#` nem `#/...` |
| `ref-unresolved` | alvo inexistente ou que não é posição de schema |
| `ref-cycle` | ciclo de "mesma posição" por `$ref`, `allOf`, `anyOf`, `oneOf`, `not`; `properties`, `items` e `additionalProperties` recursivos são válidos |
| `keyword-type` | tipo errado do valor, inclusive anotação |
| `schema-uri` | `$schema` fora da raiz ou com URI diferente de 2020-12 |
| `number-not-safe-integer` | qualquer número do documento do schema fora de ±(2^53 − 1) ou com fração ou expoente |
| `max-depth-exceeded` | schema com mais de 126 níveis de contêiner (`JSON_NESTING_MAX`, costura 9) |
| `equivalent-uri-format` | `url::Url::parse` falha, ou há caractere menor ou igual a U+0020 ou igual a U+007F |
| `empty-endpoint-list` | relation type com `from: []` ou `to: []` (ausente e `null` seguem valendo como qualquer tipo); `path` `/relations/<nome>/from` ou `/to` |
| `duplicate-endpoint-type` | nome repetido em `from` ou em `to`; um detalhe por lista |
| `duplicate-name` | dois `TypeDef` ou dois `RelationTypeDef` com o mesmo nome; um detalhe por nome repetido, em `/types/<nome>` ou `/relations/<nome>` (D-23; o TS também recusa); um tipo e um relation type podem ter o mesmo nome |

`keyword-type` também cobre `type: []` (em `/type`) e `required` com nome repetido (em `/required`), que o metaesquema 2020-12 proíbe (D-23); `enum` com valor repetido e `required: []` são aceitos. Quando a guarda `json_issues` acha algo num schema, só esses achados saem para aquele schema, e o `compile` não roda; os outros tipos do mesmo `Catalog::new` seguem sendo analisados. `validate_data` assume pilha de ao menos 2 MiB em debug (teto de avaliação de 1.024, margem medida de 1,8x).
| `format-not-supported` | `format` com valor diferente de `attachment`, ou `attachment` fora das posições permitidas ou em schema sem `type` string; `path` termina em `/format` |

O `path` aponta o valor que causa o problema (D-18): a palavra `$ref` em `ref-not-internal`, `ref-unresolved` e `ref-cycle`; a palavra em `pattern-not-allowed` e `keyword-type` (também a posição de schema que não é objeto nem booleano, como `/properties/a`); o número em `number-not-safe-integer`. `root-not-object` fica em `/types/<nome>/schema` e encerra a análise daquele schema com um só detalhe; só vale `type` igual à string `"object"` (`["object","null"]`, `{}`, `true` e `false` são recusados). `ref-cycle` sai uma vez por `$ref` que está num ciclo, em ordem de ponteiro: o `$ref` é acusado quando a posição que o contém e o alvo dele ficam na mesma componente fortemente conexa do grafo de `$ref`, `allOf`, `anyOf`, `oneOf` e `not`, calculada sem recursão por `petgraph::algo::kosaraju_scc` (o `tarjan_scc` do `petgraph` 0.8.3 é recursivo e estouraria a pilha na cadeia de 10.000 `$defs`). Exemplos: `$ref: "#"` na raiz dá `/$ref`; `allOf` dá `/allOf/0/$ref`; `not` dá `/not/$ref`; `a -> b -> a` em `$defs` dá `/$defs/a/$ref` e `/$defs/b/$ref`. A recusa de `Catalog::new` corta em 50 detalhes fixos mais `too-many-errors` em `""` (com exatamente 50, sem marcador), visitando os tipos na ordem do `Vec`, depois as relations, e dentro do schema as palavras em ordem alfabética. `$schema` só vale na raiz e só com `https://json-schema.org/draft/2020-12/schema` exato, sem `#`; fora disso, `schema-uri` no ponteiro do `$schema`.

Lista branca: `type`, `enum`, `const`, `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems`, `required`, `properties`, `additionalProperties`, `items` (sem `prefixItems`), `allOf`, `anyOf`, `oneOf`, `not`, `$ref`, `$defs`, `$schema` (raiz, URI 2020-12) e as anotações `title`, `description`, `$comment`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` (só o tipo do valor é conferido). Chaves de `properties` e de `$defs` são nomes, nunca palavras-chave. `format` (D-11, decidido pelo crítico por delegação do dono) só aceita o valor `attachment`, em `properties/<campo>` e `properties/<campo>/items` contados da raiz (sem passar por `$ref` nem `$defs`), em schema cujo `type` é `string` ou lista com `"string"`; o dado desse campo, quando é texto, tem 64 hexadecimais minúsculos (`Hash::parse`), e instância que não é texto não gera `format`. `Catalog::new` não confere se os tipos citados em `from` e `to` existem no catálogo, como o TS.

Costura 6b, interna ao `catalog` (para o teste de unidade da suíte oficial): `validate::compile(schema: &Value) -> Result<CompiledSchema, Vec<Detail>>` compila qualquer schema do subconjunto **sem** a regra de raiz, com `path` relativo à raiz do schema, e `CompiledSchema::validate(&self, data: &Value) -> Result<(), Vec<DataViolation>>` avalia. `Catalog::new` aplica a regra de raiz e a guarda de `json_issues` antes de chamar `compile`, e prefixa os `path`.

Violações de `validate_data`: todas, em ordem determinística (palavras na ordem `type`, `enum`, `const`, `minimum`, `maximum`, `min-length`, `max-length`, `format`, `min-items`, `max-items`, `required`, `properties`, `additionalProperties`, `items`, `allOf`, `anyOf`, `oneOf`, `not`, `$ref`; propriedades em ordem alfabética; arrays por índice), sem repetir o par `(pointer, code)`. `pointer` é o ponteiro RFC 6901 da instância (raiz `""`). `code` entre: `type`, `enum`, `const`, `minimum`, `maximum`, `min-length` e `max-length` (em pontos de código), `format` (campo `attachment` com texto que não é hash sha256 minúsculo), `min-items`, `max-items`, `required` (ponteiro da propriedade ausente, um por propriedade), `additional-properties` (ponteiro da chave excedente; só olha `properties` do mesmo objeto), `false-schema`, `any-of`, `one-of`, `not` (um único achado no ponteiro do valor, sem erros dos ramos), `number-not-safe-integer`, `max-depth-exceeded`. `allOf` e `$ref` são transparentes. `type` que falha encerra a avaliação daquele nó. Se `data` tem número inseguro ou mais de 126 níveis de contêiner, a resposta são só esses achados. Estouro do teto de avaliação (1.024 quadros) devolve um único `max-depth-exceeded` em `""`. Corte: no máximo 50 achados reais; havendo mais, os 50 primeiros mais um `too-many-errors` em `""` (51 itens); com exatamente 50 não há marcador.

Guarda de inteiro seguro e de profundidade: `engine::limits::json_issues` (costura 9, nasce na T2b). O `Catalog::new` a aplica ao documento inteiro do schema, e `validate_data`, a `data`, antes de avaliar o schema; a T3 só a consome.

Forma interna (D-7): compilação em `Catalog::new` e avaliação com memo em `HashMap<(NodeId, usize), bool>`, chave `std::ptr::from_ref(v).addr()`, local a cada chamada. Sem `Arc`, `Rc`, `RefCell` nem `Mutex`.

Fontes independentes dos valores esperados:

- `tests/fixtures/json-schema-test-suite/`: 21 arquivos, 124 grupos, 392 testes da suíte oficial no commit `245a45abc90190c0bff04d97e6d2b541b4bb9f12`, com `LICENSE`, `filter.py` e `MANIFEST.md`. O protótipo e o filtro do crítico estão em `/tmp/claude-1000/-home-gabriel-personal-oh-my-hexflow/c0c83fa7-2673-431f-bc84-73b5401f9c5c/scratchpad/critic/d7/`.
- `tests/fixtures/def_hashes.json`: os seis documentos da D-7, item (h), com hash calculado pelo `canonicalize` do Hexlog TS no commit `5d1e22b`, com comando e commit anotados.
- Violações e ponteiros: escritos à mão a partir das tabelas acima (a suíte oficial só confere válido ou inválido).

### Testes da T3

| Arquivo | Testes |
|---|---|
| `tests/catalog.rs`, os 10 do plano (linhas 676-685) | `it_should_reject_a_schema_with_pattern_or_pattern_properties`, `it_should_reject_an_external_ref`, `it_should_reject_a_schema_whose_root_is_not_an_object`, `it_should_reject_a_malformed_equivalent_uri_and_accept_a_wellformed_one_without_resolving_it` (com os casos de D-7 item h: espaços nas pontas, tab, LF e controle C0 recusados com `equivalent-uri-format`; `urn:` e `mailto:` aceitos), `it_should_compute_a_stable_def_hash`, `it_should_list_every_violation_with_json_pointers_up_to_50` (60 violações dão 51 itens: os 50 primeiros e `too-many-errors`), `it_should_refuse_numbers_that_are_not_safe_integers_in_data`, `it_should_accept_data_that_satisfies_the_schema`, `it_should_load_type_and_relation_type_definitions_from_json`, `it_should_reject_an_invalid_type_name_when_deserializing` |
| `tests/catalog.rs`, os 14 de D-7 item (f) | `it_should_reject_a_ref_cycle_without_descending_into_the_instance` (`$ref: "#"`, `allOf`, `anyOf` e `not` apontando `#`, e `a -> b -> a` em `$defs`: um `ref-cycle` por `$ref` no ciclo, no ponteiro do `$ref`); `it_should_accept_a_recursive_ref_guarded_by_properties` (`properties`, `items` e `additionalProperties` recursivos compilam); `it_should_validate_a_shared_ref_dag_in_polynomial_time` (`d0..d30` com `allOf` duplo e `anyOf` duplo inválido, em thread com `recv_timeout` de 5 s); `it_should_not_overflow_the_stack_on_deeply_nested_data` (126 níveis de dado, schema recursivo por `$ref` com `allOf` aninhado, thread de 2 MiB); `it_should_report_max_depth_exceeded_on_a_very_long_ref_chain` (cadeia de 10.000 `$defs`: uma violação `max-depth-exceeded`, sem pânico); `it_should_treat_property_names_as_names_not_keywords` (chaves `pattern`, `$ref` e `$defs` em `properties` compilam); `it_should_resolve_ref_pointers_with_escapes_and_empty_tokens` (`a~1b`, `c~0d`, `#/$defs/`, `x%20y`, `q%22` resolvem e validam); `it_should_reject_a_ref_to_a_non_schema_location` (`#/enum/0`, `#/properties` e alvo inexistente: `ref-unresolved`; `other.json#`, `http://x/y#` e `#foo`: `ref-not-internal`); `it_should_evaluate_ref_siblings` (`$ref` com `maxLength: 2` reprova `"abc"` com `max-length`); `it_should_not_look_into_all_of_for_additional_properties` (`a` vinda só de `allOf` é recusada pelo `additionalProperties: false` do pai); `it_should_report_a_single_violation_for_a_failing_any_of` (`any-of` e `one-of` no ponteiro do valor, sem erros dos ramos); `it_should_count_string_length_in_code_points` (`"💩"` com `maxLength: 1` passa); `it_should_use_a_pointer_with_tilde_escapes_in_violations` (chave `a/b~c` vira `/a~1b~0c`); `it_should_reject_non_safe_integers_in_the_schema` (`minimum: 1.5`, `enum: [1.0]`, `const` aninhado com `1e2` e `minimum: 9007199254740992` dão `number-not-safe-integer` com ponteiro; `maximum: 9007199254740991` passa) |
| `tests/catalog.rs`, 5 acréscimos | `it_should_name_the_missing_property_in_the_required_pointer` (`required` ausente dá ponteiro `/m1`, um achado por propriedade, na ordem do `required`); `it_should_reject_a_keyword_outside_the_whitelist` (`requred`, `$id`, `$dynamicRef`, `$async`, `uniqueItems`: `keyword-not-allowed` com o ponteiro da palavra); `it_should_accept_annotation_keywords_and_check_their_type` (as oito anotações aceitas; `title: 1` dá `keyword-type`); `it_should_not_add_too_many_errors_at_exactly_50_violations` (49 e 50 violações dão 49 e 50 itens sem marcador; 51 e 60 dão 51); `it_should_refuse_data_nested_beyond_the_record_body_limit` (126 níveis passam, 127 dão `max-depth-exceeded`) |
| `tests/catalog.rs`, `format` (D-11) | `it_should_accept_format_attachment_on_a_top_level_property_its_items_and_a_nullable_string`, `it_should_reject_a_format_other_than_attachment`, `it_should_reject_format_attachment_outside_the_top_level_property_positions`, `it_should_require_a_sha256_hash_in_a_field_marked_as_attachment` (`"abc"`, 64 `A` e 63 `a` dão `format`; `null` passa com `type: ["string", "null"]`) |
| `tests/catalog.rs`, extremos (D-11) | `it_should_reject_a_relation_type_with_an_empty_endpoint_list` (`from: []` e `to: []` recusados; ausente e `null` aceitos), `it_should_reject_a_relation_type_with_a_repeated_endpoint_type` |
| `tests/catalog.rs`, 6 da D-18 | `it_should_accept_the_2020_12_schema_uri_only_at_the_root` (o URI exato na raiz é aceito; outro URI na raiz dá `schema-uri` em `/$schema`; em `properties/a` dá `schema-uri` em `/properties/a/$schema`; valor não texto dá `keyword-type`); `it_should_reject_unknown_fields_when_deserializing_definitions` (`TypeDef` e `RelationTypeDef` com campo extra dão `Err`); `it_should_read_null_endpoint_lists_as_none_and_never_write_null` (`from: null`, `to: null` e `equivalent: null` leem `None`, e `to_value` omite as chaves); `it_should_cap_the_refusal_of_a_catalog_at_50_details_with_a_single_marker` (60 palavras desconhecidas dão 51 itens, o último `("", "too-many-errors")`; 50 dão 50 sem marcador); `it_should_point_ref_pattern_and_type_refusals_at_the_offending_keyword` (`$ref` externo e `$ref` sem alvo em `/properties/a/$ref`; `pattern` em `/properties/a/pattern`; `patternProperties` em `/patternProperties`; `title: 1` em `/title`); `it_should_report_a_ref_cycle_once_per_ref_on_the_cycle` (os cinco casos acima, com os pares exatos) |
| `tests/catalog.rs`, 5 da D-23 (T3b) | `it_should_reject_a_repeated_type_or_relation_name` (`[t, t, t]` dá exatamente `[("/types/t", "duplicate-name")]`; `[r, r]` dá `[("/relations/r", "duplicate-name")]`); `it_should_accept_a_type_and_a_relation_type_with_the_same_name`; `it_should_reject_an_empty_type_list_and_a_repeated_required_name` (`keyword-type` em `/type` e em `/required`); `it_should_accept_a_repeated_enum_value_and_an_empty_required_list`; `it_should_report_only_the_json_guard_findings_when_the_schema_also_has_other_problems` (`minimum: 1.5` e `pattern` no mesmo schema dão só `number-not-safe-integer`; outro tipo com `pattern` ainda dá `pattern-not-allowed`) |
| `tests/catalog.rs`, `def_hash` | `it_should_match_the_ts_oracle_def_hash_for_each_definition_shape`: os seis documentos de D-7 item (h), um `Catalog` por documento (0 e 1 têm o nome `note`; 3 e 4, `after`), contra `tests/fixtures/def_hashes.json`, gerada pelo oráculo TS (`canonicalize` do Hexlog TS no commit `5d1e22b`), com comando e commit no topo |
| `src/catalog/types/tests.rs` (unidade; o executor declara `#[cfg(test)] mod tests;` em `types.rs`) | `it_should_expose_requires_snapshot_def_hash_and_acyclic_through_the_definitions_port` |
| `src/catalog/validate/tests.rs` (unidade; declarado em `validate.rs`) | `it_should_pass_the_filtered_official_suite_without_skips`: lê `tests/fixtures/json-schema-test-suite/` por `concat!(env!("CARGO_MANIFEST_DIR"), ..)`, afirma 21 arquivos, 124 grupos e 392 testes antes de rodar, não tem ramo de pular, e usa `compile` e `CompiledSchema::validate` (costura 6b) |

Total de `tests/catalog.rs`: 47 (42 da T3 e 5 da emenda T3b, D-23). A T3 traz o `petgraph` ao `Cargo.toml` (D-18; antes previsto para a T5b). Fixtures da T3: `tests/fixtures/def_hashes.json` e `tests/fixtures/json-schema-test-suite/` (os 21 arquivos filtrados no commit `245a45abc90190c0bff04d97e6d2b541b4bb9f12` da suíte oficial, `LICENSE` MIT, `filter.py` e `MANIFEST.md` com repositório, commit, data, comando e contagens por arquivo). `tests/support/catalog.rs` recebe o catálogo de referência que as tabelas da seção 5 e a tabela de mutações usam (`note`, `strict-note` com `requires_snapshot`, a relation `after` com `acyclic` e os demais tipos citados lá).

## T4. Store SQLite com `sqlx` (caixa cinza `store/sqlite`)

Decisões em D-6 do log do crítico; fundamento em [store-sqlx.md](../research/gray-box/store-sqlx.md) e [store-sqlite.md](../research/gray-box/store-sqlite.md). Testes dentro do crate, em `src/store/sqlite/tests.rs` (declarado por `#[cfg(test)] mod tests;` em `sqlite/mod.rs`), porque nada do store é público; o arquivo passa pelo varredor: nada de `check`, `git`, `ignore_check_constraints` e afins em identificador ou literal, e o texto `CHECK constraint failed` não pode ser usado como asserção.

### Costura 7: `SqliteStore` e as portas

Pega: esquema, travas, abertura e fechamento, cada leitura das portas contra SQLite real. Deixa passar: as regras do `decide`.

```rust
pub(crate) struct SqliteStore { /* dono de Option<SqliteConnection> */ }
impl SqliteStore { pub(crate) fn open(db_path: &std::path::Path, busy_timeout: std::time::Duration) -> Result<SqliteStore, HexlogError>; }
impl Drop for SqliteStore { /* block_on(conn.close()) */ }
impl Store for SqliteStore { /* ensure_tenant, append_with, read: plano, seção 4.4 */ }
```

- Abertura: os 5 passos do plano, seção 4.3. `user_version` maior que 1, ou banco não vazio com versão 0: `ErrorCode::StoreError`, `details[0].code == "unsupported-schema-version"`.
- `append_with`: `begin_with("BEGIN IMMEDIATE")`, chama `decide`, insere `plan.records` e `plan.batch`, `commit()` explícito, devolve o plano. `Err` de `decide` faz `rollback()` e nada é gravado. Plano sem records e sem lote (replay) não grava nada.
- Tradução de erro (D-9): código estendido 2067 cuja mensagem do SQLite cita `records.revises` ou `records.lineage_key` vira `ForkRejected`. Qualquer outro erro do banco (2067 de outra coluna, 1555 de `batches`, 787, 1811 de trigger, 275) vira `StoreError` com `details[0].code` igual ao código estendido em decimal (`"1811"`), `path` vazio e `message` fixa `database error`, nunca a mensagem do SQLite, que pode trazer o caminho do banco. Erro que não é do banco: `"io"`, `"configuration"`, `"protocol"`, senão `"other"`. `unsupported-schema-version` segue o único código em palavras, na abertura.
- O store não cria o diretório pai do banco: `open` num diretório inexistente dá `StoreError` com `"14"`; quem cria é o `omhl` (T6).
- `StoreReader` herda dois `tenant` (`WriteReads` e `VerifyReads`): com os dois traits no escopo, `r.tenant(..)` dá E0034; chame `WriteReads::tenant(r, ..)`.
- `SQLITE_BUSY` (código primário 5, `código & 0xFF`, inclusive 261 e 517): `ErrorCode::Busy`.
- Depois de erro dentro de transação que não seja violação de restrição, o store fecha e reabre a conexão antes da próxima chamada (issue 4434 do `sqlx`).
- `read`: `begin()`, chama `f`, `commit()`; `f` recebe `&mut dyn StoreReader`.
- `u64` é lido com `try_into` e ligado como `i64`; leitura por `Row::try_get`; SQL em `const &'static str`, sem `query!` nem `SqlitePool`.
- Colunas derivadas nunca são lidas para devolver payload: `body` é a fonte (plano, princípio 2).

Leituras das portas, com a semântica das seções 4.3 e 4.4 do plano:

| Método | Afirma |
|---|---|
| `tenant` | linha do tenant; `None` para desconhecido |
| `chain_head` | tenant desconhecido: `Err` `TenantNotFound`, `path` vazio, sem ecoar o tenant; tenant vazio: `seq` 0 e a âncora; depois, o último record |
| `latest_record` | o record sem sucessor da lineage, para node e para relation; `None` sem records |
| `record_by_id` | a linha inteira, com as colunas; não filtra por tenant (o id leva o prefixo), e a `decide` confere `row.tenant` |
| `batch_by_key`, `batch_records` | a linha do lote e seus records em ordem de `seq` |
| `live_relation_pairs` | só relations vigentes cujos extremos do tenant também estão vigentes; extremo de origem `native` conta como vigente |
| `current_nodes` | latest record vigente de cada node, filtrado por `type_name`, `snapshot`, `include_retired`, `id_prefix` (`lineage_key = ?1 OR substr(lineage_key, 1, length(?1) + 1) = ?1 \|\| '.'`, sem `LIKE`) e `where_eq` (`json_type(body, ?1) = ?2 AND json_extract(body, ?1) IS ?3`, caminho `$.data."<nome>"` ligado, `?2` literal fixo em Rust: `text`, `integer`, `true` ou `false`; `Int(1)` não casa `true`, `Str("1")` não casa `1`, `null` nunca casa); no máximo `page.limit` linhas, `seq` crescente, `seq > after_seq` (a Facade pede `limit + 1` para preencher `next_after_seq`) |
| `revisions` | todos os records da lineage em ordem de `seq` |
| `latest_relations` | latest record de cada relation, com os filtros de `RelationQuery` |
| `records_from` | a partir de `from_seq`, até `limit`, em ordem de `seq` |
| `batches` | todas as linhas de lote do tenant |

### Costura 8: `tests/support/tamper.rs`

Conexão `sqlx` crua ao arquivo, com `foreign_keys(false)` e `.pragma("ignore_check_constraints", "ON")`, que remove os sete triggers por literais fixos. `Tamper::open(path: &Path) -> Tamper` não cria o arquivo (o store já o criou). API: `execute(&mut self, sql: &str, binds: &[&str]) -> u64` (linhas alteradas); `read_text(&mut self, sql: &str, binds: &[&str]) -> Vec<String>` (primeira coluna de cada linha, com `CAST(.. AS TEXT)` no SQL); `read_body(&mut self, tenant: &str, seq: u64) -> String`, atalho de `read_text`. O SQL é montado em Rust a partir de literais fixos e passa por `AssertSqlSafe`; valores sempre ligados. Bind de texto em coluna `INTEGER` de tabela `STRICT` é coagido (`seq := '99'` altera a linha), então `&[&str]` cobre toda a tabela de mutações (D-9, item g). Usa `futures_executor::block_on`.

### Testes da T4

Em `src/store/sqlite/tests.rs`: os 9 do plano (linhas 785-793), `it_should_reopen_the_same_database_repeatedly_without_a_lock_error` (100 vezes abre, escreve, solta e reabre), e os das leituras: `it_should_read_a_tenant_row_and_none_for_an_unknown_tenant`, `it_should_report_the_anchor_as_chain_head_of_an_empty_tenant_and_the_last_record_afterwards`, `it_should_find_the_latest_record_of_node_and_relation_lineages`, `it_should_find_a_record_by_id`, `it_should_find_a_batch_by_key_and_read_its_records_in_order`, `it_should_list_live_relation_pairs_without_retired_relations_or_retired_endpoints`, `it_should_list_current_nodes_by_each_filter`, `it_should_list_the_revisions_of_a_lineage_in_order`, `it_should_list_latest_relations_by_each_filter`, `it_should_read_records_from_a_seq_up_to_a_limit`, `it_should_list_the_batches_of_a_tenant`, `it_should_persist_nothing_when_decide_fails`, `it_should_append_records_and_batch_atomically`, `it_should_translate_a_unique_violation_into_fork_rejected` (raiz duplicada e segundo sucessor; `id` repetido dá `StoreError` `"2067"`), `it_should_report_busy_when_another_connection_holds_the_write_lock` (segunda conexão em `BEGIN IMMEDIATE`, `busy_timeout` de 50 ms, `ErrorCode::Busy`), `it_should_translate_a_trigger_abort_into_store_error_with_its_code` (`"1811"`, plano com `seq` pulado), `it_should_write_nothing_for_an_empty_plan_from_a_replay` e `it_should_accept_a_new_write_after_a_non_constraint_error_inside_a_transaction` (uma conexão crua renomeia `batches`, a escrita falha com `StoreError`, a conexão crua desfaz o nome e a escrita seguinte grava), `it_should_open_in_wal_mode_and_enforce_foreign_keys` (`PRAGMA journal_mode` por conexão crua dá `wal`; lote com `first_seq` inexistente dá `StoreError` `"787"`) e `it_should_translate_a_duplicate_batch_key_into_store_error` (`"1555"`, nunca `IdempotencyConflict`). Total: 30. Sem teste: o código 275 (não sai pelo `AppendPlan`) e `synchronous = FULL` (não observável sem injetar falha).

Fixado na conferência dos testes: a mensagem de topo do `StoreError` também é `database error`; `read` devolve sem alteração o `Err` do fechamento; `append_with` devolve o plano recebido; `records_from` inclui `from_seq`; `batch_records(first_seq, count)` cobre `[first_seq, first_seq + count)`; `latest_relations` e `batches` saem em `seq` crescente; `where_eq` sobre um latest que é retirement não casa nada. Os testes leem o nome do tipo de snapshot da fixture dourada, como `src/engine/model/tests.rs`.

Cuidados para quem escreve: o `AppendPlan` é montado à mão, com a cadeia certa por `hexlog::chain::{anchor_doc, anchor, hash_record, canonical}` (os triggers conferem `seq` contíguo e `prev_hash`, não o hash); o `body` do teste de `where_eq` é JSON com `data`; `tests.rs` está em `src/` e passa pelo varredor de vocabulário; use `WriteReads::tenant(r, ..)`.

## T5a. `decide`, nós, lote, idempotência e Facade básica (caixas cinza `engine/decide`, `engine/rules`, `facade`)

Decisões em D-12 a D-17 do [log do crítico](omhl-v2-core-step3-critic-log.md); fundamento no plano (seções 4.4 e 4.5) e em `commands/register/state.ts` do Hexlog TS. Fora da T5a: itens `Relation` e `Retirement` de relation (T5b), `relations`, `verify_chain` e a política `Full` (T5c; na T5a `Full` vale como `Tail`).

### Costura 10: Facade (API pública `hexlog::*`)

Pega: abertura, tenant, `append` de ponta a ponta contra SQLite real, `get`, `lineage_records`, `list`, `chain_head`. Deixa passar: relations e `verify_chain`.

```rust
// src/facade.rs; reexportados em lib.rs: Config, VerifyPolicy, Deps, Hexlog, TenantInfo, NodeView, RevisionView, Page
pub struct Config { pub db_path: PathBuf, pub limits: Limits, pub busy_timeout: Duration, pub verify_policy: VerifyPolicy }
impl Config { pub fn new(db_path: PathBuf) -> Config; } // Limits::default(), 5 s, VerifyPolicy::Tail
pub enum VerifyPolicy { Tail, Full }                    // Debug, Clone, Copy, PartialEq, Eq
pub struct Deps<'d> { pub clock: &'d dyn Clock, pub ids: &'d dyn IdGenerator, pub catalog: Catalog, pub native: &'d dyn NativeNodes }
pub struct Hexlog<'d> { /* privado; Send */ }
impl<'d> Hexlog<'d> {
    pub fn open(config: &Config, deps: Deps<'d>) -> Result<Hexlog<'d>, HexlogError>;
    pub fn ensure_tenant(&mut self, tenant: &TenantId) -> Result<TenantInfo, HexlogError>;
    pub fn append(&mut self, req: &AppendRequest) -> Result<AppendOutcome, HexlogError>;
    pub fn get(&mut self, tenant: &TenantId, id: &SemanticId) -> Result<Option<NodeView>, HexlogError>;
    pub fn lineage_records(&mut self, tenant: &TenantId, lineage: &Lineage) -> Result<Vec<RevisionView>, HexlogError>;
    pub fn list(&mut self, tenant: &TenantId, filter: &ListFilter, page: &PageRequest) -> Result<Page<NodeView>, HexlogError>;
    pub fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, HexlogError>;
    pub fn catalog(&self) -> &Catalog;
}
pub struct TenantInfo { pub tenant: TenantId, pub anchor: Hash, pub created_at: String, pub created: bool }
pub struct Page<T> { pub items: Vec<T>, pub next_after_seq: Option<u64> }
pub struct NodeView { pub id: SemanticId, pub type_name: TypeName, pub record_id: RecordId, pub seq: u64, pub at: String, pub data: Value, pub snapshot: Option<Snapshot>, pub external_id: Option<ExternalId>, pub author: Author, pub shadowed: bool }
pub struct RevisionView { pub record_id: RecordId, pub seq: u64, pub at: String, pub revises: Option<RecordId>, pub retired: bool, pub data: Option<Value>, pub snapshot: Option<Snapshot>, pub external_id: Option<ExternalId>, pub author: Author }
// Todos: Debug, Clone, PartialEq (Config e VerifyPolicy também).
```

- `open`: recusa `limits.batch_items` fora de 1 a 50, `limits.details_max` fora de 1 a 50 e `limits.page_max` fora de 1 a 200 com `InvalidInput`, detalhe `limits-out-of-range`, `path` `/limits/<campo>`; depois `SqliteStore::open(&config.db_path, config.busy_timeout)`.
- `ensure_tenant`: lê `Clock::now()` (forma de 24 caracteres, senão `Internal` `clock-format`), monta `chain::anchor_doc` e `chain::anchor`, chama `Store::ensure_tenant` e devolve a linha guardada. Tenant que já existe devolve o `created_at` e a âncora de quando nasceu, com `created: false`.
- `append`: lê `Clock::now()` uma vez, monta o `DecideCtx` e faz `store.append_with(&req.tenant, &mut |reads| decide(&ctx, req, reads))`; devolve `plan.outcome`.
- Leituras (`get`, `lineage_records`, `list`, `chain_head`): tenant desconhecido dá `TenantNotFound` (`path` `/tenant`, detalhe `tenant-not-found`) em todas. `get`: latest de `Lineage::Node(id)`; ausente ou retirado dá `None`; senão `record_by_id` e `NodeView` montada do **`body`** (`data`, `author`, `at`, `snapshot`, `external_id`, `type`), mais `seq` e `record_id` da linha, e `shadowed = native.lookup(id).is_some()`. `lineage_records`: `revisions`; lineage sem records dá `Vec` vazio; `data` é `None` na retirement. `list`: `page.limit` fora de 1 a `limits.page_max` dá `InvalidInput` `page-limit` em `/page/limit`; a Facade pede `limit + 1` a `current_nodes`, devolve `limit` itens e `next_after_seq` = `seq` do último item devolvido quando sobrou linha, senão `None`; `id_prefix` não é validado (texto que não é prefixo não casa nada). Body que não lê ou campo que não parseia: `Internal` `invalid-body`.

### Costura 11: `decide` e `rules` (internas)

Pega: ordem de recusa, regras de um vigente, snapshot e `external_id`, montagem do record, replay. Deixa passar: SQL (T4), relations (T5b), cadeia completa (T5c).

```rust
// engine/mod.rs: `pub(in crate::engine) mod decide; pub(in crate::engine) mod rules; pub(crate) use decide::{decide, DecideCtx};` (só junto com o consumidor, a Facade)
pub struct DecideCtx<'a> { pub defs: &'a dyn Definitions, pub native: &'a dyn NativeNodes, pub ids: &'a dyn IdGenerator, pub limits: &'a Limits, pub now: &'a str }
pub fn decide(ctx: &DecideCtx<'_>, req: &AppendRequest, reads: &mut dyn WriteReads) -> Result<AppendPlan, HexlogError>;
// engine/rules.rs: funções puras, sem leitura; `pub` dentro do módulo restrito
pub fn validate_shape(req: &AppendRequest, limits: &Limits) -> Result<(), HexlogError>;                                                        // passo 1
pub fn static_rules(req: &AppendRequest, defs: &dyn Definitions, native: &dyn NativeNodes, limits: &Limits) -> Result<(), HexlogError>;       // passo 4
pub fn instant_is_well_formed(text: &str) -> bool;                                                                                              // D-13
```

Quem chama o quê: `Hexlog::append` chama `Store::append_with` (que abre `BEGIN IMMEDIATE`), que chama `decide(ctx, req, reads)`, que usa `rules::*` e `reads: &mut dyn WriteReads`. O `decide` não conhece `Store`, `catalog` nem SQL; a Facade passa `&self.deps.catalog` como `&dyn Definitions` e lê os campos de `self` separadamente (campos disjuntos compilam; chamar método de `&mut self` com o `ctx` emprestado não).

Ordem de execução do `decide` (tudo dentro da transação, D-12):

| Passo | O que faz | Erro | Detalhe `code` / `path` |
|---|---|---|---|
| 1 `validate_shape` | `items` com 0 ou mais de `limits.batch_items`; `author` fora de 1 a `limits.author_field_max` unidades UTF-16; por item: corpo de relation (T5a); `data` com mais de `JSON_NESTING_MAX` contêineres. Primeiro achado, um detalhe | `InvalidInput` | `batch-size` `/items`; `author-format` `/author/agent`, `/author/client`, `/author/model`; `relation-items-not-supported` `/items/<i>/body`; `max-depth-exceeded` `/items/<i>/body/data` |
| 2 tenant | `reads.tenant` ausente | `TenantNotFound` | `tenant-not-found` `/tenant` |
| 3 replay | `fingerprint(&req.items)` (sempre; `Err` guarda `None` e pula a consulta); com `key`, `batch_by_key`: impressão igual é replay, diferente é conflito | `IdempotencyConflict` | `key-reused` `/key` |
| 4 `static_rules` | passada A (primeiro achado, código próprio): tipo ausente; id reservado. Passada B (acumula num só `InvalidRecord`): por item, `validate_data`, `data-too-large`, regras de `snapshot` e de `external_id` | `TypeNotFound`; `ReservedId`; `InvalidRecord` | `type-not-found` `/items/<i>/body/type`; `reserved-id` `/items/<i>/body/id`; códigos do catálogo em `/items/<i>/body/data<ponteiro>`; `data-too-large` `/items/<i>/body/data`; `snapshot-required` e `snapshot-not-allowed` `/items/<i>/snapshot`; `external-id-not-allowed` `/items/<i>/external_id` |
| 5 cauda | `chain_head`; com `seq > 0`, `batch_records(tenant, head.seq, 1)` e `hash_record(prev_hash, body) == hash == head.hash` | `ChainBroken` | `tail-hash-mismatch` `""` |
| 6 histórico | por item em ordem, regras abaixo | `ForkRejected`, `InvalidRecord` | abaixo |

Passo 4, detalhes. Id reservado: `Node` cujo id `NativeNodes::lookup` reconhece, inclusive revisão (o nativo prevalece); `Retirement` de node não é recusado. Passada B, por item e nesta ordem: (1) `defs.validate_data(&tipo, &data)`: cada achado vira detalhe com `path` `/items/<i>/body/data` mais o ponteiro, `code` o do achado, `message` fixa por código, nunca o valor; marcadores `too-many-errors` do item são descartados; (2) se (1) não achou nada, JCS de `data` com mais de `limits.data_max_chars` unidades UTF-16 dá `data-too-large`; (3) `Node` com `requires_snapshot` e sem `snapshot` dá `snapshot-required`; `Node` sem `requires_snapshot` e com `snapshot` dá `snapshot-not-allowed`; `Retirement` com `snapshot` dá `snapshot-not-allowed`; (4) `Retirement` com `external_id` dá `external-id-not-allowed`. Corte do lote (F-3): concatenados os achados em ordem de item, se passarem de `limits.details_max`, ou algum item trouxe marcador, saem os primeiros `details_max` mais um `too-many-errors` com `path` `""`. `snapshot-format` e `external-id-format` nascem na construção dos tipos (`Snapshot::new`, `ExternalId::parse`), nunca no `decide`.

Passo 6, por item em ordem; a chave da lineage é o id do node:
1. Chave já vista neste lote: `ForkRejected`, detalhe `lineage-in-batch`, `path` `/items/<i>/body`, `current` e `retired` ausentes. Vale para `Node` e `Retirement` em qualquer ordem.
2. `latest = reads.latest_record(tenant, &Lineage::Node(id))`.
3. `Node`: sem `latest` e com `base`: `InvalidRecord` `base-unknown` `/items/<i>/base`. Com `latest` e `base` ausente ou diferente do record id dele: `ForkRejected`, `not-current`, `/items/<i>/base`, `current = Some(latest.record_id)`, `retired = Some(latest.retired)`. Com `base` certo e tipo diferente de `latest.type_name`: `InvalidRecord` `type-changed` `/items/<i>/body/type`. Revisar sobre retirement é permitido.
4. `Retirement` de node: sem `latest`: `InvalidRecord` `lineage-not-found` `/items/<i>/body/lineage`; `base` ausente ou diferente: `ForkRejected` `not-current` como acima; `base` certo e `latest.retired`: `InvalidRecord` `already-retired` `/items/<i>/body/lineage`.

Montagem (depois dos 6 passos; `now` validado aqui por `instant_is_well_formed`, senão `Internal` `clock-format`): `seq` a partir de `head.seq + 1`; `prev_hash` do primeiro é `head.hash`; `IdGenerator::next` uma vez por record, na ordem dos itens, `RecordId` = `<tenant>:<uuid hifenizado minúsculo>` (falha de formato dá `Internal` `id-generator`); mesmo `at` em todos; `type` e `def_hash` do catálogo em `Node` e **do latest em `Retirement`**; `revises` = record id do latest quando há; `retirement` verdadeiro só na retirement; `data` só em `Node`; `batch` (`fingerprint`, `count`, `key` se houver) só no primeiro record, com ou sem `key`; `native_catalog` ausente; corpo por `RecordBody::canonical`, hash por `hash_record`. `BatchRow` só com `key`. `outcome.records` na ordem dos itens; `chain_head` do último record. Replay: `records` vazio, `batch: None`, `replayed: true`, ids de `batch_records(tenant, row.first_seq, row.item_count)`, `chain_head` atual do tenant. `fingerprint` com `Err` que chega ao fim do passo 4 é `Internal` `fingerprint-unreachable`.

Ponteiro da profundidade (D-14): vale `/items/<i>/body/data` em tudo. O `Err` de `hexlog::chain::fingerprint` passa de `/<i>/data` a `/<i>/body/data`, o ponteiro do mesmo campo no item serializado: o test-engineer da T5a ajusta `it_should_fail_the_fingerprint_for_data_nested_beyond_126_levels_without_overflowing` e o executor, `chain.rs`.

### Suporte de teste da T5a (`tests/support/mod.rs`, escrito pelo test-engineer)

`Harness { dir: TempDir, clock: FixedClock, ids: SeqIds, native: FakeNative }` com `new()`, `config()` (banco em `dir/hexlog.db`, `Config::new`), `open(&self) -> Hexlog<'_>` (`config()` e `reference_catalog()`) e `open_with(&self, config: Config, catalog: Catalog) -> Hexlog<'_>`. `FakeNative::reserve` pede `&mut`: o teste larga o `Hexlog`, altera o fake e reabre (E0502 se o `Hexlog` ainda for usado depois). Construtores sem lógica de produto: `author()` (`agent` "tester", `model` `None`, `client` "cli"), `tenant(&str)`, `sid(&str)`, `ty(&str)`, `node(id, type_name, data) -> AppendItem` (sem `base`, `snapshot` nem `external_id`), `revise(item, &RecordId) -> AppendItem` (põe `base`), `retire(id, &RecordId) -> AppendItem`, `request(tenant, key: Option<&str>, items) -> AppendRequest`. O `reference_catalog()` da T3 tem `note` (propriedades opcionais `text`, string com `maxLength` 200, e `count`, inteiro com `minimum` 0), `strict-note` (`requires_snapshot`), `task` (`{"type":"object"}`) e as relations `after` (`acyclic`) e `owned-by` (`from: [task]`, `to: [note]`). Os testes de profundidade usam uma chave que não é `text` nem `count`, e `type-changed` usa `task`. Testes que precisam de violações definem um `Catalog` local e usam `open_with` (por exemplo `titled`, com `required: ["title"]` e `title` string de `maxLength` 10; `wide`, com `required` de `f01` a `f20`, para o corte).

### Testes da T5a

`tests/node_lifecycle.rs` (24):

| Teste | Afirma |
|---|---|
| os 13 do plano (seção 5, C1) | como na tabela do plano; em `it_should_reject_a_second_root_for_an_existing_semantic_id` e `it_should_reject_a_revision_with_a_stale_base`: `ForkRejected`, `details[0].code == "not-current"`, `path` `/items/0/base`, `current` = id do latest, `retired == Some(false)`; em `it_should_reject_a_base_for_a_semantic_id_that_was_never_written`: `base-unknown` em `/items/0/base`; em `it_should_reject_a_revision_that_changes_the_type`: `type-changed` em `/items/0/body/type`; em `it_should_reject_data_violating_the_schema_and_list_every_violation_of_the_batch`: ponteiros `/items/0/body/data/...` e `/items/1/body/data/...`; em `it_should_reject_a_reserved_semantic_id`: `ReservedId` em `/items/0/body/id` |
| `it_should_round_trip_data_nested_to_the_limit_and_refuse_one_level_more` | 126 contêineres grava e `get` devolve `data` igual; 127 dá `InvalidInput` `max-depth-exceeded` em `/items/0/body/data` e `chain_head` não anda |
| `it_should_refuse_data_nested_thousands_of_levels_without_overflowing_the_stack` | 5.000 níveis em thread de 2 MiB: `InvalidInput`, sem abortar |
| `it_should_report_tenant_not_found_for_reads_and_writes_of_an_unknown_tenant` | `append`, `get`, `lineage_records`, `list` e `chain_head` dão `TenantNotFound` em `/tenant` |
| `it_should_reject_a_retirement_of_a_lineage_that_was_never_written` | `InvalidRecord` `lineage-not-found` em `/items/0/body/lineage` |
| `it_should_reject_retiring_a_node_that_is_already_retired` | `base` = id da retirement: `already-retired`; `base` = id antigo: `ForkRejected` `not-current` |
| `it_should_reject_a_retirement_with_a_stale_base` | nó com revisões 1 e 2; retirement com `base` da 1: `not-current`, `current` = id da 2, `retired == Some(false)` |
| `it_should_flag_retired_true_in_the_fork_detail_when_the_latest_record_is_a_retirement` | nó retirado; revisão com `base` antigo: `not-current`, `current` = id da retirement, `retired == Some(true)` |
| `it_should_fail_with_internal_and_write_nothing_when_the_clock_returns_a_malformed_instant` | relógio devolve `2026-11-07 12:00:00`: `Internal` `clock-format`; `chain_head` não anda |
| `it_should_stamp_every_record_of_a_batch_with_the_same_instant_and_contiguous_seq` | lote de 3: mesmo `at` nos 3 (via `lineage_records`), `seq` 1, 2, 3 |
| `it_should_create_a_tenant_once_and_derive_its_anchor_from_the_clock` | 1ª chamada `created: true`; 2ª `created: false`, mesma âncora e `created_at`; âncora igual a `chain::anchor(chain::anchor_doc(tenant, created_at))` |
| `it_should_report_the_anchor_as_chain_head_of_an_empty_tenant_and_the_last_record_after_a_write` | tenant vazio: `seq` 0 e `hash` = âncora; depois de um `append`: igual a `outcome.chain_head` |

(A primeira linha vale 13 testes; 13 + 11 = 24.)

`tests/snapshot_rules.rs` (11): os 11 do plano (S1 a S7, X1 a X4). Valor esperado: S1 `InvalidRecord` `snapshot-required` em `/items/0/snapshot`; S3, S5 e S6 `snapshot-not-allowed`; X2 `external-id-not-allowed` em `/items/0/external_id`; X3 chama `ExternalId::parse("")` e com 201 unidades UTF-16 e espera `InvalidRecord` `external-id-format`; X4 `get` devolve `external_id: None` depois da revisão sem ele.

`tests/list_filters.rs` (8): os 8 do plano. `it_should_page_by_seq_with_a_keyset_cursor`: 5 nós em 5 `append`s (seq 1 a 5), `limit` 2: páginas com `seq` [1,2] e `next_after_seq` `Some(2)`, [3,4] e `Some(4)`, [5] e `None`; com 4 nós, a 2ª página tem `next_after_seq` `None` (D-17).

`tests/batch.rs` (7):

| Teste | Afirma |
|---|---|
| `it_should_persist_nothing_when_any_item_of_the_batch_is_invalid`, `it_should_reject_empty_and_oversized_batches` (`batch-size` em `/items`) | como no plano |
| `it_should_reject_a_second_item_for_the_same_lineage_in_the_same_batch` | `ForkRejected` `lineage-in-batch` em `/items/1/body`, `current` e `retired` ausentes, nada gravado |
| `it_should_cap_the_violations_of_a_batch_at_50_with_a_single_marker` | 3 itens do tipo `wide`, 20 violações `required` cada: 51 detalhes, os 50 primeiros em ordem de item e de `required`, o 51º `too-many-errors` em `""`; com 2 itens (40), sem marcador |
| `it_should_reject_an_author_field_that_is_empty_or_longer_than_100_characters` | `agent`, `client` e `model` vazios ou com 101 unidades: `InvalidInput` `author-format` no campo |
| `it_should_reject_data_over_the_canonical_character_limit_counted_in_utf16_units` | `Config.limits.data_max_chars` 20; `data` cujo JCS tem 21 unidades (um caractere fora do plano básico conta 2): `InvalidRecord` `data-too-large` em `/items/0/body/data`; com 20, aceita |
| `it_should_report_type_not_found_before_aggregating_data_violations` | item 0 com `data` inválido e item 1 com tipo inexistente: `TypeNotFound` em `/items/1/body/type`, sem `INVALID_RECORD` |

`tests/idempotency.rs` (6): os 5 do plano, mais `it_should_replay_even_when_the_clock_returns_a_malformed_instant` (lote gravado; o relógio passa a devolver texto malformado; o reenvio dá `replayed: true`, e `outcome.chain_head` é o atual). Em `it_should_reject_the_same_key_with_different_items`: `IdempotencyConflict`, `key-reused` em `/key`.

`src/engine/rules/tests.rs` (unidade, fora da contagem; declarado por `#[cfg(test)] mod tests;` em `rules.rs`): `it_should_accept_only_the_24_character_utc_instant_shape` (aceita `2026-11-07T12:00:00.000Z`; recusa 23 e 25 caracteres, falta de `Z`, `T` trocado por espaço, letra no lugar de dígito).

Total da T5a: 56 testes (24 + 11 + 8 + 7 + 6), mais 1 de unidade e o ajuste de ponteiro em `jcs_vectors`. O teste de ponta a ponta do corpo cru (`it_should_store_the_golden_body_for_each_record_shape`) fica em `tests/append.rs`, na T5b.

### Riscos de compilação e de varredor (T5a)

- **Varredor de vocabulário (`src/`)**: veta `check`, `gate`, `unit`, `work`, `review`, `decision`, `premise`, `directive` e afins por palavra de `snake_case`, em identificador **e** em literal (`check_gate` é acusado, medido em `scanner_cases.txt`). Logo `check_shape`, `static_checks`, `work_item`, "check failed" em mensagem de erro e `review` não podem aparecer em `decide.rs`, `rules.rs` nem `facade.rs`; os nomes desta costura (`validate_shape`, `static_rules`, `tail-hash-mismatch`) já os evitam. `git` só em `engine/snapshot.rs`.
- **E0364 e `unused_imports`**: `pub(crate) use decide::{decide, DecideCtx};` exige `pub fn decide` e `pub struct DecideCtx`, e só entra junto com o consumidor (a Facade); `allow(dead_code)` não cobre `unused_imports`.
- **E0034**: `StoreReader` herda dois `tenant`. Dentro de `Store::read`, chame `WriteReads::tenant(r, ..)`; no `decide`, `reads` é `&mut dyn WriteReads` e a chamada direta compila. Não importe `WriteReads` e `VerifyReads` juntos em `facade.rs` sem qualificar.
- **E0502**: no `append`, `self.store.append_with(.., &mut |reads| decide(&ctx, ..))` com `ctx` emprestando `self.deps` compila (campos disjuntos, medido); passar por um método de `&mut self` não compila.
- **Lints do workspace**: `needless_pass_by_value` (receba `&AppendRequest`, `&DecideCtx`), `needless_pass_by_ref_mut`, `redundant_clone`, `implicit_clone`; nenhum `Rc`, `Arc`, `RefCell`, `Mutex`, `OnceLock`; `HashSet<&str>` é permitido. `u32::try_from(items.len())` em vez de `as`.
- **`store/mod.rs`**: a T4 já reexporta `pub(crate) use sqlite::SqliteStore;`.
- **Teste**: `Hexlog<'d>` empresta do `Harness`; alterar `native` exige soltar o `Hexlog` antes. `SeqIds` gera uuid v7 válido.

### Pronto (T5a)

Arquivos: `src/engine/{decide,rules}.rs`, `src/engine/mod.rs`, `src/facade.rs`, `src/lib.rs` (`pub use` da Facade e dos tipos públicos acima), `src/engine/chain.rs` (só o ponteiro da D-14), `tests/support/mod.rs` (`Harness` e construtores) e os cinco arquivos de teste. Verdes: `node_lifecycle` (24), `snapshot_rules` (11), `list_filters` (8), `batch` (7), `idempotency` (6), o de unidade de `rules` e `jcs_vectors` (11); `fmt`, `clippy -D warnings` e `test --workspace` limpos; `grep -rn "relation-items-not-supported" crates/hexlog/src` encontra só o ramo de `validate_shape`.

## T5b. Relations, `NativeNodes`, ciclos e corpo cru (caixas cinza `engine/decide`, `engine/rules`, `facade`)

Decisões em D-20 a D-22 do [log do crítico](omhl-v2-core-step3-critic-log.md); fundamento no plano (seções 4.4 e 4.5), na costura 5 e na D-8. Fora da T5b: `verify_chain`, `CHECKED_FIELDS` e a política `Full` (T5c).

### Costura 12: `Hexlog::relations` (API pública `hexlog::*`)

Pega: leitura das relations vivas e retiradas de um tenant, com o estado de cada extremo. Deixa passar: paginação (o `RelationQuery` do plano não tem limite; limite aceito) e verificação da cadeia.

```rust
// src/facade.rs; reexportados em lib.rs: RelationView, EndpointStatus (Origin e RelationQuery já são exportados)
impl<'d> Hexlog<'d> {
    pub fn relations(&mut self, tenant: &TenantId, query: &RelationQuery) -> Result<Vec<RelationView>, HexlogError>;
}
pub enum EndpointStatus { Current, Retired, MissingNative }   // Debug, Clone, Copy, PartialEq, Eq
pub struct RelationView {                                       // Debug, Clone, PartialEq
    pub record_id: RecordId, pub seq: u64, pub at: String, pub relation_type: RelationTypeName,
    pub from: SemanticId, pub to: SemanticId, pub from_origin: Origin, pub to_origin: Origin,
    pub from_status: EndpointStatus, pub to_status: EndpointStatus,
    pub native_catalog: Option<String>, pub external_id: Option<ExternalId>, pub author: Author, pub retired: bool,
}
```

- `relations` roda dentro de uma `Store::read`: `WriteReads::tenant` (ausente: `TenantNotFound`, `/tenant`, `tenant-not-found`), `latest_relations` e, por linha, uma `RelationView` montada do **`body`** (`at`, `author`, `lineage`, `external_id`, `native_catalog`; origens do `lineage` do corpo, não das colunas; `retired` = `retirement` do corpo; `record_id` e `seq` da linha). Ordem por `seq`. Sem `include_retired`, relation cujo latest é retirement não aparece.
- Estado de cada extremo, pela origem guardada: `Native` com `NativeNodes::lookup` presente é `Current`, e ausente é `MissingNative` (sem erro); `Tenant` com `latest_record(tenant, Node(id))` retirado é `Retired`, senão `Current`. Extremo do tenant sem nenhum record, corpo ilegível ou `lineage` que não é de relation: `Internal`, detalhe `invalid-body`.
- Os campos `from` e `to` do `RelationQuery` filtram por `from_id` e `to_id`, qualquer origem; `to` sozinho é a leitura inversa.

### Costura 13: regras de `Relation` e de `Retirement` de relation (internas, em `decide` e `rules`)

Muda o que a costura 11 (T5a) já tem: o passo 1 deixa de recusar relation, o passo 4 ganha dois códigos e o passo 6 ganha o ramo de relation. As assinaturas de `decide`, `validate_shape` e `static_rules` não mudam; `rules.rs` ganha só a função pura nova `fn endpoint_types_allowed(info: &RelationTypeInfo, from: &str, to: &str) -> Result<(), Endpoint>` (diz qual extremo falha; `&str` porque o tipo do extremo pode vir como `TypeName` do lote e do nativo ou como `Name` do latest gravado), `pub` dentro do módulo restrito. O teste de ciclo fica inline no `decide`: `has_path_connecting(edges, to, from, None)` (emenda pós-revisão da T5b: a função `closes_cycle` só repetia essa chamada).

Passo 1: o ramo `relation-items-not-supported` **sai** por inteiro.

Passo 4 (estáticas), acréscimos:

| Passada | O que faz | Erro | Detalhe `code` / `path` |
|---|---|---|---|
| A (primeiro achado) | `Relation` com relation type fora do catálogo | `RelationTypeNotFound` | `relation-type-not-found` `/items/<i>/body/relation_type` |
| B (acumula) | `Relation` com `from == to` | `InvalidRecord` | `self-relation` `/items/<i>/body/to` |
| B | `Relation` com `snapshot` (S8) | `InvalidRecord` | `snapshot-not-allowed` `/items/<i>/snapshot` |

`external_id` vale em `Relation` (X1) e não vale em `Retirement` de relation (X2, já da T5a). `Retirement` de relation não consulta o catálogo: o relation type pode ter saído dele, e a retirada continua possível.

Passo 6 para um item `Relation` (raiz ou revisão), em ordem, parando no primeiro achado:

| # | Regra | Erro | Detalhe `code` / `path` |
|---|---|---|---|
| 1 | lineage já vista no lote (a chave é o par tipo de lineage e chave; a chave de relation é `<relation_type>\|<from>\|<to>`; vale entre `Relation` e `Retirement` da mesma lineage) | `ForkRejected` | `lineage-in-batch` `/items/<i>/body`, sem `current` |
| 2 | sem latest e com `base` | `InvalidRecord` | `base-unknown` `/items/<i>/base` |
| 2 | com latest e `base` ausente ou diferente do record id dele | `ForkRejected` | `not-current` `/items/<i>/base`, `current = Some(latest.record_id)`, `retired = Some(latest.retired)` |
| 3 | extremo (`from` antes de `to`), resolvido por (a) `NativeNodes::lookup` (o nativo prevalece, mesmo que o tenant tenha node de mesmo id), (b) sobreposição do lote (`Node` entra vigente com o tipo; `Retirement` de node entra retirado), (c) `latest_record(tenant, Node(id))`; sem resultado, ou do tenant e retirado | `InvalidRecord` | `endpoint-not-found` `/items/<i>/body/from` ou `/to` |
| 4 | os dois extremos nativos | `InvalidRecord` | `native-only-relation` `/items/<i>/body` |
| 5 | tipo do extremo (nativo: `NativeNode.type_name`; senão sobreposição ou latest) fora de `RelationTypeInfo.from` ou `.to` (`None` é qualquer) | `InvalidRecord` | `endpoint-type` `/items/<i>/body/from` ou `/to` |
| 6 | só se `acyclic`: há caminho `to ~> from` no grafo do relation type | `CycleRejected` | `cycle` `/items/<i>/body` |

Grafo do passo 6: arestas de `live_relation_pairs`, de `NativeNodes::native_relations` e dos itens `Relation` anteriores do lote com o mesmo relation type. **Retirement dentro do lote não remove aresta** (nem de relation nem de node): o lote nunca aceita ciclo por causa de retirada do próprio lote; quem quer isso manda dois lotes. A revisão de relation valida extremos e ciclo de novo; revisar sobre retirement reinstala a relation.

Passo 6 para `Retirement` de relation: não valida extremos nem ciclo (retirar com extremo retirado é a limpeza esperada). Regras do latest como no node: sem latest, `InvalidRecord` `lineage-not-found` `/items/<i>/body/lineage`; `base` errado, `not-current`; `latest.retired`, `already-retired` `/items/<i>/body/lineage`.

Montagem:
- `Relation`: `type` e `def_hash` do relation type (`RelationTypeInfo.def_hash`); `lineage` com `from_origin` e `to_origin` da resolução do item (na revisão, recalculadas); `native_catalog` = `NativeNodes::catalog_version()` só se algum extremo é nativo; `data` e `snapshot` ausentes; `external_id` se vier; `revises` = latest quando há; colunas `lineage_kind` relation, `relation_type`, `from_id`, `to_id` e origens.
- `Retirement` de relation: copia do latest `type`, `def_hash`, as duas origens e `native_catalog`. `LatestRow` não traz origens nem `native_catalog`: o `decide` chama `reads.record_by_id(latest.record_id)` e lê as origens das colunas e `native_catalog` do `body` (`RecordBody` desserializado; falha ou origem ausente: `Internal` `invalid-body`).
- `lineage_key` de relation é `<relation_type>|<from>|<to>` com U+007C. **A T5b move `lineage_columns` de `store/sqlite/tx.rs` para `engine/model.rs`** (`pub(crate) fn lineage_columns(lineage: &Lineage) -> (&'static str, Cow<'_, str>)`), e `tx.rs` e `decide` a usam. Duas implementações desencontradas deixam `latest_record` cego, e o `FORK_REJECTED` some em silêncio.

Forma do código do ciclo: carregar antes, montar depois. Primeira passada: para cada relation type `acyclic` que aparece em itens `Relation` do lote, buscar `live_relation_pairs` e `native_relations` e guardar os `Vec<(SemanticId, SemanticId)>` num `HashMap<&RelationTypeName, _>` completo. Segunda passada: um `DiGraphMap<&str, ()>` por tipo, montado emprestando desse mapa e de `req` (as arestas do lote entram como `&str` de `req`), e `has_path_connecting(&grafo, to, from, None)`. Montar o grafo enquanto o mapa ainda recebe inserções é autorreferência e não compila. Extremo fora do grafo dá `false`, sem pânico (medido no `petgraph` 0.8.3).

### Suporte de teste da T5b (`tests/support/mod.rs`, escrito pelo test-engineer)

Acrescenta ao `Harness` da T5a: `FakeNative::forget(&mut self, id: &SemanticId)` (esquece um id reservado, para o teste de `MissingNative`); `relation(relation_type, from, to) -> AppendItem` (sem `base`, `snapshot` nem `external_id`); `revise_relation(item, &RecordId) -> AppendItem` (põe `base`); `retire_relation(relation_type, from, to, &RecordId) -> AppendItem`. Para trocar a versão do catálogo nativo, o teste solta o `Hexlog`, cria `FakeNative::new("v2")` com as mesmas reservas e reabre. Reserva padrão dos testes: `n.other` com tipo `note`. Catálogos locais por `open_with`: `loose` (relation type sem `acyclic`, qualquer extremo) e o de referência sem `after`.

### Testes da T5b

`tests/relations.rs` (31): os 17 do plano (seção 5, "Relation") e 14 novos.

| Teste | Afirma |
|---|---|
| `it_should_accept_a_relation_to_a_native_node_and_record_its_origin` | `after` de `a.one` (tenant) para `n.other` (nativo): `to_origin == Native`, `from_origin == Tenant`, `native_catalog == Some("v1")`, `to_status == Current` |
| `it_should_accept_a_relation_whose_source_is_native_and_target_is_tenant` | `from_origin == Native`, `to_origin == Tenant` |
| `it_should_reject_a_relation_to_a_missing_node` | `InvalidRecord` `endpoint-not-found`; `to` ausente em `/items/0/body/to`, `from` ausente em `/items/0/body/from` |
| `it_should_reject_a_relation_with_an_unknown_relation_type` | `RelationTypeNotFound`, `relation-type-not-found`, `/items/0/body/relation_type` |
| `it_should_reject_a_relation_between_two_native_nodes` | `native-only-relation` em `/items/0/body` |
| `it_should_reject_a_self_relation` | `self-relation` em `/items/0/body/to` |
| `it_should_reject_a_relation_whose_endpoint_type_the_relation_forbids` | `owned-by` (de `task` para `note`) com `from` do tipo `note`: `endpoint-type` em `/items/0/body/from`; com `to` do tipo `task`: em `/items/0/body/to` |
| `it_should_see_nodes_of_previous_items_when_validating_a_relation_in_the_same_batch` | nó, nó e relation no mesmo lote gravam; `relations` a devolve com os dois status `Current` |
| `it_should_reject_a_cycle_on_an_acyclic_relation_including_native_relations` | `reserve_relation(after, n.x, a.one)`; relation `after` de `a.one` para `n.x`: `CycleRejected`, `cycle`, `/items/0/body`, `chain_head` não anda |
| `it_should_ignore_relations_with_a_retired_endpoint_when_detecting_cycles` | `a -> b` e `b -> c` gravadas; retira `b`; `c -> a` grava |
| `it_should_report_retired_endpoint_status_after_the_node_is_retired` | `to_status == Retired`, `from_status == Current` |
| `it_should_report_missing_native_endpoint_status_when_the_native_disappears` | `forget(n.other)`; `to_status == MissingNative`, sem erro |
| `it_should_retire_a_relation_using_the_relation_latest_record_id_as_base` | `base` = `RelationView.record_id`; `relations` padrão não a lista; com `include_retired`, lista com `retired == true` |
| `it_should_copy_origins_and_native_catalog_from_the_latest_record_when_retiring_a_relation` | relation escrita com catálogo nativo `v1`; reabre com `FakeNative::new("v2")`; o corpo cru da retirement (`Tamper::read_body`) tem as duas origens e `native_catalog` `v1` |
| `it_should_return_inbound_relations_when_querying_by_target` | `RelationQuery { to: Some(id), .. }` devolve as relations que chegam a `id` |
| `it_should_reject_snapshot_on_a_relation` | `snapshot-not-allowed` em `/items/0/snapshot` |
| `it_should_accept_external_id_on_a_relation_and_not_inherit_it_on_revision` | `external_id` gravado; revisão (`base` = latest) sem ele: `RelationView.external_id == None` |
| `it_should_reject_a_second_item_for_the_same_relation_lineage_in_the_same_batch` | relation e `retire_relation` da mesma lineage no mesmo lote: `ForkRejected` `lineage-in-batch` em `/items/1/body`, `current` ausente, nada gravado |
| `it_should_reject_a_second_root_for_an_existing_relation_lineage` | segunda raiz da mesma relation (sem `base`): `not-current` em `/items/0/base`, `current` = id do latest, `retired == Some(false)` |
| `it_should_reject_a_relation_to_a_retired_node` | extremo retirado no banco: `endpoint-not-found` em `/items/0/body/to` |
| `it_should_reject_a_relation_to_a_node_retired_earlier_in_the_same_batch` | lote `[retirement de b, relation a -> b]`: `endpoint-not-found`, nada gravado |
| `it_should_prefer_the_native_origin_when_a_tenant_node_shadows_a_native_id` | nó `n.x` do tenant; reabre com `n.x` reservado; relation de `a.one` para `n.x`: `to_origin == Native`, `native_catalog` presente; retirar `n.x` depois deixa `to_status == Current` |
| `it_should_check_the_type_of_a_native_endpoint_against_the_relation_type` | `owned-by` com `from` nativo de tipo `note`: `endpoint-type` em `/items/0/body/from` |
| `it_should_retire_a_relation_even_when_an_endpoint_is_retired` | relation `a -> b`, retira `b`, retira a relation: grava |
| `it_should_retire_a_relation_whose_relation_type_left_the_catalog` | reabre com o catálogo sem `after`; `retire_relation` grava; a coluna `def_hash` da retirement é a do latest (`Tamper::read_text`) |
| `it_should_accept_a_cycle_on_a_relation_type_that_is_not_acyclic` | catálogo local `loose`: `a -> b` e `b -> a` gravam |
| `it_should_count_the_relations_of_earlier_items_of_the_batch_when_detecting_cycles` | lote `[a -> b, b -> c, c -> a]` em `after`: `CycleRejected` em `/items/2/body`, nada gravado |
| `it_should_reinstate_a_retired_relation_with_a_revision` | relation, retirement e revisão com `base` = id da retirement: `relations` padrão a lista de novo com `retired == false`; um extremo retirado nesse meio-tempo dá `endpoint-not-found` |
| `it_should_list_the_revisions_of_a_relation_lineage` | `lineage_records` com `Lineage::Relation`: 3 `RevisionView` (raiz, revisão, retirement) com `data == None`, `revises` encadeado e `retired` falso, falso, verdadeiro |
| `it_should_filter_relations_by_relation_type_and_source` | `after` e `owned-by` gravadas; `RelationQuery` com `relation_type` e com `from` devolve só as esperadas |
| `it_should_report_a_stale_base_before_a_missing_endpoint` | relation com `base` velho e `to` ausente: `ForkRejected` `not-current`, não `endpoint-not-found` |

`tests/append.rs` (1): `it_should_store_the_golden_body_for_each_record_shape` (D-22). A fixture `golden_bodies.json` não é reproduzível por `append` (mesmo tipo com e sem `snapshot`, lote que não fecha, marcadores no lugar de hashes) e segue valendo só para o teste de unidade de `model`; este teste escreve o esperado à mão. `FixedClock` padrão (`2026-11-07T12:00:00.000Z`), `SeqIds` (o n-ésimo record tem o n-ésimo id: `acme:0192a000-0000-7000-8000-00000000000n`), `FakeNative::new("v1")` com `n.other` do tipo `note`, tenant `acme`, autor `ag`/`m`/`cli`, catálogo de referência. Cinco lotes, seis records:

| Lote | Itens | Record | Forma coberta |
|---|---|---|---|
| 1 (sem chave) | `Node` `a.one`, tipo `note`, `data` `{"text":"one"}` | seq 1 | node raiz, `batch` com `count: 1` e sem `key` |
| 2 (sem chave) | `Node` `a.two`, tipo `strict-note`, `data` `{}`, `snapshot` do tipo de commit com 40 `c`, `external_id` `x` | seq 2 | node raiz com `snapshot` e `external_id` |
| 3 (chave `K1`) | `Node` revisão de `a.two` (`base` = id do seq 2, `snapshot` e `external_id` de novo, `data` `{"text":"é\u0001\"\\"}`); `Relation` `after` de `a.one` para `n.other` | seq 3 e seq 4 | node revisão: `revises`, `batch` com `key` e `count: 2`; relation: `native_catalog` `v1`, origens `tenant` e `native`, sem `batch`, sem `data` |
| 4 (sem chave) | `Retirement` do node `a.two` (`base` = id do seq 3) | seq 5 | retirement de node: `revises`, `retirement` verdadeiro, `type` e `def_hash` do latest, sem `data`, com `batch` |
| 5 (sem chave) | `Retirement` da relation (`base` = id do seq 4) | seq 6 | retirement de relation: copia `native_catalog` e as origens, com `batch` |

Afirma, para cada seq de 1 a 6: `Tamper::read_body("acme", seq)` é igual a `chain::canonical(esperado)`, com `esperado` escrito à mão em `serde_json::json!` a partir da tabela de presença da costura 5, `def_hash` de `Catalog::type_hash` e `relation_type_hash`, e `fingerprint` de `chain::fingerprint` dos itens do lote; e o `hash` da coluna é igual a `chain::hash_record(prev, body)`, com `prev` o chain head do tenant vazio no seq 1 e o `hash` anterior depois.

Total da T5b: 32 testes (31 + 1). Ficaram de fora cinco que repetiriam lógica já testada em node: `base-unknown` de relation, retirement de lineage de relation inexistente, retirement de relation já retirada, grafos de relation types diferentes que não se misturam, e `relations` de tenant desconhecido.

### Riscos de compilação e de varredor (T5b)

- **`petgraph`**: `features = ["std", "graphmap"]` já está no `Cargo.toml` (entrou na T3). `DiGraphMap<&str, ()>` exige nó `Copy + Ord + Hash`: só `&str`. Todos os `&str` vêm de `req` e do mapa de pares, que vivem mais que os grafos (carregar antes, montar depois).
- **E0502**: no passo 6, `reads: &mut dyn WriteReads` e as referências a `req` são independentes. O que não compila é guardar um `&SemanticId` vindo de um retorno de `reads` (por exemplo `LatestRow`) e chamar `reads` de novo; copie o que precisa.
- **Vocabulário (`src/`)**: sem `check_*`, `gate`, `review`, `work_*` nem `unit` em identificador ou literal (`endpoint_types_allowed` já os evita). O nome do tipo de snapshot só aparece em `engine/snapshot.rs`.
- **`RecordBody<'a>` com `Cow`**: o corpo desserializado do latest empresta de `row.body`; a `RecordRow` tem de viver até o `canonical()` do record novo, ou copie `native_catalog` com `Cow::into_owned`.
- **Mover `lineage_columns`**: `tx.rs` passa a importar de `crate::engine::model`; os 30 de `sqlite_store` continuam verdes e provam que nada mudou.
- **Teste**: `Hexlog<'d>` empresta do `Harness`; para `forget` e para a versão `v2`, solte o `Hexlog` antes. Use o `Tamper` só depois de soltar o `Hexlog` que escreveu.

### Pronto (T5b)

Arquivos: `src/engine/{decide,rules}.rs`, `src/engine/model.rs` (recebe `lineage_columns`), `src/store/sqlite/tx.rs` (só o `use`), `src/facade.rs`, `src/lib.rs` (`pub use` de `RelationView` e `EndpointStatus`), `tests/support/mod.rs`, `tests/relations.rs`, `tests/append.rs`. Verdes: `relations` (31) e `append` (1), mais tudo o que a T5a deixou verde e os 30 de `sqlite_store`; `fmt`, `clippy -D warnings` e `test --workspace` limpos; `grep -rn "relation-items-not-supported" crates/hexlog/src` vazio.

## T5c. `verify_chain`, `CHECKED_FIELDS`, política `Full` e testes de C3 (caixas cinza `engine/verify`, `facade`, `store/sqlite`)

Decisões em D-19 e D-24 a D-26 do [log do crítico](omhl-v2-core-step3-critic-log.md); fundamento no plano (seção 4.5, linhas 477-522) e na seção 5, C3. Esta seção prevalece sobre o plano onde divergir (por exemplo, a política `Full` na escrita roda fora da transação).

### Costura 14: `verify_chain` (API pública)

Pega: relatório de quebras, `CHECKED_FIELDS`, política `Full` nas leituras e na escrita. Deixa passar: concorrência (T5d).

```rust
// src/engine/model.rs, exportados em lib.rs: ChainReport, ChainBreak, BreakReason
pub struct ChainReport { pub ok: bool, pub total_records: u64, pub chain_head: Hash, pub breaks: Vec<ChainBreak>, pub total_breaks: u64 }  // breaks até 100
pub struct ChainBreak { pub seq: Option<u64>, pub reason: BreakReason }                                                                        // seq None: âncora, lote, chain head esperado
pub enum BreakReason { DivergingSeq, HashMismatch, InvalidBody, ColumnMismatch, BatchIndexMismatch, AnchorMismatch, ChainHeadNotFound }     // Debug, Clone, Copy, PartialEq, Eq
// src/engine/verify.rs: `pub const CHECKED_FIELDS: &[&str]` (reexportada em lib.rs) e
pub(crate) fn verify_chain(reads: &mut dyn VerifyReads, tenant: &TenantId, expected: Option<&Hash>) -> Result<ChainReport, HexlogError>;
// src/facade.rs
impl<'d> Hexlog<'d> { pub fn verify_chain(&mut self, tenant: &TenantId, expected_chain_head: Option<&Hash>) -> Result<ChainReport, HexlogError>; }
```

`CHECKED_FIELDS` tem 33 identificadores, na ordem da tabela de mutações: `seq`, `prev_hash`, `hash`, `body_content`, `id`, `at`, `type`, `lineage_kind`, `lineage_key`, `retired`, `revises`, `def_hash`, `relation_type`, `from_id`, `to_id`, `from_origin`, `to_origin`, `snapshot_kind`, `snapshot_id`, `external_id`, `body_not_canonical`, `body_not_json`, `body_tenant`, `body_seq`, `tenants_anchor`, `tenants_anchor_doc`, `tenants_created_at`, `batches_key`, `batches_fingerprint`, `batches_first_seq`, `batches_item_count`, `batches_row_missing`, `batches_row_extra`.

Algoritmo (D-24). Tenant desconhecido: `TenantNotFound`. A leitura vai em páginas de 500 por `stored_records`, a partir do último `seq` armazenado.
- **Tenant:** um único `AnchorMismatch` (`seq` `None`) se `sha256hex(anchor_doc) != anchor` ou `anchor_doc != chain::anchor_doc(tenant, created_at)`. A âncora usada na cadeia é `sha256hex(anchor_doc)` recalculada.
- **Record**, por `seq` armazenado crescente, parando no primeiro problema de cada um: (1) `seq` armazenado diferente do anterior mais 1 (1 no primeiro): `DivergingSeq`; o contador segue o `seq` armazenado, não o esperado. (2) `prev_hash` diferente do `hash` armazenado do anterior (da âncora recalculada, no primeiro), ou `hash` armazenado diferente de `hash_record(prev_hash armazenado, body armazenado)`: `HashMismatch`. (3) `body` não lê como `RecordBody`, `v != 1`, ou `body.canonical()` falha ou difere do texto: `InvalidBody`. (4) `body.tenant` diferente do tenant, ou `body.seq` diferente do `seq` armazenado: `ColumnMismatch`. (5) cada coluna derivada, comparada como texto com o valor recalculado do corpo: `ColumnMismatch`. Colunas: `id`, `at`, `type`, `lineage_kind`, `lineage_key` (id do node; `<relation_type>|<from>|<to>` na relation), `retired` (0 ou 1, do `retirement`), `revises`, `relation_type`, `from_id`, `to_id`, `from_origin`, `to_origin` (NULL no node), `snapshot_kind` e `snapshot_id` (NULL sem snapshot), `external_id`, `def_hash`.
- **Lote:** cada record com `body.batch.key` define um lote esperado (`key`, `fingerprint`, `count`, `seq` do record). As linhas de `stored_batches` são comparadas por chave: linha sem lote esperado, lote esperado sem linha, ou `fingerprint`, `first_seq` ou `item_count` diferentes dão `BatchIndexMismatch` (`seq` `None`), um por problema. Record com `InvalidBody` não alimenta o conjunto esperado.
- **Chain head esperado:** `ChainHeadNotFound` se `expected` não está em {âncora recalculada} mais {`hash` armazenado dos records sem nenhum problema}.
- **Saída:** `breaks` na ordem tenant, lotes, records por `seq`, chain head; guarda até 100; `total_breaks` conta todos; `ok = total_breaks == 0`; `total_records` é o número de linhas lidas; `chain_head` é o hash **recalculado** do último record (`hash_record(prev_hash armazenado, body armazenado)`), ou a âncora recalculada de tenant vazio (sempre um `Hash` válido). `seq` armazenado negativo ou fora de `u64` sai como `None`.

### Costura 15: leitura crua do `VerifyReads` (interna, `engine/ports.rs` e `store/sqlite/tx.rs`)

```rust
pub(crate) struct StoredTenant { pub tenant: String, pub anchor: String, pub anchor_doc: String, pub created_at: String }
pub(crate) struct StoredRow { pub tenant: String, pub seq: i64, pub id: String, pub prev_hash: String, pub hash: String, pub body: String,
    pub at: String, pub type_name: String, pub lineage_kind: String, pub lineage_key: String, pub retired: i64, pub revises: Option<String>,
    pub relation_type: Option<String>, pub from_id: Option<String>, pub to_id: Option<String>, pub from_origin: Option<String>, pub to_origin: Option<String>,
    pub snapshot_kind: Option<String>, pub snapshot_id: Option<String>, pub external_id: Option<String>, pub def_hash: String }
pub(crate) struct StoredBatch { pub key: String, pub fingerprint: String, pub first_seq: i64, pub item_count: i64 }
pub(crate) trait VerifyReads {
    fn stored_tenant(&mut self, tenant: &TenantId) -> Result<Option<StoredTenant>, HexlogError>;
    fn stored_records(&mut self, tenant: &TenantId, after_seq: Option<i64>, limit: u32) -> Result<Vec<StoredRow>, HexlogError>;  // seq > after_seq (None: todos), ordem de seq
    fn stored_batches(&mut self, tenant: &TenantId) -> Result<Vec<StoredBatch>, HexlogError>;                                      // ordem de first_seq
}
```

Substituem `tenant`, `records_from` e `batches` de `VerifyReads` (os tipados saem). `verify_chain` compara texto e nunca chama `Hash::parse`, `RecordId::parse` e afins nas colunas: uma coluna adulterada para valor que os tipos recusam (`snapshot_kind := NULL` com `snapshot_id` presente, `retired := 2`, `hash := 'ZZ'`) vira quebra, não `StoreError` `corrupt-row`. `WriteReads` e `QueryReads` continuam tipadas. Como `VerifyReads` perde o `tenant`, o E0034 do `StoreReader` desaparece, e as notas de E0034 da T4 e da T5a ficam obsoletas. Nas tabelas `STRICT`, texto e inteiro de cada coluna continuam legíveis como `String` e `i64` mesmo adulterados.

### Costura 16: política `Full` e `Tail`

- **Leitura** (`get`, `lineage_records`, `list`, `relations`, `chain_head`) com `Full`: dentro da mesma `Store::read`, antes da consulta, `verify_chain(r, tenant, None)` (o `&mut dyn StoreReader` vira `&mut dyn VerifyReads` por upcasting); `total_breaks > 0` dá `ChainBroken`, `details[0].code` = motivo da primeira quebra em kebab (`diverging-seq`, `hash-mismatch`, `invalid-body`, `column-mismatch`, `batch-index-mismatch`, `anchor-mismatch`, `chain-head-not-found`), `path` `""`, mensagem fixa. Tenant desconhecido: a verificação não roda, e a consulta responde `TenantNotFound`.
- **Escrita** com `Full`: antes de `append_with`, uma `Store::read` com a mesma verificação, e `ChainBroken` se houver quebra. Fica fora da transação de escrita, de propósito: a porta só entrega `WriteReads`, e a verificação não disputa corrida, porque um concorrente só acrescenta. Com `Full`, cadeia quebrada e entrada inválida ao mesmo tempo dão `CHAIN_BROKEN`; tenant desconhecido segue `TenantNotFound`.
- **`Tail`** na escrita: o passo 5 do `decide` (T5a) mapeia `StoreError` com detalhe `corrupt-row` da leitura da cauda para `ChainBroken`, detalhe `tail-unreadable`, `path` `""`.
- `Full` nunca se aplica a `verify_chain` nem a `ensure_tenant`.

### Costura 8, sem mudança: `tests/support/tamper.rs`

A API (`execute`, `read_text`, `read_body`) cobre a T5c. `NULL` é literal no SQL (`SET snapshot_kind = NULL`), não bind. Para as linhas "hash recalculado": ler `prev_hash` com `read_text`, montar o corpo novo em Rust, calcular `chain::hash_record(prev_hash, body)` e gravar `body` e `hash` num só `UPDATE`.

### Suporte de teste da T5c (`tests/support/mutations.rs`, escrito pelo test-engineer)

```rust
pub struct Reference { pub tenant: String, pub path: PathBuf, /* record ids e seqs de RC1..RC6, achados por forma */ }
pub enum ExpectedSeq { Target, TargetPlusOne, Absent }
pub struct Mutation { pub id: &'static str, pub reason: BreakReason, pub seq: ExpectedSeq, pub apply: fn(&mut Tamper, &Reference) -> u64 }
pub fn reference_chain(harness: &Harness, extra_nodes: usize) -> Reference;   // 0 a 5 nós extras gravados ANTES da cadeia de referência
pub fn mutations() -> Vec<Mutation>;                                           // 34 entradas, na ordem da tabela
```

Cadeia de referência (6 records, tenant `acme`, `FakeNative::new("v1")` com `n.other` do tipo `note`, catálogo de referência, `FixedClock` padrão), em cinco lotes:
1. chave `K1`: `Node` `a.one` (`note`, `data` `{"text":"one"}`) e `Node` `a.two` (`strict-note`, `data` `{}`, `snapshot` com 40 `c`, `external_id` `x`): RC1 e RC2.
2. `Node` revisão de `a.one` (`base` = id de RC1, `data` `{"text":"two"}`): RC3.
3. `Relation` `after` de `a.one` para `n.other`: RC4.
4. `Retirement` de `a.two` (`base` = id de RC2): RC5.
5. `Retirement` da relation (`base` = id de RC4): RC6.

Alvos achados **por forma**, não por posição (os nós extras deslocam os `seq`): RC3 é o primeiro record com `lineage_kind = 'node' AND retired = 0 AND revises IS NOT NULL`; RC4, o primeiro com `lineage_kind = 'relation' AND retired = 0`; RC2, o primeiro com `snapshot_kind IS NOT NULL`; RC6, o primeiro com `lineage_kind = 'relation' AND retired = 1`; o lote `K1` é a linha de `batches` com essa chave. Nós extras: `x.0` a `x.4`, tipo `note`, um por lote.

Valores adulterados: os da tabela do plano (linhas 485-520), com estas precisões. `seq := '99'` (bind de texto coagido). `id` e `revises`: `acme:0192a000-0000-7000-8000-0000000000ff` (id bem formado e não usado). `snapshot_kind`: `SET snapshot_kind = NULL`. `body_not_canonical`: trocar de lugar os dois primeiros membros do objeto, no texto. `body_not_json`: `not json`. `body_tenant`: `tenant` do corpo `zz`; `body_seq`: `seq` do corpo `7`; os quatro de corpo no RC6 (último record), com `hash` recalculado. `tenants_anchor_doc`: `anchor_doc` com outro `created_at`, com `anchor` e `created_at` intactos. `batches_row_extra`: `INSERT` da linha `K8` com o `first_seq` do RC1. Toda mutação confere `rows_affected == 1` antes de verificar. `reason` e `seq` esperados: os da tabela (`Target` é o `seq` do alvo, `TargetPlusOne` a linha `seq`, `Absent` as de tenant, lote e chain head).

### Testes da T5c

`tests/chain_tamper.rs` (15):

| Teste | Afirma |
|---|---|
| `it_should_verify_a_clean_chain` | cadeia de referência: `ok`, `total_records == 6`, `total_breaks == 0`, `chain_head` igual ao de `Hexlog::chain_head` |
| `it_should_detect_a_change_in_each_checked_field` | para cada uma das 34 linhas (banco novo por linha): aplica, `ok == false`, `breaks[0].reason` e `breaks[0].seq` como a tabela |
| `it_should_have_a_mutation_for_every_checked_field` | os `id` das 34 entradas são únicos e iguais a `CHECKED_FIELDS` mais `expected_chain_head`; `CHECKED_FIELDS.len() == 33` |
| `it_should_detect_a_deleted_middle_record` | apaga RC3: `DivergingSeq` no `seq` do RC4 |
| `it_should_detect_an_inserted_forged_record` | insere um record no `seq` 7 com `prev_hash` do RC6, `hash` de 64 `e`, `id` e `lineage_key` novos: `HashMismatch` no `seq` 7 |
| `it_should_not_detect_a_truncated_tail_without_expected_chain_head` | apaga RC6: `ok == true` (limite documentado, H:242) |
| `it_should_detect_a_truncated_tail_with_expected_chain_head` | guarda o chain head, apaga RC6, verifica com ele: `ChainHeadNotFound` |
| `it_should_detect_a_rewritten_and_rechained_tail_with_expected_chain_head` | reescreve o corpo do RC6 e recalcula o `hash`, verifica com o head antigo: `ChainHeadNotFound` |
| `it_should_report_invalid_body_for_a_duplicate_key_in_the_stored_body` | corpo do RC6 com `"at"` duas vezes, `hash` recalculado: `InvalidBody` no `seq` do RC6 |
| `it_should_report_invalid_body_for_a_stored_body_nested_beyond_the_parser_limit` | corpo do RC6 = 200 `[` e 200 `]`, `hash` recalculado, thread de 2 MiB: `InvalidBody`, sem estouro de pilha |
| `it_should_report_a_break_instead_of_a_store_error_when_a_stored_value_is_not_valid_for_its_type` | no RC3: `retired := 2` e `lineage_kind := 'x'` dão `ColumnMismatch`; `hash := 'ZZ'` dá `HashMismatch`; `type := 'Bad Type'` dá `ColumnMismatch`; sempre `ok == false` e `breaks[0].seq` = `seq` do RC3, um por vez (banco novo) |
| `it_should_fail_with_chain_broken_on_write_when_the_tail_record_has_an_unreadable_column` | `retired := 2` no RC6, política `Tail`: `append` dá `ChainBroken`, detalhe `tail-unreadable`, nada gravado |
| `it_should_refuse_reads_with_chain_broken_when_the_policy_is_full_and_a_record_is_tampered` | `hash` do RC3 adulterado; `get`, `list`, `lineage_records`, `relations` e `chain_head` dão `ChainBroken` com `details[0].code == "hash-mismatch"` |
| `it_should_not_notice_a_tampered_middle_record_on_read_under_the_tail_policy` | a mesma adulteração com `Tail`: `get` responde, e `verify_chain` reporta `HashMismatch` |
| `it_should_cap_the_reported_breaks_at_100_and_keep_counting` | 120 nós; um `UPDATE` põe outro `at` em todos: `breaks.len() == 100`, `total_breaks == 120` |

`tests/tenant_isolation.rs` (6): os 5 do plano e `it_should_report_tenant_not_found_when_verifying_an_unknown_tenant` (`TenantNotFound`, `path` `/tenant`).

`tests/chain_properties.rs` (2, `proptest` com `ProptestConfig::with_cases(24)`):
- `it_should_verify_after_any_sequence_of_valid_appends`: a estratégia gera até 12 operações (nó novo, revisão de nó existente, retirada de nó, relation `after` entre dois nós existentes); o teste as interpreta contra um modelo do que existe e só emite `append` válido; ao fim, `verify_chain` dá `ok` e `total_records` igual ao número de records gravados.
- `it_should_detect_the_mutation_of_any_row_of_the_table_after_extra_appends`: sorteia uma das 34 linhas e `extra_nodes` de 0 a 5; `ok == false` e `breaks[0]` com o motivo e o `seq` da linha.

`src/store/sqlite/tests.rs` (de 30 para 32): mudam `it_should_read_a_tenant_row_and_none_for_an_unknown_tenant` (o lado de verificação vira `stored_tenant`, quatro campos como texto), `it_should_list_the_batches_of_a_tenant` (`stored_batches`, `first_seq` e `item_count` como `i64`), `it_should_append_records_and_batch_atomically` (a leitura de `records_from` vira `batch_records(tenant, 1, 10)`) e os que liam por `records_from` (passam a `batch_records(tenant, from_seq, limit)`, mesma semântica; `it_should_read_records_from_a_seq_up_to_a_limit`, `it_should_persist_nothing_when_decide_fails` e `it_should_write_nothing_for_an_empty_plan_from_a_replay` não mudam de nome). Novos: `it_should_read_stored_records_after_a_seq_as_text` (`None` lê tudo, `Some(2)` só `seq > 2`, `limit` corta, cada campo igual ao do plano gravado) e `it_should_read_stored_rows_even_when_a_value_is_not_valid_for_its_type` (por conexão crua, que mantém triggers e restrições, insere um record de cadeia certa com `hash` `ZZ` e `type` `Bad Type`, valores que o DDL aceita: `stored_records` os devolve como texto, e `batch_records` devolve `StoreError` `corrupt-row`). Nada de `ignore_check_constraints` em `src/`: o varredor veta a palavra, e as adulterações que dependem dela ficam em `tests/chain_tamper.rs`.

### Riscos de compilação e de varredor (T5c)

- **Vocabulário (`src/`)**: nada de `check_*`, `gate`, `unit`, `review` nem `work_*` em identificador ou literal; em `verify.rs`, use `compare_*`, `expect_*`, `recompute_*`. `CHECKED_FIELDS` passa (a palavra é `checked`).
- **E0364 e `unused_imports`**: `pub use verify::CHECKED_FIELDS;` e `pub(crate) use verify::verify_chain;` só entram junto com o consumidor (`lib.rs` e a Facade); `CHECKED_FIELDS` é `pub const`, não `pub(crate)`. Os tipos do relatório vão em `model.rs`, que já é módulo `pub(crate)` com itens `pub`.
- **Upcasting**: `store.read(&mut |r| verify_chain(r, ..))`, em que `r: &mut dyn StoreReader` vira `&mut dyn VerifyReads` (estável no 1.97), sem `as`.
- **`RecordBody<'a>`** empresta de `row.body`: a `StoredRow` vive durante a conferência de cada linha; uma página de 500 linhas por vez, sem guardar corpos entre páginas (o conjunto de lotes guarda `String`s).
- **Lints**: `i64::try_from` e `u64::try_from`, nunca `as`; `u32::try_from(page.len())`; `HashMap<String, _>` é permitido.
- **Teste**: abra o `Tamper` depois de soltar o `Hexlog` que escreveu; com `Full`, reabra o `Hexlog` com `Config { verify_policy: VerifyPolicy::Full, ..harness.config() }` por `open_with`. O teste de corpo fundo roda em thread de 2 MiB.

### Pronto (T5c)

Arquivos: `src/engine/{verify,model,ports,decide,mod}.rs`, `src/store/sqlite/tx.rs`, `src/store/sqlite/tests.rs`, `src/facade.rs`, `src/lib.rs` (reexporta `CHECKED_FIELDS`, `ChainReport`, `ChainBreak`, `BreakReason`), `tests/support/mutations.rs`, `tests/{chain_tamper,tenant_isolation,chain_properties}.rs`. Verdes: `chain_tamper` (15), `tenant_isolation` (6), `chain_properties` (2) e `sqlite_store` (32), mais tudo o que a T5a e a T5b deixaram verde; `fmt`, `clippy -D warnings` e `test --workspace` limpos.

## Emenda D-31: `LineageRef` emprestado nas portas (rodada logo depois da T5c)

Decisão em D-31 do [log do crítico](omhl-v2-core-step3-critic-log.md). Emenda as costuras 7 (T4) e 13 (T5b). Onde elas dizem `&Lineage`, `Node(id)` ou `&Lineage::Node(id)` em `latest_record` e `revisions`, leia `LineageRef`.

- `latest_record` e `revisions` recebem a lineage emprestada: `lineage: LineageRef<'_>`, por valor. `LineageRef<'a>` mora em `engine/model.rs`, é `pub(crate)` e deriva `Debug + Clone + Copy + PartialEq + Eq + Hash`. Variantes: `Node(&'a SemanticId)` e `Relation { relation_type: &'a RelationTypeName, from: &'a SemanticId, to: &'a SemanticId }`. `impl<'a> From<&'a Lineage> for LineageRef<'a>` é a única conversão. `Lineage` continua o tipo público e dono (`ItemBody::Retirement`, `Hexlog::lineage_records`).
- `pub(crate) fn lineage_columns(lineage: LineageRef<'_>) -> (&'static str, Cow<'_, str>)`: o `Cow` fica preso ao `'a` da referência, não a uma cópia local.

Onde a assinatura muda:
- `engine/ports.rs`: `WriteReads::latest_record(&mut self, tenant: &TenantId, lineage: LineageRef<'_>)` e `QueryReads::revisions(&mut self, tenant: &TenantId, lineage: LineageRef<'_>)`.
- `store/sqlite/tx.rs`: as duas assinaturas; o corpo não muda (`lineage_columns(lineage)`).
- `engine/model.rs`: o tipo, o `From` e o `lineage_columns` novo.
- `engine/decide.rs`:
  - `item_lineage(&ItemBody) -> LineageRef<'_>`, sem clone, com `LineageRef::from(lineage)` na retirement;
  - `Vec<LineageRef<'_>>` e `&[LineageRef<'_>]` em `validate_history` e `assemble`;
  - `seen: HashSet<LineageRef<'_>>`, e o `format!` da chave some do passo 6 (equivalente à chave de texto, porque `is_name` não aceita `|`);
  - `resolve_endpoint` chama `latest_record(tenant, LineageRef::Node(id))`;
  - `Subject.lineage`, `subject` e `body_lineage` passam a `LineageRef<'a>`;
  - `build_record` recebe `(&AppendItem, LineageRef<'_>)`.
- `facade.rs`: `get` e `endpoint_status` chamam `latest_record(tenant, LineageRef::Node(id))`. `lineage_records(&Lineage)` continua público e converte com `LineageRef::from(lineage)`.
- `ids.rs`, macro `string_id!`: também deriva `Hash`, o que só acrescenta.

Rodada:
1. O test-engineer troca as chamadas de `latest_record` e `revisions` em `store/sqlite/tests.rs` por `LineageRef::from(&nome)`: as seis de hoje e as que a T5c tiver acrescentado (`grep -n "latest_record\|revisions(" src/store/sqlite/tests.rs`). Também acrescenta `LineageRef` ao `use`. Nenhum teste novo entra: `lineage-in-batch` já é provado por `tests/batch.rs` e `tests/relations.rs`, e os testes do store provam as duas portas. O vermelho é o arquivo não compilar contra a porta antiga.
2. O executor troca as portas e os chamadores. O verde é `cargo test --workspace`, `clippy --all-targets` e `fmt` limpos.

O ganho é de propriedade e de contrato, não de velocidade: 3 clones custam de 56 a 66 ns, contra 4,9 a 6,1 ms de um append com `fsync`. O commit não deve alegar ganho de velocidade.

Riscos:
- E0502 não ocorre, porque `reads` e `req` são independentes.
- Não guarde o `Cow` de `lineage_columns` de uma `LineageRef` local: use a cópia, que é `Copy`.
- `LineageRef` tem 24 bytes e é `Copy`, então `needless_pass_by_value` e `large_types_passed_by_value` não acusam.
- Nenhuma palavra vetada pelo varredor.

## T5d. Concorrência, política de verificação e `Send` (testes de integração)

Decisões em D-27 do [log do crítico](omhl-v2-core-step3-critic-log.md); abertura concorrente e medidas em D-6; replay sob a trava em D-12; `Full` e `Tail` em D-19 e D-26.

### Suporte de teste da T5d

- **Instâncias por thread, sem `Arc` nem `Mutex`.** O `Harness` (T5a) é `Sync` (relógio, `SeqIds` com `AtomicU64`, `FakeNative` e `TempDir`). As threads de `std::thread::scope` o pegam por `&`, e cada thread usa a sua própria instância de `Hexlog`. `std::sync::Barrier` e `AtomicU64` não estão em `clippy.toml`.
- **`busy_timeout` de 30 s só neste arquivo:** `Config { busy_timeout: Duration::from_secs(30), ..harness.config() }` e `open_with(&config, reference_catalog())`. O padrão de 5 s não muda.
- **`Inspect`**, novo em `tests/support/tamper.rs`: `Inspect::open(path: &Path) -> Inspect` (conexão `sqlx` crua, somente leitura, **sem** `DROP TRIGGER`) e `read_text(&mut self, sql: &str, binds: &[&str]) -> Vec<String>`. Existe porque `Tamper::open` remove os sete triggers ao conectar.
- Adulterações: `Tamper` depois de soltar o `Hexlog` que escreveu; cada uma confere `rows_affected == 1`.

### Testes da T5d

`tests/concurrency.rs` (8):

| Teste | Arruma e afirma |
|---|---|
| `it_should_accept_only_one_of_two_concurrent_revisions_of_the_same_base` | grava `a.one` e lê o `record_id` por `get`; abre duas instâncias na thread do teste e **move** cada uma para a sua thread de `scope`; `Barrier` de 2; as duas revisam com a mesma `base`. Um `Ok` e um `Err` `ForkRejected` com `details[0].code == "not-current"`; `lineage_records` tem 2 records; `verify_chain` dá `ok` |
| `it_should_write_once_when_the_same_key_races` | duas instâncias movidas para threads, `Barrier`, mesma `key` e mesmos itens: as duas dão `Ok`; exatamente uma com `replayed == true`; os `record_id` são os mesmos; `chain_head().seq == 1`; o `SeqIds` consumiu 1 id |
| `it_should_keep_seq_contiguous_when_many_threads_append` | 8 instâncias movidas para 8 threads, 5 `append` cada, nós distintos (`t<thread>.n<i>`): os 40 dão `Ok`; os `seq` devolvidos, ordenados, são exatamente 1 a 40; `verify_chain` dá `ok` com `total_records == 40` |
| `it_should_be_send` | asserção de compilação: `fn require_send<T: Send>()` instanciada com `Hexlog<'d>` numa função genérica sobre `'d`; e ida e volta: abre, move para uma thread de `scope`, grava lá, devolve pelo `join` e lê na thread do teste |
| `it_should_fail_with_chain_broken_on_write_when_the_tail_record_is_tampered` | `Tail`; 3 nós; `hash` do último record := 64 `f`; `append` dá `ChainBroken` com `details[0].code == "tail-hash-mismatch"`; a contagem de linhas de `records` continua 3 |
| `it_should_not_notice_a_tampered_middle_record_on_write_under_the_tail_policy` | `Tail`; 5 nós; `hash` do `seq` 2 := 64 `f`; `append` dá `Ok` (o `seq` 6 grava); `verify_chain` dá `ok == false` e `breaks[0]` `HashMismatch` no `seq` 2 (documenta o limite de `Tail`, desvio de H:243) |
| `it_should_fail_with_chain_broken_on_write_for_a_tampered_middle_record_under_the_full_policy` | a mesma adulteração com `Full`: `append` dá `ChainBroken`, `details[0].code == "hash-mismatch"`; a contagem de linhas continua 5 |
| `it_should_open_a_fresh_database_from_many_threads_without_error` | 50 rodadas, cada uma com `TempDir` novo e 8 threads, `Barrier` de 8 antes do `Hexlog::open` (a abertura é feita dentro da thread: é o objeto do teste); as 8 aberturas dão `Ok`; por `Inspect`, `sqlite_master` tem as tabelas `batches`, `records` e `tenants` e os sete triggers (`batches_no_delete`, `batches_no_update`, `records_chain_guard`, `records_no_delete`, `records_no_update`, `tenants_no_delete`, `tenants_no_update`), cada um uma só vez, e `user_version` vale 1 |

### Riscos de compilação e de estabilidade (T5d)

- **`move` para a thread:** o `Hexlog<'d>` empresta do `Harness`, e a thread de `scope` pode ter o empréstimo. Fora de `scope` (`thread::spawn`) não compila, e é o que se quer.
- **Sem `sleep`:** a ordem vem do `Barrier` e do `BEGIN IMMEDIATE`. Teste que precise de `sleep` para passar indica erro de projeto do teste.
- **Estabilidade:** o "Pronto" roda o arquivo 20 vezes seguidas. Em CI lento, aumente o `busy_timeout` só nos testes (já é 30 s), nunca o padrão.
- **`Tamper` e `Inspect`:** abrir `Tamper` remove os triggers do arquivo para sempre; use `Inspect` onde os triggers precisam existir.
- **Lints:** `needless_pass_by_value` (receba `&Config`); nada de `Arc`, `Mutex`, `OnceLock`.

### Pronto (T5d)

`tests/concurrency.rs` (8) verde e estável: `for i in $(seq 20); do cargo test -p hexlog --test concurrency || break; done` sem falha; `fmt`, `clippy -D warnings` e `test --workspace` limpos. Arquivos: `tests/concurrency.rs` e `tests/support/tamper.rs` (`Inspect`); ajuste em `src/store/sqlite/*` só se a corrida expuser lacuna real, registrado no log.

## T6. Terceiro grafo, `omhl` mínimo, travas finais e fechamento

Decisões em D-28 a D-30 do [log do crítico](omhl-v2-core-step3-critic-log.md).

### Costura 17: `omhl` mínimo (nenhum código novo)

O `omhl` fica como a T1 o entregou: `omhl --version` imprime `omhl <versão do pacote>` no stdout e sai com 0; qualquer outro argumento, ou nenhum, imprime `usage: omhl --version` no stderr e sai com 2. Não abre banco, não lê ambiente e não usa `hexlog` (a dependência é do passo 4). **O diretório pai do banco não é criado nesta etapa:** quem chama `Hexlog::open` o cria (o `omhl` do passo 4); o `hexlog` recusa diretório inexistente com `StoreError` `"14"`. A frase da costura 7, "quem cria é o `omhl` (T6)", vale como "quem cria é o chamador de `Hexlog::open` (o `omhl`, passo 4)" (D-28).

### Costura 18: `third_graph.json` e o terceiro grafo

`tests/fixtures/third_graph.json` é **escrito à mão** para a T6, com nomes neutros e sem equivalente no Hexlog TS (não há oráculo; o que se afirma é comportamento, não hash). Formato: `{ "source": "...", "types": [TypeDef], "relations": [RelationTypeDef] }`, lido com `serde_json::from_str` para um struct local ao teste que usa `TypeDef` e `RelationTypeDef` públicos. Conteúdo fixo:

| Grafo | Tipos | Relation types |
|---|---|---|
| `outline` | `section` (`required: ["title"]`, `title` string de 1 a 100), `paragraph` (`required: ["text"]`, `text` string até 2000) | `contains`: `from: ["section"]`, `to: ["section", "paragraph"]`, `acyclic: true` |
| `roster` | `person` e `team` (`required: ["name"]`, `name` string de pelo menos 1) | `member-of`: `from: ["person"]`, `to: ["team"]` |
| `tickets` | `ticket` (`requires_snapshot: true`, `required: ["status"]`, `status` enum `open`/`closed`, `title` string opcional), `milestone` (`{"type":"object"}`) | `blocks`: `from: ["ticket"]`, `to: ["ticket"]`, `acyclic: true`; `targets`: `from: ["ticket"]`, `to: ["milestone"]` |

O plano diz "um relation type `acyclic`"; aqui são dois, `contains` e `blocks`, porque cada grafo com hierarquia precisa dele. Um só `Catalog` carrega os três grafos.

### Testes da T6

`tests/third_graph.rs` (2):

| Teste | Afirma |
|---|---|
| `it_should_accept_a_third_graph_defined_only_by_data` | carrega o JSON no `Catalog` e abre a Facade pelo `Harness`; em cada grafo: grava nós (o `ticket` com `snapshot` de 40 `c`), liga por relation, consulta por `get` e `relations`, revisa um nó, retira uma relation e um nó; ao fim, `verify_chain` dá `ok`. O arquivo importa só `hexlog::` (e `serde`, `serde_json`, `tempfile` dos dev-deps) |
| `it_should_let_the_adapter_enforce_a_cross_record_invariant_the_engine_does_not_know` | `ticket` `open` gravado; `fn transition_allowed(from: &str, to: &str) -> bool` local ao teste (só `open` para `closed`); o teste afirma `transition_allowed("closed", "open") == false` e, chamando a Facade direto, que o motor aceita a revisão de `closed` para `open`. Documenta o padrão: quem conhece a regra valida antes |

`crates/omhl/tests/cli.rs` (2, novos, sem dependência nova; `env!("CARGO_BIN_EXE_omhl")` e `std::process::Command`): `it_should_print_the_version_and_exit_zero` (stdout `omhl <CARGO_PKG_VERSION>\n`, código 0) e `it_should_print_usage_and_exit_two_for_any_other_argument` (sem argumento e com `--bogus`: stderr `usage: omhl --version\n`, código 2, stdout vazio).

`tests/architecture.rs` (10, já existem): rodam sobre o código final. A T6 só os revisa: a tabela de filhos de `engine/mod.rs` (`decide`, `rules`, `verify` com `pub(in crate::engine)`) e a de `lib.rs` têm de bater com o que foi declarado; nada se acrescenta, a menos que o teste acuse.

### Remoção dos `allow` e da dependência sem uso

Ordem, para o executor da T6 (sem editar teste):
1. Apagar `time = "=0.3.55"` de `crates/hexlog/Cargo.toml` (D-29: só aparece `std::time`; `cargo tree -i time` mostra `hexlog` como único dependente) e regerar o `Cargo.lock`; conferir com `cargo tree -p hexlog -e normal` que nada proibido entrou e que o fecho encolheu.
2. Apagar `#![allow(dead_code)]` de `engine/mod.rs` e `#![allow(dead_code, unused_imports)]` de `store/mod.rs`, com a linha `// removido na T6`, e rodar `cargo clippy --workspace --all-targets -- -D warnings`. **Não apagar nada ainda:** listar os avisos.
3. Classificar cada aviso, nesta ordem (D-30): (a) alcançável por `pub use`: não aparece; (b) só usado por teste de unidade em `src/`: `#[cfg(test)]` no item; (c) campo de porta nunca lido, método de porta nunca chamado em produção, auxiliar sem chamador: **apagar**, junto com a atribuição no store e a asserção do teste que o exercitava; (d) não há "manter com motivo": nem `allow` nem `#[expect(dead_code)]` voltam.
4. O executor devolve a lista dos casos (c) à sessão principal; o test-engineer ajusta os testes (o executor não edita teste); só então o executor apaga os itens.

Ensaio do crítico numa cópia da árvore com T3 e T4 prontas: 88 avisos, todos de consumidor ausente. Candidatos prováveis depois da T5c, a confirmar rodando: `LatestRow.seq`, `TenantRow.anchor_doc` e `TenantRow.tenant`.

### Riscos (T6)

- **`pub use` antes de apagar:** confira o item contra `lib.rs` (por exemplo os itens de `Limits`, `ChainBreak`, `RelationView`); apagar item público quebra os testes de `tests/`.
- **`#[cfg(test)]` no item** não pode esconder o que `tests/` (integração) usa: a integração só enxerga o que é `pub` em `lib.rs`.
- **Remover `time`** não deve mudar nenhum `use`; se o compilador acusar, a dependência fica, registrada no log.
- **Vocabulário (`src/`):** o `omhl` e `tests/` não passam pelo varredor, mas `hexlog/src` sim.

### Pronto (T6)

`third_graph` (2), `architecture` (10) e o `cli` do `omhl` (2) verdes; todos os comandos de verificação da seção 5 limpos, incluindo o laço de 20 execuções de `concurrency`; `grep -rnE "todo!|unimplemented!|relation-items-not-supported" crates/hexlog/src` vazio; `grep -rn "allow(dead_code" crates/hexlog/src` vazio (o `allow` de `tests/support/mod.rs` fica, porque está em `tests/`); `time` fora do `Cargo.toml` e do `Cargo.lock`; nenhum `// removido na T6` restante. O formato do record não muda aqui: o fim da T2 já o congelou.
