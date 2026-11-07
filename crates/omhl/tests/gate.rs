mod support;

use ::omhl::error::{OmhlError, RuleCode};
use ::omhl::gate::{EVIDENCE_CAP, GateReport, Question, QuestionKind, read_consistent};
use hexlog::{EndpointStatus, FieldName, ListFilter, RelationView, Scalar};
use serde_json::json;
use support::gate::{
    FakeSource, all_filter, append_acme, chain_head, evaluate_on_hexlog, evaluate_one,
    expected_report, node_item, node_views, open_acme, relation_item, relation_view,
    retirement_item, snapshot_node_item, type_filter,
};
use support::raw::RawHarness;
use support::{EmptyNative, rty, snap, tenant, tname};

// Approved

#[test]
fn it_should_pass_approved_when_every_match_has_a_current_supporter_and_no_contradiction() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["a.one"]))
        .with_relations(
            "supports",
            vec![relation_view("supports", "s.one", "a.one")],
        );
    let question = Question::Approved {
        of: type_filter("note"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, true, 1, &[])
    );
}

#[test]
fn it_should_fail_approved_when_nothing_matches() {
    let mut source = FakeSource::new().with_nodes(type_filter("note"), Vec::new());
    let question = Question::Approved {
        of: type_filter("note"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, false, 0, &[])
    );
}

#[test]
fn it_should_fail_approved_and_list_a_match_without_a_supporter() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["a.one", "a.two"]))
        .with_relations(
            "supports",
            vec![relation_view("supports", "s.one", "a.one")],
        );
    let question = Question::Approved {
        of: type_filter("note"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, false, 2, &["a.two"])
    );
}

#[test]
fn it_should_fail_approved_when_a_match_is_contradicted() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["a.one"]))
        .with_relations(
            "supports",
            vec![relation_view("supports", "s.one", "a.one")],
        )
        .with_relations(
            "contradicts",
            vec![relation_view("contradicts", "c.one", "a.one")],
        );
    let question = Question::Approved {
        of: type_filter("note"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, false, 1, &["a.one"])
    );
}

#[test]
fn it_should_ignore_a_supporter_whose_node_is_retired() {
    let retired_supporter = RelationView {
        from_status: EndpointStatus::Retired,
        ..relation_view("supports", "s.one", "a.one")
    };
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["a.one"]))
        .with_relations("supports", vec![retired_supporter]);
    let question = Question::Approved {
        of: type_filter("note"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, false, 1, &["a.one"])
    );
}

#[test]
fn it_should_ignore_a_supporter_whose_native_endpoint_is_missing() {
    let missing_supporter = RelationView {
        from_status: EndpointStatus::MissingNative,
        ..relation_view("supports", "s.one", "a.one")
    };
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["a.one"]))
        .with_relations("supports", vec![missing_supporter]);
    let question = Question::Approved {
        of: type_filter("note"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Approved, false, 1, &["a.one"])
    );
}

// Occurred

#[test]
fn it_should_pass_occurred_when_the_match_count_reaches_the_minimum() {
    let mut source = FakeSource::new().with_nodes(type_filter("note"), node_views(&["a.one"]));
    let question = Question::Occurred {
        select: type_filter("note"),
        min: 1,
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Occurred, true, 1, &[])
    );
}

#[test]
fn it_should_fail_occurred_below_the_minimum() {
    let mut source = FakeSource::new().with_nodes(type_filter("note"), node_views(&["a.one"]));
    let question = Question::Occurred {
        select: type_filter("note"),
        min: 2,
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::Occurred, false, 1, &[])
    );
}

// NoPending

#[test]
fn it_should_pass_no_pending_when_every_pending_node_has_a_current_resolver() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["p.one"]))
        .with_relations(
            "resolves",
            vec![relation_view("resolves", "r.one", "p.one")],
        );
    let question = Question::NoPending {
        pending: type_filter("note"),
        resolved_by: rty("resolves"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::NoPending, true, 1, &[])
    );
}

#[test]
fn it_should_list_the_unresolved_nodes_when_no_pending_fails() {
    let mut source = FakeSource::new()
        .with_nodes(
            type_filter("note"),
            node_views(&["p.one", "p.two", "p.three"]),
        )
        .with_relations(
            "resolves",
            vec![relation_view("resolves", "r.one", "p.two")],
        );
    let question = Question::NoPending {
        pending: type_filter("note"),
        resolved_by: rty("resolves"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::NoPending, false, 3, &["p.one", "p.three"])
    );
}

#[test]
fn it_should_pass_no_pending_when_nothing_is_pending() {
    let mut source = FakeSource::new().with_nodes(type_filter("note"), Vec::new());
    let question = Question::NoPending {
        pending: type_filter("note"),
        resolved_by: rty("resolves"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::NoPending, true, 0, &[])
    );
}

// NoOpenContradiction

#[test]
fn it_should_pass_no_open_contradiction_without_contradicts_relations() {
    let mut source =
        FakeSource::new().with_nodes(type_filter("note"), node_views(&["a.one", "a.two"]));
    let question = Question::NoOpenContradiction {
        of: Some(type_filter("note")),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::NoOpenContradiction, true, 2, &[])
    );
}

#[test]
fn it_should_fail_no_open_contradiction_when_a_current_contradicts_targets_a_match() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("note"), node_views(&["a.one", "a.two"]))
        .with_relations(
            "contradicts",
            vec![relation_view("contradicts", "c.one", "a.two")],
        );
    let question = Question::NoOpenContradiction {
        of: Some(type_filter("note")),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::NoOpenContradiction, false, 2, &["a.two"])
    );
}

#[test]
fn it_should_default_no_open_contradiction_to_all_current_nodes() {
    let mut source = FakeSource::new()
        .with_nodes(all_filter(), node_views(&["a.one", "a.two", "a.three"]))
        .with_relations(
            "contradicts",
            vec![relation_view("contradicts", "c.one", "a.three")],
        );
    let question = Question::NoOpenContradiction { of: None };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        report,
        expected_report(QuestionKind::NoOpenContradiction, false, 3, &["a.three"])
    );
}

// Corte da evidência

#[test]
fn it_should_cap_each_evidence_list_at_100_ids_and_report_the_omitted_count() {
    let ids: Vec<String> = (0..130).map(|index| format!("p.n{index:03}")).collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let mut source = FakeSource::new().with_nodes(type_filter("note"), node_views(&id_refs));
    let question = Question::NoPending {
        pending: type_filter("note"),
        resolved_by: rty("resolves"),
    };

    let report = evaluate_one(&mut source, question);

    assert_eq!(
        (report.failing.len(), report.omitted),
        (EVIDENCE_CAP, 130 - EVIDENCE_CAP)
    );
}

// Relatório do gate

#[test]
fn it_should_fail_the_gate_when_any_question_fails_and_keep_every_question_report() {
    let passing = expected_report(QuestionKind::Occurred, true, 1, &[]);
    let failing = expected_report(QuestionKind::NoPending, false, 1, &["p.one"]);
    let head = chain_head(7, 'a');

    let report = GateReport::assemble(
        "pr-gate",
        head.clone(),
        vec![passing.clone(), failing.clone()],
    );

    assert_eq!(
        report,
        GateReport {
            gate: "pr-gate".to_owned(),
            passed: false,
            questions: vec![passing, failing],
            chain_head: head,
        }
    );
}

#[test]
fn it_should_not_pass_a_gate_without_questions() {
    let report = GateReport::assemble("pr-gate", chain_head(7, 'a'), Vec::new());

    assert!(!report.passed);
}

// Leitura consistente

#[test]
fn it_should_return_the_head_and_the_value_of_a_consistent_read() {
    let head = chain_head(7, 'a');
    let mut source = FakeSource::new().with_heads(vec![head.clone(), head.clone()]);

    let result = read_consistent(&mut source, &tenant("acme"), |_| Ok(42u32));

    assert_eq!(result, Ok((head, 42)));
}

#[test]
fn it_should_fail_with_gate_inconsistent_when_the_head_moves_during_the_read() {
    let mut source = FakeSource::new().with_heads(vec![chain_head(7, 'a'), chain_head(8, 'b')]);

    let result = read_consistent(&mut source, &tenant("acme"), |_| Ok(42u32));

    assert_eq!(result, Err(OmhlError::rule(RuleCode::GateInconsistent, "")));
}

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
