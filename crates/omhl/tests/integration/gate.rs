use crate::support;

use ::omhl::gate::{Question, QuestionKind};
use hexlog::{FieldName, ListFilter, Scalar};
use serde_json::json;
use support::gate::{
    all_filter, append_acme, evaluate_on_hexlog, expected_report, node_item, open_acme,
    relation_item, retirement_item, snapshot_node_item, type_filter,
};
use support::raw::RawHarness;
use support::{EmptyNative, snap, tname};

// Hexlog real

#[test]
fn it_should_forward_type_id_prefix_snapshot_and_where_eq_to_the_store() {
    let harness = RawHarness::new();
    let native = EmptyNative;
    let mut hexlog = open_acme(&harness, &native);
    let open = json!({"status": "open"});
    append_acme(
        &mut hexlog,
        vec![
            snapshot_node_item("team.match", "snap-note", open.clone(), snap('a')),
            snapshot_node_item("team.type", "snap-memo", open.clone(), snap('a')),
            snapshot_node_item("other.prefix", "snap-note", open.clone(), snap('a')),
            snapshot_node_item("team.snapshot", "snap-note", open, snap('b')),
            snapshot_node_item(
                "team.field",
                "snap-note",
                json!({"status": "closed"}),
                snap('a'),
            ),
        ],
    );
    let select = ListFilter {
        type_name: Some(tname("snap-note")),
        id_prefix: Some("team".to_owned()),
        snapshot: Some(snap('a')),
        where_eq: vec![(
            FieldName::parse("status").expect("valid field name"),
            Scalar::Str("open".to_owned()),
        )],
    };
    let question = Question::Occurred { select, min: 1 };

    let report = evaluate_on_hexlog(&mut hexlog, 50, question);

    assert_eq!(report.matched, 1);
}

#[test]
fn it_should_read_every_page_of_a_selection() {
    let harness = RawHarness::new();
    let native = EmptyNative;
    let mut hexlog = open_acme(&harness, &native);
    for batch in 0..3 {
        let items = (0..40)
            .map(|index| {
                node_item(
                    &format!("bulk.n{:03}", batch * 40 + index),
                    "note",
                    json!({}),
                )
            })
            .collect();
        append_acme(&mut hexlog, items);
    }
    let question = Question::Occurred {
        select: type_filter("note"),
        min: 1,
    };

    let report = evaluate_on_hexlog(&mut hexlog, 50, question);

    assert_eq!(report.matched, 120);
}

#[test]
fn it_should_ignore_a_retired_relation() {
    let harness = RawHarness::new();
    let native = EmptyNative;
    let mut hexlog = open_acme(&harness, &native);
    append_acme(
        &mut hexlog,
        vec![
            node_item("target.one", "note", json!({})),
            node_item("source.one", "note", json!({})),
        ],
    );
    let relation_ids = append_acme(
        &mut hexlog,
        vec![relation_item("supports", "source.one", "target.one")],
    );
    append_acme(
        &mut hexlog,
        vec![retirement_item(
            "supports",
            "source.one",
            "target.one",
            &relation_ids[0],
        )],
    );
    let of = ListFilter {
        id_prefix: Some("target".to_owned()),
        ..all_filter()
    };
    let question = Question::Approved { of };

    let report = evaluate_on_hexlog(&mut hexlog, 50, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, false, 1, &["target.one"])
    );
}
