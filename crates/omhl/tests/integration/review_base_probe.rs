use crate::support;

use ::omhl::native::review_trigger_paths;
use ::omhl::ports::git::{GitBlob, GitProbe};
use ::omhl::probe::ProbeError;
use ::omhl::project::layout::EXT_FILE_MAX_BYTES;
use support::git::GitRepo;

/// `main` com um commit; o branch `topic` sai dele.
fn repo_on_topic() -> (GitRepo, String) {
    let repo = GitRepo::init();
    repo.write("src/a.rs", "a\n");
    let fork_point = repo.commit_all("base");
    repo.git(&["switch", "-q", "-c", "topic"]);

    (repo, fork_point)
}

fn triggers_after(repo: &GitRepo, change: impl FnOnce(&GitRepo)) -> Vec<String> {
    change(repo);
    repo.commit_all("change");
    let probe = GitProbe::new(repo.path());
    let merge_base = probe
        .merge_base("main")
        .expect("git answers")
        .expect("main resolves");

    review_trigger_paths(&probe.changed_names(&merge_base).expect("git answers"))
}

#[test]
fn it_should_find_the_fork_point_of_the_base_and_head() {
    let (repo, fork_point) = repo_on_topic();
    repo.write("src/b.rs", "b\n");
    repo.commit_all("on topic");

    let merge_base = GitProbe::new(repo.path()).merge_base("main");

    assert_eq!(merge_base, Ok(Some(fork_point)));
}

#[test]
fn it_should_answer_none_for_a_base_that_does_not_resolve() {
    let (repo, _) = repo_on_topic();

    let merge_base = GitProbe::new(repo.path()).merge_base("no-such-ref");

    assert_eq!(merge_base, Ok(None));
}

#[test]
fn it_should_answer_head_as_the_merge_base_when_the_base_is_head() {
    let (repo, head) = repo_on_topic();

    let merge_base = GitProbe::new(repo.path()).merge_base("HEAD");

    assert_eq!(merge_base, Ok(Some(head)));
}

#[test]
fn it_should_list_the_paths_changed_since_the_merge_base() {
    let (repo, fork_point) = repo_on_topic();
    repo.write("src/b.rs", "b\n");
    repo.write(".omhl/workflow.toml", "version = 1\n");
    repo.commit_all("on topic");

    let names = GitProbe::new(repo.path()).changed_names(&fork_point);

    assert_eq!(
        names,
        Ok(vec![
            ".omhl/workflow.toml".to_owned(),
            "src/b.rs".to_owned()
        ])
    );
}

#[test]
fn it_should_list_both_paths_of_a_rename() {
    let (repo, fork_point) = repo_on_topic();
    repo.git(&["mv", "src/a.rs", "src/c.rs"]);
    repo.commit_all("rename");

    let names = GitProbe::new(repo.path()).changed_names(&fork_point);

    assert_eq!(
        names,
        Ok(vec!["src/a.rs".to_owned(), "src/c.rs".to_owned()])
    );
}

#[test]
fn it_should_require_the_review_when_only_the_workflow_file_changed() {
    let (repo, _) = repo_on_topic();

    let triggers = triggers_after(&repo, |repo| {
        repo.write(".omhl/workflow.toml", "version = 1\n")
    });

    assert_eq!(triggers, [".omhl/workflow.toml"]);
}

#[test]
fn it_should_require_the_review_when_only_a_check_toml_changed() {
    let (repo, _) = repo_on_topic();

    let triggers = triggers_after(&repo, |repo| repo.write(".omhl/checks/x.toml", "a = 1\n"));

    assert_eq!(triggers, [".omhl/checks/x.toml"]);
}

#[test]
fn it_should_require_the_review_when_only_a_check_markdown_changed() {
    let (repo, _) = repo_on_topic();

    let triggers = triggers_after(&repo, |repo| repo.write(".omhl/checks/x.md", "text\n"));

    assert_eq!(triggers, [".omhl/checks/x.md"]);
}

#[test]
fn it_should_require_the_review_when_a_check_toml_is_moved_out_of_the_folder() {
    let (repo, _) = repo_on_topic();
    repo.write(".omhl/checks/x.toml", "a = 1\n");
    repo.commit_all("add the check on main");
    repo.git(&["branch", "-f", "main"]);

    let triggers = triggers_after(&repo, |repo| {
        repo.git(&["mv", ".omhl/checks/x.toml", "src/x.toml"]);
    });

    assert_eq!(triggers, [".omhl/checks/x.toml"]);
}

#[test]
fn it_should_not_require_the_review_when_only_a_source_file_changed() {
    let (repo, _) = repo_on_topic();

    let triggers = triggers_after(&repo, |repo| repo.write("src/a.rs", "changed\n"));

    assert!(triggers.is_empty(), "{triggers:?}");
}

fn head_blob(repo: &GitRepo, path: &str) -> Result<Option<GitBlob>, ProbeError> {
    GitProbe::new(repo.path()).read_blob("HEAD", path)
}

#[test]
fn it_should_read_the_mode_size_and_content_of_a_regular_file_at_head() {
    let (repo, _) = repo_on_topic();
    repo.write(".omhl/workflow.toml", "version = 1\n");
    repo.commit_all("add the workflow");

    let blob = head_blob(&repo, ".omhl/workflow.toml");

    assert_eq!(
        blob,
        Ok(Some(GitBlob {
            mode: "100644".to_owned(),
            size: 12,
            content: b"version = 1\n".to_vec(),
        }))
    );
}

#[test]
fn it_should_read_the_committed_version_not_the_working_tree_one() {
    let (repo, _) = repo_on_topic();
    repo.write(".omhl/workflow.toml", "committed\n");
    repo.commit_all("add the workflow");
    repo.write(".omhl/workflow.toml", "edited after the commit\n");

    let blob = head_blob(&repo, ".omhl/workflow.toml");

    assert_eq!(
        blob.map(|blob| blob.map(|blob| blob.content)),
        Ok(Some(b"committed\n".to_vec()))
    );
}

#[test]
fn it_should_report_the_symlink_mode_of_a_linked_file_at_head() {
    let (repo, _) = repo_on_topic();
    // The index entry must point at a blob the object store already holds.
    repo.git(&["hash-object", "-w", "/dev/null"]);
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "120000,e69de29bb2d1d6434b8b29ae775ad8c2e48c5391,.omhl/workflow.toml",
    ]);
    repo.git(&["commit", "-q", "-m", "link"]);

    let blob = head_blob(&repo, ".omhl/workflow.toml");

    assert_eq!(
        blob.map(|blob| blob.map(|blob| blob.mode)),
        Ok(Some("120000".to_owned()))
    );
}

#[test]
fn it_should_report_the_full_size_of_a_file_larger_than_the_extension_limit() {
    let (repo, _) = repo_on_topic();
    let big = "a".repeat(EXT_FILE_MAX_BYTES as usize + 1);
    repo.write(".omhl/workflow.toml", &big);
    repo.commit_all("add a large workflow");

    let blob = head_blob(&repo, ".omhl/workflow.toml");

    assert_eq!(
        blob.map(|blob| blob.map(|blob| blob.size)),
        Ok(Some(EXT_FILE_MAX_BYTES + 1))
    );
}

#[test]
fn it_should_answer_none_for_a_path_that_is_not_in_the_head_commit() {
    let (repo, _) = repo_on_topic();

    let blob = head_blob(&repo, ".omhl/workflow.toml");

    assert_eq!(blob, Ok(None));
}

#[test]
fn it_should_fail_reading_a_blob_when_the_ref_does_not_exist() {
    let (repo, _) = repo_on_topic();

    let blob = GitProbe::new(repo.path()).read_blob("no-such-ref", "src/a.rs");

    assert!(blob.is_err(), "{blob:?}");
}

#[test]
fn it_should_require_the_review_when_a_change_only_tightens_blocking_in_a_check_file() {
    let (repo, _) = repo_on_topic();
    repo.write(".omhl/checks/x.toml", "blocking = false\n");
    repo.commit_all("add the check on main");
    repo.git(&["branch", "-f", "main"]);

    let triggers = triggers_after(&repo, |repo| {
        repo.write(".omhl/checks/x.toml", "blocking = true\n")
    });

    assert_eq!(triggers, [".omhl/checks/x.toml"]);
}
