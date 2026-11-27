use crate::support;

use std::fs;

use omhl::ports::git::GitProbe;
use serde_json::{Value, json};
use support::appfx::AppFixture;
use support::git::GitRepo;
use support::omhl::work_item_data;

const W1: &str = "work-items.w1";
const PR7: &str = "work-items.pr-7";
const WORKFLOW: &str = ".omhl/workflow.toml";
const BUSINESS: &str = "requires:business-premise";
const SPEC: &str = "requires:spec";
const TECHNICAL: &str = "requires:technical-premise-confirmed";

/// Repositório com `develop` checado out e um commit, como a `develop` de um projeto.
fn repo_on_develop() -> GitRepo {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    repo.commit_all("initial");
    repo.git(&["checkout", "-q", "-b", "develop"]);

    repo
}

fn fixture_for(repo: &GitRepo) -> AppFixture {
    AppFixture::new()
        .with_project_root(repo.path())
        .with_native(GitProbe::new(repo.path()))
}

fn node(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn work_item_with_type(title: &str, type_value: &str) -> Value {
    let mut data = work_item_data(title);
    data["type"] = json!(type_value);

    data
}

/// Abre o nó e devolve o id do registro gravado, que a próxima revisão aponta como `base`.
fn open(fixture: &AppFixture, id: &str, data: &Value) -> String {
    let response = fixture
        .append_as_user(json!({ "items": [node(id, "work-item", data)] }))
        .expect("the work item is opened");

    response["records"][0]["id"]
        .as_str()
        .expect("the response carries a record id")
        .to_owned()
}

fn revise(fixture: &AppFixture, base: &str, id: &str, data: &Value) -> Result<Value, Value> {
    let mut item = node(id, "work-item", data);
    item["base"] = json!(base);

    fixture.append_as_user(json!({ "items": [item] }))
}

fn labels(report: &Value) -> Vec<String> {
    report["questions"]
        .as_array()
        .expect("the report lists its questions")
        .iter()
        .map(|question| question["label"].as_str().expect("a label").to_owned())
        .collect()
}

fn question<'r>(report: &'r Value, label: &str) -> &'r Value {
    report["questions"]
        .as_array()
        .expect("the report lists its questions")
        .iter()
        .find(|question| question["label"] == json!(label))
        .unwrap_or_else(|| panic!("the report asks {label}"))
}

fn code_of(refusal: &Value) -> &str {
    refusal["error"]["code"].as_str().expect("a refusal code")
}

fn type_of(fixture: &AppFixture, id: &str) -> Value {
    fixture.get(id)["data"]["type"].clone()
}

// CA-003: o tipo do work item e do pull-request

#[test]
fn it_should_take_the_recorded_type_over_the_branch_prefix() {
    let repo = repo_on_develop();
    repo.git(&["branch", "feat/wi-w1__s"]);
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "refactor"));

    let gate = fixture.gate_pr(W1);

    assert!(labels(&gate).contains(&TECHNICAL.to_owned()));
    assert!(!labels(&gate).contains(&BUSINESS.to_owned()));
}

#[test]
fn it_should_take_the_only_branch_prefix_when_no_type_is_recorded() {
    let repo = repo_on_develop();
    repo.git(&["branch", "feat/wi-w1__s"]);
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_data("Ship it"));

    let gate = fixture.gate_pr(W1);

    assert!(labels(&gate).contains(&BUSINESS.to_owned()));
}

#[test]
fn it_should_take_the_branch_prefix_of_a_pull_request_without_a_type() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let pull_request = json!({
        "title": "Ship it",
        "status": "open",
        "provider": "github",
        "repo": "acme/oh-my-hexflow",
        "number": 7,
        "branch": "feat/wi-w1__s",
    });
    fixture
        .append_as_user(json!({ "items": [node(PR7, "pull-request", &pull_request)] }))
        .expect("the pull request is opened");

    let gate = fixture.gate_pr(PR7);

    assert!(labels(&gate).contains(&BUSINESS.to_owned()));
}

#[test]
fn it_should_refuse_a_work_item_with_two_branch_prefixes_and_no_type() {
    let repo = repo_on_develop();
    repo.git(&["branch", "feat/wi-w1__s"]);
    repo.git(&["branch", "chore/wi-w1__t"]);
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_data("Ship it"));

    let refusal = fixture.try_gate_pr(W1).expect_err("the type is unknown");

    assert_eq!(code_of(&refusal), "work-item-type-unknown");
}

#[test]
fn it_should_refuse_a_recorded_type_outside_the_table_and_record_nothing_on_close() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "xyz"));

    let refusal = fixture.close(W1).expect_err("the type is unknown");

    assert_eq!(code_of(&refusal), "work-item-type-unknown");
    assert_eq!(fixture.get(W1)["data"]["status"], json!("open"));
}

#[test]
fn it_should_refuse_a_work_item_with_no_type_and_no_branch() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_data("Ship it"));

    let refusal = fixture.try_gate_pr(W1).expect_err("the type is unknown");

    assert_eq!(code_of(&refusal), "work-item-type-unknown");
}

// CA-004: cada átomo do tipo vira uma pergunta `requires:<átomo>`

#[test]
fn it_should_ask_a_requires_question_that_fails_for_each_unmet_atom_of_the_type() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "feat"));

    let gate = fixture.gate_pr(W1);

    assert_eq!(question(&gate, BUSINESS)["passed"], json!(false));
    assert_eq!(question(&gate, SPEC)["passed"], json!(false));
}

#[test]
fn it_should_ask_no_requires_question_for_a_type_with_no_atom() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "chore"));

    let gate = fixture.gate_pr(W1);

    assert!(
        labels(&gate)
            .iter()
            .all(|label| !label.starts_with("requires:"))
    );
}

fn business_premise(origin: Option<&str>) -> Value {
    let mut data = json!({
        "text": "Owners want it",
        "status": "confirmed",
        "kind": "business",
        "work_item": W1,
        "confirmed_by": { "by": "person:tester", "question": "Want it?", "answer": "Yes." },
    });
    if let Some(origin) = origin {
        data["origin"] = json!(origin);
    }

    data
}

#[test]
fn it_should_satisfy_the_business_premise_atom_with_a_confirmed_main_premise() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "feat"));
    fixture
        .append_as_user(
            json!({ "items": [node("premises.p1", "premise", &business_premise(None))] }),
        )
        .expect("the premise is recorded");

    let gate = fixture.gate_pr(W1);

    assert_eq!(question(&gate, BUSINESS)["passed"], json!(true));
}

#[test]
fn it_should_not_satisfy_the_business_premise_atom_with_a_late_premise() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "feat"));
    let late = business_premise(Some("late"));
    fixture
        .append_as_user(json!({ "items": [node("premises.p1", "premise", &late)] }))
        .expect("the premise is recorded");

    let gate = fixture.gate_pr(W1);

    assert_eq!(question(&gate, BUSINESS)["passed"], json!(false));
}

// CA-016: a leitura de tipo e matriz usa o `workflow.toml` do diretório principal

#[test]
fn it_should_read_the_main_directory_file_for_the_matrix_of_a_type() {
    let repo = repo_on_develop();
    fs::create_dir_all(repo.path().join(".omhl")).expect("the directory is created");
    fs::write(
        repo.path().join(WORKFLOW),
        "version = 1\n\n[prefix.chore]\nrequire = [\"spec\"]\n",
    )
    .expect("the file is written");
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "chore"));

    let gate = fixture.gate_pr(W1);

    assert!(labels(&gate).contains(&SPEC.to_owned()));
}

#[test]
fn it_should_refuse_the_close_with_invalid_workflow_config_when_the_main_file_is_unreadable() {
    let repo = repo_on_develop();
    fs::create_dir_all(repo.path().join(".omhl")).expect("the directory is created");
    fs::write(repo.path().join(WORKFLOW), "version = [\n").expect("the file is written");
    let fixture = fixture_for(&repo);
    open(&fixture, W1, &work_item_with_type("Ship it", "chore"));

    let refusal = fixture.close(W1).expect_err("the file is unreadable");

    assert_eq!(code_of(&refusal), "invalid-workflow-config");
    assert_eq!(fixture.get(W1)["data"]["status"], json!("open"));
}

#[test]
fn it_should_refuse_the_close_with_main_dir_unknown_before_a_missing_work_item_with_and_without_force()
 {
    let fixture = AppFixture::new().without_native();

    let plain = fixture
        .close("work-items.ghost")
        .expect_err("the main directory is unknown");
    let forced = fixture
        .close_forced("work-items.ghost", "owner approved the override")
        .expect_err("the main directory is unknown");

    assert_eq!(
        (code_of(&plain), code_of(&forced)),
        ("main-dir-unknown", "main-dir-unknown")
    );
}

// CA-005: a troca do tipo recusada na gravação

#[test]
fn it_should_refuse_a_change_from_feat_to_chore_and_keep_the_recorded_type() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_with_type("Ship it", "feat"));

    let refusal = revise(
        &fixture,
        &base,
        W1,
        &work_item_with_type("Ship it", "chore"),
    )
    .expect_err("the change loosens the matrix");

    assert_eq!(code_of(&refusal), "type-change-loosens");
    assert_eq!(type_of(&fixture, W1), json!("feat"));
}

#[test]
fn it_should_allow_a_change_from_chore_to_feat_and_from_feat_to_fix() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_with_type("Ship it", "chore"));

    let base = revise(&fixture, &base, W1, &work_item_with_type("Ship it", "feat"))
        .map(|response| {
            response["records"][0]["id"]
                .as_str()
                .expect("an id")
                .to_owned()
        })
        .expect("chore to feat is allowed");
    revise(&fixture, &base, W1, &work_item_with_type("Ship it", "fix"))
        .expect("feat to fix is allowed");
}

#[test]
fn it_should_refuse_the_removal_of_a_recorded_type_that_requires_something() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_with_type("Ship it", "feat"));

    let refusal = revise(&fixture, &base, W1, &work_item_data("Ship it"))
        .expect_err("removing the type loosens the matrix");

    assert_eq!(code_of(&refusal), "type-change-loosens");
}

#[test]
fn it_should_refuse_a_change_from_refactor_to_bugfix() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_with_type("Ship it", "refactor"));

    let refusal = revise(
        &fixture,
        &base,
        W1,
        &work_item_with_type("Ship it", "bugfix"),
    )
    .expect_err("the technical premise is dropped");

    assert_eq!(code_of(&refusal), "type-change-loosens");
}

#[test]
fn it_should_allow_the_first_type_of_a_work_item_with_no_type_and_no_branch() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_data("Ship it"));

    revise(&fixture, &base, W1, &work_item_with_type("Ship it", "feat"))
        .expect("the first type is free");
}

#[test]
fn it_should_refuse_a_type_set_over_a_branch_prefix_that_requires_more() {
    let repo = repo_on_develop();
    repo.git(&["branch", "feat/wi-w1__s"]);
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_data("Ship it"));

    let refusal = revise(
        &fixture,
        &base,
        W1,
        &work_item_with_type("Ship it", "chore"),
    )
    .expect_err("the branch says feat");

    assert_eq!(code_of(&refusal), "type-change-loosens");
}

#[test]
fn it_should_allow_a_change_away_from_a_recorded_type_outside_the_table() {
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_with_type("Ship it", "release"));

    revise(
        &fixture,
        &base,
        W1,
        &work_item_with_type("Ship it", "chore"),
    )
    .expect("the old type is not compared");
}

#[test]
fn it_should_refuse_the_type_change_with_invalid_workflow_config_when_the_main_file_is_unreadable()
{
    let repo = repo_on_develop();
    let fixture = fixture_for(&repo);
    let base = open(&fixture, W1, &work_item_with_type("Ship it", "feat"));
    fs::create_dir_all(repo.path().join(".omhl")).expect("the directory is created");
    fs::write(repo.path().join(WORKFLOW), "version = [\n").expect("the file is written");

    let refusal = revise(
        &fixture,
        &base,
        W1,
        &work_item_with_type("Ship it", "chore"),
    )
    .expect_err("the file is unreadable");

    assert_eq!(code_of(&refusal), "invalid-workflow-config");
}
