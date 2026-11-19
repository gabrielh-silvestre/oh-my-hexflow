#![cfg(target_os = "linux")]

use crate::ports::process::ProcessRunner;
use crate::runner::CommandRunner;
use crate::runner::RunSpec;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

const TWENTY_SECS: Duration = Duration::from_secs(20);

fn spec(argv: &[&str], cwd: &Path, timeout: Duration) -> RunSpec {
    RunSpec {
        argv: argv.iter().map(|item| (*item).to_owned()).collect(),
        cwd: cwd.to_path_buf(),
        project_root: cwd.to_path_buf(),
        timeout,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        max_stream_bytes: 4_194_304,
    }
}

fn write_executable(path: &Path, mode: u32) {
    fs::write(path, "#!/bin/sh\n").expect("the program is written");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("the mode is set");
}

#[test]
fn it_should_resolve_the_program_through_the_path_the_command_will_see() {
    let work = TempDir::new().expect("work directory is created");
    let bin = TempDir::new().expect("bin directory is created");
    write_executable(&bin.path().join("mytool"), 0o755);
    let mut run_spec = spec(&["mytool", "--flag"], work.path(), TWENTY_SECS);
    run_spec.env_set = vec![("PATH".to_owned(), bin.path().display().to_string())];

    let resolved = ProcessRunner::new().resolve_program(&run_spec);

    assert_eq!(resolved, Some(bin.path().join("mytool")));
}

#[test]
fn it_should_skip_a_non_executable_file_when_resolving_the_program() {
    let work = TempDir::new().expect("work directory is created");
    let first = TempDir::new().expect("first directory is created");
    let second = TempDir::new().expect("second directory is created");
    write_executable(&first.path().join("mytool"), 0o644);
    write_executable(&second.path().join("mytool"), 0o755);
    let mut run_spec = spec(&["mytool"], work.path(), TWENTY_SECS);
    run_spec.env_set = vec![(
        "PATH".to_owned(),
        format!("{}:{}", first.path().display(), second.path().display()),
    )];

    let resolved = ProcessRunner::new().resolve_program(&run_spec);

    assert_eq!(resolved, Some(second.path().join("mytool")));
}

#[test]
fn it_should_resolve_a_program_with_a_slash_against_the_cwd() {
    let work = TempDir::new().expect("work directory is created");
    write_executable(&work.path().join("local-tool"), 0o755);
    let run_spec = spec(&["./local-tool"], work.path(), TWENTY_SECS);

    let resolved = ProcessRunner::new().resolve_program(&run_spec);

    assert_eq!(resolved, Some(work.path().join("local-tool")));
}

#[test]
fn it_should_not_resolve_a_program_that_is_not_on_the_path() {
    let work = TempDir::new().expect("work directory is created");
    let mut run_spec = spec(&["omhl-no-such-program"], work.path(), TWENTY_SECS);
    run_spec.env_set = vec![("PATH".to_owned(), work.path().display().to_string())];

    let resolved = ProcessRunner::new().resolve_program(&run_spec);

    assert_eq!(resolved, None);
}
