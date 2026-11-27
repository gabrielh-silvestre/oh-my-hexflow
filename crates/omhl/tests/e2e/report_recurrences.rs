use crate::support;

use std::fs;

use serde_json::{Value, json};
use support::child::{Output, omhl_json};
use support::git::{commit_marker, git_in};
use support::omhl::typed_work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const W3: &str = "work-items.w3";
const TLS: &str = "Use TLS for every external call.";

// Relatório: `omhl report recurrences` (CLI) sobre um banco real

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn premise_node(id: &str, work_item: &str, kind: &str, text: &str) -> Value {
    let data = json!({ "text": text, "status": "assumed", "kind": kind, "work_item": work_item });

    node(id, "premise", data)
}

/// Premissa técnica `assumed` com `TLS` no work item dado.
fn tls_node(id: &str, work_item: &str) -> Value {
    premise_node(id, work_item, "technical", TLS)
}

fn confirmed_by() -> Value {
    json!({ "by": "person:ana", "question": "q", "answer": "a" })
}

fn directive_node(id: &str, text: &str) -> Value {
    node(
        id,
        "guideline",
        json!({ "text": text, "kind": "business", "confirmed_by": confirmed_by() }),
    )
}

fn unconfirmed_directive_node(id: &str, text: &str) -> Value {
    node(id, "guideline", json!({ "text": text, "kind": "business" }))
}

fn open_work_item(id: &str) -> Value {
    node(id, "work-item", typed_work_item_data("t"))
}

fn in_project(project: &Project, args: &[&str]) -> Output {
    omhl_json(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn append_output(project: &Project, items: Vec<Value>) -> Output {
    let mut request = json!({});
    request["items"] = Value::Array(items);

    omhl_json(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(request.to_string())
        .run()
}

fn append(project: &Project, items: Vec<Value>) {
    let appended = append_output(project, items);

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

/// Work items abertos `w1`, `w2` e `w3`, depois as premissas.
fn seed(project: &Project, premises: Vec<Value>) {
    append(
        project,
        vec![open_work_item(W1), open_work_item(W2), open_work_item(W3)],
    );
    append(project, premises);
}

fn seed_three_occurrences(project: &Project) {
    seed(
        project,
        vec![
            tls_node("premises.p1", W1),
            tls_node("premises.p2", W2),
            tls_node("premises.p3", W3),
        ],
    );
}

fn write_workflow(project: &Project, toml: &str) {
    let path = project.dir().join(".omhl/workflow.toml");
    fs::create_dir_all(path.parent().expect("the file has a parent")).expect("parent");
    fs::write(path, toml).expect("workflow.toml is written");
}

fn report_with(project: &Project, extra: &[&str]) -> Value {
    let mut args = vec!["report", "recurrences"];
    args.extend(extra);
    let output = in_project(project, &args);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stdout.lines().count(), 1, "one JSON line");

    json_line(&output.stdout)
}

fn report(project: &Project) -> Value {
    report_with(project, &[])
}

fn group_texts(report: &Value) -> Vec<String> {
    report["recurrences"]
        .as_array()
        .expect("recurrences is an array")
        .iter()
        .map(|group| {
            group["text"]
                .as_str()
                .expect("the text is a string")
                .to_owned()
        })
        .collect()
}

#[test]
fn it_should_list_a_text_repeated_in_three_work_items_with_different_case_and_punctuation() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            premise_node("premises.p1", W1, "technical", "Use TLS."),
            premise_node("premises.p2", W2, "technical", "use   tls"),
            premise_node("premises.p3", W3, "technical", "USE TLS!"),
        ],
    );

    let report = report(&project);

    assert_eq!(group_texts(&report), ["use tls"]);
}

#[test]
fn it_should_not_list_a_text_repeated_in_only_two_work_items_with_the_default_minimum() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            premise_node("premises.p1", W1, "technical", "Use TLS."),
            premise_node("premises.p2", W2, "technical", "use tls"),
        ],
    );

    let report = report(&project);

    assert_eq!(group_texts(&report), Vec::<String>::new());
}

#[test]
fn it_should_report_the_group_with_its_kind_work_items_and_premises() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            premise_node("premises.p1", W1, "technical", "Use TLS."),
            premise_node("premises.p2", W2, "technical", "use tls"),
            premise_node("premises.p3", W2, "technical", "Use TLS"),
        ],
    );

    let report = report_with(&project, &["--min", "2"]);

    assert_eq!(
        report["recurrences"],
        json!([{
            "kind": "technical",
            "text": "use tls",
            "work_items": [W1, W2],
            "premises": ["premises.p1", "premises.p2", "premises.p3"],
        }])
    );
}

#[test]
fn it_should_report_the_minimum_it_applied() {
    let project = Project::new("acme");

    let report = report_with(&project, &["--min", "2"]);

    assert_eq!(report, json!({ "min": 2, "recurrences": [] }));
}

#[test]
fn it_should_count_three_premises_of_the_same_work_item_once() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            tls_node("premises.p1", W1),
            tls_node("premises.p2", W1),
            tls_node("premises.p3", W1),
        ],
    );

    let report = report_with(&project, &["--min", "2"]);

    assert_eq!(group_texts(&report), Vec::<String>::new());
}

#[test]
fn it_should_count_a_closed_work_item() {
    let project = Project::new("acme");
    seed_three_occurrences(&project);
    commit_marker(project.dir());

    let closed = in_project(&project, &["close", W3]);
    let report = report(&project);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(group_texts(&report), ["use tls for every external call"]);
}

#[test]
fn it_should_not_group_premises_of_different_kinds() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            premise_node("premises.p1", W1, "technical", TLS),
            premise_node("premises.p2", W2, "business", TLS),
            premise_node("premises.p3", W3, "technical", TLS),
        ],
    );

    let report = report(&project);

    assert_eq!(group_texts(&report), Vec::<String>::new());
}

#[test]
fn it_should_list_a_group_of_two_when_the_minimum_is_two() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![tls_node("premises.p1", W1), tls_node("premises.p2", W2)],
    );

    let report = report_with(&project, &["--min", "2"]);

    assert_eq!(group_texts(&report), ["use tls for every external call"]);
}

#[test]
fn it_should_ignore_a_legacy_premise_without_kind_and_work_item() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![tls_node("premises.p1", W1), tls_node("premises.p2", W2)],
    );
    append(
        &project,
        vec![node(
            "premises.p3",
            "premise",
            json!({ "text": TLS, "status": "assumed" }),
        )],
    );

    let report = report(&project);

    assert_eq!(group_texts(&report), Vec::<String>::new());
}

#[test]
fn it_should_list_the_groups_in_the_order_their_first_premise_was_recorded() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            premise_node("premises.p1", W1, "technical", "second text"),
            premise_node("premises.p2", W1, "technical", "first text"),
            premise_node("premises.p3", W2, "technical", "second text"),
            premise_node("premises.p4", W2, "technical", "first text"),
        ],
    );

    let report = report_with(&project, &["--min", "2"]);

    assert_eq!(group_texts(&report), ["second text", "first text"]);
}

// Mínimo vindo de `.omhl/workflow.toml`

#[test]
fn it_should_apply_the_default_minimum_of_three_without_a_workflow_file() {
    let project = Project::new("acme");

    let report = report(&project);

    assert_eq!(report["min"], json!(3));
}

#[test]
fn it_should_apply_min_work_items_from_workflow_toml_when_no_minimum_is_given() {
    let project = Project::new("acme");
    write_workflow(&project, "version = 1\n\n[promotion]\nmin_work_items = 2\n");
    seed(
        &project,
        vec![tls_node("premises.p1", W1), tls_node("premises.p2", W2)],
    );

    let report = report(&project);

    assert_eq!(group_texts(&report), ["use tls for every external call"]);
}

#[test]
fn it_should_stop_listing_a_group_when_workflow_toml_raises_the_minimum() {
    let project = Project::new("acme");
    seed(
        &project,
        vec![
            tls_node("premises.p1", W1),
            tls_node("premises.p2", W2),
            tls_node("premises.p3", W3),
        ],
    );
    let before = group_texts(&self::report(&project));
    write_workflow(&project, "version = 1\n\n[promotion]\nmin_work_items = 4\n");

    let after = group_texts(&self::report(&project));

    assert_eq!((before.len(), after.len()), (1, 0));
}

#[test]
fn it_should_let_the_minimum_flag_win_over_workflow_toml() {
    let project = Project::new("acme");
    write_workflow(&project, "version = 1\n\n[promotion]\nmin_work_items = 2\n");
    seed(
        &project,
        vec![tls_node("premises.p1", W1), tls_node("premises.p2", W2)],
    );

    let report = report_with(&project, &["--min", "3"]);

    assert_eq!(group_texts(&report), Vec::<String>::new());
}

#[test]
fn it_should_apply_the_minimum_of_the_main_directory_workflow_file_from_a_linked_worktree() {
    let project = Project::new("acme");
    write_workflow(&project, "version = 1\n\n[promotion]\nmin_work_items = 2\n");
    commit_marker(project.dir());
    let scratch = tempfile::TempDir::new().expect("scratch directory is created");
    let linked = scratch.path().join("wt");
    let added = git_in(
        project.dir(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/other",
            &linked.to_string_lossy(),
        ],
    );
    assert!(added.status.success(), "git worktree add failed");
    fs::write(
        linked.join(".omhl/workflow.toml"),
        "version = 1\n\n[promotion]\nmin_work_items = 4\n",
    )
    .expect("the branch version of workflow.toml is written");

    let output = omhl_json(&["report", "recurrences"])
        .cwd(&linked)
        .xdg(project.xdg())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["min"], json!(2));
}

// Promoção: a diretiva confirmada tira o grupo; sem confirmação nada é registrado (AC 4)

#[test]
fn it_should_drop_the_group_once_a_confirmed_guideline_has_the_same_normalized_text() {
    let project = Project::new("acme");
    seed_three_occurrences(&project);
    let before = group_texts(&report(&project));

    append(
        &project,
        vec![directive_node(
            "guidelines.tls",
            "  USE TLS for every   external call!! ",
        )],
    );

    let after = group_texts(&report(&project));
    assert_eq!((before.len(), after.len()), (1, 0));
}

#[test]
fn it_should_keep_the_group_when_the_confirmed_guideline_has_a_different_text() {
    let project = Project::new("acme");
    seed_three_occurrences(&project);

    append(
        &project,
        vec![directive_node(
            "guidelines.other",
            "Prefer small pull requests",
        )],
    );

    let report = report(&project);
    assert_eq!(group_texts(&report), ["use tls for every external call"]);
}

#[test]
fn it_should_not_suggest_a_recurrence_covered_by_a_native_directive() {
    let project = Project::new("acme");
    let native_text = "By default, ask before committing and pushing at the end of a task; a project guideline may state a different policy.";
    seed(
        &project,
        vec![
            premise_node("premises.p1", W1, "technical", native_text),
            premise_node("premises.p2", W2, "technical", native_text),
            premise_node("premises.p3", W3, "technical", native_text),
        ],
    );

    let report = report(&project);

    assert!(group_texts(&report).is_empty(), "{report}");
}

#[test]
fn it_should_refuse_a_guideline_without_confirmed_by_as_confirmation_required() {
    let project = Project::new("acme");
    seed_three_occurrences(&project);

    let output = append_output(
        &project,
        vec![unconfirmed_directive_node("guidelines.tls", TLS)],
    );

    let detail = &json_line(&output.stderr)["error"]["details"][0];
    assert_eq!(output.code, Some(1));
    assert_eq!(
        (&detail["code"], &detail["path"]),
        (
            &json!("confirmation-required"),
            &json!("/items/0/body/data/confirmed_by")
        )
    );
}

#[test]
fn it_should_keep_the_group_when_the_guideline_without_confirmed_by_is_refused() {
    let project = Project::new("acme");
    seed_three_occurrences(&project);
    append_output(
        &project,
        vec![unconfirmed_directive_node("guidelines.tls", TLS)],
    );

    let report = report(&project);

    assert_eq!(group_texts(&report), ["use tls for every external call"]);
}
