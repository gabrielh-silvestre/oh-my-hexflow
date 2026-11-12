mod support;

use hexlog::{ErrorCode, ExternalId, SnapshotKind};
use serde_json::json;
use support::tamper::Inspect;
use support::{
    Harness, append_one, assert_refusal, commit, node, request, retire, revise, sid, tenant, tree,
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

#[test]
fn it_should_round_trip_a_git_tree_snapshot_through_append_and_get() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let item = with_snapshot(node("a.one", "strict-note", json!({})), tree('a'));

    let appended = hexlog.append(&request("acme", None, vec![item]));
    let view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();

    let snapshot = view.snapshot.clone().unwrap();
    assert!(appended.is_ok());
    assert_eq!(view.snapshot, Some(tree('a')));
    assert_eq!(snapshot.kind(), SnapshotKind::GitTree);
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        json!({"kind": "git-tree", "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})
    );
}

#[test]
fn it_should_keep_the_chain_verifying_with_a_git_tree_snapshot() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    append_one(
        &mut hexlog,
        with_snapshot(node("a.one", "strict-note", json!({})), commit('a')),
    );
    append_one(
        &mut hexlog,
        with_snapshot(node("a.two", "strict-note", json!({})), tree('b')),
    );

    let report = hexlog.verify_chain(&tenant("acme"), None).unwrap();

    let head = hexlog.chain_head(&tenant("acme")).unwrap();
    assert!(report.ok);
    assert_eq!(report.total_records, 2);
    assert_eq!(report.total_breaks, 0);
    assert!(report.breaks.is_empty());
    assert_eq!(report.chain_head, head.hash);
}

#[test]
fn it_should_still_verify_a_database_written_before_the_git_tree_kind_existed() {
    let harness = Harness::new();
    {
        let mut hexlog = harness.open_acme();
        append_one(
            &mut hexlog,
            with_snapshot(node("a.one", "strict-note", json!({})), commit('a')),
        );
        append_one(
            &mut hexlog,
            with_snapshot(node("a.two", "strict-note", json!({})), commit('b')),
        );
    }
    let mut inspect = Inspect::open(&harness.db_path());
    let columns = inspect.read_text(
        "SELECT snapshot_kind || ' ' || snapshot_id FROM records WHERE tenant = ? ORDER BY seq",
        &["acme"],
    );
    let first_body = inspect.read_text(
        "SELECT CAST(body AS TEXT) FROM records WHERE tenant = ? AND seq = 1",
        &["acme"],
    );
    drop(inspect);

    let mut hexlog = harness.open();
    let before = hexlog.verify_chain(&tenant("acme"), None).unwrap();
    let old_view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();
    append_one(
        &mut hexlog,
        with_snapshot(node("a.three", "strict-note", json!({})), tree('c')),
    );
    let after = hexlog.verify_chain(&tenant("acme"), None).unwrap();

    assert_eq!(
        columns,
        [
            "git-commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "git-commit bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ]
    );
    assert!(first_body[0].contains(
        r#""snapshot":{"id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","kind":"git-commit"}"#
    ));
    assert!(before.ok);
    assert_eq!(before.total_records, 2);
    assert_eq!(before.total_breaks, 0);
    assert_eq!(old_view.snapshot, Some(commit('a')));
    assert_eq!(old_view.snapshot.unwrap().kind(), SnapshotKind::GitCommit);
    assert!(after.ok);
    assert_eq!(after.total_records, 3);
    assert!(after.breaks.is_empty());
}
