use crate::support;

use std::path::Path;

use serde_json::{Value, json};
use support::child::{Cmd, omhl, omhl_json};
use support::git::commit_marker;
use support::omhl::{LIMIT_RULE, decision_data, work_item_data};
use support::project::Project;

const W1: &str = "work-items.w1";
const RUN_TESTS: &str = "checks.run-tests";
const USE_WAL: &str = "decisions.use-wal";

// Apoio ao aceite pelo binário

/// Um `omhl` rodando em `dir`, com o banco em `xdg`.
fn at(dir: &Path, xdg: &Path, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(dir).xdg(xdg)
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
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

/// O `additionalContext` que o hook `SessionStart` injeta para o projeto em `dir`.
fn hook_context(dir: &Path, xdg: &Path) -> String {
    let stdin = json!({ "hook_event_name": "SessionStart", "session_id": "s1", "cwd": dir, "source": "startup" });
    let output = omhl(&["hook", "session-start"])
        .cwd(dir)
        .xdg(xdg)
        .stdin(stdin.to_string())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    json_line(&output.stdout)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_owned()
}

// Resultado da CLI

#[test]
fn it_should_list_the_current_nodes_from_the_cli() {
    let project = Project::new("acme");
    append_decision(project.dir(), project.xdg(), "decisions.first");
    append_decision(project.dir(), project.xdg(), "decisions.second");

    let cli = at(project.dir(), project.xdg(), &["list"]).run();

    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert_eq!(
        json_line(&cli.stdout)["items"].as_array().map(Vec::len),
        Some(2)
    );
}

#[test]
fn it_should_return_the_attestation_from_the_cli() {
    let project = Project::new("acme");
    commit_marker(project.dir());
    append_node(
        project.dir(),
        project.xdg(),
        W1,
        "work-item",
        &work_item_data("t"),
    );

    let cli = at(
        project.dir(),
        project.xdg(),
        &["attest", RUN_TESTS, W1, "--outcome", "passed"],
    )
    .run();

    let value = json_line(&cli.stdout);
    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert!(value["attestation"].is_object(), "value: {value}");
}

#[test]
fn it_should_render_the_decisions_from_the_cli() {
    let project = Project::new("acme");
    append_decision(project.dir(), project.xdg(), USE_WAL);

    let cli = at(project.dir(), project.xdg(), &["render"]).run();

    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert!(json_line(&cli.stdout).to_string().contains(USE_WAL));
}

#[test]
fn it_should_report_an_invalid_id_from_the_cli() {
    let project = Project::new("acme");

    let cli = at(project.dir(), project.xdg(), &["get", "Bad Id"]).run();

    assert_eq!(cli.code, Some(1));
    assert!(json_line(&cli.stderr)["error"].is_object());
}

#[test]
fn it_should_report_the_limit_error_from_the_cli() {
    let project = Project::new("acme");

    let cli = at(project.dir(), project.xdg(), &["list", "--limit", "0"]).run();

    let expected = json!({
        "code": "invalid-arguments",
        "message": LIMIT_RULE,
        "details": [{
            "path": "/limit",
            "code": "invalid-arguments",
            "message": LIMIT_RULE,
        }],
    });
    assert_eq!(cli.code, Some(1));
    assert_eq!(json_line(&cli.stderr)["error"], expected);
}

// Sessão completa e isolamento

#[test]
fn it_should_run_a_full_session_across_hook_and_cli() {
    let project = Project::new("acme");
    commit_marker(project.dir());
    let (dir, xdg) = (project.dir(), project.xdg());

    let before = hook_context(dir, xdg);
    append_node(
        dir,
        xdg,
        USE_WAL,
        "decision",
        &decision_data("Use WAL", None),
    );
    let after = hook_context(dir, xdg);
    append_node(dir, xdg, W1, "work-item", &work_item_data("t"));
    let closed = at(dir, xdg, &["close", W1]).run();
    let verified = at(dir, xdg, &["verify"]).run();
    let fetched = at(dir, xdg, &["get", W1]).run();

    assert!(before.contains(RUN_TESTS), "the native base is injected");
    assert!(!before.contains(USE_WAL), "the decision is not there yet");
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
