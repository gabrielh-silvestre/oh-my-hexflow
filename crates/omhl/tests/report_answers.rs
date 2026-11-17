mod support;

use ::omhl::workflow::gap::{Answerer, Edge, Fact, GapFacts, GapState, answered_by, gap_state};
use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::ext::ExtTree;
use support::git::commit_marker;
use support::omhl::decision_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const GAP: &str = "gaps.g1";
const OTHER_GAP: &str = "gaps.g2";
const ANSWER: &str = "premises.p1";
const DIRECTIVE: &str = "guidelines.d1";

// Camada pura: `omhl::workflow::gap::{gap_state, answered_by}`

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

/// O gap `GAP` entra sempre; `nodes` e `edges` são o resto do grafo.
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

// Relatório: `omhl report answers` (CLI) sobre um banco real

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn relation(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": {
        "kind": "relation", "relation_type": relation_type, "from": from, "to": to,
    } })
}

fn gap(id: &str, work_item: &str) -> Value {
    let data = json!({ "question": "q", "context": "c", "work_item": work_item });

    node(id, "gap", data)
}

fn agent_premise_node(id: &str) -> Value {
    let data = json!({ "text": "t", "status": "assumed", "kind": "technical", "work_item": W1 });

    node(id, "premise", data)
}

fn human_premise_node(id: &str) -> Value {
    let data = json!({
        "text": "t", "status": "confirmed", "kind": "business", "work_item": W1,
        "confirmed_by": confirmed_by(),
    });

    node(id, "premise", data)
}

fn directive_node(id: &str) -> Value {
    node(
        id,
        "guideline",
        json!({ "text": "t", "kind": "business", "confirmed_by": confirmed_by() }),
    )
}

fn work_item_nodes() -> Vec<Value> {
    [W1, W2]
        .iter()
        .map(|id| node(id, "work-item", json!({ "title": "t", "status": "open" })))
        .collect()
}

fn in_project(project: &Project, args: &[&str]) -> Output {
    omhl(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn append(project: &Project, items: Vec<Value>) {
    let mut request = json!({});
    request["items"] = Value::Array(items);
    let appended = omhl(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(request.to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

/// Grava os work items `w1` e `w2`, depois os nodes e, por fim, as relations.
fn seed(project: &Project, nodes: Vec<Value>, relations: Vec<Value>) {
    append(project, work_item_nodes());
    append(project, nodes);
    if !relations.is_empty() {
        append(project, relations);
    }
}

fn report_with(project: &Project, extra: &[&str]) -> Value {
    let mut args = vec!["report", "answers"];
    args.extend(extra);
    let output = in_project(project, &args);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stdout.lines().count(), 1, "one JSON line");

    json_line(&output.stdout)
}

fn report(project: &Project) -> Value {
    report_with(project, &[])
}

fn entry(report: &Value, id: &str) -> Value {
    report["gaps"]
        .as_array()
        .expect("gaps is an array")
        .iter()
        .find(|gap| gap["id"] == json!(id))
        .unwrap_or_else(|| panic!("the report has {id}: {report}"))
        .clone()
}

fn gap_ids(report: &Value) -> Vec<String> {
    report["gaps"]
        .as_array()
        .expect("gaps is an array")
        .iter()
        .map(|gap| gap["id"].as_str().expect("the id is text").to_owned())
        .collect()
}

/// Um gap respondido por `ANSWER`; os nodes extras e as relations extras entram no mesmo banco.
fn seed_answered(
    project: &Project,
    premise: Value,
    extra_nodes: Vec<Value>,
    extra_relations: Vec<Value>,
) {
    let mut nodes = vec![gap(GAP, W1), premise];
    nodes.extend(extra_nodes);
    let mut relations = vec![relation("answers", ANSWER, GAP)];
    relations.extend(extra_relations);

    seed(project, nodes, relations);
}

#[test]
fn it_should_report_an_unanswered_gap_as_open() {
    let project = Project::new("acme");
    seed(&project, vec![gap(GAP, W1)], vec![]);

    let report = report(&project);

    assert_eq!(
        entry(&report, GAP),
        json!({ "id": GAP, "work_item": W1, "state": "open" })
    );
}

#[test]
fn it_should_close_a_gap_when_a_premise_answers_it() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![gap(GAP, W1), agent_premise_node(ANSWER)],
        vec![],
    );
    let before = entry(&report(&project), GAP)["state"].clone();

    append(&project, vec![relation("answers", ANSWER, GAP)]);

    let after = entry(&report(&project), GAP)["state"].clone();
    assert_eq!((before, after), (json!("open"), json!("answered")));
}

#[test]
fn it_should_report_the_answering_premise() {
    let project = Project::new("acme");
    seed_answered(&project, agent_premise_node(ANSWER), vec![], vec![]);

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["answered_by"], json!(ANSWER));
}

#[test]
fn it_should_report_a_premise_with_confirmed_by_as_answered_by_a_human() {
    let project = Project::new("acme");
    seed_answered(&project, human_premise_node(ANSWER), vec![], vec![]);

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["answerer"], json!("human"));
}

#[test]
fn it_should_report_a_technical_assumed_premise_as_answered_autonomously() {
    let project = Project::new("acme");
    seed_answered(&project, agent_premise_node(ANSWER), vec![], vec![]);

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["answerer"], json!("autonomous"));
}

#[test]
fn it_should_mark_an_answer_without_a_supporting_directive_as_having_no_criterion() {
    let project = Project::new("acme");
    seed_answered(&project, agent_premise_node(ANSWER), vec![], vec![]);

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["no_criterion"], json!(true));
}

#[test]
fn it_should_not_mark_no_criterion_when_a_confirmed_directive_supports_the_answer() {
    let project = Project::new("acme");
    seed_answered(
        &project,
        agent_premise_node(ANSWER),
        vec![directive_node(DIRECTIVE)],
        vec![relation("supports", DIRECTIVE, ANSWER)],
    );

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["no_criterion"], json!(false));
}

#[test]
fn it_should_not_mark_no_criterion_when_a_loaded_extension_directive_supports_the_answer() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).guideline("tone", "kind = \"business\"\ntext = \"t\"\n");
    let loaded = in_project(&project, &["load"]);
    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    seed_answered(
        &project,
        agent_premise_node(ANSWER),
        vec![],
        vec![relation("supports", "guidelines.tone", ANSWER)],
    );

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["no_criterion"], json!(false));
}

#[test]
fn it_should_report_needs_human_when_the_answer_contradicts_a_confirmed_directive() {
    let project = Project::new("acme");
    seed_answered(
        &project,
        agent_premise_node(ANSWER),
        vec![directive_node(DIRECTIVE)],
        vec![relation("contradicts", ANSWER, DIRECTIVE)],
    );

    let report = report(&project);

    assert_eq!(
        entry(&report, GAP),
        json!({
            "id": GAP, "work_item": W1, "state": "needs-human", "answered_by": ANSWER,
            "author": { "agent": "user", "client": "cli" },
        })
    );
}

#[test]
fn it_should_keep_a_gap_open_when_a_premise_answers_only_another_premise() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            gap(GAP, W1),
            agent_premise_node(ANSWER),
            agent_premise_node("premises.p2"),
        ],
        vec![relation("answers", "premises.p2", ANSWER)],
    );

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["state"], json!("open"));
}

#[test]
fn it_should_keep_a_gap_open_when_only_a_decision_answers_it() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            gap(GAP, W1),
            node("decisions.x1", "decision", decision_data("t", None)),
        ],
        vec![relation("answers", "decisions.x1", GAP)],
    );

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["state"], json!("open"));
}

#[test]
fn it_should_report_the_author_of_the_answering_premise_as_recorded() {
    let project = Project::new("acme");
    seed_answered(&project, agent_premise_node(ANSWER), vec![], vec![]);

    let report = report(&project);

    assert_eq!(
        entry(&report, GAP)["author"],
        json!({ "agent": "user", "client": "cli" })
    );
}

#[test]
fn it_should_report_every_gap_in_creation_order_without_a_filter() {
    let project = Project::new("acme");
    seed(&project, vec![gap(GAP, W1), gap(OTHER_GAP, W2)], vec![]);

    let report = report(&project);

    assert_eq!(gap_ids(&report), [GAP, OTHER_GAP]);
}

#[test]
fn it_should_show_only_the_gaps_of_the_work_item_given_to_the_filter() {
    let project = Project::new("acme");
    seed(&project, vec![gap(GAP, W1), gap(OTHER_GAP, W2)], vec![]);

    let report = report_with(&project, &["--work-item", W2]);

    assert_eq!(gap_ids(&report), [OTHER_GAP]);
}

#[test]
fn it_should_report_the_gaps_of_a_closed_work_item() {
    let project = Project::new("acme");
    seed(&project, vec![gap(GAP, W1)], vec![]);
    commit_marker(project.dir());
    let closed = in_project(&project, &["close", W1]);

    let report = report_with(&project, &["--work-item", W1]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(gap_ids(&report), [GAP]);
}

#[test]
fn it_should_report_no_gaps_for_a_work_item_without_gaps() {
    let project = Project::new("acme");
    seed(&project, vec![gap(GAP, W1)], vec![]);

    let report = report_with(&project, &["--work-item", W2]);

    assert_eq!(report, json!({ "gaps": [] }));
}

#[test]
fn it_should_refuse_a_work_item_that_does_not_exist() {
    let project = Project::new("acme");
    seed(&project, vec![gap(GAP, W1)], vec![]);

    let output = in_project(
        &project,
        &["report", "answers", "--work-item", "work-items.naoexiste"],
    );

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(error["code"], json!("work-item-not-found"));
    assert_eq!(error["details"][0]["path"], json!("/work_item"));
}

#[test]
fn it_should_refuse_a_malformed_work_item_at_the_work_item_pointer() {
    let project = Project::new("acme");

    let output = in_project(&project, &["report", "answers", "--work-item", "Bad_Id"]);

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(
        json_line(&output.stderr)["error"]["details"][0]["path"],
        json!("/work_item")
    );
}
