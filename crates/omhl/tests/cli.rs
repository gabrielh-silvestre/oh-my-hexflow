mod support;

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use support::child::{Cmd, Output as Child, omhl};
use support::ext::ExtTree;
use support::git::{GitRepo, git_in};
use support::omhl::{
    DIVERGENCE_TEXT, FIELD_NOT_TEXT, FIELD_NOT_TEXT_ANY, FIELD_REPEATED, FIELD_UNDECLARED,
    ID_PREFIX_RULE, NO_TYPE_DECLARES_ALL, PREFIX_NAMESPACE_ANY, PREFIX_NAMESPACE_TYPE,
    TYPE_NOT_DEFINED, VALUE_OUTSIDE_ENUM, decision_data, divergence_wires,
    divergence_work_item_wires, list_refusal, work_item_data,
};
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const LINT_CHECK: &str = "checks.lint";
const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const COMMANDS: [&str; 9] = [
    "list", "get", "append", "attest", "render", "gate-pr", "close", "verify", "hook",
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
    assert!(!output.stdout.contains("mcp"), "help does not cite mcp");
}

#[test]
fn it_should_exit_two_for_an_unknown_subcommand() {
    let output = omhl(&["bogus"]).run();

    assert_eq!(output.code, Some(2));
    assert!(output.stdout.is_empty(), "stdout stays empty");
}

#[test]
fn it_should_refuse_mcp_as_an_unknown_subcommand() {
    let output = omhl(&["mcp"]).run();

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
    assert!(
        json_line(&output.stderr)["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("rebase")),
        "the message points to a rebase: {}",
        output.stderr
    );
}

#[test]
fn it_should_refuse_a_malformed_id_prefix_from_the_cli() {
    let project = Project::new("acme");

    let output = in_project(&project, &["list", "--id-prefix", "decisions.Bad_Prefix"]).run();

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(
        json_line(&output.stderr),
        json!({
            "error": {
                "code": "invalid-arguments",
                "message": ID_PREFIX_RULE,
                "details": [{
                    "path": "/id_prefix",
                    "code": "invalid-arguments",
                    "message": ID_PREFIX_RULE,
                }],
            }
        })
    );
}

#[test]
fn it_should_refuse_a_malformed_node_id_in_get_from_the_cli() {
    let project = Project::new("acme");

    let output = in_project(&project, &["get", "Bad Id"]).run();

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    let error = &json_line(&output.stderr)["error"];
    assert_eq!(error["code"], json!("INVALID_INPUT"));
    assert_eq!(error["details"][0]["path"], json!("/id"));
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

/// `author` gravado por um `omhl append`, com ou sem `CLAUDECODE=1` no ambiente do processo.
fn author_of_append(claude_code: bool) -> Value {
    let project = Project::new("acme");
    let request = json!({
        "items": [{ "body": {
            "kind": "node", "id": "decisions.x", "type": "decision",
            "data": decision_data("Use WAL", None),
        } }]
    });
    let mut append = in_project(&project, &["append"]).stdin(request.to_string());
    if claude_code {
        append = append.env("CLAUDECODE", "1");
    }

    let appended = append.run();
    let fetched = in_project(&project, &["get", "decisions.x"]).run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    json_line(&fetched.stdout)["node"]["author"].clone()
}

#[test]
fn it_should_record_the_claude_agent_when_append_runs_under_claude_code() {
    let author = author_of_append(true);

    assert_eq!(author, json!({ "agent": "claude", "client": "cli" }));
}

#[test]
fn it_should_record_the_user_agent_when_append_runs_outside_claude_code() {
    let author = author_of_append(false);

    assert_eq!(author, json!({ "agent": "user", "client": "cli" }));
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
    let gate_json = json_line(&gate.stdout);
    let questions: Vec<(&str, bool)> = gate_json["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .map(|question| {
            (
                question["label"].as_str().expect("the label is text"),
                question["passed"].as_bool().expect("passed is a bool"),
            )
        })
        .collect();
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
    assert_eq!(
        first_json["attestation"]["snapshot"]["kind"],
        json!("git-commit")
    );
    assert_eq!(json_line(&gate.stdout)["passed"], json!(true));
    assert_eq!(
        questions,
        vec![
            (RUN_TESTS, true),
            (CI_GREEN, true),
            ("no-open-contradiction", true),
            ("decisions-anchored", true),
            ("anchors-confirmed", true),
        ]
    );
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

#[test]
fn it_should_refuse_a_db_inside_the_project_root_without_creating_files() {
    let project = Project::new("acme");
    let inside = project.dir().join("abs.db");
    let cases = ["rel.db", inside.to_str().expect("the path is UTF-8")];

    for db in cases {
        let output = omhl(&["--db", db, "list"]).cwd(project.dir()).run();

        assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
        assert_eq!(
            json_line(&output.stderr)["error"]["code"],
            json!("db-inside-project")
        );
    }
    assert!(!project.dir().join("rel.db").exists());
    assert!(!project.dir().join("abs.db").exists());
    assert!(!project.dir().join("blobs").exists());
}

// load e project

fn listed_ids(project: &Project, type_name: &str) -> Vec<String> {
    let listed = in_project(project, &["list", "--type", type_name]).run();

    assert_eq!(listed.code, Some(0), "stderr: {}", listed.stderr);
    json_line(&listed.stdout)["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|item| item["id"].as_str().expect("the id is text").to_owned())
        .collect()
}

#[test]
fn it_should_load_extensions_through_the_cli() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let output = in_project(&project, &["load"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["created"], json!([LINT_CHECK]));
}

#[test]
fn it_should_dry_run_without_writing_through_the_cli() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let output = in_project(&project, &["load", "--dry-run"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["dry_run"], json!(true));
    assert!(
        !listed_ids(&project, "check")
            .iter()
            .any(|id| id == LINT_CHECK),
        "the dry run stored nothing"
    );
}

#[test]
fn it_should_project_through_the_cli() {
    let project = Project::new("acme");
    open_work_item(&project);

    let output = in_project(&project, &["project"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout)["written"],
        json!(["work-items.w1.md"])
    );
    assert!(
        project
            .dir()
            .join(".omhl/projection/work-items.w1.md")
            .is_file(),
        "the projection file exists"
    );
}

// list --where

/// Grava os work items `w1` e `w2` e, depois, as três divergências de `DIVERGENCE_ROWS`.
fn seed_divergences(project: &Project) {
    for items in [divergence_work_item_wires(), divergence_wires()] {
        let appended = in_project(project, &["append"])
            .stdin(json!({ "items": items }).to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }
}

fn list_divergences(project: &Project, filters: &[&str]) -> Child {
    let mut args = vec!["list", "--type", "divergence"];
    for filter in filters {
        args.extend(["--where", filter]);
    }

    in_project(project, &args).run()
}

fn divergence_ids_where(project: &Project, filters: &[&str]) -> Vec<String> {
    let listed = list_divergences(project, filters);

    assert_eq!(listed.code, Some(0), "stderr: {}", listed.stderr);
    json_line(&listed.stdout)["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|item| item["id"].as_str().expect("the id is text").to_owned())
        .collect()
}

fn where_error_code(project: &Project, filters: &[&str]) -> Value {
    let output = list_divergences(project, filters);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");

    json_line(&output.stderr)["error"]["code"].clone()
}

#[test]
fn it_should_list_the_divergences_of_a_work_item_with_where() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = divergence_ids_where(&project, &["work_item=work-items.w2"]);

    assert_eq!(ids, ["divergences.d2", "divergences.d3"]);
}

#[test]
fn it_should_list_the_divergences_of_a_kind_with_where() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = divergence_ids_where(&project, &["kind=missing"]);

    assert_eq!(ids, ["divergences.d1", "divergences.d2"]);
}

#[test]
fn it_should_list_the_divergences_of_a_resolution_with_where() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = divergence_ids_where(&project, &["resolution=open"]);

    assert_eq!(ids, ["divergences.d1", "divergences.d3"]);
}

#[test]
fn it_should_combine_repeated_where_flags_into_one_match() {
    let project = Project::new("acme");
    seed_divergences(&project);
    let filters = [
        "work_item=work-items.w2",
        "kind=missing",
        "resolution=code-fixed",
    ];

    let ids = divergence_ids_where(&project, &filters);

    assert_eq!(ids, ["divergences.d2"]);
}

#[test]
fn it_should_return_nothing_when_no_divergence_matches_every_where_flag() {
    let project = Project::new("acme");
    seed_divergences(&project);
    let filters = ["work_item=work-items.w2", "kind=missing", "resolution=open"];

    let ids = divergence_ids_where(&project, &filters);

    assert!(ids.is_empty(), "each flag matches a different row: {ids:?}");
}

#[test]
fn it_should_accept_eight_where_flags() {
    let project = Project::new("acme");
    seed_divergences(&project);
    let filters = ["text=absent"; 8];

    let ids = divergence_ids_where(&project, &filters);

    assert!(ids.is_empty(), "no divergence has that text: {ids:?}");
}

#[test]
fn it_should_refuse_more_than_eight_where_flags() {
    let project = Project::new("acme");
    let filters = ["text=absent"; 9];

    let code = where_error_code(&project, &filters);

    assert_eq!(code, json!("where-eq-too-many"));
}

#[test]
fn it_should_accept_a_where_value_of_256_characters() {
    let project = Project::new("acme");
    seed_divergences(&project);
    let filter = format!("text={}", "x".repeat(256));

    let ids = divergence_ids_where(&project, &[&filter]);

    assert!(ids.is_empty(), "no divergence has that text: {ids:?}");
}

#[test]
fn it_should_refuse_a_where_value_longer_than_256_characters() {
    let project = Project::new("acme");
    let filter = format!("text={}", "x".repeat(257));

    let code = where_error_code(&project, &[&filter]);

    assert_eq!(code, json!("where-eq-value-too-long"));
}

#[test]
fn it_should_refuse_a_where_flag_without_an_equals_sign() {
    let project = Project::new("acme");

    let code = where_error_code(&project, &["kind"]);

    assert_eq!(code, json!("invalid-arguments"));
}

#[test]
fn it_should_refuse_a_where_flag_with_an_empty_field() {
    let project = Project::new("acme");

    let code = where_error_code(&project, &["=missing"]);

    assert_eq!(code, json!("invalid-arguments"));
}

// list: regras do filtro

fn list_refusal_of(project: &Project, filters: &[&str]) -> Value {
    let mut args = vec!["list"];
    args.extend(filters);

    let output = in_project(project, &args).run();

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(output.stderr.matches('\n').count(), 1, "one error line");

    json_line(&output.stderr)
}

fn list_ids_of(project: &Project, filters: &[&str]) -> Vec<String> {
    let mut args = vec!["list"];
    args.extend(filters);

    let listed = in_project(project, &args).run();

    assert_eq!(listed.code, Some(0), "stderr: {}", listed.stderr);
    json_line(&listed.stdout)["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|item| item["id"].as_str().expect("the id is text").to_owned())
        .collect()
}

/// `premises` seguido de segmentos de `a` com os tamanhos dados.
fn premises_prefix(segments: &[usize]) -> String {
    let mut parts = vec!["premises".to_owned()];
    parts.extend(segments.iter().map(|size| "a".repeat(*size)));

    parts.join(".")
}

#[test]
fn it_should_refuse_an_id_prefix_with_an_invalid_segment() {
    let project = Project::new("acme");
    let segment_too_long = format!("premises.{}", "a".repeat(64));
    let prefixes = [
        "",
        "premises.",
        ".a",
        "a..b",
        "premises.-x",
        &segment_too_long,
    ];

    for prefix in prefixes {
        let refusal = list_refusal_of(&project, &["--id-prefix", prefix]);

        assert_eq!(
            refusal,
            list_refusal("/id_prefix", ID_PREFIX_RULE),
            "{prefix:?}"
        );
    }
}

#[test]
fn it_should_accept_an_id_prefix_of_one_namespace() {
    let project = Project::new("acme");

    let ids = list_ids_of(&project, &["--id-prefix", "premises"]);

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_accept_an_id_prefix_with_a_hyphenated_segment() {
    let project = Project::new("acme");

    let ids = list_ids_of(&project, &["--id-prefix", "premises.cli1-b1"]);

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_accept_an_id_prefix_segment_of_63_characters() {
    let project = Project::new("acme");
    let prefix = premises_prefix(&[63]);

    let ids = list_ids_of(&project, &["--id-prefix", &prefix]);

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_refuse_an_id_prefix_segment_of_64_characters() {
    let project = Project::new("acme");
    let prefix = premises_prefix(&[64]);

    let refusal = list_refusal_of(&project, &["--id-prefix", &prefix]);

    assert_eq!(refusal, list_refusal("/id_prefix", ID_PREFIX_RULE));
}

#[test]
fn it_should_accept_an_id_prefix_of_200_characters() {
    let project = Project::new("acme");
    let prefix = premises_prefix(&[63, 63, 63]);

    let ids = list_ids_of(&project, &["--id-prefix", &prefix]);

    assert_eq!(prefix.len(), 200);
    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_refuse_an_id_prefix_of_201_characters() {
    let project = Project::new("acme");
    let prefix = premises_prefix(&[63, 63, 62, 1]);

    let refusal = list_refusal_of(&project, &["--id-prefix", &prefix]);

    assert_eq!(prefix.len(), 201);
    assert_eq!(refusal, list_refusal("/id_prefix", ID_PREFIX_RULE));
}

#[test]
fn it_should_refuse_a_type_that_is_not_in_the_catalog() {
    let project = Project::new("acme");

    for type_name in ["nopetype", "premises"] {
        let refusal = list_refusal_of(&project, &["--type", type_name]);

        assert_eq!(
            refusal,
            list_refusal("/type", TYPE_NOT_DEFINED),
            "{type_name}"
        );
    }
}

#[test]
fn it_should_refuse_an_id_prefix_outside_every_namespace() {
    let project = Project::new("acme");

    for prefix in ["premise", "premis"] {
        let refusal = list_refusal_of(&project, &["--id-prefix", prefix]);

        assert_eq!(
            refusal,
            list_refusal("/id_prefix", PREFIX_NAMESPACE_ANY),
            "{prefix}"
        );
    }
}

#[test]
fn it_should_refuse_an_id_prefix_outside_the_namespace_of_the_type() {
    let project = Project::new("acme");

    let refusal = list_refusal_of(&project, &["--type", "premise", "--id-prefix", "decisions"]);

    assert_eq!(refusal, list_refusal("/id_prefix", PREFIX_NAMESPACE_TYPE));
}

#[test]
fn it_should_accept_an_id_prefix_inside_the_namespace_of_the_type() {
    let project = Project::new("acme");

    let ids = list_ids_of(
        &project,
        &["--type", "premise", "--id-prefix", "premises.cli1"],
    );

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_accept_the_shared_work_items_namespace_for_a_pull_request() {
    let project = Project::new("acme");

    let ids = list_ids_of(
        &project,
        &["--type", "pull-request", "--id-prefix", "work-items"],
    );

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_refuse_where_on_a_non_text_field() {
    let project = Project::new("acme");

    let refusal = list_refusal_of(&project, &["--type", "check", "--where", "blocking=true"]);

    assert_eq!(refusal, list_refusal("/where/0", FIELD_NOT_TEXT));
}

#[test]
fn it_should_refuse_where_on_an_undeclared_field() {
    let project = Project::new("acme");

    let refusal = list_refusal_of(&project, &["--type", "divergence", "--where", "nope=x"]);

    assert_eq!(refusal, list_refusal("/where/0", FIELD_UNDECLARED));
}

#[test]
fn it_should_refuse_where_with_a_value_outside_the_enum() {
    let project = Project::new("acme");
    let cases = [("premise", "status=confimed"), ("check", "concern=nope")];

    for (type_name, filter) in cases {
        let refusal = list_refusal_of(&project, &["--type", type_name, "--where", filter]);

        assert_eq!(
            refusal,
            list_refusal("/where/0", VALUE_OUTSIDE_ENUM),
            "{type_name} {filter}"
        );
    }
}

#[test]
fn it_should_accept_where_on_an_enum_value_of_a_premise() {
    let project = Project::new("acme");

    let ids = list_ids_of(
        &project,
        &["--type", "premise", "--where", "status=confirmed"],
    );

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_accept_where_on_an_enum_value_of_a_check() {
    let project = Project::new("acme");

    let ids = list_ids_of(&project, &["--type", "check", "--where", "concern=test"]);

    assert!(ids.is_empty(), "nothing is stored: {ids:?}");
}

#[test]
fn it_should_accept_where_on_the_text_of_a_divergence() {
    let project = Project::new("acme");
    seed_divergences(&project);
    let filter = format!("text={DIVERGENCE_TEXT}");

    let ids = divergence_ids_where(&project, &[&filter]);

    assert_eq!(ids, ["divergences.d1", "divergences.d2", "divergences.d3"]);
}

#[test]
fn it_should_refuse_a_repeated_where_field_with_different_values() {
    let project = Project::new("acme");
    let typed = [
        "--type",
        "premise",
        "--where",
        "status=assumed",
        "--where",
        "status=confirmed",
    ];
    let untyped = ["--where", "status=assumed", "--where", "status=confirmed"];

    for filters in [&typed[..], &untyped[..]] {
        let refusal = list_refusal_of(&project, filters);

        assert_eq!(
            refusal,
            list_refusal("/where/1", FIELD_REPEATED),
            "{filters:?}"
        );
    }
}

#[test]
fn it_should_accept_the_same_where_pair_twice() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = divergence_ids_where(&project, &["kind=missing", "kind=missing"]);

    assert_eq!(ids, ["divergences.d1", "divergences.d2"]);
}

#[test]
fn it_should_refuse_where_without_type_on_an_unknown_field() {
    let project = Project::new("acme");

    let refusal = list_refusal_of(&project, &["--where", "blokcing=true"]);

    assert_eq!(refusal, list_refusal("/where/0", FIELD_NOT_TEXT_ANY));
}

#[test]
fn it_should_refuse_where_without_type_on_a_field_that_is_never_text() {
    let project = Project::new("acme");

    let refusal = list_refusal_of(&project, &["--where", "blocking=true"]);

    assert_eq!(refusal, list_refusal("/where/0", FIELD_NOT_TEXT_ANY));
}

#[test]
fn it_should_refuse_where_when_the_id_prefix_type_lacks_the_field() {
    let project = Project::new("acme");
    let cases = [("checks", "kind=missing"), ("work-items", "kind=x")];

    for (prefix, filter) in cases {
        let refusal = list_refusal_of(&project, &["--id-prefix", prefix, "--where", filter]);

        assert_eq!(
            refusal,
            list_refusal("/where/0", FIELD_NOT_TEXT_ANY),
            "{prefix}"
        );
    }
}

#[test]
fn it_should_refuse_where_fields_that_no_single_type_declares() {
    let project = Project::new("acme");
    let cases = [
        ["--where", "title=x", "--where", "kind=missing"],
        ["--where", "outcome=passed", "--where", "status=open"],
    ];

    for filters in cases {
        let refusal = list_refusal_of(&project, &filters);

        assert_eq!(
            refusal,
            list_refusal("/where/1", NO_TYPE_DECLARES_ALL),
            "{filters:?}"
        );
    }
}

#[test]
fn it_should_refuse_where_without_type_with_a_value_outside_every_enum() {
    let project = Project::new("acme");

    let refusal = list_refusal_of(&project, &["--where", "status=confimed"]);

    assert_eq!(refusal, list_refusal("/where/0", VALUE_OUTSIDE_ENUM));
}

#[test]
fn it_should_accept_where_without_type_on_a_text_field() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = list_ids_of(&project, &["--where", "work_item=work-items.w2"]);

    assert_eq!(ids, ["divergences.d2", "divergences.d3"]);
}

#[test]
fn it_should_accept_where_without_type_on_a_status_enum_that_has_no_type() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = list_ids_of(&project, &["--where", "status=open"]);

    assert_eq!(ids, ["work-items.w1", "work-items.w2"]);
}

#[test]
fn it_should_accept_where_without_type_on_a_kind_enum_that_has_no_type() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = list_ids_of(&project, &["--where", "kind=missing"]);

    assert_eq!(ids, ["divergences.d1", "divergences.d2"]);
}

#[test]
fn it_should_accept_where_fields_that_the_premise_type_declares_together() {
    let project = Project::new("acme");

    let ids = list_ids_of(
        &project,
        &[
            "--where",
            "work_item=work-items.w1",
            "--where",
            "status=confirmed",
        ],
    );

    assert!(ids.is_empty(), "no premise is stored: {ids:?}");
}

#[test]
fn it_should_accept_where_fields_that_the_work_item_types_declare_together() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = list_ids_of(&project, &["--where", "title=t", "--where", "status=open"]);

    assert_eq!(ids, ["work-items.w1", "work-items.w2"]);
}

#[test]
fn it_should_accept_where_on_an_id_prefix_that_fixes_the_candidate_types() {
    let project = Project::new("acme");
    seed_divergences(&project);

    let ids = list_ids_of(
        &project,
        &["--id-prefix", "work-items", "--where", "status=open"],
    );

    assert_eq!(ids, ["work-items.w1", "work-items.w2"]);
}
