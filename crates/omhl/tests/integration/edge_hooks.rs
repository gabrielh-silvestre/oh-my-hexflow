use crate::support;

use std::fs;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use omhl::app::{
    BashEvent, GuardOutcome, McpWriteEvent, ProviderAccess, command_guard, mcp_write_warning,
};
use omhl::runner::{RunError, RunReport, RunSpec};
use support::git::git_ok;
use support::runner::FakeRunner;
use tempfile::TempDir;

const SMALL_TIMEOUT: Duration = Duration::from_millis(50);
const PROVIDER_ON: &str = "version = 1\n\n[provider]\nname = \"github\"\n";
const PUSH_WARNING: &str = "protected-branch-push";
const UNREAD: &str = "default branch could not be read";

// CA-017 e CA-018: a leitura da branch padrão pelo `gh`, com o runner falso

fn repo(workflow: Option<&str>) -> TempDir {
    let dir = TempDir::new().expect("repo directory is created");
    fs::write(
        dir.path().join(".omhl.toml"),
        "version = 1\ntenant = \"acme\"\n",
    )
    .expect("marker is written");
    if let Some(text) = workflow {
        fs::create_dir_all(dir.path().join(".omhl")).expect("the .omhl folder is created");
        fs::write(dir.path().join(".omhl/workflow.toml"), text).expect("workflow is written");
    }
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Test Author"],
        &["config", "user.email", "author@example.com"],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
        &["add", "-A"],
        &["commit", "-q", "-m", "base"],
    ] {
        git_ok(dir.path(), args);
    }

    dir
}

fn view_argv() -> Vec<String> {
    [
        "gh",
        "repo",
        "view",
        "github.com/acme/widgets",
        "--json",
        "defaultBranchRef",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn report(exit: i32, stdout: &str, timed_out: bool) -> Result<RunReport, RunError> {
    Ok(RunReport {
        exit_code: Some(exit),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out,
        truncated: false,
        duration: Duration::ZERO,
    })
}

fn trunk() -> Result<RunReport, RunError> {
    report(0, "{\"defaultBranchRef\":{\"name\":\"trunk\"}}\n", false)
}

fn gh_answering(response: Result<RunReport, RunError>) -> (FakeRunner, Receiver<RunSpec>) {
    FakeRunner::new(vec![(view_argv(), response)])
}

fn guard(cwd: &Path, command: &str, runner: &FakeRunner) -> GuardOutcome {
    let event = BashEvent {
        cwd: cwd.to_path_buf(),
        command: command.to_owned(),
    };
    let access = ProviderAccess {
        runner,
        timeout: SMALL_TIMEOUT,
    };

    command_guard(&event, &access).expect("the guard answers")
}

fn write(cwd: &Path, owner: &str, branch: Option<&str>, runner: &FakeRunner) -> GuardOutcome {
    let event = McpWriteEvent {
        cwd: cwd.to_path_buf(),
        owner: Some(owner.to_owned()),
        repo: Some("widgets".to_owned()),
        branch: branch.map(str::to_owned),
    };
    let access = ProviderAccess {
        runner,
        timeout: SMALL_TIMEOUT,
    };

    mcp_write_warning(&event, &access).expect("the warning answers")
}

fn warned(outcome: &GuardOutcome) -> bool {
    outcome
        .context
        .iter()
        .any(|line| line.contains(PUSH_WARNING))
}

fn context_text(outcome: &GuardOutcome) -> String {
    outcome.context.join("\n")
}

fn calls(received: &Receiver<RunSpec>) -> Vec<RunSpec> {
    received.try_iter().collect()
}

// CA-017: aviso de push com a integração ligada

#[test]
fn it_should_call_gh_repo_view_once_for_a_push_to_another_branch() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    guard(dir.path(), "git push origin trunk", &runner);

    let argvs: Vec<Vec<String>> = calls(&received).into_iter().map(|spec| spec.argv).collect();
    assert_eq!(argvs, vec![view_argv()]);
}

#[test]
fn it_should_warn_citing_the_default_branch_when_gh_answers_trunk() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, _received) = gh_answering(trunk());

    let outcome = guard(dir.path(), "git push origin trunk", &runner);

    assert!(warned(&outcome), "{}", context_text(&outcome));
    assert!(context_text(&outcome).contains("trunk"));
    assert_eq!(outcome.deny, None);
}

#[test]
fn it_should_warn_for_a_push_of_main_without_calling_gh() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    let outcome = guard(dir.path(), "git push origin main", &runner);

    assert!(warned(&outcome));
    assert!(calls(&received).is_empty(), "gh is not started");
}

#[test]
fn it_should_warn_covering_main_when_gh_exits_one() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, _received) = gh_answering(report(1, "", false));

    let outcome = guard(dir.path(), "git push origin trunk", &runner);

    assert!(warned(&outcome));
    assert!(
        context_text(&outcome).contains(UNREAD),
        "{}",
        context_text(&outcome)
    );
    assert_eq!(outcome.deny, None);
}

#[test]
fn it_should_warn_covering_main_when_gh_runs_past_the_limit() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, _received) = gh_answering(report(0, "", true));

    let outcome = guard(dir.path(), "git push origin trunk", &runner);

    assert!(warned(&outcome));
    assert!(
        context_text(&outcome).contains(UNREAD),
        "{}",
        context_text(&outcome)
    );
    assert_eq!(outcome.deny, None);
}

#[test]
fn it_should_give_gh_the_time_limit_of_the_caller() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    guard(dir.path(), "git push origin trunk", &runner);

    let timeouts: Vec<Duration> = calls(&received)
        .into_iter()
        .map(|spec| spec.timeout)
        .collect();
    assert_eq!(timeouts, vec![SMALL_TIMEOUT]);
}

#[test]
fn it_should_never_start_gh_and_not_warn_for_another_branch_with_the_integration_off() {
    let dir = repo(None);
    let (runner, received) = gh_answering(trunk());

    let outcome = guard(dir.path(), "git push origin trunk", &runner);

    assert!(calls(&received).is_empty(), "gh is not started");
    assert!(!warned(&outcome));
}

#[test]
fn it_should_remove_the_gh_variables_that_redirect_or_force_a_terminal() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    guard(dir.path(), "git push origin trunk", &runner);

    let spec = calls(&received).remove(0);
    for name in ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"] {
        assert!(
            spec.env_remove.iter().any(|removed| removed == name),
            "{name}"
        );
    }
}

#[test]
fn it_should_keep_gh_token_in_the_gh_environment() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    guard(dir.path(), "git push origin trunk", &runner);

    let spec = calls(&received).remove(0);
    assert!(!spec.env_remove.iter().any(|removed| removed == "GH_TOKEN"));
}

#[test]
fn it_should_disable_the_gh_prompts() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    guard(dir.path(), "git push origin trunk", &runner);

    let spec = calls(&received).remove(0);
    assert!(
        spec.env_set
            .contains(&("GH_PROMPT_DISABLED".to_owned(), "1".to_owned()))
    );
}

// CA-018: as três ferramentas do GitHub MCP que escrevem

#[test]
fn it_should_warn_without_decision_for_a_write_to_main_in_any_project() {
    let dir = repo(None);
    let (runner, _received) = gh_answering(trunk());

    let outcome = write(dir.path(), "acme", Some("main"), &runner);

    assert!(warned(&outcome));
    assert_eq!(outcome.deny, None);
}

#[test]
fn it_should_not_warn_for_a_write_to_another_branch() {
    let dir = repo(None);
    let (runner, _received) = gh_answering(trunk());

    let outcome = write(dir.path(), "acme", Some("feat/x"), &runner);

    assert!(!warned(&outcome));
}

#[test]
fn it_should_warn_for_a_write_without_branch_as_a_possible_default_branch_target() {
    let dir = repo(None);
    let (runner, _received) = gh_answering(trunk());

    let outcome = write(dir.path(), "acme", None, &runner);

    assert!(warned(&outcome));
    assert!(
        context_text(&outcome).contains("default branch"),
        "{}",
        context_text(&outcome)
    );
}

#[test]
fn it_should_warn_for_a_write_to_the_default_branch_of_the_origin_with_the_integration_on() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, _received) = gh_answering(trunk());

    let outcome = write(dir.path(), "acme", Some("trunk"), &runner);

    assert!(warned(&outcome));
}

#[test]
fn it_should_not_start_gh_for_a_write_to_another_repository() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, received) = gh_answering(trunk());

    let outcome = write(dir.path(), "someone-else", Some("trunk"), &runner);

    assert!(!warned(&outcome));
    assert!(calls(&received).is_empty(), "gh is not started");
}

#[test]
fn it_should_still_warn_for_main_on_another_repository() {
    let dir = repo(Some(PROVIDER_ON));
    let (runner, _received) = gh_answering(trunk());

    let outcome = write(dir.path(), "someone-else", Some("main"), &runner);

    assert!(warned(&outcome));
}
