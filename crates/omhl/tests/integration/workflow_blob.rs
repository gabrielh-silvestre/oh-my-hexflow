use crate::support;

use ::omhl::ports::git::{BlobReason, GitProbe, WorkflowSource, workflow_text};
use ::omhl::project::layout::EXT_FILE_MAX_BYTES;
use support::git::GitRepo;

const PATH: &str = ".omhl/workflow.toml";

/// Deixa o estado no índice e, para o `HEAD`, também no commit.
fn stage(repo: &GitRepo, committed: bool) {
    repo.git(&["add", "-A"]);
    if committed {
        repo.commit_all("workflow");
    }
}

fn read(repo: &GitRepo, committed: bool) -> Result<String, BlobReason> {
    let probe = GitProbe::new(repo.path());
    let source = if committed {
        WorkflowSource::Rev("HEAD")
    } else {
        WorkflowSource::Index
    };

    workflow_text(&probe, source)
}

fn repo_with_base() -> GitRepo {
    let repo = GitRepo::init();
    repo.write("src/a.rs", "a\n");
    repo.commit_all("base");

    repo
}

fn text_of_a_regular_file(committed: bool) -> Result<String, BlobReason> {
    let repo = repo_with_base();
    repo.write(PATH, "version = 1\n");
    stage(&repo, committed);

    read(&repo, committed)
}

fn reason_of_a_link(committed: bool) -> Result<String, BlobReason> {
    let repo = repo_with_base();
    std::fs::create_dir_all(repo.path().join(".omhl")).expect("dir");
    std::os::unix::fs::symlink("../src/a.rs", repo.path().join(PATH)).expect("link");
    stage(&repo, committed);

    read(&repo, committed)
}

fn reason_of_an_oversized_file(committed: bool) -> Result<String, BlobReason> {
    let repo = repo_with_base();
    repo.write(PATH, &"a".repeat(EXT_FILE_MAX_BYTES as usize + 1));
    stage(&repo, committed);

    read(&repo, committed)
}

fn reason_of_an_absent_file(committed: bool) -> Result<String, BlobReason> {
    let repo = repo_with_base();

    read(&repo, committed)
}

fn reason_of_a_non_utf8_file(committed: bool) -> Result<String, BlobReason> {
    let repo = repo_with_base();
    std::fs::create_dir_all(repo.path().join(".omhl")).expect("dir");
    std::fs::write(repo.path().join(PATH), [0xff, 0xfe, 0x00]).expect("write");
    stage(&repo, committed);

    read(&repo, committed)
}

#[test]
fn it_should_return_the_text_of_a_regular_file_at_head() {
    assert_eq!(text_of_a_regular_file(true), Ok("version = 1\n".to_owned()));
}

#[test]
fn it_should_return_the_text_of_a_regular_file_in_the_index() {
    assert_eq!(
        text_of_a_regular_file(false),
        Ok("version = 1\n".to_owned())
    );
}

#[test]
fn it_should_return_the_link_reason_for_a_symbolic_link_at_head() {
    assert_eq!(reason_of_a_link(true), Err(BlobReason::Link));
}

#[test]
fn it_should_return_the_link_reason_for_a_symbolic_link_in_the_index() {
    assert_eq!(reason_of_a_link(false), Err(BlobReason::Link));
}

#[test]
fn it_should_return_the_too_large_reason_for_a_file_over_the_limit_at_head() {
    assert_eq!(reason_of_an_oversized_file(true), Err(BlobReason::TooLarge));
}

#[test]
fn it_should_return_the_too_large_reason_for_a_file_over_the_limit_in_the_index() {
    assert_eq!(
        reason_of_an_oversized_file(false),
        Err(BlobReason::TooLarge)
    );
}

#[test]
fn it_should_return_the_absent_reason_for_a_missing_file_at_head() {
    assert_eq!(reason_of_an_absent_file(true), Err(BlobReason::Absent));
}

#[test]
fn it_should_return_the_absent_reason_for_a_missing_file_in_the_index() {
    assert_eq!(reason_of_an_absent_file(false), Err(BlobReason::Absent));
}

#[test]
fn it_should_return_the_not_utf8_reason_for_a_binary_file_at_head() {
    assert_eq!(reason_of_a_non_utf8_file(true), Err(BlobReason::NotUtf8));
}

#[test]
fn it_should_return_the_not_utf8_reason_for_a_binary_file_in_the_index() {
    assert_eq!(reason_of_a_non_utf8_file(false), Err(BlobReason::NotUtf8));
}

#[test]
fn it_should_read_a_file_at_the_size_limit_as_text() {
    let repo = repo_with_base();
    repo.write(PATH, &"a".repeat(EXT_FILE_MAX_BYTES as usize));
    stage(&repo, true);

    let text = read(&repo, true);

    assert_eq!(text.map(|text| text.len()), Ok(EXT_FILE_MAX_BYTES as usize));
}

#[test]
fn it_should_read_the_staged_version_not_the_working_tree_one() {
    let repo = repo_with_base();
    repo.write(PATH, "staged\n");
    repo.git(&["add", "-A"]);
    repo.write(PATH, "edited after staging\n");

    let text = read(&repo, false);

    assert_eq!(text, Ok("staged\n".to_owned()));
}

#[test]
fn it_should_answer_the_index_blob_of_a_staged_file_with_its_mode_and_content() {
    let repo = repo_with_base();
    repo.write(PATH, "version = 1\n");
    repo.git(&["add", "-A"]);

    let blob = GitProbe::new(repo.path()).read_index_blob(PATH);

    assert_eq!(
        blob.map(|blob| blob.map(|blob| (blob.mode, blob.content))),
        Ok(Some(("100644".to_owned(), b"version = 1\n".to_vec())))
    );
}

#[test]
fn it_should_answer_none_for_a_path_that_is_not_in_the_index() {
    let repo = repo_with_base();

    let blob = GitProbe::new(repo.path()).read_index_blob(PATH);

    assert_eq!(blob, Ok(None));
}
