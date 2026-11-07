mod support;

use std::sync::mpsc;
use std::time::Duration;

use hexlog::{
    Catalog, DataViolation, ErrorCode, HexlogError, RelationTypeDef, RelationTypeName, TypeDef,
    TypeName,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

const DEF_HASHES: &str = include_str!("fixtures/def_hashes.json");
const SCHEMA_PREFIX: &str = "/types/t/schema";
const STACK_2_MIB: usize = 2 * 1024 * 1024;

fn type_name(text: &str) -> TypeName {
    TypeName::parse(text).unwrap()
}

fn relation_name(text: &str) -> RelationTypeName {
    RelationTypeName::parse(text).unwrap()
}

fn type_def(name: &str, schema: Value) -> TypeDef {
    TypeDef {
        name: type_name(name),
        schema,
        requires_snapshot: false,
        equivalent: None,
    }
}

fn relation_def(name: &str, from: Option<&[&str]>, to: Option<&[&str]>) -> RelationTypeDef {
    let names = |list: &[&str]| list.iter().map(|text| type_name(text)).collect();
    RelationTypeDef {
        name: relation_name(name),
        from: from.map(names),
        to: to.map(names),
        acyclic: false,
        equivalent: None,
    }
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(left, right)| ((*left).to_owned(), (*right).to_owned()))
        .collect()
}

fn refused(result: Result<Catalog, HexlogError>) -> Vec<(String, String)> {
    let Err(error) = result else {
        panic!("the catalog accepted a definition that must be refused");
    };
    assert_eq!(error.code, ErrorCode::InvalidSchema);
    error
        .details
        .into_iter()
        .map(|detail| (detail.path, detail.code))
        .collect()
}

/// Recusa de um schema do tipo `t`, com o ponteiro relativo à raiz do schema.
fn schema_refusal(schema: Value) -> Vec<(String, String)> {
    let result = Catalog::new(vec![type_def("t", schema)], vec![]);

    refused(result)
        .into_iter()
        .map(|(path, code)| {
            let pointer = path
                .strip_prefix(SCHEMA_PREFIX)
                .unwrap_or_else(|| panic!("path {path:?} is not under {SCHEMA_PREFIX}"));
            (pointer.to_owned(), code)
        })
        .collect()
}

fn relation_refusal(def: RelationTypeDef) -> Vec<(String, String)> {
    refused(Catalog::new(vec![], vec![def]))
}

fn accepted(schema: Value) -> Catalog {
    match Catalog::new(vec![type_def("t", schema)], vec![]) {
        Ok(catalog) => catalog,
        Err(error) => panic!("the catalog refused a schema of the subset: {error:?}"),
    }
}

fn violations(schema: Value, data: &Value) -> Vec<(String, String)> {
    match accepted(schema).validate_data(&type_name("t"), data) {
        Ok(()) => Vec::new(),
        Err(found) => found
            .into_iter()
            .map(|DataViolation { pointer, code }| (pointer, code))
            .collect(),
    }
}

fn json_text(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// `levels` contêineres aninhados: `innermost` e `levels - 1` objetos `{"c": ..}` em volta.
fn nested_around(levels: usize, innermost: Value) -> Value {
    let mut value = innermost;
    for _ in 1..levels {
        let mut wrapper = Map::new();
        wrapper.insert("c".to_owned(), value);
        value = Value::Object(wrapper);
    }
    value
}

fn nested(levels: usize) -> Value {
    nested_around(levels, Value::Object(Map::new()))
}

fn string_fields_schema(count: usize) -> Value {
    let properties: Map<String, Value> = (0..count)
        .map(|index| (format!("f{index:02}"), json!({"type": "string"})))
        .collect();
    json!({"type": "object", "properties": properties})
}

/// Os primeiros `wrong` campos recebem número; os demais, texto.
fn fields_data(count: usize, wrong: usize) -> Value {
    let fields: Map<String, Value> = (0..count)
        .map(|index| {
            let value = if index < wrong { json!(1) } else { json!("ok") };
            (format!("f{index:02}"), value)
        })
        .collect();
    Value::Object(fields)
}

fn within_five_seconds<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });

    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the work finished within 5 seconds")
}

fn on_a_2_mib_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(STACK_2_MIB)
        .spawn(work)
        .unwrap()
        .join()
        .unwrap()
}

/// `$defs` d0..d<depth>: d0 é inteiro não negativo e cada nível aplica o anterior duas vezes.
fn dag_schema(combinator: &str, depth: usize) -> Value {
    let mut defs = Map::new();
    defs.insert("d0".to_owned(), json!({"type": "integer", "minimum": 0}));
    for level in 1..=depth {
        let previous = json!({"$ref": format!("#/$defs/d{}", level - 1)});
        let mut def = Map::new();
        def.insert(combinator.to_owned(), json!([previous.clone(), previous]));
        defs.insert(format!("d{level}"), Value::Object(def));
    }
    json!({
        "type": "object",
        "properties": {"v": {"$ref": format!("#/$defs/d{depth}")}},
        "$defs": defs
    })
}

// ---- os 10 do plano

#[test]
fn it_should_reject_a_schema_with_pattern_or_pattern_properties() {
    let with_pattern =
        json!({"type": "object", "properties": {"a": {"type": "string", "pattern": "^x"}}});
    let with_pattern_properties =
        json!({"type": "object", "patternProperties": {"^x": {"type": "string"}}});

    let first = schema_refusal(with_pattern);
    let second = schema_refusal(with_pattern_properties);

    assert_eq!(
        first,
        pairs(&[("/properties/a/pattern", "pattern-not-allowed")])
    );
    assert_eq!(
        second,
        pairs(&[("/patternProperties", "pattern-not-allowed")])
    );
}

#[test]
fn it_should_reject_an_external_ref() {
    let schema = json!({
        "type": "object",
        "properties": {"a": {"$ref": "https://example.com/other.json"}}
    });

    let codes: Vec<String> = schema_refusal(schema)
        .into_iter()
        .map(|(_, code)| code)
        .collect();

    assert_eq!(codes, ["ref-not-internal"]);
}

#[test]
fn it_should_reject_a_schema_whose_root_is_not_an_object() {
    let roots = [
        json!({"type": "string"}),
        json!({}),
        json!(true),
        json!(false),
        json!({"type": ["object", "null"]}),
    ];

    for root in roots {
        let label = root.to_string();

        let refusal = schema_refusal(root);

        assert_eq!(refusal, pairs(&[("", "root-not-object")]), "root {label}");
    }
}

#[test]
fn it_should_reject_a_malformed_equivalent_uri_and_accept_a_wellformed_one_without_resolving_it() {
    let malformed = [
        " https://example.com",
        "https://example.com ",
        "https://exa\tmple.com",
        "https://example.com/a\nb",
        "https://example.com/a\u{1}b",
        "https://example.com/a b",
        "https://example.com\u{7f}",
        "foo/bar",
        "",
    ];
    let wellformed = [
        "urn:isbn:0451450523",
        "mailto:someone@example.com",
        "https://example.com/n",
        "https://unresolvable-host.invalid/n",
    ];

    for text in malformed {
        let mut def = type_def("t", json!({"type": "object"}));
        def.equivalent = Some(text.to_owned());

        let refusal = refused(Catalog::new(vec![def], vec![]));

        assert_eq!(
            refusal,
            pairs(&[("/types/t/equivalent", "equivalent-uri-format")]),
            "uri {text:?}"
        );
    }
    for text in wellformed {
        let mut def = type_def("t", json!({"type": "object"}));
        def.equivalent = Some(text.to_owned());

        let result = Catalog::new(vec![def], vec![]);

        assert!(result.is_ok(), "uri {text:?} must be accepted");
    }
    let mut relation = relation_def("r", None, None);
    relation.equivalent = Some(" https://example.com".to_owned());
    assert_eq!(
        relation_refusal(relation),
        pairs(&[("/relations/r/equivalent", "equivalent-uri-format")])
    );
}

#[test]
fn it_should_compute_a_stable_def_hash() {
    let schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
    let other_schema = json!({"type": "object", "properties": {"a": {"type": "integer"}}});
    let relation = relation_def("after", Some(&["t"]), None);

    let first = Catalog::new(vec![type_def("t", schema.clone())], vec![relation.clone()]).unwrap();
    let second = Catalog::new(vec![type_def("t", schema)], vec![relation]).unwrap();
    let different = Catalog::new(vec![type_def("t", other_schema)], vec![]).unwrap();

    let name = type_name("t");
    let relation_key = relation_name("after");
    assert!(first.type_hash(&name).is_some());
    assert_eq!(first.type_hash(&name), second.type_hash(&name));
    assert_ne!(first.type_hash(&name), different.type_hash(&name));
    assert!(first.relation_type_hash(&relation_key).is_some());
    assert_eq!(
        first.relation_type_hash(&relation_key),
        second.relation_type_hash(&relation_key)
    );
    assert_eq!(first.type_hash(&type_name("missing")), None);
}

#[test]
fn it_should_list_every_violation_with_json_pointers_up_to_50() {
    let schema = string_fields_schema(60);
    let data = fields_data(60, 60);

    let found = violations(schema, &data);

    let mut expected: Vec<(String, String)> = (0..50)
        .map(|index| (format!("/f{index:02}"), "type".to_owned()))
        .collect();
    expected.push((String::new(), "too-many-errors".to_owned()));
    assert_eq!(found, expected);
}

#[test]
fn it_should_refuse_numbers_that_are_not_safe_integers_in_data() {
    let schema = json!({"type": "object"});
    let unsafe_numbers = [
        "1.0",
        "1e2",
        "0.5",
        "-0",
        "-0.0",
        "9007199254740992",
        "-9007199254740992",
        "18446744073709551615",
    ];
    let safe_numbers = ["0", "7", "9007199254740991", "-9007199254740991"];

    for text in unsafe_numbers {
        let data = json_text(&format!(r#"{{"a":{text}}}"#));

        let found = violations(schema.clone(), &data);

        assert_eq!(
            found,
            pairs(&[("/a", "number-not-safe-integer")]),
            "number {text}"
        );
    }
    for text in safe_numbers {
        let data = json_text(&format!(r#"{{"a":{text}}}"#));

        let found = violations(schema.clone(), &data);

        assert!(found.is_empty(), "number {text} must be accepted");
    }
    let mixed_schema = json!({"type": "object", "properties": {"a": {"type": "integer"}}});
    let mixed = violations(mixed_schema, &json_text(r#"{"a":"x","b":0.5}"#));
    assert_eq!(mixed, pairs(&[("/b", "number-not-safe-integer")]));
}

#[test]
fn it_should_accept_data_that_satisfies_the_schema() {
    let catalog = support::catalog::reference_catalog();
    let name = type_name("note");

    let empty = catalog.validate_data(&name, &json!({}));
    let full = catalog.validate_data(&name, &json!({"text": "hello", "count": 3}));

    assert_eq!(empty, Ok(()));
    assert_eq!(full, Ok(()));
}

#[test]
fn it_should_load_type_and_relation_type_definitions_from_json() {
    #[derive(Deserialize)]
    struct Model {
        types: Vec<TypeDef>,
        relations: Vec<RelationTypeDef>,
    }
    let model: Model = serde_json::from_str(
        r#"{
          "types": [
            {"name": "note", "schema": {"type": "object"}},
            {"name": "strict-note", "schema": {"type": "object", "required": ["text"]},
             "requires_snapshot": true, "equivalent": "urn:example:strict-note"}
          ],
          "relations": [
            {"name": "after", "from": ["note"], "acyclic": true, "equivalent": null}
          ]
        }"#,
    )
    .unwrap();

    let catalog = Catalog::new(model.types, model.relations).unwrap();

    let note = TypeDef {
        name: type_name("note"),
        schema: json!({"type": "object"}),
        requires_snapshot: false,
        equivalent: None,
    };
    let strict = TypeDef {
        name: type_name("strict-note"),
        schema: json!({"type": "object", "required": ["text"]}),
        requires_snapshot: true,
        equivalent: Some("urn:example:strict-note".to_owned()),
    };
    let after = RelationTypeDef {
        name: relation_name("after"),
        from: Some(vec![type_name("note")]),
        to: None,
        acyclic: true,
        equivalent: None,
    };
    assert_eq!(catalog.type_def(&type_name("note")), Some(&note));
    assert_eq!(catalog.type_def(&type_name("strict-note")), Some(&strict));
    assert_eq!(
        catalog.relation_type_def(&relation_name("after")),
        Some(&after)
    );
}

#[test]
fn it_should_reject_an_invalid_type_name_when_deserializing() {
    let type_json = json!({"name": "Bad Name", "schema": {"type": "object"}});
    let relation_json = json!({"name": "Bad Name"});

    let type_result = serde_json::from_value::<TypeDef>(type_json);
    let relation_result = serde_json::from_value::<RelationTypeDef>(relation_json);

    assert!(type_result.is_err());
    assert!(relation_result.is_err());
}

// ---- D-7 item (f)

#[test]
fn it_should_reject_a_ref_cycle_without_descending_into_the_instance() {
    let cycles = [
        json!({"type": "object", "$ref": "#"}),
        json!({"type": "object", "allOf": [{"$ref": "#"}]}),
        json!({"type": "object", "anyOf": [{"$ref": "#"}]}),
        json!({"type": "object", "not": {"$ref": "#"}}),
        json!({"type": "object", "$defs": {
            "a": {"$ref": "#/$defs/b"},
            "b": {"$ref": "#/$defs/a"}
        }}),
    ];

    for schema in cycles {
        let label = schema.to_string();

        let refusal = schema_refusal(schema);

        assert!(!refusal.is_empty(), "schema {label}");
        assert!(
            refusal.iter().all(|(_, code)| code == "ref-cycle"),
            "schema {label}: {refusal:?}"
        );
    }
}

#[test]
fn it_should_accept_a_recursive_ref_guarded_by_properties() {
    let recursive = [
        json!({"type": "object", "properties": {"n": {"type": "integer"}, "child": {"$ref": "#"}}}),
        json!({"type": "object", "properties": {"list": {"type": "array", "items": {"$ref": "#"}}}}),
        json!({"type": "object", "additionalProperties": {"$ref": "#"}}),
    ];

    for schema in &recursive {
        let result = Catalog::new(vec![type_def("t", schema.clone())], vec![]);

        assert!(result.is_ok(), "schema {schema}");
    }
    let tree = recursive[0].clone();
    let data = json!({"child": {"child": {"n": "x"}}});
    assert_eq!(
        violations(tree, &data),
        pairs(&[("/child/child/n", "type")])
    );
}

#[test]
fn it_should_validate_a_shared_ref_dag_in_polynomial_time() {
    let (all_of_valid, all_of_invalid, any_of_invalid) = within_five_seconds(|| {
        let all_of = dag_schema("allOf", 30);
        let any_of = dag_schema("anyOf", 30);
        (
            violations(all_of.clone(), &json!({"v": 5})),
            violations(all_of, &json!({"v": -1})),
            violations(any_of, &json!({"v": -1})),
        )
    });

    assert!(all_of_valid.is_empty());
    assert_eq!(all_of_invalid, pairs(&[("/v", "minimum")]));
    assert_eq!(any_of_invalid, pairs(&[("/v", "any-of")]));
}

#[test]
fn it_should_not_overflow_the_stack_on_deeply_nested_data() {
    let outcome = on_a_2_mib_stack(|| {
        let schema = json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer"},
                "c": {"allOf": [{"allOf": [{"$ref": "#"}]}]}
            }
        });
        let data = nested_around(126, json!({"n": "x"}));

        violations(schema, &data)
    });

    let deepest = format!("{}/n", "/c".repeat(125));
    assert_eq!(outcome, pairs(&[(deepest.as_str(), "type")]));
}

#[test]
fn it_should_report_max_depth_exceeded_on_a_very_long_ref_chain() {
    let found = on_a_2_mib_stack(|| {
        let mut defs = Map::new();
        defs.insert("d0".to_owned(), json!({"type": "integer"}));
        for level in 1..10_000 {
            let target = format!("#/$defs/d{}", level - 1);
            defs.insert(format!("d{level}"), json!({"$ref": target}));
        }
        let schema = json!({
            "type": "object",
            "properties": {"v": {"$ref": "#/$defs/d9999"}},
            "$defs": defs
        });

        violations(schema, &json!({"v": 1}))
    });

    assert_eq!(found, pairs(&[("", "max-depth-exceeded")]));
}

#[test]
fn it_should_treat_property_names_as_names_not_keywords() {
    let schema = json!({
        "type": "object",
        "properties": {
            "pattern": {"type": "string"},
            "$ref": {"type": "integer"},
            "$defs": {"type": "boolean"}
        }
    });

    let valid = violations(
        schema.clone(),
        &json!({"pattern": "x", "$ref": 1, "$defs": true}),
    );
    let invalid = violations(schema, &json!({"pattern": 1, "$ref": "x", "$defs": 2}));

    assert!(valid.is_empty());
    assert_eq!(
        invalid,
        pairs(&[("/$defs", "type"), ("/$ref", "type"), ("/pattern", "type")])
    );
}

#[test]
fn it_should_resolve_ref_pointers_with_escapes_and_empty_tokens() {
    let schema = json!({
        "type": "object",
        "properties": {
            "pa": {"$ref": "#/$defs/a~1b"},
            "pc": {"$ref": "#/$defs/c~0d"},
            "pe": {"$ref": "#/$defs/"},
            "pq": {"$ref": "#/$defs/q%22"},
            "px": {"$ref": "#/$defs/x%20y"}
        },
        "$defs": {
            "a/b": {"type": "string"},
            "c~d": {"type": "integer"},
            "": {"type": "boolean"},
            "q\"": {"type": "null"},
            "x y": {"type": "array"}
        }
    });
    let matching = json!({"pa": "s", "pc": 3, "pe": true, "pq": null, "px": []});
    let swapped = json!({"pa": 1, "pc": "s", "pe": 1, "pq": 1, "px": 1});

    let valid = violations(schema.clone(), &matching);
    let invalid = violations(schema, &swapped);

    assert!(valid.is_empty());
    assert_eq!(
        invalid,
        pairs(&[
            ("/pa", "type"),
            ("/pc", "type"),
            ("/pe", "type"),
            ("/pq", "type"),
            ("/px", "type"),
        ])
    );
}

#[test]
fn it_should_reject_a_ref_to_a_non_schema_location() {
    let unresolved = [
        json!({"type": "object", "enum": [{"type": "object"}], "properties": {"a": {"$ref": "#/enum/0"}}}),
        json!({"type": "object", "properties": {"a": {"$ref": "#/properties"}}}),
        json!({"type": "object", "properties": {"a": {"$ref": "#/$defs/missing"}}}),
    ];
    let not_internal = ["other.json#", "http://x/y#", "#foo"];

    for schema in unresolved {
        let label = schema.to_string();

        let codes: Vec<String> = schema_refusal(schema)
            .into_iter()
            .map(|(_, code)| code)
            .collect();

        assert_eq!(codes, ["ref-unresolved"], "schema {label}");
    }
    for target in not_internal {
        let schema = json!({"type": "object", "properties": {"a": {"$ref": target}}});

        let codes: Vec<String> = schema_refusal(schema)
            .into_iter()
            .map(|(_, code)| code)
            .collect();

        assert_eq!(codes, ["ref-not-internal"], "ref {target}");
    }
}

#[test]
fn it_should_evaluate_ref_siblings() {
    let schema = json!({
        "type": "object",
        "properties": {"s": {"$ref": "#/$defs/str", "maxLength": 2}},
        "$defs": {"str": {"type": "string"}}
    });

    let too_long = violations(schema.clone(), &json!({"s": "abc"}));
    let within = violations(schema, &json!({"s": "ab"}));

    assert_eq!(too_long, pairs(&[("/s", "max-length")]));
    assert!(within.is_empty());
}

#[test]
fn it_should_not_look_into_all_of_for_additional_properties() {
    let schema = json!({
        "type": "object",
        "allOf": [{"properties": {"a": {"type": "string"}}}],
        "additionalProperties": false
    });

    let found = violations(schema, &json!({"a": "x"}));

    assert_eq!(found, pairs(&[("/a", "additional-properties")]));
}

#[test]
fn it_should_report_a_single_violation_for_a_failing_any_of() {
    let any_of = json!({"type": "object", "properties": {"v": {"anyOf": [{"type": "string"}, {"type": "boolean"}]}}});
    let one_of_none = json!({"type": "object", "properties": {"v": {"oneOf": [{"type": "string"}, {"type": "boolean"}]}}});
    let one_of_two = json!({"type": "object", "properties": {"v": {"oneOf": [{"type": "integer"}, {"minimum": 0}]}}});

    let any_of_found = violations(any_of, &json!({"v": 1}));
    let one_of_none_found = violations(one_of_none, &json!({"v": 1}));
    let one_of_two_found = violations(one_of_two, &json!({"v": 1}));

    assert_eq!(any_of_found, pairs(&[("/v", "any-of")]));
    assert_eq!(one_of_none_found, pairs(&[("/v", "one-of")]));
    assert_eq!(one_of_two_found, pairs(&[("/v", "one-of")]));
}

#[test]
fn it_should_count_string_length_in_code_points() {
    let max_one =
        json!({"type": "object", "properties": {"s": {"type": "string", "maxLength": 1}}});
    let min_two =
        json!({"type": "object", "properties": {"s": {"type": "string", "minLength": 2}}});

    let emoji_within_max = violations(max_one.clone(), &json!({"s": "💩"}));
    let two_chars_over_max = violations(max_one, &json!({"s": "ab"}));
    let emoji_under_min = violations(min_two, &json!({"s": "💩"}));

    assert!(emoji_within_max.is_empty());
    assert_eq!(two_chars_over_max, pairs(&[("/s", "max-length")]));
    assert_eq!(emoji_under_min, pairs(&[("/s", "min-length")]));
}

#[test]
fn it_should_use_a_pointer_with_tilde_escapes_in_violations() {
    let schema = json!({"type": "object", "properties": {"a/b~c": {"type": "integer"}}});

    let found = violations(schema, &json!({"a/b~c": "x"}));

    assert_eq!(found, pairs(&[("/a~1b~0c", "type")]));
}

#[test]
fn it_should_reject_non_safe_integers_in_the_schema() {
    let refused_schemas = [
        (
            r#"{"type":"object","properties":{"n":{"type":"integer","minimum":1.5}}}"#,
            "/properties/n/minimum",
        ),
        (
            r#"{"type":"object","properties":{"n":{"enum":[1.0]}}}"#,
            "/properties/n/enum/0",
        ),
        (
            r#"{"type":"object","properties":{"n":{"const":{"a":[1e2]}}}}"#,
            "/properties/n/const/a/0",
        ),
        (
            r#"{"type":"object","properties":{"n":{"type":"integer","minimum":9007199254740992}}}"#,
            "/properties/n/minimum",
        ),
    ];
    let boundary = json_text(
        r#"{"type":"object","properties":{"n":{"type":"integer","maximum":9007199254740991}}}"#,
    );

    for (text, pointer) in refused_schemas {
        let refusal = schema_refusal(json_text(text));

        assert_eq!(
            refusal,
            pairs(&[(pointer, "number-not-safe-integer")]),
            "schema {text}"
        );
    }
    assert!(Catalog::new(vec![type_def("t", boundary)], vec![]).is_ok());
}

// ---- acréscimos

#[test]
fn it_should_name_the_missing_property_in_the_required_pointer() {
    let schema = json!({
        "type": "object",
        "required": ["m2", "m1", "a/b~c"],
        "properties": {"o": {"type": "object", "required": ["x"]}}
    });
    let data = json!({"a/b~c": 1, "o": {}});

    let found = violations(schema, &data);

    assert_eq!(
        found,
        pairs(&[
            ("/m2", "required"),
            ("/m1", "required"),
            ("/o/x", "required")
        ])
    );
}

#[test]
fn it_should_reject_a_keyword_outside_the_whitelist() {
    let keywords = [
        ("requred", json!(["a"])),
        ("$id", json!("https://example.com/s")),
        ("$dynamicRef", json!("#x")),
        ("$async", json!(true)),
        ("uniqueItems", json!(true)),
    ];

    for (keyword, value) in keywords {
        let mut schema = json!({"type": "object"});
        schema[keyword] = value;

        let refusal = schema_refusal(schema);

        let pointer = format!("/{keyword}");
        assert_eq!(
            refusal,
            pairs(&[(pointer.as_str(), "keyword-not-allowed")]),
            "keyword {keyword}"
        );
    }
}

#[test]
fn it_should_accept_annotation_keywords_and_check_their_type() {
    let annotated = json!({
        "type": "object",
        "title": "T",
        "description": "d",
        "$comment": "c",
        "default": {},
        "examples": [{}],
        "deprecated": true,
        "readOnly": false,
        "writeOnly": false
    });
    let wrong_type = json!({"type": "object", "title": 1});

    let accepted_result = Catalog::new(vec![type_def("t", annotated)], vec![]);
    let refusal = schema_refusal(wrong_type);

    assert!(accepted_result.is_ok());
    assert_eq!(refusal, pairs(&[("/title", "keyword-type")]));
}

#[test]
fn it_should_not_add_too_many_errors_at_exactly_50_violations() {
    let cases = [
        (49, 49, false),
        (50, 50, false),
        (51, 51, true),
        (60, 51, true),
    ];

    for (wrong, items, marker) in cases {
        let found = violations(string_fields_schema(60), &fields_data(60, wrong));

        let last = found
            .last()
            .map(|(pointer, code)| (pointer.as_str(), code.as_str()));
        assert_eq!(found.len(), items, "{wrong} violations");
        assert_eq!(
            last == Some(("", "too-many-errors")),
            marker,
            "{wrong} violations"
        );
    }
}

#[test]
fn it_should_refuse_data_nested_beyond_the_record_body_limit() {
    let schema = json!({"type": "object"});

    let at_limit = violations(schema.clone(), &nested(126));
    let beyond = violations(schema, &nested(127));

    assert!(at_limit.is_empty());
    assert_eq!(
        beyond,
        pairs(&[("/c".repeat(126).as_str(), "max-depth-exceeded")])
    );
}

// ---- format (D-11)

#[test]
fn it_should_accept_format_attachment_on_a_top_level_property_its_items_and_a_nullable_string() {
    let schema = json!({
        "type": "object",
        "properties": {
            "f": {"type": "string", "format": "attachment"},
            "l": {"type": "array", "items": {"type": "string", "format": "attachment"}},
            "n": {"type": ["string", "null"], "format": "attachment"}
        }
    });

    let result = Catalog::new(vec![type_def("t", schema)], vec![]);

    assert!(result.is_ok());
}

#[test]
fn it_should_reject_a_format_other_than_attachment() {
    let cases = [
        (
            json!({"type": "string", "format": "email"}),
            "format-not-supported",
        ),
        (
            json!({"type": "string", "format": "git-sha"}),
            "format-not-supported",
        ),
        (
            json!({"type": "string", "format": "date"}),
            "format-not-supported",
        ),
        (
            json!({"type": "string", "format": ""}),
            "format-not-supported",
        ),
        (json!({"type": "string", "format": 1}), "keyword-type"),
        (json!({"format": "attachment"}), "format-not-supported"),
        (
            json!({"type": "integer", "format": "attachment"}),
            "format-not-supported",
        ),
    ];

    for (property, code) in cases {
        let label = property.to_string();
        let schema = json!({"type": "object", "properties": {"f": property}});

        let refusal = schema_refusal(schema);

        assert_eq!(
            refusal,
            pairs(&[("/properties/f/format", code)]),
            "property {label}"
        );
    }
}

#[test]
fn it_should_reject_format_attachment_outside_the_top_level_property_positions() {
    let attachment = json!({"type": "string", "format": "attachment"});
    let cases = [
        (
            json!({"type": "object", "properties": {"o": {"type": "object", "properties": {"f": attachment}}}}),
            "/properties/o/properties/f/format",
        ),
        (
            json!({"type": "object", "properties": {"f": {"anyOf": [attachment]}}}),
            "/properties/f/anyOf/0/format",
        ),
        (
            json!({"type": "object", "$defs": {"h": attachment}}),
            "/$defs/h/format",
        ),
        (json!({"type": "object", "format": "attachment"}), "/format"),
    ];

    for (schema, pointer) in cases {
        let refusal = schema_refusal(schema);

        assert_eq!(
            refusal,
            pairs(&[(pointer, "format-not-supported")]),
            "pointer {pointer}"
        );
    }
}

#[test]
fn it_should_require_a_sha256_hash_in_a_field_marked_as_attachment() {
    let schema = json!({
        "type": "object",
        "properties": {
            "f": {"type": "string", "format": "attachment"},
            "l": {"type": "array", "items": {"type": "string", "format": "attachment"}},
            "n": {"type": ["string", "null"], "format": "attachment"}
        }
    });
    let hash = "a".repeat(64);
    let rejected = [
        (json!({"f": "abc"}), "/f", "format"),
        (json!({"f": "A".repeat(64)}), "/f", "format"),
        (json!({"f": "a".repeat(63)}), "/f", "format"),
        (json!({"l": [hash, "bad"]}), "/l/1", "format"),
        (json!({"f": 5}), "/f", "type"),
    ];

    let valid = violations(schema.clone(), &json!({"f": hash, "n": null}));

    assert!(valid.is_empty());
    for (data, pointer, code) in rejected {
        let found = violations(schema.clone(), &data);

        assert_eq!(found, pairs(&[(pointer, code)]), "data {data}");
    }
}

// ---- extremos de relation type (D-11)

#[test]
fn it_should_reject_a_relation_type_with_an_empty_endpoint_list() {
    let empty_from = relation_def("r", Some(&[]), None);
    let empty_to = relation_def("r", None, Some(&[]));
    let null_from: RelationTypeDef = serde_json::from_str(r#"{"name":"r","from":null}"#).unwrap();

    let from_refusal = relation_refusal(empty_from);
    let to_refusal = relation_refusal(empty_to);

    assert_eq!(
        from_refusal,
        pairs(&[("/relations/r/from", "empty-endpoint-list")])
    );
    assert_eq!(
        to_refusal,
        pairs(&[("/relations/r/to", "empty-endpoint-list")])
    );
    assert!(Catalog::new(vec![], vec![relation_def("r", None, None)]).is_ok());
    assert!(Catalog::new(vec![], vec![null_from]).is_ok());
}

#[test]
fn it_should_reject_a_relation_type_with_a_repeated_endpoint_type() {
    let repeated_from = relation_def("r", Some(&["note", "note"]), None);
    let repeated_to = relation_def("r", None, Some(&["task", "task"]));
    let repeated_both = relation_def("r", Some(&["note", "note"]), Some(&["task", "task"]));

    let from_refusal = relation_refusal(repeated_from);
    let to_refusal = relation_refusal(repeated_to);
    let mut both_refusal = relation_refusal(repeated_both);

    both_refusal.sort();
    assert_eq!(
        from_refusal,
        pairs(&[("/relations/r/from", "duplicate-endpoint-type")])
    );
    assert_eq!(
        to_refusal,
        pairs(&[("/relations/r/to", "duplicate-endpoint-type")])
    );
    assert_eq!(
        both_refusal,
        pairs(&[
            ("/relations/r/from", "duplicate-endpoint-type"),
            ("/relations/r/to", "duplicate-endpoint-type"),
        ])
    );
}

// ---- D-18

#[test]
fn it_should_accept_the_2020_12_schema_uri_only_at_the_root() {
    let uri = "https://json-schema.org/draft/2020-12/schema";
    let at_root = json!({"$schema": uri, "type": "object"});
    let other_uris = [
        "http://json-schema.org/draft-07/schema#",
        "https://json-schema.org/draft/2020-12/schema#",
    ];
    let nested = json!({"type": "object", "properties": {"a": {"$schema": uri}}});
    let not_text = json!({"$schema": 1, "type": "object"});

    let accepted_result = Catalog::new(vec![type_def("t", at_root)], vec![]);
    let nested_refusal = schema_refusal(nested);
    let not_text_refusal = schema_refusal(not_text);

    assert!(accepted_result.is_ok());
    for other in other_uris {
        let refusal = schema_refusal(json!({"$schema": other, "type": "object"}));

        assert_eq!(refusal, pairs(&[("/$schema", "schema-uri")]), "uri {other}");
    }
    assert_eq!(
        nested_refusal,
        pairs(&[("/properties/a/$schema", "schema-uri")])
    );
    assert_eq!(not_text_refusal, pairs(&[("/$schema", "keyword-type")]));
}

#[test]
fn it_should_reject_unknown_fields_when_deserializing_definitions() {
    let type_json = json!({"name": "t", "schema": {"type": "object"}, "requires_snapshots": true});
    let relation_json = json!({"name": "r", "acyclics": true});

    let type_result = serde_json::from_value::<TypeDef>(type_json);
    let relation_result = serde_json::from_value::<RelationTypeDef>(relation_json);

    assert!(type_result.is_err());
    assert!(relation_result.is_err());
}

#[test]
fn it_should_read_null_endpoint_lists_as_none_and_never_write_null() {
    let relation_json = json!({"name": "r", "from": null, "to": null, "equivalent": null});
    let type_json = json!({"name": "t", "schema": {"type": "object"}, "equivalent": null});

    let relation: RelationTypeDef = serde_json::from_value(relation_json).unwrap();
    let type_def_read: TypeDef = serde_json::from_value(type_json).unwrap();

    assert_eq!(
        (&relation.from, &relation.to, &relation.equivalent),
        (&None, &None, &None)
    );
    assert_eq!(type_def_read.equivalent, None);
    assert_eq!(
        serde_json::to_value(&relation).unwrap(),
        json!({"name": "r", "acyclic": false})
    );
    assert_eq!(
        serde_json::to_value(&type_def_read).unwrap(),
        json!({"name": "t", "schema": {"type": "object"}, "requires_snapshot": false})
    );
}

#[test]
fn it_should_cap_the_refusal_of_a_catalog_at_50_details_with_a_single_marker() {
    let schema_with_unknown_keywords = |count: usize| {
        let mut schema = Map::new();
        schema.insert("type".to_owned(), json!("object"));
        for index in 0..count {
            schema.insert(format!("k{index:02}"), json!(1));
        }
        Value::Object(schema)
    };

    let sixty = refused(Catalog::new(
        vec![type_def("t", schema_with_unknown_keywords(60))],
        vec![],
    ));
    let fifty = refused(Catalog::new(
        vec![type_def("t", schema_with_unknown_keywords(50))],
        vec![],
    ));

    let keyword_details = |count: usize| -> Vec<(String, String)> {
        (0..count)
            .map(|index| {
                (
                    format!("{SCHEMA_PREFIX}/k{index:02}"),
                    "keyword-not-allowed".to_owned(),
                )
            })
            .collect()
    };
    let mut expected_sixty = keyword_details(50);
    expected_sixty.push((String::new(), "too-many-errors".to_owned()));
    assert_eq!(sixty, expected_sixty);
    assert_eq!(fifty, keyword_details(50));
}

#[test]
fn it_should_point_ref_pattern_and_type_refusals_at_the_offending_keyword() {
    let cases = [
        (
            json!({"type": "object", "properties": {"a": {"$ref": "https://example.com/s.json"}}}),
            "/properties/a/$ref",
            "ref-not-internal",
        ),
        (
            json!({"type": "object", "properties": {"a": {"$ref": "#/$defs/missing"}}}),
            "/properties/a/$ref",
            "ref-unresolved",
        ),
        (
            json!({"type": "object", "properties": {"a": {"type": "string", "pattern": "^x"}}}),
            "/properties/a/pattern",
            "pattern-not-allowed",
        ),
        (
            json!({"type": "object", "patternProperties": {"^x": {"type": "string"}}}),
            "/patternProperties",
            "pattern-not-allowed",
        ),
        (
            json!({"type": "object", "title": 1}),
            "/title",
            "keyword-type",
        ),
    ];

    for (schema, pointer, code) in cases {
        let refusal = schema_refusal(schema);

        assert_eq!(refusal, pairs(&[(pointer, code)]), "pointer {pointer}");
    }
}

#[test]
fn it_should_report_a_ref_cycle_once_per_ref_on_the_cycle() {
    let cases = [
        (json!({"type": "object", "$ref": "#"}), vec!["/$ref"]),
        (
            json!({"type": "object", "allOf": [{"$ref": "#"}]}),
            vec!["/allOf/0/$ref"],
        ),
        (
            json!({"type": "object", "anyOf": [{"$ref": "#"}]}),
            vec!["/anyOf/0/$ref"],
        ),
        (
            json!({"type": "object", "not": {"$ref": "#"}}),
            vec!["/not/$ref"],
        ),
        (
            json!({"type": "object", "$defs": {
                "a": {"$ref": "#/$defs/b"},
                "b": {"$ref": "#/$defs/a"}
            }}),
            vec!["/$defs/a/$ref", "/$defs/b/$ref"],
        ),
    ];

    for (schema, pointers) in cases {
        let label = schema.to_string();
        let expected: Vec<(String, String)> = pointers
            .iter()
            .map(|pointer| ((*pointer).to_owned(), "ref-cycle".to_owned()))
            .collect();

        let refusal = schema_refusal(schema);

        assert_eq!(refusal, expected, "schema {label}");
    }
}

// ---- def_hash contra o oráculo TS

#[test]
fn it_should_match_the_ts_oracle_def_hash_for_each_definition_shape() {
    let fixture = json_text(DEF_HASHES);
    let definitions = fixture["definitions"].as_array().unwrap();
    assert_eq!(definitions.len(), 6);

    for (index, definition) in definitions.iter().enumerate() {
        let kind = definition["kind"].as_str().unwrap();
        let expected = definition["def_hash"].as_str().unwrap();
        let document = definition["document"].clone();

        let actual = if kind == "type" {
            let def: TypeDef = serde_json::from_value(document).unwrap();
            let name = def.name.clone();
            let catalog = Catalog::new(vec![def], vec![]).unwrap();
            catalog.type_hash(&name).unwrap().as_str().to_owned()
        } else {
            let def: RelationTypeDef = serde_json::from_value(document).unwrap();
            let name = def.name.clone();
            let catalog = Catalog::new(vec![], vec![def]).unwrap();
            catalog
                .relation_type_hash(&name)
                .unwrap()
                .as_str()
                .to_owned()
        };

        assert_eq!(actual, expected, "definition {index} ({kind})");
    }
}

// ---- emenda T3b (D-23)

#[test]
fn it_should_reject_a_repeated_type_or_relation_name() {
    let schema = json!({"type": "object"});
    let types = vec![
        type_def("t", schema.clone()),
        type_def("t", schema.clone()),
        type_def("t", schema),
    ];
    let relations = vec![relation_def("r", None, None), relation_def("r", None, None)];

    let type_refusal = refused(Catalog::new(types, vec![]));
    let repeated_relation_refusal = refused(Catalog::new(vec![], relations));

    assert_eq!(type_refusal, pairs(&[("/types/t", "duplicate-name")]));
    assert_eq!(
        repeated_relation_refusal,
        pairs(&[("/relations/r", "duplicate-name")])
    );
}

#[test]
fn it_should_accept_a_type_and_a_relation_type_with_the_same_name() {
    let types = vec![type_def("same", json!({"type": "object"}))];
    let relations = vec![relation_def("same", None, None)];

    let result = Catalog::new(types, relations);

    assert!(result.is_ok(), "refused: {:?}", result.err());
}

#[test]
fn it_should_reject_an_empty_type_list_and_a_repeated_required_name() {
    let empty_type_list = json!({"type": "object", "properties": {"a": {"type": []}}});
    let repeated_required = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "required": ["a", "a"]
    });

    let type_refusal = schema_refusal(empty_type_list);
    let required_refusal = schema_refusal(repeated_required);

    assert_eq!(
        type_refusal,
        pairs(&[("/properties/a/type", "keyword-type")])
    );
    assert_eq!(required_refusal, pairs(&[("/required", "keyword-type")]));
}

#[test]
fn it_should_accept_a_repeated_enum_value_and_an_empty_required_list() {
    let schema = json!({
        "type": "object",
        "properties": {"a": {"type": "string", "enum": ["x", "x"]}},
        "required": []
    });

    let found = violations(schema, &json!({"a": "x"}));

    assert_eq!(found, pairs(&[]));
}

#[test]
fn it_should_report_only_the_json_guard_findings_when_the_schema_also_has_other_problems() {
    let unsafe_number_and_pattern = json!({
        "type": "object",
        "properties": {
            "a": {"type": "number", "minimum": 1.5},
            "b": {"type": "string", "pattern": "^x"}
        }
    });
    let pattern_only = json!({
        "type": "object",
        "properties": {"b": {"type": "string", "pattern": "^x"}}
    });
    let types = vec![
        type_def("t", unsafe_number_and_pattern),
        type_def("u", pattern_only),
    ];

    let refusal = refused(Catalog::new(types, vec![]));

    assert_eq!(
        refusal,
        pairs(&[
            (
                "/types/t/schema/properties/a/minimum",
                "number-not-safe-integer"
            ),
            (
                "/types/u/schema/properties/b/pattern",
                "pattern-not-allowed"
            ),
        ])
    );
}
