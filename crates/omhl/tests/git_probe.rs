mod support;

use std::time::{Duration, Instant};

use ::omhl::ports::git::GitProbe;
use ::omhl::probe::{ProbeError, ProbeOutcome, SnapshotProbe};
use hexlog::{Snapshot, SnapshotKind};
use support::git::{GitRepo, fake_program, git_in};
use tempfile::TempDir;

fn probe_dir(dir: &std::path::Path) -> Result<ProbeOutcome, ProbeError> {
    GitProbe::new(dir).probe()
}

fn reproducible(sha: &str) -> Result<ProbeOutcome, ProbeError> {
    Ok(ProbeOutcome::Reproducible(
        Snapshot::new(SnapshotKind::GitCommit, sha).expect("a valid commit id"),
    ))
}

fn error_message(result: Result<ProbeOutcome, ProbeError>) -> String {
    result.expect_err("the probe is expected to fail").message
}

fn clean_repo() -> (GitRepo, String) {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    let head = repo.commit_all("initial");
    (repo, head)
}

// Matriz do repositório real

#[test]
fn it_should_report_the_head_commit_when_the_tree_is_clean() {
    let (repo, head) = clean_repo();

    let result = probe_dir(repo.path());

    assert_eq!(result, reproducible(&head));
}

#[test]
fn it_should_report_dirty_for_a_modified_tracked_file() {
    let (repo, _head) = clean_repo();
    repo.write("tracked.txt", "changed\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_a_staged_new_file() {
    let (repo, _head) = clean_repo();
    repo.write("staged.txt", "new\n");
    repo.git(&["add", "staged.txt"]);

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_an_untracked_file() {
    let (repo, _head) = clean_repo();
    repo.write("untracked.txt", "new\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_ignore_a_gitignored_file() {
    let repo = GitRepo::init();
    repo.write(".gitignore", "ignored.txt\n");
    let head = repo.commit_all("initial");
    repo.write("ignored.txt", "noise\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, reproducible(&head));
}

#[test]
fn it_should_judge_the_whole_enclosing_repository_from_a_subdirectory() {
    let repo = GitRepo::init();
    repo.write("sub/inner.txt", "inner\n");
    repo.write("top.txt", "top\n");
    let head = repo.commit_all("initial");
    let sub = repo.path().join("sub");
    let before = probe_dir(&sub);
    repo.write("top.txt", "changed outside sub\n");

    let after = probe_dir(&sub);

    assert_eq!(
        (before, after),
        (reproducible(&head), Ok(ProbeOutcome::Dirty))
    );
}

#[test]
fn it_should_refuse_a_branch_without_commits() {
    let repo = GitRepo::init();

    let result = probe_dir(repo.path());

    assert!(error_message(result).contains("no commits"));
}

#[test]
fn it_should_fail_outside_a_repository() {
    let outside = TempDir::new().expect("temp dir");
    let enclosing = git_in(outside.path(), &["rev-parse", "--git-dir"]);
    assert!(
        !enclosing.status.success(),
        "temp dir is inside a repository"
    );

    let result = probe_dir(outside.path());

    assert!(result.is_err());
}

#[test]
fn it_should_report_dirty_for_an_unresolved_merge_conflict() {
    let repo = GitRepo::init();
    repo.write("shared.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-b", "other"]);
    repo.write("shared.txt", "other side\n");
    repo.commit_all("other");
    repo.git(&["checkout", "main"]);
    repo.write("shared.txt", "main side\n");
    repo.commit_all("main");
    let merge = repo.git_unchecked(&["merge", "other"]);
    assert!(!merge.status.success(), "the merge is expected to conflict");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_a_submodule_with_a_modified_file() {
    let upstream = GitRepo::init();
    upstream.write("lib.txt", "lib\n");
    upstream.commit_all("lib");
    let repo = GitRepo::init();
    repo.write("top.txt", "top\n");
    repo.commit_all("initial");
    let upstream_path = upstream.path().to_string_lossy().into_owned();
    repo.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        &upstream_path,
        "vendor",
    ]);
    repo.commit_all("add submodule");
    repo.write("vendor/lib.txt", "edited inside the submodule\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_not_run_the_fsmonitor_program_of_the_repository_config() {
    let (repo, head) = clean_repo();
    let tools = TempDir::new().expect("temp dir");
    let marker = tools.path().join("fsmonitor-ran");
    let program = fake_program(tools.path(), &format!("touch '{}'", marker.display()));
    repo.git(&["config", "core.fsmonitor", &program.to_string_lossy()]);

    let result = probe_dir(repo.path());

    assert_eq!((result, marker.exists()), (reproducible(&head), false));
}

// Programa falso

#[test]
fn it_should_relay_the_error_text_when_git_exits_nonzero() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(
        tools.path(),
        "echo 'fatal: detected dubious ownership' >&2\nexit 128",
    );

    let result = GitProbe::new(tools.path()).with_program(program).probe();

    assert!(error_message(result).contains("detected dubious ownership"));
}

#[test]
fn it_should_fail_when_the_program_exceeds_the_timeout() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "exec sleep 5");
    let probe = GitProbe::new(tools.path())
        .with_program(program)
        .with_timeout(Duration::from_millis(200));
    let started = Instant::now();

    let result = probe.probe();

    assert!(error_message(result).contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn it_should_time_out_even_when_a_grandchild_holds_the_pipe() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "sleep 5 &\nexit 0");
    let probe = GitProbe::new(tools.path())
        .with_program(program)
        .with_timeout(Duration::from_millis(200));
    let started = Instant::now();

    let result = probe.probe();

    assert!(error_message(result).contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn it_should_truncate_a_long_git_error_message() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(
        tools.path(),
        "head -c 1000 /dev/zero | tr '\\0' Z >&2\nexit 128",
    );

    let result = GitProbe::new(tools.path()).with_program(program).probe();

    assert_eq!(error_message(result).matches('Z').count(), 200);
}

#[test]
fn it_should_fail_on_malformed_output() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "echo garbage");

    let result = GitProbe::new(tools.path()).with_program(program).probe();

    assert!(error_message(result).contains("unexpected git output"));
}

#[test]
fn it_should_not_hang_on_output_larger_than_the_pipe_buffer() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "head -c 2097152 /dev/zero | tr '\\0' a");
    let probe = GitProbe::new(tools.path()).with_program(program);
    let started = Instant::now();

    let result = probe.probe();

    assert!(error_message(result).contains("unexpected git output"));
    assert!(started.elapsed() < Duration::from_secs(5));
}
