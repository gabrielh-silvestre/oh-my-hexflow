use crate::support;

use ::omhl::native::review_trigger_paths;
use ::omhl::ports::git::GitProbe;
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
