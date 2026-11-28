use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use super::GhProvider;
use crate::provider::{Provider, ProviderError, RepoRef};
use crate::runner::{CommandRunner, RunError, RunErrorKind, RunReport, RunSpec};

const LIMIT: Duration = Duration::from_millis(50);

/// Runner local: manda cada `RunSpec` ao `Receiver` do teste e devolve sempre a mesma resposta.
struct Canned {
    answer: Result<RunReport, RunError>,
    sent: Sender<RunSpec>,
}

impl Canned {
    fn new(answer: Result<RunReport, RunError>) -> (Canned, Receiver<RunSpec>) {
        let (sent, seen) = mpsc::channel();

        (Canned { answer, sent }, seen)
    }
}

impl CommandRunner for Canned {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        let _ = self.sent.send(spec.clone());
        self.answer.clone()
    }
}

fn only_spec(seen: &Receiver<RunSpec>) -> RunSpec {
    let specs: Vec<RunSpec> = seen.try_iter().collect();
    assert_eq!(specs.len(), 1, "one gh call");

    specs[0].clone()
}

fn report(stdout: &str, timed_out: bool) -> Result<RunReport, RunError> {
    Ok(RunReport {
        exit_code: Some(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out,
        truncated: false,
        duration: Duration::ZERO,
    })
}

fn widgets() -> RepoRef {
    RepoRef {
        host: "github.com".to_owned(),
        path: vec!["acme".to_owned(), "widgets".to_owned()],
    }
}

fn read(runner: &Canned) -> Result<String, ProviderError> {
    GhProvider::new(runner, "github.com", LIMIT).default_branch(&widgets())
}

#[test]
fn it_should_read_the_default_branch_name_from_the_json_of_gh() {
    let (runner, _seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"trunk\"}}\n",
        false,
    ));

    let branch = read(&runner);

    assert!(matches!(branch.as_deref(), Ok("trunk")));
}

#[test]
fn it_should_call_gh_repo_view_with_the_slug_and_the_json_field() {
    let (runner, seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"main\"}}\n",
        false,
    ));

    let _ = read(&runner);

    assert_eq!(
        only_spec(&seen).argv,
        [
            "gh",
            "repo",
            "view",
            "github.com/acme/widgets",
            "--json",
            "defaultBranchRef"
        ]
    );
}

#[test]
fn it_should_give_gh_the_timeout_of_the_constructor() {
    let (runner, seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"main\"}}\n",
        false,
    ));

    let _ = read(&runner);

    assert_eq!(only_spec(&seen).timeout, LIMIT);
}

#[test]
fn it_should_isolate_the_gh_environment() {
    let (runner, seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"main\"}}\n",
        false,
    ));

    let _ = read(&runner);

    let spec = only_spec(&seen);
    for name in ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"] {
        assert!(
            spec.env_remove.iter().any(|removed| removed == name),
            "{name}"
        );
    }
    assert!(!spec.env_remove.iter().any(|removed| removed == "GH_TOKEN"));
    assert!(
        spec.env_set
            .contains(&("GH_PROMPT_DISABLED".to_owned(), "1".to_owned()))
    );
}

#[test]
fn it_should_answer_timeout_when_gh_runs_past_the_limit() {
    let (runner, _seen) = Canned::new(report("", true));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::Timeout)));
}

#[test]
fn it_should_answer_cli_missing_when_gh_is_not_found() {
    let (runner, _seen) = Canned::new(Err(RunError {
        kind: RunErrorKind::NotFound,
        message: "gh not found".to_owned(),
    }));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::CliMissing)));
}

#[test]
fn it_should_answer_malformed_for_a_json_without_the_branch() {
    let (runner, _seen) = Canned::new(report("{}\n", false));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::Malformed)));
}
