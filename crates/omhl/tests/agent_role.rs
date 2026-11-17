mod support;

use std::ffi::OsString;

use omhl::adapters::cli;
use omhl::project::Env;
use serde_json::{Value, json};
use support::omhl::decision_data;
use support::project::Project;

const DECISION_ID: &str = "decisions.x";

/// Saída de um `omhl` rodado no mesmo processo, com o ambiente dado por valor.
struct Run {
    stdout: String,
    stderr: String,
}

fn run_omhl(
    project: &Project,
    args: &[&str],
    stdin: &str,
    claude_code: bool,
    role: Option<&str>,
) -> Run {
    let env = Env {
        xdg_data_home: Some(project.xdg().as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
        claude_code,
        role: role.map(str::to_owned),
    };
    let argv = ["omhl", "--project-dir"]
        .into_iter()
        .map(OsString::from)
        .chain([project.dir().as_os_str().to_owned()])
        .chain(args.iter().map(OsString::from));
    let (mut out, mut err) = (Vec::new(), Vec::new());

    cli::run(argv, &mut stdin.as_bytes(), &mut out, &mut err, &env);

    Run {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    }
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// Um `decision`: tipo que o critic continua podendo gravar quando a matriz entrar.
fn append_decision(project: &Project, claude_code: bool, role: Option<&str>) -> Run {
    let request = json!({
        "items": [{ "body": {
            "kind": "node", "id": DECISION_ID, "type": "decision",
            "data": decision_data("Use WAL", None),
        } }]
    });

    run_omhl(
        project,
        &["append"],
        &request.to_string(),
        claude_code,
        role,
    )
}

/// `author` do node gravado; o `get` roda sem papel e fora do Claude Code.
fn recorded_author(project: &Project) -> Value {
    let fetched = run_omhl(project, &["get", DECISION_ID], "", false, None);

    json_line(&fetched.stdout)["node"]["author"].clone()
}

#[test]
fn it_should_record_omhl_critic_as_the_author_agent_when_omhl_role_is_omhl_critic() {
    let project = Project::new("acme");

    append_decision(&project, true, Some("omhl:critic"));

    assert_eq!(recorded_author(&project)["agent"], json!("omhl:critic"));
}

#[test]
fn it_should_keep_claude_as_the_author_agent_when_omhl_role_is_not_set_under_claude_code() {
    let project = Project::new("acme");

    append_decision(&project, true, None);

    assert_eq!(recorded_author(&project)["agent"], json!("claude"));
}

#[test]
fn it_should_refuse_an_omhl_role_outside_the_seven_roles() {
    let project = Project::new("acme");

    let appended = append_decision(&project, true, Some("omhl:wizard"));

    assert_eq!(
        json_line(&appended.stderr)["error"]["code"],
        json!("role-unknown")
    );
}

#[test]
fn it_should_refuse_an_omhl_role_outside_claude_code() {
    let project = Project::new("acme");

    let appended = append_decision(&project, false, Some("omhl:critic"));

    assert_eq!(
        json_line(&appended.stderr)["error"]["code"],
        json!("role-unknown")
    );
}

#[test]
fn it_should_keep_the_author_client_unchanged_when_omhl_role_is_set() {
    let project = Project::new("acme");

    append_decision(&project, true, Some("omhl:critic"));

    assert_eq!(recorded_author(&project)["client"], json!("cli"));
}
