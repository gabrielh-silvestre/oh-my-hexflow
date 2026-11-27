use crate::support;

use hexlog::{FieldName, ListFilter, Scalar};
use omhl::gate::{GateQuestion, Question, QuestionClass};
use support::omhl::{
    LINT_DOCS, OmhlHarness, acme, append_ok, check_data, decision_data, node_item, premise_data,
};
use support::pr_gate::{DECISION, PREMISE, relation_item};
use support::{FakeProbe, rty, sid, tname};

fn of_type(type_name: &str) -> ListFilter {
    ListFilter {
        type_name: Some(tname(type_name)),
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    }
}

#[test]
fn it_should_register_a_project_check_and_read_it_back_by_semantic_id() {
    let harness = OmhlHarness::new(FakeProbe::dirty());
    let mut omhl = harness.open();
    let data = check_data("before-commit", "run the docs linter", true);
    append_ok(&mut omhl, vec![node_item(LINT_DOCS, "check", data.clone())]);

    let view = omhl
        .get(&acme(), &sid(LINT_DOCS))
        .expect("the read succeeds")
        .expect("the check is found");

    assert_eq!(view.data, data);
    assert!(!view.shadowed);
}

#[test]
fn it_should_run_a_decision_flow_with_approved_and_no_pending_questions() {
    let harness = OmhlHarness::new(FakeProbe::dirty());
    let mut omhl = harness.open();
    append_ok(
        &mut omhl,
        vec![
            node_item(PREMISE, "premise", premise_data("SQLite is fast", None)),
            node_item(DECISION, "decision", decision_data("Use SQLite", None)),
        ],
    );
    append_ok(
        &mut omhl,
        vec![
            relation_item("supports", PREMISE, DECISION),
            relation_item("answers", DECISION, PREMISE),
        ],
    );
    let questions = vec![
        GateQuestion::new(
            "approved",
            Question::Approved {
                of: of_type("decision"),
            },
            QuestionClass::Warn,
        ),
        GateQuestion::new(
            "no-pending",
            Question::NoPending {
                pending: ListFilter {
                    where_eq: vec![(
                        FieldName::parse("status").expect("valid field name"),
                        Scalar::Str("assumed".to_owned()),
                    )],
                    ..of_type("premise")
                },
                resolved_by: rty("answers"),
            },
            QuestionClass::Warn,
        ),
    ];

    let report = omhl
        .evaluate(&acme(), "decision-flow", &questions)
        .expect("the gate is evaluated");

    let passed: Vec<bool> = report.questions.iter().map(|q| q.passed).collect();
    assert_eq!(passed, vec![true, true]);
}
