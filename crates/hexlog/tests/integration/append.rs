use crate::support;

use hexlog::chain::{canonical, fingerprint, hash_record};
use hexlog::{AppendItem, AppendRequest, Author, ChainHead, RecordId};
use serde_json::{Value, json};
use support::catalog::reference_catalog;
use support::tamper::Tamper;
use support::{
    DEFAULT_INSTANT, Harness, commit, node, relation, request, retire, retire_relation, revise,
    rty, tenant, ty, with_external, with_snapshot,
};

const TEXT_WITH_ESCAPES: &str = "é\u{1}\"\\";

fn record_id(n: u64) -> String {
    format!("acme:0192a000-0000-7000-8000-{n:012x}")
}

fn rid(n: u64) -> RecordId {
    RecordId::parse(&record_id(n)).expect("valid record id")
}

fn author() -> Author {
    Author {
        agent: "ag".to_owned(),
        model: Some("m".to_owned()),
        client: "cli".to_owned(),
    }
}

fn request_by_author(key: Option<&str>, items: Vec<AppendItem>) -> AppendRequest {
    AppendRequest {
        author: author(),
        ..request("acme", key, items)
    }
}

/// Fingerprint do lote, calculado antes de `items` ir para a requisição.
fn batch_of(items: &[AppendItem], key: Option<&str>) -> Value {
    let print = fingerprint(items).unwrap();
    let count = items.len();
    match key {
        Some(key) => json!({"fingerprint": print.as_str(), "count": count, "key": key}),
        None => json!({"fingerprint": print.as_str(), "count": count}),
    }
}

fn written_author() -> Value {
    json!({"agent": "ag", "model": "m", "client": "cli"})
}

fn node_lineage_body(id: &str) -> Value {
    json!({"kind": "node", "id": id})
}

fn relation_lineage_body() -> Value {
    json!({
        "kind": "relation",
        "relation_type": "after",
        "from": "a.one",
        "to": "n.other",
        "from_origin": "tenant",
        "to_origin": "native",
    })
}

#[test]
fn it_should_store_the_golden_body_for_each_record_shape() {
    let harness = Harness::with_native_other();
    let catalog = reference_catalog();
    let note_hash = catalog.type_hash(&ty("note")).unwrap().as_str().to_owned();
    let strict_hash = catalog
        .type_hash(&ty("strict-note"))
        .unwrap()
        .as_str()
        .to_owned();
    let after_hash = catalog
        .relation_type_hash(&rty("after"))
        .unwrap()
        .as_str()
        .to_owned();
    let snapshot = json!({"kind": "git-commit", "id": "c".repeat(40)});
    let mut hexlog = harness.open_acme();
    let empty_head = hexlog.chain_head(&tenant("acme")).unwrap();

    let batch1 = vec![node("a.one", "note", json!({"text": "one"}))];
    hexlog
        .append(&request_by_author(None, batch1.clone()))
        .unwrap();

    let batch2 = vec![with_external(
        with_snapshot(node("a.two", "strict-note", json!({})), commit('c')),
        "x",
    )];
    hexlog
        .append(&request_by_author(None, batch2.clone()))
        .unwrap();

    let revision = revise(
        with_external(
            with_snapshot(
                node("a.two", "strict-note", json!({ "text": TEXT_WITH_ESCAPES })),
                commit('c'),
            ),
            "x",
        ),
        &rid(2),
    );
    let batch3 = vec![revision, relation("after", "a.one", "n.other")];
    hexlog
        .append(&request_by_author(Some("K1"), batch3.clone()))
        .unwrap();

    let batch4 = vec![retire("a.two", &rid(3))];
    hexlog
        .append(&request_by_author(None, batch4.clone()))
        .unwrap();

    let batch5 = vec![retire_relation("after", "a.one", "n.other", &rid(4))];
    hexlog
        .append(&request_by_author(None, batch5.clone()))
        .unwrap();
    drop(hexlog);

    let expected = [
        json!({
            "v": 1, "tenant": "acme", "seq": 1, "id": record_id(1), "at": DEFAULT_INSTANT,
            "type": "note", "lineage": node_lineage_body("a.one"), "retirement": false,
            "data": {"text": "one"}, "author": written_author(), "def_hash": note_hash,
            "batch": batch_of(&batch1, None),
        }),
        json!({
            "v": 1, "tenant": "acme", "seq": 2, "id": record_id(2), "at": DEFAULT_INSTANT,
            "type": "strict-note", "lineage": node_lineage_body("a.two"), "retirement": false,
            "data": {}, "author": written_author(), "snapshot": snapshot, "external_id": "x",
            "def_hash": strict_hash, "batch": batch_of(&batch2, None),
        }),
        json!({
            "v": 1, "tenant": "acme", "seq": 3, "id": record_id(3), "at": DEFAULT_INSTANT,
            "type": "strict-note", "lineage": node_lineage_body("a.two"),
            "revises": record_id(2), "retirement": false, "data": {"text": TEXT_WITH_ESCAPES},
            "author": written_author(), "snapshot": snapshot, "external_id": "x",
            "def_hash": strict_hash, "batch": batch_of(&batch3, Some("K1")),
        }),
        json!({
            "v": 1, "tenant": "acme", "seq": 4, "id": record_id(4), "at": DEFAULT_INSTANT,
            "type": "after", "lineage": relation_lineage_body(), "retirement": false,
            "author": written_author(), "native_catalog": "v1", "def_hash": after_hash,
        }),
        json!({
            "v": 1, "tenant": "acme", "seq": 5, "id": record_id(5), "at": DEFAULT_INSTANT,
            "type": "strict-note", "lineage": node_lineage_body("a.two"),
            "revises": record_id(3), "retirement": true, "author": written_author(),
            "def_hash": strict_hash, "batch": batch_of(&batch4, None),
        }),
        json!({
            "v": 1, "tenant": "acme", "seq": 6, "id": record_id(6), "at": DEFAULT_INSTANT,
            "type": "after", "lineage": relation_lineage_body(),
            "revises": record_id(4), "retirement": true, "author": written_author(),
            "native_catalog": "v1", "def_hash": after_hash, "batch": batch_of(&batch5, None),
        }),
    ];
    let mut tamper = Tamper::open(&harness.db_path());
    let stored_bodies: Vec<String> = (1..=6).map(|seq| tamper.read_body("acme", seq)).collect();
    let stored_hashes = tamper.read_text(
        "SELECT CAST(hash AS TEXT) FROM records WHERE tenant = ? ORDER BY seq",
        &["acme"],
    );
    let expected_bodies: Vec<String> = expected
        .iter()
        .map(|body| canonical(body).unwrap())
        .collect();
    let expected_hashes = chained_hashes(&empty_head, &expected_bodies);
    assert_eq!(stored_bodies, expected_bodies);
    assert_eq!(stored_hashes, expected_hashes);
}

/// Hash de cada record: `hash_record(prev, body)`, com `prev` o chain head vazio e depois o hash anterior.
fn chained_hashes(empty_head: &ChainHead, bodies: &[String]) -> Vec<String> {
    let mut previous = empty_head.hash.as_str().to_owned();
    bodies
        .iter()
        .map(|body| {
            previous = hash_record(&previous, body).as_str().to_owned();
            previous.clone()
        })
        .collect()
}
