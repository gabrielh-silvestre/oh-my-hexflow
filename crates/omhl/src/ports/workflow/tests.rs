#![cfg(target_os = "linux")]

use crate::ports::workflow::read_workflow;
use crate::project::layout::{EXT_FILE_MAX_BYTES, OMHL_DIR, WORKFLOW_FILE};
use crate::workflow::config::WorkflowConfig;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

const ERROR_CODE: &str = "invalid-workflow-config";

fn workflow_path(root: &Path) -> PathBuf {
    let omhl_dir = root.join(OMHL_DIR);
    fs::create_dir_all(&omhl_dir).unwrap();
    omhl_dir.join(WORKFLOW_FILE)
}

#[test]
fn it_should_read_the_defaults_when_the_workflow_file_is_absent() {
    let root = tempfile::tempdir().unwrap();

    let config = read_workflow(root.path());

    assert_eq!(config, Ok(WorkflowConfig::default()));
}

#[test]
fn it_should_read_the_native_checks_of_the_workflow_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        workflow_path(root.path()),
        "version = 1\n[native]\nchecks = [\"checks.run-tests\"]\n",
    )
    .unwrap();

    let config = read_workflow(root.path()).unwrap();

    assert!(config.native.checks.contains("checks.run-tests"));
}

#[test]
fn it_should_refuse_a_workflow_file_that_is_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target.toml");
    fs::write(&target, "version = 1\n").unwrap();
    symlink(&target, workflow_path(root.path())).unwrap();

    let error = read_workflow(root.path()).unwrap_err();

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_workflow_file_that_is_a_directory() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(workflow_path(root.path())).unwrap();

    let error = read_workflow(root.path()).unwrap_err();

    assert_eq!(error.code, ERROR_CODE);
    assert!(
        error.message.contains("not-regular-file"),
        "{}",
        error.message
    );
}

#[test]
fn it_should_refuse_a_workflow_file_above_the_size_cap() {
    let root = tempfile::tempdir().unwrap();
    let padding = "#".repeat(usize::try_from(EXT_FILE_MAX_BYTES).unwrap() + 1);
    fs::write(
        workflow_path(root.path()),
        format!("version = 1\n{padding}"),
    )
    .unwrap();

    let error = read_workflow(root.path()).unwrap_err();

    assert_eq!(error.code, ERROR_CODE);
}
