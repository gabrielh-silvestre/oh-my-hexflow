mod support;

use hexlog::chain::{anchor, anchor_doc};
use hexlog::{ChainHead, ErrorCode};
use serde_json::json;
use support::catalog::{catalog_of, reference_catalog};
use support::tamper::Tamper;
use support::{
    DEFAULT_INSTANT, FixedClock, Harness, all, append_one, assert_fork_rejected, assert_refusal,
    detail_pairs, nested_data, node, node_lineage, on_small_stack, page, request, retire, revise,
    sid, tenant, ty, unused_record_id,
};

#[test]
fn it_should_return_the_current_revision_when_getting_by_semantic_id() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(
        &mut hexlog,
        node("area.sub.item", "note", json!({"text": "one"})),
    );
    let second = append_one(
        &mut hexlog,
        revise(
            node("area.sub.item", "note", json!({"text": "two"})),
            &first,
        ),
    );

    let view = hexlog
        .get(&tenant("acme"), &sid("area.sub.item"))
        .unwrap()
        .unwrap();

    assert_eq!(view.record_id, second);
    assert_eq!(view.type_name, ty("note"));
    assert_eq!(view.data, json!({"text": "two"}));
    assert_eq!(view.seq, 2);
}

#[test]
fn it_should_list_the_records_of_a_lineage_in_order_when_reading_lineage_records() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(
        &mut hexlog,
        node("area.sub.item", "note", json!({"text": "one"})),
    );
    append_one(
        &mut hexlog,
        revise(
            node("area.sub.item", "note", json!({"text": "two"})),
            &first,
        ),
    );

    let records = hexlog
        .lineage_records(&tenant("acme"), &node_lineage("area.sub.item"))
        .unwrap();

    let summary: Vec<_> = records
        .into_iter()
        .map(|record| (record.seq, record.revises))
        .collect();
    assert_eq!(summary, [(1, None), (2, Some(first))]);
}

#[test]
fn it_should_return_none_when_the_id_was_never_written() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();

    let view = hexlog.get(&tenant("acme"), &sid("area.sub.item")).unwrap();

    assert_eq!(view, None);
}

#[test]
fn it_should_hide_a_retired_node_and_allow_revising_on_top_of_the_retirement() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({"text": "one"})));
    let retirement = append_one(&mut hexlog, retire("a.one", &first));
    let hidden = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap();

    append_one(
        &mut hexlog,
        revise(node("a.one", "note", json!({"text": "back"})), &retirement),
    );

    let revived = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();
    assert_eq!(hidden, None);
    assert_eq!(revived.data, json!({"text": "back"}));
}

#[test]
fn it_should_copy_type_and_def_hash_from_the_latest_record_when_retiring() {
    let harness = Harness::new();
    let original_hash = reference_catalog()
        .type_hash(&ty("note"))
        .unwrap()
        .as_str()
        .to_owned();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({"text": "one"})));
    drop(hexlog);
    let changed = catalog_of(vec![(
        "note",
        json!({"type": "object", "description": "changed after the first write"}),
    )]);
    let mut hexlog = harness.open_with(&harness.config(), changed);

    append_one(&mut hexlog, retire("a.one", &first));

    drop(hexlog);
    let stored = Tamper::open(&harness.db_path()).read_text(
        "SELECT type || ' ' || def_hash FROM records WHERE tenant = ? AND seq = ?",
        &["acme", "2"],
    );
    assert_eq!(stored, [format!("note {original_hash}")]);
}

#[test]
fn it_should_reject_a_second_root_for_an_existing_semantic_id() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("a.one", "note", json!({}))],
        ))
        .unwrap_err();

    assert_fork_rejected(&error, "/items/0/base", &first, false);
}

#[test]
fn it_should_reject_a_revision_with_a_stale_base() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));
    let second = append_one(
        &mut hexlog,
        revise(node("a.one", "note", json!({})), &first),
    );

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![revise(node("a.one", "note", json!({})), &first)],
        ))
        .unwrap_err();

    assert_fork_rejected(&error, "/items/0/base", &second, false);
}

#[test]
fn it_should_reject_a_base_for_a_semantic_id_that_was_never_written() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let item = revise(node("a.one", "note", json!({})), &unused_record_id());

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "base-unknown",
        "/items/0/base",
    );
}

#[test]
fn it_should_reject_a_revision_that_changes_the_type() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![revise(node("a.one", "task", json!({})), &first)],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "type-changed",
        "/items/0/body/type",
    );
}

#[test]
fn it_should_reject_data_violating_the_schema_and_list_every_violation_of_the_batch() {
    let harness = Harness::new();
    let titled = catalog_of(vec![(
        "titled",
        json!({
            "type": "object",
            "required": ["title"],
            "properties": {"title": {"type": "string", "maxLength": 10}}
        }),
    )]);
    let mut hexlog = harness.open_acme_with(&harness.config(), titled);
    let items = vec![
        node("a.one", "titled", json!({"title": "far too long a title"})),
        node("a.two", "titled", json!({})),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidRecord);
    assert_eq!(
        detail_pairs(&error),
        [
            ("/items/0/body/data/title", "max-length"),
            ("/items/1/body/data/title", "required"),
        ]
    );
}

#[test]
fn it_should_reject_a_reserved_semantic_id() {
    let mut harness = Harness::new();
    harness
        .native
        .reserve(sid("reserved.example.id"), ty("note"));
    let mut hexlog = harness.open_acme();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("reserved.example.id", "note", json!({}))],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::ReservedId,
        "reserved-id",
        "/items/0/body/id",
    );
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap().seq, 0);
}

#[test]
fn it_should_mark_a_tenant_node_as_shadowed_when_a_native_gains_the_same_id() {
    let mut harness = Harness::new();
    let mut hexlog = harness.open_acme();
    append_one(&mut hexlog, node("a.one", "note", json!({})));
    drop(hexlog);
    harness.native.reserve(sid("a.one"), ty("note"));
    let mut hexlog = harness.open();

    let view = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();

    assert!(view.shadowed);
}

#[test]
fn it_should_not_echo_data_values_in_error_messages() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let secret = "SECRET-VALUE-XYZ";
    let item = node("a.one", "note", json!({"count": secret}));

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert!(!format!("{error:?}").contains(secret));
}

#[test]
fn it_should_round_trip_data_nested_to_the_limit_and_refuse_one_level_more() {
    let (stored, error, head_before, head_after) = on_small_stack(|| {
        let harness = Harness::new();
        let mut hexlog = harness.open_acme();
        append_one(&mut hexlog, node("a.deep", "note", nested_data(126)));
        let head_before = hexlog.chain_head(&tenant("acme")).unwrap();
        let stored = hexlog
            .get(&tenant("acme"), &sid("a.deep"))
            .unwrap()
            .unwrap()
            .data;

        let error = hexlog
            .append(&request(
                "acme",
                None,
                vec![node("a.deeper", "note", nested_data(127))],
            ))
            .unwrap_err();

        let head_after = hexlog.chain_head(&tenant("acme")).unwrap();
        (stored, error, head_before, head_after)
    });

    assert_eq!(stored, nested_data(126));
    assert_refusal(
        &error,
        ErrorCode::InvalidInput,
        "max-depth-exceeded",
        "/items/0/body/data",
    );
    assert_eq!(head_after, head_before);
}

#[test]
fn it_should_refuse_data_nested_thousands_of_levels_without_overflowing_the_stack() {
    const BIG_STACK: usize = 64 * 1024 * 1024;
    const SMALL_STACK: usize = 2 * 1024 * 1024;

    // O valor fundo nasce e morre na thread grande: o `Drop` recursivo do `Value` não cabe em 2 MiB.
    let (error, head_before, head_after) = std::thread::Builder::new()
        .stack_size(BIG_STACK)
        .spawn(|| {
            let harness = Harness::new();
            let mut hexlog = harness.open_acme();
            let deep = request(
                "acme",
                None,
                vec![node("a.deep", "note", nested_data(5_000))],
            );
            let head_before = hexlog.chain_head(&tenant("acme")).unwrap();

            let error = std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(SMALL_STACK)
                    .spawn_scoped(scope, || hexlog.append(&deep).unwrap_err())
                    .expect("thread spawns")
                    .join()
                    .expect("thread does not panic")
            });

            let head_after = hexlog.chain_head(&tenant("acme")).unwrap();
            (error, head_before, head_after)
        })
        .expect("thread spawns")
        .join()
        .expect("thread does not panic");

    assert_refusal(
        &error,
        ErrorCode::InvalidInput,
        "max-depth-exceeded",
        "/items/0/body/data",
    );
    assert_eq!(head_after, head_before);
}

#[test]
fn it_should_report_tenant_not_found_for_reads_and_writes_of_an_unknown_tenant() {
    let harness = Harness::new();
    let mut hexlog = harness.open();
    let ghost = tenant("ghost");

    let errors = [
        hexlog
            .append(&request(
                "ghost",
                None,
                vec![node("a.one", "note", json!({}))],
            ))
            .unwrap_err(),
        hexlog.get(&ghost, &sid("a.one")).unwrap_err(),
        hexlog
            .lineage_records(&ghost, &node_lineage("a.one"))
            .unwrap_err(),
        hexlog.list(&ghost, &all(), &page(10)).unwrap_err(),
        hexlog.chain_head(&ghost).unwrap_err(),
    ];

    for error in &errors {
        assert_refusal(
            error,
            ErrorCode::TenantNotFound,
            "tenant-not-found",
            "/tenant",
        );
    }
}

#[test]
fn it_should_reject_a_retirement_of_a_lineage_that_was_never_written() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![retire("a.one", &unused_record_id())],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "lineage-not-found",
        "/items/0/body/lineage",
    );
}

#[test]
fn it_should_reject_retiring_a_node_that_is_already_retired() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));
    let retirement = append_one(&mut hexlog, retire("a.one", &first));

    let again = hexlog
        .append(&request("acme", None, vec![retire("a.one", &retirement)]))
        .unwrap_err();
    let stale = hexlog
        .append(&request("acme", None, vec![retire("a.one", &first)]))
        .unwrap_err();

    assert_refusal(
        &again,
        ErrorCode::InvalidRecord,
        "already-retired",
        "/items/0/body/lineage",
    );
    assert_fork_rejected(&stale, "/items/0/base", &retirement, true);
}

#[test]
fn it_should_reject_a_retirement_with_a_stale_base() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));
    let second = append_one(
        &mut hexlog,
        revise(node("a.one", "note", json!({})), &first),
    );

    let error = hexlog
        .append(&request("acme", None, vec![retire("a.one", &first)]))
        .unwrap_err();

    assert_fork_rejected(&error, "/items/0/base", &second, false);
}

#[test]
fn it_should_flag_retired_true_in_the_fork_detail_when_the_latest_record_is_a_retirement() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let first = append_one(&mut hexlog, node("a.one", "note", json!({})));
    let retirement = append_one(&mut hexlog, retire("a.one", &first));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![revise(node("a.one", "note", json!({})), &first)],
        ))
        .unwrap_err();

    assert_fork_rejected(&error, "/items/0/base", &retirement, true);
}

#[test]
fn it_should_fail_with_internal_and_write_nothing_when_the_clock_returns_a_malformed_instant() {
    let mut harness = Harness::new();
    let head_before = harness.open_acme().chain_head(&tenant("acme")).unwrap();
    harness.clock = FixedClock::at("2026-11-07 12:00:00");
    let mut hexlog = harness.open();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("a.one", "note", json!({}))],
        ))
        .unwrap_err();

    assert_refusal(&error, ErrorCode::Internal, "clock-format", "");
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_stamp_every_record_of_a_batch_with_the_same_instant_and_contiguous_seq() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let items = vec![
        node("a.one", "note", json!({})),
        node("a.two", "note", json!({})),
        node("a.three", "note", json!({})),
    ];

    let outcome = hexlog.append(&request("acme", None, items)).unwrap();

    let stamps: Vec<_> = ["a.one", "a.two", "a.three"]
        .into_iter()
        .map(|id| {
            let mut records = hexlog
                .lineage_records(&tenant("acme"), &node_lineage(id))
                .unwrap();
            records.remove(0).at
        })
        .collect();
    let seqs: Vec<_> = outcome.records.iter().map(|record| record.seq).collect();
    assert_eq!(stamps, [DEFAULT_INSTANT; 3]);
    assert_eq!(seqs, [1, 2, 3]);
}

#[test]
fn it_should_create_a_tenant_once_and_derive_its_anchor_from_the_clock() {
    let harness = Harness::new();
    let mut hexlog = harness.open();
    let acme = tenant("acme");

    let first = hexlog.ensure_tenant(&acme).unwrap();
    let second = hexlog.ensure_tenant(&acme).unwrap();

    assert!(first.created);
    assert!(!second.created);
    assert_eq!(second.anchor, first.anchor);
    assert_eq!(second.created_at, first.created_at);
    assert_eq!(first.created_at, DEFAULT_INSTANT);
    assert_eq!(first.anchor, anchor(&anchor_doc(&acme, DEFAULT_INSTANT)));
}

#[test]
fn it_should_report_the_anchor_as_chain_head_of_an_empty_tenant_and_the_last_record_after_a_write()
{
    let harness = Harness::new();
    let mut hexlog = harness.open();
    let info = hexlog.ensure_tenant(&tenant("acme")).unwrap();
    let empty = hexlog.chain_head(&tenant("acme")).unwrap();

    let outcome = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("a.one", "note", json!({}))],
        ))
        .unwrap();

    let after = hexlog.chain_head(&tenant("acme")).unwrap();
    assert_eq!(
        empty,
        ChainHead {
            seq: 0,
            hash: info.anchor
        }
    );
    assert_eq!(after, outcome.chain_head);
}
