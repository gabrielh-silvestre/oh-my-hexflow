mod support;

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

fn confirmed_by() -> Value {
    json!({ "by": "person:ana", "question": "q", "answer": "a" })
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

fn technical_gap(id: &str, work_item: &str) -> Value {
    let data =
        json!({ "question": "q", "context": "c", "work_item": work_item, "kind": "technical" });

    node(id, "gap", data)
}

fn critic_premise_node(id: &str) -> Value {
    let data = json!({
        "text": "t", "status": "confirmed", "kind": "technical", "work_item": W1,
        "confirmed_by": { "by": "critic", "question": "q", "answer": "a" },
    });

    node(id, "premise", data)
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
    append_with(project, items, omhl(&["append"]));
}

/// `append` sob o Claude Code com `OMHL_ROLE=omhl:critic`: o autor gravado é o critic.
fn append_as_critic(project: &Project, items: Vec<Value>) {
    let critic = omhl(&["append"])
        .env("CLAUDECODE", "1")
        .env("OMHL_ROLE", "omhl:critic");

    append_with(project, items, critic);
}

fn append_with(project: &Project, items: Vec<Value>, command: support::child::Cmd) {
    let mut request = json!({});
    request["items"] = Value::Array(items);
    let appended = command
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

/// Gap técnico `GAP` respondido pelo critic com `ANSWER`; `directives` entram antes, como o dono os gravou,
/// e `contradicting` lista os destinos que a premissa do critic contradiz.
fn seed_answered_by_critic(project: &Project, directives: Vec<Value>, contradicting: &[&str]) {
    let mut nodes = vec![technical_gap(GAP, W1)];
    nodes.extend(directives);
    seed(project, nodes, vec![]);
    append_as_critic(project, vec![critic_premise_node(ANSWER)]);
    let mut relations = vec![relation("answers", ANSWER, GAP)];
    relations.extend(
        contradicting
            .iter()
            .map(|target| relation("contradicts", ANSWER, target)),
    );
    append_as_critic(project, relations);
}

#[test]
fn it_should_report_a_critic_confirmed_answer_as_critic_in_report_answers() {
    let project = Project::new("acme");
    seed_answered_by_critic(&project, vec![], &[]);

    let report = report(&project);

    assert_eq!(entry(&report, GAP)["answerer"], json!("critic"));
}

#[test]
fn it_should_report_needs_human_when_a_critic_confirmed_answer_contradicts_a_confirmed_directive() {
    let project = Project::new("acme");
    seed_answered_by_critic(&project, vec![directive_node(DIRECTIVE)], &[DIRECTIVE]);

    let report = report(&project);

    assert_eq!(
        entry(&report, GAP),
        json!({
            "id": GAP, "work_item": W1, "state": "needs-human", "answered_by": ANSWER,
            "author": { "agent": "omhl:critic", "client": "cli" },
        })
    );
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
