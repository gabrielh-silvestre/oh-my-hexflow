use crate::error::{OmhlError, RuleCode};
use crate::gate::{
    EVIDENCE_CAP, GateReport, Predicate, Question, QuestionKind, TargetCheck, TargetFact,
    read_consistent,
};
use crate::test_support::gate::{
    FakeSource, all_filter, chain_head, evaluate_one, expected_report, node_view_with, node_views,
    relation_view, type_filter,
};
use crate::test_support::{rty, tenant};
use hexlog::{EndpointStatus, NodeView, Origin, RelationView};
use serde_json::json;

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

// Anchored

/// Predicado de teste: o alvo está confirmado quando o `data` traz `flagged: true`.
fn is_flagged(fact: &TargetFact<'_>) -> bool {
    fact.data["flagged"] == true
}

fn is_native(fact: &TargetFact<'_>) -> bool {
    fact.native
}

fn anchored(target: Option<TargetCheck>) -> Question {
    Question::Anchored {
        select: type_filter("decision"),
        by: rty("rests-on"),
        target,
    }
}

fn flagged_anchors() -> Option<TargetCheck> {
    Some(TargetCheck {
        scope: vec![type_filter("anchor")],
        confirmed: Predicate(is_flagged),
    })
}

fn anchor_view(id: &str, flagged: bool) -> NodeView {
    node_view_with(id, json!({ "flagged": flagged }))
}

#[test]
fn it_should_fail_an_unanchored_node() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("decision"), node_views(&["d.one", "d.two"]))
        .with_relations(
            "rests-on",
            vec![relation_view("rests-on", "d.one", "t.one")],
        );

    let report = evaluate_one(&mut source, anchored(None));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, false, 2, &["d.two"])
    );
}

#[test]
fn it_should_fail_a_node_resting_on_an_unconfirmed_target() {
    let mut source = FakeSource::new()
        .with_nodes(
            type_filter("decision"),
            node_views(&["d.one", "d.two", "d.three"]),
        )
        .with_nodes(
            type_filter("anchor"),
            vec![anchor_view("t.good", true), anchor_view("t.bad", false)],
        )
        .with_relations(
            "rests-on",
            vec![
                relation_view("rests-on", "d.one", "t.good"),
                relation_view("rests-on", "d.two", "t.bad"),
            ],
        );

    let report = evaluate_one(&mut source, anchored(flagged_anchors()));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, false, 3, &["d.two"])
    );
}

#[test]
fn it_should_pass_when_nothing_matches() {
    let mut source = FakeSource::new().with_nodes(type_filter("decision"), Vec::new());

    let report = evaluate_one(&mut source, anchored(None));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, true, 0, &[])
    );
}

#[test]
fn it_should_apply_the_predicate_to_every_current_target() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("decision"), node_views(&["d.one", "d.two"]))
        .with_nodes(
            type_filter("anchor"),
            vec![anchor_view("t.good", true), anchor_view("t.bad", false)],
        )
        .with_relations(
            "rests-on",
            vec![
                relation_view("rests-on", "d.one", "t.good"),
                relation_view("rests-on", "d.one", "t.bad"),
                relation_view("rests-on", "d.two", "t.good"),
            ],
        );

    let report = evaluate_one(&mut source, anchored(flagged_anchors()));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, false, 2, &["d.one"])
    );
}

#[test]
fn it_should_not_count_a_retired_target_as_an_anchor() {
    let retired_target = RelationView {
        to_status: EndpointStatus::Retired,
        ..relation_view("rests-on", "d.one", "t.one")
    };
    let mut source = FakeSource::new()
        .with_nodes(type_filter("decision"), node_views(&["d.one"]))
        .with_relations("rests-on", vec![retired_target]);

    let report = evaluate_one(&mut source, anchored(None));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, false, 1, &["d.one"])
    );
}

#[test]
fn it_should_fail_a_node_resting_on_a_target_outside_the_scope() {
    let mut source = FakeSource::new()
        .with_nodes(type_filter("decision"), node_views(&["d.one"]))
        .with_nodes(type_filter("other"), vec![anchor_view("t.outside", true)])
        .with_relations(
            "rests-on",
            vec![relation_view("rests-on", "d.one", "t.outside")],
        );

    let report = evaluate_one(&mut source, anchored(flagged_anchors()));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, false, 1, &["d.one"])
    );
}

#[test]
fn it_should_hand_a_native_target_to_the_predicate_as_native() {
    let native_target = RelationView {
        to_origin: Origin::Native,
        ..relation_view("rests-on", "d.one", "t.native")
    };
    let mut source = FakeSource::new()
        .with_nodes(type_filter("decision"), node_views(&["d.one"]))
        .with_relations("rests-on", vec![native_target]);
    let target = Some(TargetCheck {
        scope: vec![type_filter("anchor")],
        confirmed: Predicate(is_native),
    });

    let report = evaluate_one(&mut source, anchored(target));

    assert_eq!(
        report,
        expected_report(QuestionKind::Anchored, true, 1, &[])
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
