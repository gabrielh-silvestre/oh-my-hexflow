use crate::support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use support::child::{Cmd, Output, omhl_json};
use support::ext::ExtTree;
use support::git::git_ok;
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const ANCHORING: &str = "checks.verify-review-comment-anchoring";
const LINT: &str = "checks.lint";
const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const BASE_LABELS: [&str; 3] = [
    "no-open-contradiction",
    "decisions-anchored",
    "anchors-confirmed",
];

fn in_dir(project: &Project, dir: &Path, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(dir).xdg(project.xdg())
}

fn in_project(project: &Project, args: &[&str]) -> Cmd {
    in_dir(project, project.dir(), args)
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn open_work_item(project: &Project) {
    let request = json!({ "items": [{ "body": {
        "kind": "node", "id": W1, "type": "work-item", "data": work_item_data("t"),
    } }] });
    let appended = in_project(project, &["append"])
        .stdin(request.to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn labels_of(gate: &Output) -> Vec<String> {
    json_line(&gate.stdout)["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .map(|question| question["label"].as_str().expect("a label").to_owned())
        .collect()
}

fn error_of(output: &Output) -> Value {
    json_line(&output.stderr)["error"].clone()
}

fn commit_all(dir: &Path, message: &str) {
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", message]);
}

/// Repositório git com o marcador e o `workflow` dado já commitados.
fn repo_with_workflow(workflow: Option<&str>) -> Project {
    let project = Project::new("acme");
    if let Some(text) = workflow {
        let dir = project.dir().join(".omhl");
        fs::create_dir_all(&dir).expect("the .omhl directory is created");
        fs::write(dir.join("workflow.toml"), text).expect("the workflow file is written");
    }
    support::git::commit_marker(project.dir());

    project
}

// CA-006: sem arquivo nem hooks, o gate traz só as três perguntas do método

#[test]
fn it_should_pass_the_gate_with_only_the_three_method_questions_without_a_workflow_file() {
    let project = repo_with_workflow(None);
    open_work_item(&project);

    let gate = in_project(&project, &["gate-pr", W1]).run();

    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);
    assert_eq!(labels_of(&gate), BASE_LABELS);
    assert_eq!(json_line(&gate.stdout)["passed"], json!(true));
}

#[test]
fn it_should_close_with_a_passed_gate_without_a_workflow_file() {
    let project = repo_with_workflow(None);
    open_work_item(&project);

    let closed = in_project(&project, &["close", W1]).run();

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(json_line(&closed.stdout)["report"]["passed"], json!(true));
}

// CA-009 e CA-010: arquivo inválido e sem git

#[test]
fn it_should_refuse_gate_pr_and_close_with_the_file_path_and_the_next_step_for_an_invalid_workflow()
{
    let project = repo_with_workflow(Some("version = 1\ncolour = true\n"));
    open_work_item(&project);

    let gate = in_project(&project, &["gate-pr", W1]).run();
    let closed = in_project(&project, &["close", W1]).run();
    let work_item = in_project(&project, &["get", W1]).run();

    for refused in [&gate, &closed] {
        let error = error_of(refused);
        assert_eq!(refused.code, Some(1), "stderr: {}", refused.stderr);
        assert_eq!(error["code"], json!("invalid-workflow-config"));
        assert!(
            error["message"]
                .as_str()
                .expect("a message")
                .contains(".omhl/workflow.toml"),
            "{error}"
        );
        assert!(
            error["details"][0]["next_step"]
                .as_str()
                .expect("a next step")
                .contains("omhl load --dry-run"),
            "{error}"
        );
    }
    assert_eq!(
        json_line(&work_item.stdout)["node"]["data"]["status"],
        json!("open")
    );
}

#[test]
fn it_should_refuse_gate_pr_and_close_with_probe_failed_outside_a_repository() {
    let project = Project::new("acme");
    open_work_item(&project);

    let gate = in_project(&project, &["gate-pr", W1]).run();
    let closed = in_project(&project, &["close", W1]).run();

    for refused in [&gate, &closed] {
        assert_eq!(refused.code, Some(1), "stderr: {}", refused.stderr);
        assert_eq!(error_of(refused)["code"], json!("probe-failed"));
    }
}

// CA-008: a ativação vem do diretório principal, de qualquer worktree

fn linked_worktree(project: &Project) -> std::path::PathBuf {
    let path = project.dir().with_extension("linked");
    git_ok(
        project.dir(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/wi-w1__x",
            path.to_str().expect("utf-8"),
        ],
    );

    path
}

#[test]
fn it_should_ask_the_native_questions_in_gate_pr_and_close_from_a_worktree_when_the_main_directory_enables_them()
 {
    let project = repo_with_workflow(None);
    open_work_item(&project);
    project.enable_natives(&[RUN_TESTS, CI_GREEN]);
    let linked = linked_worktree(&project);

    let gate = in_dir(&project, &linked, &["gate-pr", W1]).run();
    let closed = in_dir(&project, &linked, &["close", W1]).run();

    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);
    assert_eq!(labels_of(&gate)[..2], [RUN_TESTS, CI_GREEN]);
    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(json_line(&closed.stdout)["report"]["passed"], json!(false));
    fs::remove_dir_all(linked).expect("the worktree is removed");
}

#[test]
fn it_should_not_ask_the_native_questions_from_a_worktree_when_only_its_copy_enables_them() {
    let project = repo_with_workflow(None);
    open_work_item(&project);
    let linked = linked_worktree(&project);
    let copy = linked.join(".omhl");
    fs::create_dir_all(&copy).expect("the linked .omhl is created");
    fs::write(
        copy.join("workflow.toml"),
        "version = 1\n[native]\nchecks = [\"checks.run-tests\"]\n",
    )
    .expect("the linked copy is written");
    commit_all(&linked, "enable in the linked copy only");

    let gate = in_dir(&project, &linked, &["gate-pr", W1]).run();

    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);
    assert_eq!(labels_of(&gate), BASE_LABELS);
    fs::remove_dir_all(linked).expect("the worktree is removed");
}

// CA-007: o histórico que cita um nativo desligado segue legível

fn attest_every_native(project: &Project) {
    for check in [RUN_TESTS, CI_GREEN, ANCHORING] {
        let attested = in_project(project, &["attest", check, W1, "--outcome", "passed"]).run();
        assert_eq!(attested.code, Some(0), "{check}: {}", attested.stderr);
    }
}

fn succeeds(project: &Project, args: &[&str]) {
    let output = in_project(project, args).run();

    assert_eq!(output.code, Some(0), "{args:?}: {}", output.stderr);
}

fn project_with_native_history_then_disabled() -> Project {
    let project = Project::new("acme");
    project.enable_natives(&[RUN_TESTS, CI_GREEN, ANCHORING]);
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    support::git::commit_marker(project.dir());
    succeeds(&project, &["load"]);
    open_work_item(&project);
    attest_every_native(&project);
    let after = json!({ "items": [{ "body": {
        "kind": "relation", "relation_type": "after", "from": LINT, "to": CI_GREEN,
    } }] });
    let appended = in_project(&project, &["append"])
        .stdin(after.to_string())
        .run();
    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    project.enable_natives(&[]);
    commit_all(project.dir(), "disable the natives");

    project
}

#[test]
fn it_should_read_and_evaluate_the_history_of_disabled_natives_without_error() {
    let project = project_with_native_history_then_disabled();

    for args in [
        &["list"][..],
        &["get", CI_GREEN],
        &["render"],
        &["report", "retro", W1],
        &["report", "sessions"],
        &["gate-pr", W1],
        &["close", W1],
    ] {
        succeeds(&project, args);
    }
    succeeds(&project, &["project"]);
}

#[test]
fn it_should_accept_a_new_attestation_of_each_disabled_native() {
    let project = project_with_native_history_then_disabled();

    attest_every_native(&project);
}

#[test]
fn it_should_keep_refusing_a_loaded_extension_with_a_native_id_while_the_natives_are_disabled() {
    let project = repo_with_workflow(None);
    for slug in [
        "run-tests",
        "ci-green-before-merge",
        "verify-review-comment-anchoring",
    ] {
        ExtTree::at(project.dir()).check(slug, LINT_TOML);
    }

    let loaded = in_project(&project, &["load"]).run();

    let details = error_of(&loaded)["details"].to_string();
    assert_eq!(loaded.code, Some(1), "stderr: {}", loaded.stderr);
    assert_eq!(details.matches("reserved-id").count(), 3, "{details}");
}
