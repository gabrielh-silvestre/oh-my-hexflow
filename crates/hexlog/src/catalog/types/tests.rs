use serde_json::json;

use super::{Catalog, RelationTypeDef, TypeDef};
use crate::engine::chain::{canonical, sha256hex};
use crate::engine::ports::Definitions;
use crate::ids::{Hash, RelationTypeName, TypeName};

fn type_name(text: &str) -> TypeName {
    TypeName::parse(text).unwrap()
}

fn relation_name(text: &str) -> RelationTypeName {
    RelationTypeName::parse(text).unwrap()
}

fn hash_of(document: &serde_json::Value) -> Hash {
    sha256hex(&canonical(document).unwrap())
}

#[test]
fn it_should_expose_requires_snapshot_def_hash_and_acyclic_through_the_definitions_port() {
    let strict_schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
    let many_fields: serde_json::Map<String, serde_json::Value> = (0..60)
        .map(|index| (format!("f{index:02}"), json!({"type": "string"})))
        .collect();
    let many_schema = json!({"type": "object", "properties": many_fields});
    let many_values: serde_json::Value = (0..60)
        .map(|index| (format!("f{index:02}"), json!(1)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    let types = vec![
        TypeDef {
            name: type_name("strict"),
            schema: strict_schema.clone(),
            requires_snapshot: true,
            equivalent: None,
        },
        TypeDef {
            name: type_name("plain"),
            schema: json!({"type": "object"}),
            requires_snapshot: false,
            equivalent: None,
        },
        TypeDef {
            name: type_name("many"),
            schema: many_schema,
            requires_snapshot: false,
            equivalent: None,
        },
    ];
    let relations = vec![
        RelationTypeDef {
            name: relation_name("after"),
            from: Some(vec![type_name("strict")]),
            to: None,
            acyclic: true,
            equivalent: None,
        },
        RelationTypeDef {
            name: relation_name("links"),
            from: None,
            to: None,
            acyclic: false,
            equivalent: None,
        },
    ];
    let catalog = Catalog::new(types, relations).unwrap();
    let port: &dyn Definitions = &catalog;

    let strict = port
        .node_type(&type_name("strict"))
        .expect("strict is defined");
    let plain = port
        .node_type(&type_name("plain"))
        .expect("plain is defined");
    let after = port
        .relation_type(&relation_name("after"))
        .expect("after is defined");
    let links = port
        .relation_type(&relation_name("links"))
        .expect("links is defined");

    assert!(strict.requires_snapshot);
    assert_eq!(
        strict.def_hash,
        hash_of(&json!({"name": "strict", "schema": strict_schema, "requires_snapshot": true}))
    );
    assert!(!plain.requires_snapshot);
    assert!(after.acyclic);
    assert_eq!(after.from, Some(vec![type_name("strict")]));
    assert_eq!(after.to, None);
    assert_eq!(
        after.def_hash,
        hash_of(&json!({"name": "after", "from": ["strict"], "acyclic": true}))
    );
    assert!(!links.acyclic);
    assert!(port.node_type(&type_name("missing")).is_none());
    assert!(port.relation_type(&relation_name("missing")).is_none());
    let single = port.validate_data(&type_name("strict"), &json!({"a": 1}));
    let overflowed = port.validate_data(&type_name("many"), &many_values);
    assert_eq!(single.err().map(|found| found.len()), Some(1));
    assert_eq!(overflowed.err().map(|found| found.len()), Some(51));
}

#[test]
fn it_should_iterate_over_every_type_definition_of_the_catalog() {
    let types = ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|name| TypeDef {
            name: type_name(name),
            schema: json!({"type": "object"}),
            requires_snapshot: false,
            equivalent: None,
        })
        .collect();
    let catalog = Catalog::new(types, Vec::new()).unwrap();

    let mut names: Vec<&str> = catalog.type_defs().map(|def| def.name.as_str()).collect();
    names.sort_unstable();

    assert_eq!(names, ["alpha", "beta", "gamma"]);
}
