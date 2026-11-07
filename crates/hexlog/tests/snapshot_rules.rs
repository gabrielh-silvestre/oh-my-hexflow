mod support;

use hexlog::{ErrorCode, ExternalId};
use serde_json::json;
use support::{
    Harness, append_one, assert_refusal, commit, node, request, retire, revise, sid, tenant,
    with_external, with_snapshot,
};

#[test]
fn it_should_reject_a_node_without_snapshot_when_the_type_requires_it() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("a.one", "strict-note", json!({}))],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "snapshot-required",
        "/items/0/snapshot",
    );
}

#[test]
fn it_should_accept_a_node_with_a_valid_snapshot_when_the_type_requires_it() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let item = with_snapshot(node("a.one", "strict-note", json!({})), commit('a'));

    hexlog.append(&request("acme", None, vec![item])).unwrap();

    let view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();
    assert_eq!(view.snapshot, Some(commit('a')));
}

#[test]
fn it_should_reject_a_node_with_snapshot_when_the_type_does_not_require_it() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let item = with_snapshot(node("a.one", "note", json!({})), commit('a'));

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "snapshot-not-allowed",
        "/items/0/snapshot",
    );
}

#[test]
fn it_should_require_snapshot_again_on_each_revision_of_a_requires_snapshot_type() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(
        &mut hexlog,
        with_snapshot(node("a.one", "strict-note", json!({})), commit('a')),
    );

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![revise(node("a.one", "strict-note", json!({})), &first)],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "snapshot-required",
        "/items/0/snapshot",
    );
}

#[test]
fn it_should_reject_snapshot_on_a_revision_of_a_type_without_requires_snapshot() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));
    let item = with_snapshot(
        revise(node("a.one", "note", json!({})), &first),
        commit('a'),
    );

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "snapshot-not-allowed",
        "/items/0/snapshot",
    );
}

#[test]
fn it_should_reject_snapshot_on_a_retirement_of_a_requires_snapshot_type() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(
        &mut hexlog,
        with_snapshot(node("a.one", "strict-note", json!({})), commit('a')),
    );
    let item = with_snapshot(retire("a.one", &first), commit('b'));

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "snapshot-not-allowed",
        "/items/0/snapshot",
    );
}

#[test]
fn it_should_accept_a_retirement_without_snapshot_for_a_requires_snapshot_type() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(
        &mut hexlog,
        with_snapshot(node("a.one", "strict-note", json!({})), commit('a')),
    );

    append_one(&mut hexlog, retire("a.one", &first));

    let view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap();
    assert_eq!(view, None);
}

#[test]
fn it_should_accept_external_id_on_a_node() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let item = with_external(node("a.one", "note", json!({})), "ext-1");

    hexlog.append(&request("acme", None, vec![item])).unwrap();

    let view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();
    assert_eq!(view.external_id, Some(ExternalId::parse("ext-1").unwrap()));
}

#[test]
fn it_should_reject_external_id_on_a_retirement() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));
    let item = with_external(retire("a.one", &first), "ext-1");

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "external-id-not-allowed",
        "/items/0/external_id",
    );
}

#[test]
fn it_should_reject_external_id_that_is_empty_or_longer_than_200_characters() {
    let too_long_in_utf16_units = format!("{}{}", "a".repeat(199), "\u{1F4A9}");
    let cases = [String::new(), "a".repeat(201), too_long_in_utf16_units];

    for text in &cases {
        let error = ExternalId::parse(text).unwrap_err();

        assert_refusal(&error, ErrorCode::InvalidRecord, "external-id-format", "");
    }
}

#[test]
fn it_should_not_inherit_external_id_when_a_revision_omits_it() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(
        &mut hexlog,
        with_external(node("a.one", "note", json!({})), "ext-1"),
    );

    append_one(
        &mut hexlog,
        revise(node("a.one", "note", json!({})), &first),
    );

    let view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();
    assert_eq!(view.external_id, None);
}
