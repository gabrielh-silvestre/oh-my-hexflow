//! CA-019 com a EMD-034: só o adaptador do GitHub nomeia e inicia o `gh`; o reconhecedor do Bash
//! só o lê como texto de um comando.

use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use std::time::Duration;

use omhl::ports::process::ProcessRunner;
use omhl::ports::provider::GhProvider;
use omhl::provider::{Provider, ProviderError};
use omhl::runner::{CommandRunner, RunError, RunReport, RunSpec};
use regex::Regex;
use support::repo_checks::repo_root;

const ADAPTER: &str = "ports/provider/gh.rs";
const RECOGNIZER: &str = "workflow/shell/git_command.rs";

fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("the source folder is read") {
        let path = entry.expect("the entry is read").path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }

    found
}

fn src() -> PathBuf {
    repo_root().join("crates/omhl/src")
}

/// Os arquivos de produção (fora dos `tests.rs` e de `test_support/`) com uma linha que casa `pattern`.
fn files_matching(pattern: &str) -> Vec<String> {
    let pattern = Regex::new(pattern).expect("the pattern compiles");
    let root = src();
    let mut files: Vec<String> = sources(&root)
        .into_iter()
        .filter(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("under src")
                .to_string_lossy()
                .into_owned();
            !relative.ends_with("tests.rs")
                && !relative.contains("/tests/")
                && !relative.starts_with("test_support/")
        })
        .filter(|path| {
            fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .any(|line| pattern.is_match(line))
        })
        .map(|path| {
            path.strip_prefix(&root)
                .expect("under src")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();

    files
}

#[test]
fn it_should_name_gh_only_in_the_adapter_and_the_bash_recognizer() {
    let files = files_matching(r#"Command::new\("gh"\)|"gh""#);

    assert_eq!(files, [ADAPTER, RECOGNIZER]);
}

#[test]
fn it_should_never_start_gh_from_the_bash_recognizer() {
    let text = fs::read_to_string(src().join(RECOGNIZER)).expect("the recognizer is read");

    assert!(
        !text.contains("Command::new"),
        "the recognizer only reads text"
    );
}

/// Repassa ao `ProcessRunner` real trocando o programa por um script que dorme, sem mexer no `PATH` do teste.
struct SleepingGh {
    script: PathBuf,
}

impl CommandRunner for SleepingGh {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        let mut spec = spec.clone();
        spec.argv[0] = self.script.display().to_string();

        ProcessRunner::new().run(&spec)
    }
}

#[test]
fn it_should_end_a_sleeping_gh_at_the_injected_limit_as_a_timeout() {
    let dir = tempfile::TempDir::new().expect("a folder is created");
    let script = dir.path().join("gh");
    support::git::write_executable(&script, "#!/bin/sh\nsleep 30\n");
    let runner = SleepingGh { script };

    let checked = GhProvider::new(&runner, "github.com", Duration::from_millis(200)).preflight();

    assert_eq!(checked, Err(ProviderError::Timeout));
}

#[test]
fn it_should_start_gh_nowhere_through_a_literal_command() {
    let files = files_matching(r#"Command::new\("gh"\)"#);

    assert_eq!(files, Vec::<String>::new());
}
