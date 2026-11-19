use serde_json::{Number, Value, json};

use crate::chain::{anchor, anchor_doc, canonical, fingerprint, hash_record, sha256hex};
use crate::{
    AppendItem, AppendRequest, Author, ErrorCode, ExternalId, HexlogError, IdempotencyKey,
    ItemBody, Lineage, RecordId, RelationTypeName, SemanticId, Snapshot, SnapshotKind, TenantId,
    TypeName,
};

const VECTORS: &str = include_str!("../../../tests/fixtures/rfc8785_vectors.json");
const RECORDS: &str = include_str!("../../../tests/fixtures/golden_records.json");
const FINGERPRINTS: &str = include_str!("../../../tests/fixtures/golden_fingerprints.json");
const BASE: &str = "acme:0192a000-0000-7000-8000-000000000001";

fn fixture(source: &str) -> Value {
    serde_json::from_str(source).unwrap()
}

fn vector_input(vector: &Value) -> Value {
    match vector.get("ieee754") {
        Some(bits) => {
            let raw = u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap();
            Value::Number(Number::from_f64(f64::from_bits(raw)).unwrap())
        }
        None => serde_json::from_str(vector["input"].as_str().unwrap()).unwrap(),
    }
}

fn semantic(text: &str) -> SemanticId {
    SemanticId::parse(text).unwrap()
}

fn base() -> RecordId {
    RecordId::parse(BASE).unwrap()
}

fn node_full() -> AppendItem {
    AppendItem {
        body: ItemBody::Node {
            id: semantic("a.b"),
            type_name: TypeName::parse("note").unwrap(),
            data: json!({"k": "v"}),
        },
        base: Some(base()),
        snapshot: Some(Snapshot::new(SnapshotKind::GitCommit, &"c".repeat(40)).unwrap()),
        external_id: Some(ExternalId::parse("x").unwrap()),
    }
}

fn node_minimal() -> AppendItem {
    AppendItem {
        body: ItemBody::Node {
            id: semantic("a.b"),
            type_name: TypeName::parse("note").unwrap(),
            data: json!({}),
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

fn relation() -> AppendItem {
    AppendItem {
        body: ItemBody::Relation {
            relation_type: RelationTypeName::parse("after").unwrap(),
            from: semantic("a.b"),
            to: semantic("c.d"),
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

fn retirement_node() -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: Lineage::Node(semantic("a.b")),
        },
        base: Some(base()),
        snapshot: None,
        external_id: None,
    }
}

fn retirement_relation() -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: Lineage::Relation {
                relation_type: RelationTypeName::parse("after").unwrap(),
                from: semantic("a.b"),
                to: semantic("c.d"),
            },
        },
        base: Some(base()),
        snapshot: None,
        external_id: None,
    }
}

fn items_for(shape: &str) -> Vec<AppendItem> {
    match shape {
        "node_full" => vec![node_full()],
        "node_minimal" => vec![node_minimal()],
        "relation" => vec![relation()],
        "retirement_node" => vec![retirement_node()],
        "retirement_relation" => vec![retirement_relation()],
        "list_node_then_relation" => vec![node_full(), relation()],
        "list_relation_then_node" => vec![relation(), node_full()],
        other => panic!("fixture shape without a builder: {other}"),
    }
}

fn request(author: Author, key: Option<IdempotencyKey>, items: Vec<AppendItem>) -> AppendRequest {
    AppendRequest {
        tenant: TenantId::parse("acme").unwrap(),
        author,
        key,
        items,
    }
}

#[test]
fn it_should_canonicalize_per_rfc_8785_vectors() {
    let vectors = fixture(VECTORS);

    for vector in vectors["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let input = vector_input(vector);

        let text = canonical(&input).unwrap();

        assert_eq!(text, vector["expected"].as_str().unwrap(), "vector {name}");
    }
}

#[test]
fn it_should_match_the_hexlog_ts_hash_for_each_golden_record() {
    let golden = fixture(RECORDS);

    for (name, record) in golden["records"].as_object().unwrap() {
        let text = canonical(&record["document"]).unwrap();

        let hash = hash_record(record["prev_hash"].as_str().unwrap(), &text);

        assert_eq!(text, record["jcs"].as_str().unwrap(), "jcs of {name}");
        assert_eq!(
            hash.as_str(),
            record["hash"].as_str().unwrap(),
            "hash of {name}"
        );
    }
}

#[test]
fn it_should_match_the_hexlog_ts_anchor_for_a_tenant() {
    let golden = fixture(RECORDS);
    let expected = &golden["anchor"];
    let tenant = TenantId::parse(expected["tenant"].as_str().unwrap()).unwrap();

    let doc = anchor_doc(&tenant, expected["created_at"].as_str().unwrap());

    assert_eq!(doc, expected["anchor_doc"].as_str().unwrap());
    assert_eq!(anchor(&doc).as_str(), expected["anchor"].as_str().unwrap());
}

#[test]
fn it_should_refuse_an_integer_beyond_2_to_the_53_in_canonical() {
    let cases = [
        json!(9_007_199_254_740_993_u64),
        json!(-9_007_199_254_740_993_i64),
        json!(u64::MAX),
        json!({"outer": [{"n": 9_007_199_254_740_993_u64}]}),
    ];

    for value in &cases {
        let error = canonical(value).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidInput, "code for {value}");
        assert_eq!(error.details[0].code, "number-out-of-range", "{value}");
        assert_eq!(error.details[0].path, "", "path for {value}");
    }
}

#[test]
fn it_should_fail_the_fingerprint_for_an_integer_beyond_2_to_the_53() {
    let mut item = node_minimal();
    item.body = ItemBody::Node {
        id: semantic("a.b"),
        type_name: TypeName::parse("note").unwrap(),
        data: json!({"n": 9_007_199_254_740_993_u64}),
    };

    let result = fingerprint(&[item]);

    assert!(result.is_err());
}

#[test]
fn it_should_give_the_same_fingerprint_to_one_and_one_point_zero() {
    let node_with = |data: Value| AppendItem {
        body: ItemBody::Node {
            id: semantic("a.b"),
            type_name: TypeName::parse("note").unwrap(),
            data,
        },
        ..node_minimal()
    };
    let integer = node_with(serde_json::from_str(r#"{"a":1}"#).unwrap());
    let float = node_with(serde_json::from_str(r#"{"a":1.0}"#).unwrap());

    let from_integer = fingerprint(&[integer]).unwrap();
    let from_float = fingerprint(&[float]).unwrap();

    assert_eq!(from_integer, from_float);
}

#[test]
fn it_should_derive_the_fingerprint_without_key_and_author() {
    let shape = "list_node_then_relation";
    let first = request(
        Author {
            agent: "agent-one".to_owned(),
            model: Some("model-one".to_owned()),
            client: "cli".to_owned(),
        },
        Some(IdempotencyKey::parse("key-one").unwrap()),
        items_for(shape),
    );
    let second = request(
        Author {
            agent: "agent-two".to_owned(),
            model: None,
            client: "mcp".to_owned(),
        },
        None,
        items_for(shape),
    );

    let from_first = fingerprint(&first.items).unwrap();
    let from_second = fingerprint(&second.items).unwrap();

    assert_eq!(from_first, from_second);
}

#[test]
fn it_should_match_the_ts_fingerprint_for_each_item_shape() {
    let golden = fixture(FINGERPRINTS);

    for (shape, expected) in golden["shapes"].as_object().unwrap() {
        let items = items_for(shape);

        let hash = fingerprint(&items).unwrap();

        assert_eq!(
            hash.as_str(),
            expected["fingerprint"].as_str().unwrap(),
            "shape {shape}"
        );
    }
}

const SMALL_STACK: usize = 2 * 1024 * 1024;

type Refusal = (ErrorCode, String, String);

fn nested(containers: usize) -> Value {
    let mut value = Value::Null;
    for _ in 0..containers {
        value = Value::Array(vec![value]);
    }
    value
}

fn refusal<T>(result: Result<T, HexlogError>) -> Option<Refusal> {
    let error = result.err()?;
    let detail = &error.details[0];
    Some((error.code, detail.code.clone(), detail.path.clone()))
}

// O valor nasce e morre dentro da thread: o Drop recursivo do Value também usa a pilha.
fn on_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(work)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn it_should_emit_noncharacters_raw_like_the_ts_oracle() {
    // Reproduz o hash esperado com o Hexlog TS (commit 5d1e22b, sem npm install):
    // node -e "const c=require('/home/gabriel/personal/hexlog/node_modules/canonicalize');const f=c.default??c;const k='a'+String.fromCodePoint(0xFFFE,0xFFFF)+'b'+String.fromCodePoint(0xFDD0,0xFDEF,0x1FFFE,0x10FFFF)+'z';console.log(require('crypto').createHash('sha256').update(f({k})).digest('hex'))"
    let raw = "a\u{FFFE}\u{FFFF}b\u{FDD0}\u{FDEF}\u{1FFFE}\u{10FFFF}z";
    let value = json!({ "k": raw });

    let text = canonical(&value).unwrap();

    assert_eq!(text, format!(r#"{{"k":"{raw}"}}"#));
    assert_eq!(
        sha256hex(&text).as_str(),
        "cccc4a026451b00f922cc2986c70a03ad855b8a0aabb65156dd148e8a610e6dd"
    );
}

#[test]
fn it_should_refuse_a_value_nested_beyond_the_canonical_stack_limit() {
    let refuse_at = |depth| on_small_stack(move || refusal(canonical(&nested(depth))));
    let expected = Some((
        ErrorCode::InvalidInput,
        "max-depth-exceeded".to_owned(),
        String::new(),
    ));

    assert_eq!(refuse_at(256), None);
    assert_eq!(refuse_at(257), expected);
    assert_eq!(refuse_at(5_000), expected);
}

#[test]
fn it_should_fail_the_fingerprint_for_data_nested_beyond_126_levels_without_overflowing() {
    let refuse_at = |depth| {
        on_small_stack(move || {
            let item = AppendItem {
                body: ItemBody::Node {
                    id: semantic("a.b"),
                    type_name: TypeName::parse("note").unwrap(),
                    data: nested(depth),
                },
                ..node_minimal()
            };
            refusal(fingerprint(&[item]))
        })
    };
    let expected = Some((
        ErrorCode::InvalidInput,
        "max-depth-exceeded".to_owned(),
        "/0/body/data".to_owned(),
    ));

    assert_eq!(refuse_at(126), None);
    assert_eq!(refuse_at(127), expected);
    assert_eq!(refuse_at(5_000), expected);
}
