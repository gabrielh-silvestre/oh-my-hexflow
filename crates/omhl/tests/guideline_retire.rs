mod support;

use std::fs;

use hexlog::{Author, Lineage};
use serde_json::{Value, json};
use support::appfx::{AppFixture, RETIRE_SLUG_MARK};
use support::child::{Cmd, omhl};
use support::ext::ExtTree;
use support::omhl::{acme, decision_data, work_item_data};
use support::project::Project;
use support::sid;

const TONE: &str = "guidelines.tone";
const BRIEF: &str = "guidelines.brief";
const OLD: &str = "guidelines.old";
const MID: &str = "guidelines.mid";
const NEW: &str = "guidelines.new";
const GHOST: &str = "guidelines.ghost";
const NATIVE_GUIDELINE: &str = "guidelines.commit-policy";
const W1: &str = "work-items.w1";
const DECISION: &str = "decisions.d1";
const DECISIONS_ANCHORED: &str = "decisions-anchored";

// Apoio

fn refused(result: Result<Value, Value>) -> Value {
    result.expect_err("the retirement is refused")
}

fn code_and_pointer(error: &Value) -> (Value, Value) {
    (
        error["error"]["code"].clone(),
        error["error"]["details"][0]["path"].clone(),
    )
}

fn total_records(fx: &AppFixture) -> u64 {
    fx.verify()["total_records"]
        .as_u64()
        .expect("verify counts the records")
}

fn context_has(fx: &AppFixture, id: &str) -> bool {
    fx.render()["text"]
        .as_str()
        .expect("the render has a text")
        .contains(id)
}

fn question_passed(gate: &Value, label: &str) -> bool {
    gate["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .find(|question| question["label"] == label)
        .unwrap_or_else(|| panic!("the gate asks {label}: {gate}"))["passed"]
        .as_bool()
        .expect("the question has a verdict")
}

fn node_wire(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

// Recusas de pré-condição

#[test]
fn it_should_refuse_while_the_toml_file_exists() {
    let fx = AppFixture::new();
    fx.write_guideline_file("tone");
    fx.load(false).expect("the guideline loads");

    let error = refused(fx.guideline_retire(TONE, None));

    assert_eq!(
        code_and_pointer(&error),
        (json!("guideline-not-orphaned"), json!("/id"))
    );
}

#[test]
fn it_should_keep_the_guideline_current_when_the_toml_file_exists() {
    let fx = AppFixture::new();
    fx.write_guideline_file("tone");
    fx.load(false).expect("the guideline loads");
    let before = total_records(&fx);

    let _ = fx.guideline_retire(TONE, None);

    assert_eq!(fx.get(TONE)["id"], json!(TONE));
    assert_eq!(total_records(&fx), before);
}

#[test]
fn it_should_refuse_a_native_guideline_as_not_found() {
    let fx = AppFixture::new();

    let error = refused(fx.guideline_retire(NATIVE_GUIDELINE, None));

    assert_eq!(
        code_and_pointer(&error),
        (json!("guideline-not-found"), json!("/id"))
    );
}

#[test]
fn it_should_refuse_a_nonexistent_guideline_as_not_found() {
    let fx = AppFixture::new();

    let error = refused(fx.guideline_retire(GHOST, None));

    assert_eq!(
        code_and_pointer(&error),
        (json!("guideline-not-found"), json!("/id"))
    );
}

#[test]
fn it_should_refuse_an_already_retired_guideline_as_not_found() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    fx.guideline_retire(TONE, None)
        .expect("the guideline is retired");

    let error = refused(fx.guideline_retire(TONE, None));

    assert_eq!(
        code_and_pointer(&error),
        (json!("guideline-not-found"), json!("/id"))
    );
}

#[test]
fn it_should_refuse_a_node_that_is_not_a_guideline_as_not_found() {
    let fx = AppFixture::new();
    let decision = node_wire(DECISION, "decision", &decision_data("use sqlite", None));
    fx.append_as_user(json!({ "items": [decision] }))
        .expect("the decision is accepted");

    let error = refused(fx.guideline_retire(DECISION, None));

    assert_eq!(
        code_and_pointer(&error),
        (json!("guideline-not-found"), json!("/id"))
    );
}

// Sucessora

#[test]
fn it_should_refuse_a_successor_that_is_not_current() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone", "brief"]);
    fx.guideline_retire(BRIEF, None)
        .expect("the successor is retired");

    let error = refused(fx.guideline_retire(TONE, Some(BRIEF)));

    assert_eq!(
        code_and_pointer(&error),
        (json!("successor-not-current"), json!("/by"))
    );
}

#[test]
fn it_should_refuse_a_successor_that_does_not_exist() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);

    let error = refused(fx.guideline_retire(TONE, Some(GHOST)));

    assert_eq!(
        code_and_pointer(&error),
        (json!("successor-not-current"), json!("/by"))
    );
}

#[test]
fn it_should_refuse_a_successor_that_is_not_a_guideline() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    let decision = node_wire(DECISION, "decision", &decision_data("use sqlite", None));
    fx.append_as_user(json!({ "items": [decision] }))
        .expect("the decision is accepted");

    let error = refused(fx.guideline_retire(TONE, Some(DECISION)));

    assert_eq!(
        code_and_pointer(&error),
        (json!("successor-not-current"), json!("/by"))
    );
}

#[test]
fn it_should_refuse_a_successor_equal_to_the_retired_id() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);

    let error = refused(fx.guideline_retire(TONE, Some(TONE)));

    assert_eq!(
        code_and_pointer(&error),
        (json!("successor-is-self"), json!("/by"))
    );
}

#[test]
fn it_should_refuse_a_supersedes_cycle() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["old", "mid", "new"]);
    fx.append_supersedes(OLD, MID).expect("old supersedes mid");
    fx.append_supersedes(MID, NEW).expect("mid supersedes new");

    let error = refused(fx.guideline_retire(OLD, Some(NEW)));

    assert_eq!(
        code_and_pointer(&error),
        (json!("supersedes-cycle"), json!("/by"))
    );
}

#[test]
fn it_should_write_nothing_when_the_supersedes_cycle_is_refused() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["old", "mid", "new"]);
    fx.append_supersedes(OLD, MID).expect("old supersedes mid");
    fx.append_supersedes(MID, NEW).expect("mid supersedes new");
    let before = (total_records(&fx), fx.relation_edges("supersedes"));

    let _ = fx.guideline_retire(OLD, Some(NEW));

    assert_eq!(
        (total_records(&fx), fx.relation_edges("supersedes")),
        before
    );
    assert_eq!(fx.get(OLD)["id"], json!(OLD));
}

// Recusas com causa e próximo passo

#[test]
fn it_should_refuse_a_guideline_retirement_with_cause_and_next_step_without_echoing_the_slug() {
    let fx = AppFixture::new();

    let errors = fx.guideline_retire_refusals();

    let outcomes: Vec<(String, bool)> = errors
        .iter()
        .map(|error| {
            let message = error["error"]["message"].as_str().unwrap_or_default();
            let next_step = error["error"]["details"][0]["next_step"]
                .as_str()
                .unwrap_or_default();
            let complete = !message.is_empty()
                && !next_step.is_empty()
                && !error.to_string().contains(RETIRE_SLUG_MARK);

            (
                error["error"]["code"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                complete,
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            ("guideline-not-found".to_owned(), true),
            ("guideline-not-orphaned".to_owned(), true),
            ("successor-not-current".to_owned(), true),
            ("successor-is-self".to_owned(), true),
            ("supersedes-cycle".to_owned(), true),
        ]
    );
}

// Retirada

#[test]
fn it_should_retire_and_drop_the_guideline_from_the_context() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    let before = context_has(&fx, TONE);

    fx.guideline_retire(TONE, None)
        .expect("the guideline is retired");
    let after = context_has(&fx, TONE);

    assert_eq!((before, after), (true, false));
}

#[test]
fn it_should_report_the_retired_id_and_no_successor() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);

    let report = fx.guideline_retire(TONE, None);

    assert_eq!(
        report,
        Ok(json!({ "retired": TONE, "superseded_by": null }))
    );
}

#[test]
fn it_should_report_the_retired_id_and_its_successor() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone", "brief"]);

    let report = fx.guideline_retire(TONE, Some(BRIEF));

    assert_eq!(
        report,
        Ok(json!({ "retired": TONE, "superseded_by": BRIEF }))
    );
}

#[test]
fn it_should_skip_the_retired_id_on_the_next_load() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    fx.guideline_retire(TONE, None)
        .expect("the guideline is retired");
    fx.write_guideline_file("tone");

    let report = fx.load(false).expect("the load is accepted");

    assert_eq!(
        report["skipped"],
        json!([{"id": TONE, "code": "extension-retired"}])
    );
    assert_eq!(fx.list("guideline"), json!([]));
}

#[test]
fn it_should_write_supersedes_and_the_retirement_in_one_batch() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone", "brief"]);
    let before = total_records(&fx);

    fx.guideline_retire(TONE, Some(BRIEF))
        .expect("the guideline is retired");

    assert_eq!(
        (fx.relation_edges("supersedes"), total_records(&fx)),
        (vec![(BRIEF.to_owned(), TONE.to_owned())], before + 2)
    );
    assert_eq!(fx.get(TONE)["id"], Value::Null);
}

#[test]
fn it_should_write_only_the_retirement_without_a_successor() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    let before = total_records(&fx);

    fx.guideline_retire(TONE, None)
        .expect("the guideline is retired");

    assert_eq!(
        (fx.relation_edges("supersedes"), total_records(&fx)),
        (Vec::new(), before + 1)
    );
}

#[test]
fn it_should_author_the_retirement_with_the_retire_client_and_the_real_agent() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    fx.guideline_retire(TONE, None)
        .expect("the guideline is retired");

    let mut omhl = fx.omhl();
    let revisions = omhl
        .lineage_records(&acme(), &Lineage::Node(sid(TONE)))
        .expect("the lineage is read");

    let retirement = revisions.last().expect("the lineage has a retirement");
    assert!(retirement.retired);
    assert_eq!(
        retirement.author,
        Author {
            agent: "user".to_owned(),
            model: None,
            client: "guideline-retire".to_owned(),
        }
    );
}

#[test]
fn it_should_keep_the_chain_valid_after_retiring_a_guideline() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    let before = total_records(&fx);

    fx.guideline_retire(TONE, None)
        .expect("the guideline is retired");
    let report = fx.verify();

    assert_eq!(report["ok"], json!(true));
    assert_eq!(report["total_records"], json!(before + 1));
}

#[test]
fn it_should_retire_a_guideline_that_decisions_rest_on_and_then_fail_the_anchor() {
    let fx = AppFixture::new();
    fx.load_orphaned_guidelines(&["tone"]);
    let work_item = node_wire(W1, "work-item", &work_item_data("Ship it"));
    fx.append_as_user(json!({ "items": [work_item] }))
        .expect("the work item is accepted");
    let decision = node_wire(DECISION, "decision", &decision_data("use sqlite", Some(W1)));
    let rests_on = json!({
        "body": { "kind": "relation", "relation_type": "rests-on", "from": DECISION, "to": TONE }
    });
    fx.append_as_user(json!({ "items": [decision, rests_on] }))
        .expect("the decision resting on the guideline is accepted");
    let anchored_before = question_passed(&fx.gate_pr(W1), DECISIONS_ANCHORED);

    fx.guideline_retire(TONE, None)
        .expect("the engine accepts retiring a guideline with decisions resting on it");
    let anchored_after = question_passed(&fx.gate_pr(W1), DECISIONS_ANCHORED);

    assert_eq!((anchored_before, anchored_after), (true, false));
}

// Linha de comando

const TONE_TOML: &str = "kind = \"business\"\ntext = \"Keep every answer brief\"\n";
const BRIEF_TOML: &str = "kind = \"business\"\ntext = \"Be concise\"\n";

fn omhl_in(project: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(project.dir()).xdg(project.xdg())
}

/// Projeto com as diretivas `tone` e `brief` já carregadas; os arquivos seguem na árvore.
fn loaded_project() -> Project {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .guideline("tone", TONE_TOML)
        .guideline("brief", BRIEF_TOML);
    let loaded = omhl_in(&project, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);

    project
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

#[test]
fn it_should_retire_a_guideline_with_its_successor_from_the_command_line() {
    let project = loaded_project();
    fs::remove_file(project.dir().join(".omhl/guidelines/tone.toml"))
        .expect("the guideline file is removed");

    let output = omhl_in(&project, &["guideline", "retire", TONE, "--by", BRIEF]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "retired": TONE, "superseded_by": BRIEF })
    );
}

#[test]
fn it_should_refuse_from_the_command_line_while_the_toml_file_exists() {
    let project = loaded_project();

    let output = omhl_in(&project, &["guideline", "retire", TONE]).run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(error["code"], json!("guideline-not-orphaned"));
    assert_eq!(error["details"][0]["path"], json!("/id"));
}
