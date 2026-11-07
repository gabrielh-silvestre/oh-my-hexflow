mod support;

use hexlog::{AppendRequest, Author, ErrorCode};
use serde_json::json;
use support::{
    FixedClock, Harness, append_one, assert_refusal, author, node, request, revise, sid, tenant, ty,
};

#[test]
fn it_should_replay_the_same_key_and_items_without_writing() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let req = request("acme", Some("K1"), vec![node("a.one", "note", json!({}))]);
    let first = hexlog.append(&req).unwrap();

    let second = hexlog.append(&req).unwrap();

    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(second.records, first.records);
    assert_eq!(second.chain_head, first.chain_head);
    assert_eq!(
        hexlog.chain_head(&tenant("acme")).unwrap(),
        first.chain_head
    );
}

#[test]
fn it_should_reject_the_same_key_with_different_items() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let written = request("acme", Some("K1"), vec![node("a.one", "note", json!({}))]);
    hexlog.append(&written).unwrap();
    let different = request(
        "acme",
        Some("K1"),
        vec![node("a.one", "note", json!({"text": "other"}))],
    );

    let error = hexlog.append(&different).unwrap_err();

    assert_refusal(&error, ErrorCode::IdempotencyConflict, "key-reused", "/key");
}

#[test]
fn it_should_replay_before_history_dependent_rules_when_the_base_was_superseded() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let req = request("acme", Some("K1"), vec![node("a.one", "note", json!({}))]);
    let first = hexlog.append(&req).unwrap();
    append_one(
        &mut hexlog,
        revise(
            node("a.one", "note", json!({"text": "newer"})),
            &first.records[0].id,
        ),
    );

    let replay = hexlog.append(&req).unwrap();

    assert!(replay.replayed);
}

#[test]
fn it_should_replay_before_static_rules_when_the_id_was_reserved_afterwards() {
    let mut harness = Harness::new();
    let req = request("acme", Some("K1"), vec![node("a.one", "note", json!({}))]);
    harness.open_acme().append(&req).unwrap();
    harness.native.reserve(sid("a.one"), ty("note"));
    let mut hexlog = harness.open();

    let replay = hexlog.append(&req).unwrap();

    assert!(replay.replayed);
}

#[test]
fn it_should_replay_regardless_of_the_author_and_scope_keys_per_tenant() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    hexlog.ensure_tenant(&tenant("beta")).unwrap();
    let items = || vec![node("a.one", "note", json!({}))];
    let original = request("acme", Some("K1"), items());
    hexlog.append(&original).unwrap();
    let other_author = AppendRequest {
        author: Author {
            agent: "someone-else".to_owned(),
            model: Some("m".to_owned()),
            ..author()
        },
        ..request("acme", Some("K1"), items())
    };

    let replay = hexlog.append(&other_author).unwrap();
    let other_tenant = hexlog
        .append(&request("beta", Some("K1"), items()))
        .unwrap();

    assert!(replay.replayed);
    assert!(!other_tenant.replayed);
}

#[test]
fn it_should_replay_even_when_the_clock_returns_a_malformed_instant() {
    let mut harness = Harness::new();
    let req = request("acme", Some("K1"), vec![node("a.one", "note", json!({}))]);
    harness.open_acme().append(&req).unwrap();
    harness.clock = FixedClock::at("2026-11-07 12:00:00");
    let mut hexlog = harness.open();

    let replay = hexlog.append(&req).unwrap();

    assert!(replay.replayed);
    assert_eq!(
        replay.chain_head,
        hexlog.chain_head(&tenant("acme")).unwrap()
    );
}
