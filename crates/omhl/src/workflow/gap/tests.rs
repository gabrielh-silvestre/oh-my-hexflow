use crate::test_support::omhl::decision_data;
use crate::workflow::gap::{
    Answerer, Edge, Fact, GapFacts, GapState, answered_by, gap_state, is_confirmed_premise,
};
use serde_json::{Value, json};

#[test]
fn it_should_confirm_a_premise_only_with_confirmed_status_and_confirmed_by() {
    let by = json!({"by": "person:ana"});
    let confirmed = json!({"status": "confirmed", "confirmed_by": by});
    let legacy = json!({"status": "confirmed"});
    let assumed_with_confirmation = json!({"status": "assumed", "confirmed_by": by});

    assert!(is_confirmed_premise(&confirmed));
    assert!(!is_confirmed_premise(&legacy));
    assert!(!is_confirmed_premise(&assumed_with_confirmation));
}

const W1: &str = "work-items.w1";
const GAP: &str = "gaps.g1";
const OTHER_GAP: &str = "gaps.g2";
const ANSWER: &str = "premises.p1";
const DIRECTIVE: &str = "guidelines.d1";

// Camada pura: `crate::workflow::gap::{gap_state, answered_by}`

fn fact(id: &str, type_name: &str, native: bool, data: Value) -> Fact {
    Fact {
        id: id.to_owned(),
        type_name: type_name.to_owned(),
        native,
        loader: false,
        data,
    }
}

fn confirmed_by() -> Value {
    json!({ "by": "person:ana", "question": "q", "answer": "a" })
}

fn human_premise(id: &str) -> Fact {
    let data = json!({
        "text": "t", "status": "confirmed", "kind": "business", "work_item": W1,
        "confirmed_by": confirmed_by(),
    });

    fact(id, "premise", false, data)
}

/// Premissa técnica que o critic confirma: `confirmed_by.by` é `critic`, não uma pessoa.
fn critic_premise(id: &str) -> Fact {
    let data = json!({
        "text": "t", "status": "confirmed", "kind": "technical", "work_item": W1,
        "confirmed_by": { "by": "critic", "question": "q", "answer": "a" },
    });

    fact(id, "premise", false, data)
}

fn agent_premise(id: &str) -> Fact {
    let data = json!({ "text": "t", "status": "assumed", "kind": "technical", "work_item": W1 });

    fact(id, "premise", false, data)
}

fn legacy_confirmed_premise(id: &str) -> Fact {
    fact(
        id,
        "premise",
        false,
        json!({ "text": "t", "status": "confirmed" }),
    )
}

fn confirmed_directive(id: &str) -> Fact {
    fact(
        id,
        "guideline",
        false,
        json!({ "text": "t", "confirmed_by": confirmed_by() }),
    )
}

fn unconfirmed_directive(id: &str) -> Fact {
    fact(id, "guideline", false, json!({ "text": "t" }))
}

fn native_directive(id: &str) -> Fact {
    fact(id, "guideline", true, Value::Null)
}

/// Diretiva de extension: o `loader` a grava sem `confirmed_by`.
fn loader_directive(id: &str) -> Fact {
    Fact {
        loader: true,
        ..unconfirmed_directive(id)
    }
}

fn gap_fact(id: &str) -> Fact {
    fact(id, "gap", false, json!({ "question": "q", "context": "c" }))
}

fn decision_fact(id: &str) -> Fact {
    fact(id, "decision", false, decision_data("t", None))
}

fn edge(relation: &str, from: &str, to: &str) -> Edge {
    Edge {
        relation: relation.to_owned(),
        from: from.to_owned(),
        to: to.to_owned(),
    }
}

fn facts(mut nodes: Vec<Fact>, edges: Vec<Edge>) -> GapFacts {
    nodes.push(gap_fact(GAP));

    GapFacts { nodes, edges }
}

fn state_of(nodes: Vec<Fact>, edges: Vec<Edge>) -> GapState {
    gap_state(&facts(nodes, edges), GAP)
}

fn answered(by: Answerer, criterion: bool) -> GapState {
    GapState::Answered { by, criterion }
}

#[test]
fn it_should_leave_a_gap_open_when_nothing_answers_it() {
    let state = state_of(vec![], vec![]);

    assert_eq!(state, GapState::Open);
}

#[test]
fn it_should_attribute_to_a_human_a_premise_with_confirmed_by() {
    let nodes = vec![human_premise(ANSWER)];
    let edges = vec![edge("answers", ANSWER, GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Human, false));
}

#[test]
fn it_should_attribute_to_an_agent_a_technical_assumed_premise() {
    let nodes = vec![agent_premise(ANSWER)];
    let edges = vec![edge("answers", ANSWER, GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_attribute_to_an_agent_a_legacy_confirmed_premise_without_confirmed_by() {
    let nodes = vec![legacy_confirmed_premise(ANSWER)];
    let edges = vec![edge("answers", ANSWER, GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_attribute_to_an_agent_an_assumed_premise_that_carries_confirmed_by() {
    let data = json!({
        "text": "t", "status": "assumed", "kind": "technical", "work_item": W1,
        "confirmed_by": confirmed_by(),
    });
    let nodes = vec![fact(ANSWER, "premise", false, data)];
    let edges = vec![edge("answers", ANSWER, GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_have_no_criterion_when_no_directive_supports_the_answer() {
    let nodes = vec![agent_premise(ANSWER), confirmed_directive(DIRECTIVE)];
    let edges = vec![edge("answers", ANSWER, GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_have_criterion_when_a_confirmed_directive_supports_the_answer() {
    let nodes = vec![agent_premise(ANSWER), confirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("supports", DIRECTIVE, ANSWER),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, true));
}

#[test]
fn it_should_have_criterion_when_a_native_directive_supports_the_answer() {
    let nodes = vec![agent_premise(ANSWER), native_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("supports", DIRECTIVE, ANSWER),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, true));
}

#[test]
fn it_should_have_criterion_when_a_loader_directive_supports_the_answer() {
    let nodes = vec![agent_premise(ANSWER), loader_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("supports", DIRECTIVE, ANSWER),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, true));
}

#[test]
fn it_should_have_no_criterion_when_the_supporting_directive_has_no_confirmed_by() {
    let nodes = vec![agent_premise(ANSWER), unconfirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("supports", DIRECTIVE, ANSWER),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_have_no_criterion_when_the_directive_supports_another_premise() {
    let nodes = vec![
        agent_premise(ANSWER),
        agent_premise("premises.p2"),
        confirmed_directive(DIRECTIVE),
    ];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("supports", DIRECTIVE, "premises.p2"),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_need_a_human_when_the_answer_contradicts_a_confirmed_directive_without_confirmed_by() {
    let nodes = vec![agent_premise(ANSWER), confirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::NeedsHuman);
}

#[test]
fn it_should_need_a_human_when_the_answer_contradicts_a_native_directive() {
    let nodes = vec![agent_premise(ANSWER), native_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::NeedsHuman);
}

#[test]
fn it_should_need_a_human_when_the_answer_contradicts_a_loader_directive() {
    let nodes = vec![agent_premise(ANSWER), loader_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::NeedsHuman);
}

#[test]
fn it_should_accept_a_contradicting_answer_that_has_confirmed_by() {
    let nodes = vec![human_premise(ANSWER), confirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Human, false));
}

#[test]
fn it_should_not_need_a_human_when_the_contradicted_directive_has_no_confirmed_by() {
    let nodes = vec![agent_premise(ANSWER), unconfirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Autonomous, false));
}

#[test]
fn it_should_need_a_human_when_a_critic_confirmed_answer_contradicts_a_confirmed_directive() {
    let nodes = vec![critic_premise(ANSWER), confirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::NeedsHuman);
}

#[test]
fn it_should_take_a_later_person_confirmed_answer_over_an_earlier_critic_answer_that_needs_the_human()
 {
    let nodes = vec![
        critic_premise("premises.p2"),
        human_premise(ANSWER),
        confirmed_directive(DIRECTIVE),
    ];
    let edges = vec![
        edge("answers", "premises.p2", GAP),
        edge("contradicts", "premises.p2", DIRECTIVE),
        edge("answers", ANSWER, GAP),
    ];
    let facts = facts(nodes, edges);

    let taken = (gap_state(&facts, GAP), answered_by(&facts, GAP));

    assert_eq!(
        taken,
        (answered(Answerer::Human, false), Some(ANSWER.to_owned()))
    );
}

#[test]
fn it_should_keep_a_gap_open_when_a_premise_answers_another_premise() {
    let nodes = vec![agent_premise(ANSWER), agent_premise("premises.p2")];
    let edges = vec![edge("answers", "premises.p2", ANSWER)];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::Open);
}

#[test]
fn it_should_keep_a_gap_open_when_a_decision_answers_it() {
    let nodes = vec![decision_fact("decisions.x1")];
    let edges = vec![edge("answers", "decisions.x1", GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::Open);
}

#[test]
fn it_should_keep_a_gap_open_when_the_premise_answers_another_gap() {
    let nodes = vec![agent_premise(ANSWER), gap_fact(OTHER_GAP)];
    let edges = vec![edge("answers", ANSWER, OTHER_GAP)];

    let state = state_of(nodes, edges);

    assert_eq!(state, GapState::Open);
}

#[test]
fn it_should_prefer_the_human_answer_when_a_gap_has_two_answers() {
    let nodes = vec![agent_premise("premises.p2"), human_premise(ANSWER)];
    let edges = vec![
        edge("answers", "premises.p2", GAP),
        edge("answers", ANSWER, GAP),
    ];

    let state = state_of(nodes, edges);

    assert_eq!(state, answered(Answerer::Human, false));
}

#[test]
fn it_should_name_the_answering_premise() {
    let nodes = vec![agent_premise(ANSWER)];
    let edges = vec![edge("answers", ANSWER, GAP)];

    let premise = answered_by(&facts(nodes, edges), GAP);

    assert_eq!(premise.as_deref(), Some(ANSWER));
}

#[test]
fn it_should_name_the_answering_premise_of_a_gap_that_needs_a_human() {
    let nodes = vec![agent_premise(ANSWER), confirmed_directive(DIRECTIVE)];
    let edges = vec![
        edge("answers", ANSWER, GAP),
        edge("contradicts", ANSWER, DIRECTIVE),
    ];

    let premise = answered_by(&facts(nodes, edges), GAP);

    assert_eq!(premise.as_deref(), Some(ANSWER));
}

#[test]
fn it_should_name_the_human_premise_when_a_gap_has_two_answers() {
    let nodes = vec![agent_premise("premises.p2"), human_premise(ANSWER)];
    let edges = vec![
        edge("answers", "premises.p2", GAP),
        edge("answers", ANSWER, GAP),
    ];

    let premise = answered_by(&facts(nodes, edges), GAP);

    assert_eq!(premise.as_deref(), Some(ANSWER));
}

#[test]
fn it_should_name_no_premise_when_only_a_decision_answers_the_gap() {
    let nodes = vec![decision_fact("decisions.x1")];
    let edges = vec![edge("answers", "decisions.x1", GAP)];

    let premise = answered_by(&facts(nodes, edges), GAP);

    assert_eq!(premise, None);
}
