mod support;

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use support::child::{Cmd, Output as Child, omhl};
use support::git::{GitRepo, git_in};
use support::omhl::{decision_data, work_item_data};
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const COMMANDS: [&str; 10] = [
    "list", "get", "append", "attest", "render", "gate-pr", "close", "verify", "mcp", "hook",
];

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omhl"))
        .args(args)
        .output()
        .expect("the omhl binary runs")
}

#[test]
fn it_should_print_the_version_and_exit_zero() {
    let output = run(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("omhl {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn it_should_print_usage_and_exit_two_for_any_other_argument() {
    let none = run(&[]);
    let bogus = run(&["--bogus"]);

    for output in [none, bogus] {
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Usage:"),
            "stderr carries the clap usage"
        );
        assert!(output.stdout.is_empty(), "stdout stays empty");
    }
}

// Apoio

/// Um `omhl` rodando dentro do projeto, com o banco na pasta de dados dele.
fn in_project(project: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(project.dir()).xdg(project.xdg())
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// Transforma a pasta do projeto num repositório limpo, com o marcador commitado; devolve o HEAD.
fn commit_marker(project: &Project) -> String {
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

    head_of(project.dir())
}

fn head_of(dir: &Path) -> String {
    let output = git_in(dir, &["rev-parse", "HEAD"]);

    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn append_in(project: &Project, id: &str, type_name: &str, data: &Value) -> Child {
    let request = json!({
        "items": [{ "body": { "kind": "node", "id": id, "type": type_name, "data": data } }]
    });

    in_project(project, &["append"])
        .stdin(request.to_string())
        .run()
}

fn open_work_item(project: &Project) {
    let appended = append_in(project, W1, "work-item", &work_item_data("t"));

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn first_help_words(help: &str) -> Vec<&str> {
    help.lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

// Roteamento e uso

#[test]
fn it_should_list_the_commands_in_help() {
    let output = omhl(&["--help"]).run();

    let words = first_help_words(&output.stdout);
    assert_eq!(output.code, Some(0));
    for command in COMMANDS {
        assert!(words.contains(&command), "help lists {command}");
    }
    assert!(!output.stdout.contains("sync"), "help does not cite sync");
}

#[test]
fn it_should_exit_two_for_an_unknown_subcommand() {
    let output = omhl(&["bogus"]).run();

    assert_eq!(output.code, Some(2));
    assert!(output.stdout.is_empty(), "stdout stays empty");
}

// Formato da saída

#[test]
fn it_should_print_one_compact_json_line_per_command() {
    let project = Project::new("acme");

    let output = in_project(&project, &["list"]).run();

    let line = output
        .stdout
        .strip_suffix('\n')
        .expect("the line ends with LF");
    let value: Value = serde_json::from_str(line).expect("the line is JSON");
    assert_eq!(output.code, Some(0));
    assert!(!line.contains('\n'), "one single line");
    assert_eq!(serde_json::to_string(&value).expect("re-serialized"), line);
}

#[test]
fn it_should_exit_one_with_a_json_error_when_there_is_no_marker() {
    let bare = TempDir::new().expect("bare directory is created");
    let xdg = TempDir::new().expect("data directory is created");
    for ancestor in bare.path().ancestors() {
        assert!(
            !ancestor.join(".omhl.toml").exists(),
            "no parent of the test directory carries a marker"
        );
    }

    let output = omhl(&["list"]).cwd(bare.path()).xdg(xdg.path()).run();

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(output.stderr.matches('\n').count(), 1, "one error line");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("no-marker")
    );
}

#[test]
fn it_should_find_the_project_by_the_project_dir_flag() {
    let project = Project::new("acme");
    let elsewhere = TempDir::new().expect("other directory is created");
    let data = decision_data("Use WAL", None);
    append_in(&project, "decisions.x", "decision", &data);
    let dir = project.dir().to_str().expect("the path is UTF-8");

    let output = omhl(&["--project-dir", dir, "get", "decisions.x"])
        .cwd(elsewhere.path())
        .xdg(project.xdg())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["node"]["data"], data);
}

// append, get e render

#[test]
fn it_should_append_from_stdin() {
    let project = Project::new("acme");
    let data = decision_data("Use WAL", None);

    let appended = append_in(&project, "decisions.x", "decision", &data);
    let fetched = in_project(&project, &["get", "decisions.x"]).run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    assert_eq!(fetched.code, Some(0), "stderr: {}", fetched.stderr);
    assert_eq!(json_line(&fetched.stdout)["node"]["data"], data);
}

#[test]
fn it_should_exit_one_on_invalid_json_input() {
    let project = Project::new("acme");

    let output = in_project(&project, &["append"]).stdin("{").run();

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("input-invalid-json")
    );
}

#[test]
fn it_should_print_null_for_an_unknown_node() {
    let project = Project::new("acme");

    let output = in_project(&project, &["get", "decisions.nope"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "{\"node\":null}\n");
}

#[test]
fn it_should_print_the_render_json() {
    let project = Project::new("acme");

    let output = in_project(&project, &["render"]).run();

    let value = json_line(&output.stdout);
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        value["text"]
            .as_str()
            .expect("text is a string")
            .starts_with("# omhl context for tenant acme\n")
    );
    assert_eq!(value["omitted"], json!(0));
}

// Fluxo do pull request num repositório real

#[test]
fn it_should_evaluate_and_close_a_pull_request_in_a_real_repo() {
    let project = Project::new("acme");
    let head = commit_marker(&project);
    open_work_item(&project);

    let gate = in_project(&project, &["gate-pr", W1]).run();
    let close = in_project(&project, &["close", W1]).run();
    let fetched = in_project(&project, &["get", W1]).run();

    let node = json_line(&fetched.stdout)["node"].clone();
    assert_eq!([gate.code, close.code, fetched.code], [Some(0); 3]);
    assert_eq!(json_line(&gate.stdout)["gate"], json!("pull-request"));
    assert_eq!(json_line(&gate.stdout)["passed"], json!(false));
    assert_eq!(json_line(&close.stdout)["report"]["passed"], json!(false));
    assert_eq!(node["data"]["status"], json!("closed"));
    assert_eq!(node["data"]["gate"]["snapshot"]["id"], json!(head));
}

#[test]
fn it_should_attest_both_required_checks_and_pass_the_gate_in_a_real_repo() {
    let project = Project::new("acme");
    let head = commit_marker(&project);
    open_work_item(&project);

    let first = in_project(&project, &["attest", RUN_TESTS, W1, "--outcome", "passed"]).run();
    let second = in_project(
        &project,
        &[
            "attest",
            CI_GREEN,
            W1,
            "--outcome",
            "passed_with_warning",
            "--note",
            "ok",
        ],
    )
    .run();
    let gate = in_project(&project, &["gate-pr", W1]).run();
    let first_json = json_line(&first.stdout);
    let attestation_id = first_json["attestation"]["id"]
        .as_str()
        .expect("the attestation id is text");
    let fetched = in_project(&project, &["get", attestation_id]).run();

    assert_eq!([first.code, second.code, gate.code], [Some(0); 3]);
    assert_eq!(first_json["attestation"]["snapshot"]["id"], json!(head));
    assert_eq!(
        json_line(&second.stdout)["attestation"]["snapshot"]["id"],
        json!(head)
    );
    assert_eq!(json_line(&gate.stdout)["passed"], json!(true));
    assert_eq!(
        json_line(&fetched.stdout)["node"]["data"]["outcome"],
        json!("passed")
    );
}

#[test]
fn it_should_refuse_to_close_twice() {
    let project = Project::new("acme");
    commit_marker(&project);
    open_work_item(&project);
    in_project(&project, &["close", W1]).run();

    let second = in_project(&project, &["close", W1]).run();

    assert_eq!(second.code, Some(1));
    assert_eq!(
        json_line(&second.stderr)["error"]["code"],
        json!("invalid-transition")
    );
}

#[test]
fn it_should_refuse_to_close_on_a_dirty_repo() {
    let project = Project::new("acme");
    commit_marker(&project);
    open_work_item(&project);
    std::fs::write(project.dir().join("dirty.txt"), "x").expect("dirty file is written");

    let output = in_project(&project, &["close", W1]).run();

    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error["code"], json!("snapshot-dirty"));
    assert_eq!(error["details"][0]["path"], json!("/snapshot"));
}

#[test]
fn it_should_ignore_hostile_git_environment() {
    let project = Project::new("acme");
    let head = commit_marker(&project);
    open_work_item(&project);
    let other = GitRepo::init();
    other.write("b.txt", "b");
    let other_head = other.commit_all("other repo");
    let other_git = other.path().join(".git");
    let other_index = other_git.join("index");

    let closed = in_project(&project, &["close", W1])
        .env("GIT_DIR", &other_git)
        .env("GIT_WORK_TREE", other.path())
        .env("GIT_INDEX_FILE", &other_index)
        .run();
    let fetched = in_project(&project, &["get", W1]).run();

    assert_ne!(head, other_head, "the two repositories differ");
    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(
        json_line(&fetched.stdout)["node"]["data"]["gate"]["snapshot"]["id"],
        json!(head)
    );
}

// Banco

#[cfg(unix)]
#[test]
fn it_should_create_the_database_at_the_db_flag_with_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let project = Project::new("acme");
    let base = TempDir::new().expect("base directory is created");
    let dir = base.path().join("d");
    let db = dir.join("omhl.db");
    let db_arg = db.to_str().expect("the path is UTF-8");

    let output = omhl(&["--db", db_arg, "list"]).cwd(project.dir()).run();

    let file_mode = std::fs::metadata(&db)
        .expect("the database exists")
        .permissions()
        .mode();
    let dir_mode = std::fs::metadata(&dir)
        .expect("the directory exists")
        .permissions()
        .mode();
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(file_mode & 0o777, 0o600);
    assert_eq!(dir_mode & 0o777, 0o700);
}
