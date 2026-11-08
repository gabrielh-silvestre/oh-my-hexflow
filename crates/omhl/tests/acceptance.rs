mod support;

use std::path::Path;

use hexlog::{FieldName, ListFilter, Scalar};
use omhl::gate::{GateQuestion, Question};
use serde_json::{Value, json};
use support::child::{Cmd, omhl};
use support::git::git_in;
use support::mcp_session::McpSession;
use support::omhl::{
    LINT_DOCS, OmhlHarness, acme, append_ok, check_data, decision_data, node_item, premise_data,
    work_item_data,
};
use support::pr_gate::{DECISION, PREMISE, relation_item};
use support::project::Project;
use support::{FakeProbe, rty, sid, tname};

const W1: &str = "work-items.w1";
const RUN_TESTS: &str = "checks.run-tests";
const USE_WAL: &str = "decisions.use-wal";

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
    let data = check_data("before-publishing-docs", "run the docs linter", true);
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
        GateQuestion {
            label: "approved".to_owned(),
            question: Question::Approved {
                of: of_type("decision"),
            },
        },
        GateQuestion {
            label: "no-pending".to_owned(),
            question: Question::NoPending {
                pending: ListFilter {
                    where_eq: vec![(
                        FieldName::parse("status").expect("valid field name"),
                        Scalar::Str("assumed".to_owned()),
                    )],
                    ..of_type("premise")
                },
                resolved_by: rty("answers"),
            },
        },
    ];

    let report = omhl
        .evaluate(&acme(), "decision-flow", &questions)
        .expect("the gate is evaluated");

    let passed: Vec<bool> = report.questions.iter().map(|q| q.passed).collect();
    assert_eq!(passed, vec![true, true]);
}

// Apoio ao aceite pelo binário

/// Um `omhl` rodando em `dir`, com o banco em `xdg`.
fn at(dir: &Path, xdg: &Path, args: &[&str]) -> Cmd {
    omhl(args).cwd(dir).xdg(xdg)
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// Transforma a pasta do projeto num repositório limpo, com o marcador commitado.
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

fn node_request(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "items": [{ "body": { "kind": "node", "id": id, "type": type_name, "data": data } }] })
}

/// Grava um node pela CLI, em `dir` com o banco em `xdg`.
fn append_node(dir: &Path, xdg: &Path, id: &str, type_name: &str, data: &Value) {
    let appended = at(dir, xdg, &["append"])
        .stdin(node_request(id, type_name, data).to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn append_decision(dir: &Path, xdg: &Path, id: &str) {
    append_node(dir, xdg, id, "decision", &decision_data("Use WAL", None));
}

fn mcp_call(session: &McpSession, name: &str, arguments: &Value) -> Value {
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    }));

    session.recv()["result"].clone()
}

/// O `additionalContext` que o hook `SessionStart` injeta para o projeto em `dir`.
fn hook_context(dir: &Path, xdg: &Path) -> String {
    let stdin = json!({ "hook_event_name": "SessionStart", "session_id": "s1", "cwd": dir, "source": "startup" });
    let output = at(dir, xdg, &["hook", "session-start"])
        .stdin(stdin.to_string())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    json_line(&output.stdout)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_owned()
}

fn keys_of(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("the value is an object")
        .keys()
        .cloned()
        .collect()
}

// CLI e MCP devolvem o mesmo resultado

#[test]
fn it_should_return_the_same_list_from_the_cli_and_the_mcp() {
    let project = Project::new("acme");
    append_decision(project.dir(), project.xdg(), "decisions.first");
    append_decision(project.dir(), project.xdg(), "decisions.second");
    let session = McpSession::start(project.dir(), project.xdg());

    let cli = at(project.dir(), project.xdg(), &["list"]).run();
    let mcp = mcp_call(&session, "list", &json!({}));

    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert_eq!(json_line(&cli.stdout), mcp["structuredContent"]);
}

#[test]
fn it_should_return_the_same_attestation_from_the_cli_and_the_mcp() {
    let project = Project::new("acme");
    commit_marker(&project);
    append_node(
        project.dir(),
        project.xdg(),
        W1,
        "work-item",
        &work_item_data("t"),
    );
    let session = McpSession::start(project.dir(), project.xdg());

    let cli = at(
        project.dir(),
        project.xdg(),
        &["attest", RUN_TESTS, W1, "--outcome", "passed"],
    )
    .run();
    let mcp = mcp_call(
        &session,
        "attest",
        &json!({ "check": RUN_TESTS, "work_item": W1, "outcome": "passed" }),
    );

    let cli_value = json_line(&cli.stdout);
    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert!(mcp.get("isError").is_none(), "the MCP call succeeds");
    assert_eq!(
        cli_value["attestation"],
        mcp["structuredContent"]["attestation"]
    );
    assert_eq!(keys_of(&cli_value), keys_of(&mcp["structuredContent"]));
}

#[test]
fn it_should_return_the_same_render_from_the_cli_and_the_mcp() {
    let project = Project::new("acme");
    append_decision(project.dir(), project.xdg(), USE_WAL);
    let session = McpSession::start(project.dir(), project.xdg());

    let cli = at(project.dir(), project.xdg(), &["render"]).run();
    let mcp = mcp_call(&session, "render", &json!({}));

    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert_eq!(json_line(&cli.stdout), mcp["structuredContent"]);
}

#[test]
fn it_should_return_the_same_error_from_the_cli_and_the_mcp() {
    let project = Project::new("acme");
    let session = McpSession::start(project.dir(), project.xdg());

    let cli = at(project.dir(), project.xdg(), &["get", "Bad Id"]).run();
    let mcp = mcp_call(&session, "get", &json!({ "id": "Bad Id" }));

    assert_eq!(cli.code, Some(1));
    assert_eq!(mcp["isError"], json!(true));
    assert_eq!(
        json_line(&cli.stderr)["error"],
        mcp["structuredContent"]["error"]
    );
}

// Sessão completa e isolamento

#[test]
fn it_should_run_a_full_session_across_hook_mcp_and_cli() {
    let project = Project::new("acme");
    commit_marker(&project);
    let (dir, xdg) = (project.dir(), project.xdg());
    let session = McpSession::start(dir, xdg);

    let before = hook_context(dir, xdg);
    let appended = mcp_call(
        &session,
        "append",
        &node_request(USE_WAL, "decision", &decision_data("Use WAL", None)),
    );
    let after = hook_context(dir, xdg);
    append_node(dir, xdg, W1, "work-item", &work_item_data("t"));
    let closed = at(dir, xdg, &["close", W1]).run();
    let verified = at(dir, xdg, &["verify"]).run();
    let fetched = at(dir, xdg, &["get", W1]).run();

    assert!(before.contains(RUN_TESTS), "the native base is injected");
    assert!(!before.contains(USE_WAL), "the decision is not there yet");
    assert!(appended.get("isError").is_none(), "the MCP append succeeds");
    assert!(after.contains(&format!("### {USE_WAL}")));
    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(json_line(&verified.stdout)["ok"], json!(true));
    assert_eq!(
        json_line(&fetched.stdout)["node"]["data"]["status"],
        json!("closed")
    );
}

#[test]
fn it_should_isolate_two_tenants_sharing_one_database() {
    let acme_project = Project::new("acme");
    let beta_project = Project::new("beta");
    let xdg = acme_project.xdg();
    append_decision(acme_project.dir(), xdg, USE_WAL);

    let acme_context = hook_context(acme_project.dir(), xdg);
    let beta_context = hook_context(beta_project.dir(), xdg);
    let acme_verify = at(acme_project.dir(), xdg, &["verify"]).run();
    let beta_verify = at(beta_project.dir(), xdg, &["verify"]).run();

    assert!(acme_context.contains(USE_WAL), "acme sees its decision");
    assert!(!beta_context.contains(USE_WAL), "beta does not see it");
    assert_eq!(json_line(&acme_verify.stdout)["ok"], json!(true));
    assert_eq!(json_line(&beta_verify.stdout)["ok"], json!(true));
}
