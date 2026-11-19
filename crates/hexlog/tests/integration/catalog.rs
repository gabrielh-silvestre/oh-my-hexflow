use std::sync::mpsc;
use std::time::Duration;

use hexlog::{Catalog, DataViolation, TypeDef, TypeName};
use serde_json::{Map, Value, json};

fn type_name(text: &str) -> TypeName {
    TypeName::parse(text).unwrap()
}

fn type_def(name: &str, schema: Value) -> TypeDef {
    TypeDef {
        name: type_name(name),
        schema,
        requires_snapshot: false,
        equivalent: None,
    }
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(left, right)| ((*left).to_owned(), (*right).to_owned()))
        .collect()
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

fn within_five_seconds<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });

    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the work finished within 5 seconds")
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
