use std::borrow::Cow;
use std::fmt::Debug;

use serde_json::{Value, json};

use super::{BodyAuthor, BodyBatch, BodyLineage, BodySnapshot, RecordBody};
use crate::chain::hash_record;
use crate::{ErrorCode, FieldName};

const BODIES: &str = include_str!("../../../tests/fixtures/golden_bodies.json");

fn fixture() -> Value {
    serde_json::from_str(BODIES).unwrap()
}

fn text<'a>(object: &'a Value, key: &str) -> Cow<'a, str> {
    Cow::Borrowed(object[key].as_str().unwrap())
}

fn optional_text<'a>(object: &'a Value, key: &str) -> Option<Cow<'a, str>> {
    object
        .get(key)
        .map(|value| Cow::Borrowed(value.as_str().unwrap()))
}

fn integer<T: TryFrom<u64>>(value: &Value) -> T
where
    T::Error: Debug,
{
    T::try_from(value.as_u64().unwrap()).unwrap()
}

fn lineage_of(lineage: &Value) -> BodyLineage<'_> {
    match lineage["kind"].as_str().unwrap() {
        "node" => BodyLineage::Node {
            id: text(lineage, "id"),
        },
        "relation" => BodyLineage::Relation {
            relation_type: text(lineage, "relation_type"),
            from: text(lineage, "from"),
            to: text(lineage, "to"),
            from_origin: text(lineage, "from_origin"),
            to_origin: text(lineage, "to_origin"),
        },
        other => panic!("fixture lineage kind without a builder: {other}"),
    }
}

fn body_of(fields: &Value) -> RecordBody<'_> {
    RecordBody {
        v: integer(&fields["v"]),
        tenant: text(fields, "tenant"),
        seq: integer(&fields["seq"]),
        id: text(fields, "id"),
        at: text(fields, "at"),
        type_name: text(fields, "type"),
        lineage: lineage_of(&fields["lineage"]),
        revises: optional_text(fields, "revises"),
        retirement: fields["retirement"].as_bool().unwrap(),
        data: fields.get("data").map(Cow::Borrowed),
        author: BodyAuthor {
            agent: text(&fields["author"], "agent"),
            model: optional_text(&fields["author"], "model"),
            client: text(&fields["author"], "client"),
        },
        snapshot: fields.get("snapshot").map(|snapshot| BodySnapshot {
            kind: text(snapshot, "kind"),
            id: text(snapshot, "id"),
        }),
        external_id: optional_text(fields, "external_id"),
        native_catalog: optional_text(fields, "native_catalog"),
        def_hash: text(fields, "def_hash"),
        batch: fields.get("batch").map(|batch| BodyBatch {
            fingerprint: text(batch, "fingerprint"),
            key: optional_text(batch, "key"),
            count: integer(&batch["count"]),
        }),
    }
}

#[test]
fn it_should_serialize_each_record_shape_like_the_golden_body() {
    let golden = fixture();
    let prev_hash = golden["prev_hash"].as_str().unwrap();

    for (shape, expected) in golden["shapes"].as_object().unwrap() {
        let body = body_of(&expected["fields"]);

        let canonical = body.canonical().unwrap();

        assert_eq!(
            canonical,
            expected["jcs"].as_str().unwrap(),
            "jcs of {shape}"
        );
        assert_eq!(
            hash_record(prev_hash, &canonical).as_str(),
            expected["hash"].as_str().unwrap(),
            "hash of {shape}"
        );
    }
}

#[test]
fn it_should_read_each_golden_body_back_to_the_same_text() {
    let golden = fixture();

    for (shape, expected) in golden["shapes"].as_object().unwrap() {
        let jcs = expected["jcs"].as_str().unwrap();

        let body: RecordBody = serde_json::from_str(jcs).unwrap();

        assert_eq!(body.canonical().unwrap(), jcs, "round trip of {shape}");
    }
}

#[test]
fn it_should_refuse_a_body_with_an_unknown_field() {
    let golden = fixture();
    let mut value: Value =
        serde_json::from_str(golden["shapes"]["node_full"]["jcs"].as_str().unwrap()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("surplus".to_owned(), json!(1));
    let tampered = value.to_string();

    let parsed = serde_json::from_str::<RecordBody>(&tampered);

    assert!(parsed.is_err());
}

#[test]
fn it_should_reject_a_where_field_name_with_json_path_or_sql_metacharacters() {
    let cases = [
        "a.b".to_owned(),
        "a[0]".to_owned(),
        "a\"b".to_owned(),
        "a'b".to_owned(),
        String::new(),
        "a".repeat(65),
    ];

    for name in &cases {
        let error = FieldName::parse(name).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidInput, "code for {name:?}");
    }
}
