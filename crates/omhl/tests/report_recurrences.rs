mod support;

use std::fs;

use ::omhl::workflow::recurrence::{Premise, Recurrence, normalize, recurrences};
use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::git::git_in;
use support::mcp_session::McpSession;
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const W3: &str = "work-items.w3";
const TLS: &str = "Use TLS for every external call.";

// Camada pura: `omhl::workflow::recurrence::{normalize, recurrences}`

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn pure(id: &str, kind: &str, text: &str, work_item: &str) -> Premise {
    Premise {
        id: id.to_owned(),
        kind: kind.to_owned(),
        text: text.to_owned(),
        work_item: work_item.to_owned(),
    }
}

/// Premissa técnica com o texto `TLS` no work item dado.
fn tls(id: &str, work_item: &str) -> Premise {
    pure(id, "technical", TLS, work_item)
}

fn found(premises: &[Premise], directives: &[&str], min: u32) -> Vec<Recurrence> {
    recurrences(premises, &strings(directives), min)
}

#[test]
fn it_should_trim_the_edges_of_a_text() {
    assert_eq!(normalize("  use tls \n"), "use tls");
}

#[test]
fn it_should_lowercase_an_ascii_text() {
    assert_eq!(normalize("Use TLS"), "use tls");
}

#[test]
fn it_should_lowercase_a_non_ascii_text() {
    assert_eq!(normalize("AÇÃO Única"), "ação única");
}

#[test]
fn it_should_collapse_runs_of_whitespace_into_one_space() {
    assert_eq!(normalize("use  \t tls\n\nalways"), "use tls always");
}

#[test]
fn it_should_remove_trailing_sentence_punctuation() {
    for mark in [".", ",", ";", ":", "!", "?"] {
        let text = format!("use tls{mark}");

        assert_eq!(normalize(&text), "use tls", "mark {mark:?}");
    }
}

#[test]
fn it_should_remove_a_run_of_trailing_punctuation() {
    assert_eq!(normalize("use tls?!..."), "use tls");
}

#[test]
fn it_should_remove_the_space_left_before_the_removed_punctuation() {
    assert_eq!(normalize("use tls . "), "use tls");
}

#[test]
fn it_should_keep_interior_punctuation() {
    assert_eq!(
        normalize("Use TLS, always: no exceptions"),
        "use tls, always: no exceptions"
    );
}

#[test]
fn it_should_keep_a_trailing_closing_parenthesis() {
    assert_eq!(normalize("call connect()"), "call connect()");
}

#[test]
fn it_should_give_one_key_to_texts_that_differ_only_in_case_spacing_and_final_punctuation() {
    assert_eq!(normalize("  Use   TLS!! "), normalize("use tls."));
}

#[test]
fn it_should_list_a_text_seen_in_as_many_distinct_work_items_as_the_minimum() {
    let premises = [
        tls("premises.p1", W1),
        tls("premises.p2", W2),
        tls("premises.p3", W3),
    ];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups.len(), 1);
}

#[test]
fn it_should_not_list_a_text_seen_in_fewer_distinct_work_items_than_the_minimum() {
    let premises = [tls("premises.p1", W1), tls("premises.p2", W2)];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups, []);
}

#[test]
fn it_should_count_a_work_item_once_however_many_premises_it_has() {
    let premises = [
        tls("premises.p1", W1),
        tls("premises.p2", W1),
        tls("premises.p3", W1),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(groups, []);
}

#[test]
fn it_should_group_texts_that_differ_in_case_spacing_and_final_punctuation() {
    let premises = [
        pure("premises.p1", "technical", "Use TLS.", W1),
        pure("premises.p2", "technical", "use   tls", W2),
        pure("premises.p3", "technical", "USE TLS!", W3),
    ];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups.len(), 1);
}

#[test]
fn it_should_keep_premises_of_different_kinds_apart() {
    let premises = [
        pure("premises.p1", "technical", TLS, W1),
        pure("premises.p2", "business", TLS, W2),
        pure("premises.p3", "technical", TLS, W3),
    ];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups, []);
}

#[test]
fn it_should_describe_a_group_with_its_kind_normalized_text_work_items_and_premises() {
    let premises = [
        pure("premises.p1", "technical", "Use TLS.", W1),
        pure("premises.p2", "technical", "use tls", W2),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(
        groups,
        [Recurrence {
            kind: "technical".to_owned(),
            text: "use tls".to_owned(),
            work_items: strings(&[W1, W2]),
            premises: strings(&["premises.p1", "premises.p2"]),
        }]
    );
}

#[test]
fn it_should_list_each_work_item_once_in_the_order_it_first_appears() {
    let premises = [
        tls("premises.p1", W2),
        tls("premises.p2", W1),
        tls("premises.p3", W2),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(groups[0].work_items, strings(&[W2, W1]));
}

#[test]
fn it_should_list_every_premise_of_the_group_in_input_order() {
    let premises = [
        tls("premises.p1", W1),
        tls("premises.p2", W1),
        tls("premises.p3", W2),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(
        groups[0].premises,
        strings(&["premises.p1", "premises.p2", "premises.p3"])
    );
}

#[test]
fn it_should_order_the_groups_by_their_first_premise() {
    let premises = [
        pure("premises.p1", "technical", "second text", W1),
        pure("premises.p2", "technical", "first text", W1),
        pure("premises.p3", "technical", "second text", W2),
        pure("premises.p4", "technical", "first text", W2),
    ];

    let groups = found(&premises, &[], 2);

    let texts: Vec<&str> = groups.iter().map(|group| group.text.as_str()).collect();
    assert_eq!(texts, ["second text", "first text"]);
}

#[test]
fn it_should_hide_a_group_whose_text_matches_a_directive_after_normalization() {
    let premises = [tls("premises.p1", W1), tls("premises.p2", W2)];

    let groups = found(&premises, &["  USE tls for   every external call "], 2);

    assert_eq!(groups, []);
}

#[test]
fn it_should_keep_a_group_when_the_directive_text_differs() {
    let premises = [tls("premises.p1", W1), tls("premises.p2", W2)];

    let groups = found(&premises, &["Use TLS for internal calls only"], 2);

    assert_eq!(groups.len(), 1);
}

#[test]
fn it_should_find_nothing_without_premises() {
    let groups = found(&[], &[], 1);

    assert_eq!(groups, []);
}

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
    node(id, "work-item", work_item_data("t"))
}

fn in_project(project: &Project, args: &[&str]) -> Output {
    omhl(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn append_output(project: &Project, items: Vec<Value>) -> Output {
    let mut request = json!({});
    request["items"] = Value::Array(items);

    omhl(&["append"])
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

/// Repositório git com o marcador do projeto commitado, que o `close` exige.
fn commit_marker(project: &Project) {
    let steps: [&[&str]; 5] = [
        &["init", "-b", "main"],
        &["config", "user.name", "Test Author"],
        &["config", "user.email", "author@example.com"],
        &["add", "-A"],
        &["commit", "-m", "add marker"],
    ];
    for args in steps {
        let output = git_in(project.dir(), args);
        assert!(output.status.success(), "git {args:?} failed");
    }
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
    commit_marker(&project);

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
    commit_marker(&project);
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

    let output = omhl(&["report", "recurrences"])
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

// Relatório: tool MCP `report_recurrences`

fn tool_call(name: &str, arguments: Value) -> Value {
    let mut params = json!({ "name": name });
    params["arguments"] = arguments;

    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params })
}

fn mcp_call(session: &McpSession, name: &str, arguments: Value) -> Value {
    session.send(&tool_call(name, arguments));

    session.recv()["result"].clone()
}

fn session_with_two_occurrences() -> (Project, McpSession) {
    let project = Project::new("acme");
    seed(
        &project,
        vec![tls_node("premises.p1", W1), tls_node("premises.p2", W2)],
    );
    let session = McpSession::start(project.dir(), project.xdg());

    (project, session)
}

#[test]
fn it_should_report_the_group_through_the_report_recurrences_tool() {
    let (_project, session) = session_with_two_occurrences();

    let result = mcp_call(&session, "report_recurrences", json!({ "min": 2 }));

    assert_ne!(result["isError"], json!(true), "result: {result}");
    assert_eq!(
        result["structuredContent"],
        json!({
            "min": 2,
            "recurrences": [{
                "kind": "technical",
                "text": "use tls for every external call",
                "work_items": [W1, W2],
                "premises": ["premises.p1", "premises.p2"],
            }],
        })
    );
}

#[test]
fn it_should_apply_the_default_minimum_in_the_report_recurrences_tool() {
    let (_project, session) = session_with_two_occurrences();

    let result = mcp_call(&session, "report_recurrences", json!({}));

    assert_eq!(
        result["structuredContent"],
        json!({ "min": 3, "recurrences": [] })
    );
}

#[test]
fn it_should_refuse_an_unknown_argument_in_the_report_recurrences_tool() {
    let (_project, session) = session_with_two_occurrences();

    let result = mcp_call(&session, "report_recurrences", json!({ "foo": "bar" }));

    assert_eq!(result["isError"], json!(true), "result: {result}");
    assert_eq!(
        result["structuredContent"]["error"]["details"][0]["path"],
        json!("/foo")
    );
}
