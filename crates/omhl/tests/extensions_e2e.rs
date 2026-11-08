mod support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use support::child::{Cmd, Output as Child, omhl};
use support::ext::ExtTree;
use support::git::git_in;
use support::mcp_session::McpSession;
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const LINT_CHECK: &str = "checks.lint";
const LINT_HOW: &str = "Run the lint suite before every commit";
const LINT_TOML: &str = "blocking = true\nhow = \"Run the lint suite before every commit\"\n\n[when]\nevent = \"pre-commit\"\n";
const LINT_TOML_EDITED: &str = "blocking = true\nhow = \"Run the lint suite and the formatter\"\n\n[when]\nevent = \"pre-commit\"\n";
const TONE_TOML: &str = "text = \"Keep every answer brief\"\n";

// Apoio

/// Um `omhl` rodando dentro do projeto, com o banco na pasta de dados dele.
fn in_project(project: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(project.dir()).xdg(project.xdg())
}

fn run_ok(project: &Project, args: &[&str]) -> Child {
    let output = in_project(project, args).run();

    assert_eq!(
        output.code,
        Some(0),
        "omhl {args:?} stderr: {}",
        output.stderr
    );
    output
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git_in(dir, args);

    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Faz da pasta do projeto um repositório git e commita tudo o que já existe nela.
fn init_and_commit(project: &Project) {
    git_ok(project.dir(), &["init", "-b", "main"]);
    git_ok(project.dir(), &["config", "user.name", "Test Author"]);
    git_ok(
        project.dir(),
        &["config", "user.email", "author@example.com"],
    );
    commit_all(project, "initial");
}

fn commit_all(project: &Project, message: &str) {
    git_ok(project.dir(), &["add", "-A"]);
    git_ok(project.dir(), &["commit", "-m", message]);
}

fn open_work_item(project: &Project) {
    let request = json!({
        "items": [{
            "body": {
                "kind": "node",
                "id": W1,
                "type": "work-item",
                "data": work_item_data("t"),
            }
        }]
    });

    let appended = in_project(project, &["append"])
        .stdin(request.to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

/// O `additionalContext` que o hook `SessionStart` injeta para o projeto.
fn hook_context(project: &Project) -> String {
    let stdin = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": project.dir(),
        "source": "startup",
    });

    let output = in_project(project, &["hook", "session-start"])
        .stdin(stdin.to_string())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    json_line(&output.stdout)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_owned()
}

fn render_text(project: &Project) -> String {
    let rendered = run_ok(project, &["render"]);

    json_line(&rendered.stdout)["text"]
        .as_str()
        .expect("the render has a text")
        .to_owned()
}

fn total_records(project: &Project) -> u64 {
    json_line(&run_ok(project, &["verify"]).stdout)["total_records"]
        .as_u64()
        .expect("verify counts the records")
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// Injeção e aviso

#[test]
fn it_should_inject_a_loaded_check_into_the_render() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    run_ok(&project, &["load"]);

    let text = render_text(&project);

    assert!(
        text.contains(LINT_HOW),
        "the render carries the check's how"
    );
}

#[test]
fn it_should_show_the_note_before_load_and_nothing_after() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let before = hook_context(&project);
    run_ok(&project, &["load"]);
    let after = hook_context(&project);

    assert!(before.contains("NOTE:"), "the first session shows the note");
    assert!(!after.contains("NOTE:"), "the second session has no note");
}

// Projeção e git

#[test]
fn it_should_keep_the_projection_out_of_git() {
    let project = Project::new("acme");
    init_and_commit(&project);
    open_work_item(&project);
    run_ok(&project, &["project"]);

    let status = git_in(project.dir(), &["status", "--porcelain"]);

    assert!(status.status.success(), "git status runs");
    assert_eq!(String::from_utf8_lossy(&status.stdout), "");
}

#[test]
fn it_should_refuse_attest_while_an_edited_extension_is_uncommitted() {
    let project = Project::new("acme");
    let tree = ExtTree::at(project.dir());
    tree.check("lint", LINT_TOML);
    init_and_commit(&project);
    run_ok(&project, &["load"]);
    open_work_item(&project);
    let attest = ["attest", LINT_CHECK, W1, "--outcome", "passed"];
    tree.check("lint", LINT_TOML_EDITED);

    let refused = in_project(&project, &attest).run();
    commit_all(&project, "edit lint");
    let accepted = in_project(&project, &attest).run();

    assert_eq!(refused.code, Some(1));
    assert_eq!(
        json_line(&refused.stderr)["error"]["code"],
        json!("snapshot-dirty")
    );
    assert_eq!(accepted.code, Some(0), "stderr: {}", accepted.stderr);
}

// Órfãos e recarga

#[test]
fn it_should_report_orphans_after_a_file_is_deleted_and_keep_injecting_it() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    run_ok(&project, &["load"]);
    fs::remove_file(project.dir().join(".omhl/checks/lint.toml")).expect("the file is deleted");

    let report = json_line(&run_ok(&project, &["load"]).stdout);
    let text = render_text(&project);

    assert_eq!(report["orphans"], json!([LINT_CHECK]));
    assert!(text.contains(LINT_HOW), "the orphan is still injected");
}

#[test]
fn it_should_load_twice_without_changes() {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .check("lint", LINT_TOML)
        .guideline("tone", TONE_TOML);
    run_ok(&project, &["load"]);
    let records_after_first = total_records(&project);

    let second = json_line(&run_ok(&project, &["load"]).stdout);

    assert_eq!(second["created"], json!([]));
    assert_eq!(second["revised"], json!([]));
    assert_eq!(second["unchanged"], json!([LINT_CHECK, "guidelines.tone"]));
    assert_eq!(total_records(&project), records_after_first);
}

// CLI e MCP

#[test]
fn it_should_give_cli_and_mcp_the_same_report_shape() {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .check("lint", LINT_TOML)
        .guideline("tone", TONE_TOML);
    let session = McpSession::start(project.dir(), project.xdg());

    let cli = run_ok(&project, &["load", "--dry-run"]);
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "load", "arguments": { "dry_run": true } },
    }));
    let mcp = session.recv()["result"].clone();

    assert!(mcp.get("isError").is_none(), "the MCP call succeeds");
    assert_eq!(json_line(&cli.stdout), mcp["structuredContent"]);
}

// Documentação

#[test]
fn it_should_document_extensions_in_toml_in_the_glossary() {
    let glossary = fs::read_to_string(repo_root().join("GLOSSARY.md")).expect("GLOSSARY.md exists");
    let guide = repo_root().join("docs/extensions-and-projection.md");

    let entry = glossary
        .split("\n\n")
        .find(|block| block.starts_with("**Extension**"))
        .expect("the glossary has an Extension entry");

    assert!(guide.is_file(), "docs/extensions-and-projection.md exists");
    assert!(entry.contains("TOML"), "the entry cites TOML");
    assert!(!entry.contains("YAML"), "the entry does not cite YAML");
}
