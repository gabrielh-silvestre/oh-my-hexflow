mod support;

use std::fs;
use std::path::{Path, PathBuf};

use ::omhl::ports::git::GitProbe;
use ::omhl::probe::GitTopology;
use support::git::GitRepo;
use tempfile::TempDir;

const WITH_WORKTREE: &str = "feat/with-wt";
const WITHOUT_WORKTREE: &str = "feat/no-wt";

struct Fixture {
    repo: GitRepo,
    _scratch: TempDir,
    linked: PathBuf,
}

/// `main` (principal), `feat/with-wt` (worktree ligada) e `feat/no-wt` (branch sem worktree).
fn fixture() -> Fixture {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    repo.commit_all("initial");
    repo.git(&["branch", WITHOUT_WORKTREE]);
    let scratch = TempDir::new().expect("temp dir");
    let linked = scratch.path().join("wt");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        WITH_WORKTREE,
        &linked.to_string_lossy(),
    ]);

    Fixture {
        repo,
        _scratch: scratch,
        linked,
    }
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).expect("the path exists")
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

// main_dir

#[test]
fn it_should_resolve_the_main_dir_from_inside_the_main_worktree() {
    let fixture = fixture();

    let main_dir = GitProbe::new(fixture.repo.path()).main_dir();

    assert_eq!(
        main_dir.map(|dir: PathBuf| canonical(&dir)),
        Ok(canonical(fixture.repo.path()))
    );
}

#[test]
fn it_should_resolve_the_same_main_dir_from_inside_a_linked_worktree() {
    let fixture = fixture();

    let main_dir = GitProbe::new(&fixture.linked).main_dir();

    assert_eq!(
        main_dir.map(|dir: PathBuf| canonical(&dir)),
        Ok(canonical(fixture.repo.path()))
    );
}

#[test]
fn it_should_resolve_the_same_main_dir_from_a_subdirectory_of_a_linked_worktree() {
    let fixture = fixture();
    let nested = fixture.linked.join("deep/er");
    fs::create_dir_all(&nested).expect("create the subdirectory");

    let main_dir = GitProbe::new(&nested).main_dir();

    assert_eq!(
        main_dir.map(|dir: PathBuf| canonical(&dir)),
        Ok(canonical(fixture.repo.path()))
    );
}

#[test]
fn it_should_fail_to_resolve_the_main_dir_outside_a_git_repository() {
    let outside = TempDir::new().expect("temp dir");

    let main_dir = GitProbe::new(outside.path()).main_dir();

    assert!(main_dir.is_err(), "expected an error, got {main_dir:?}");
}

// branch_exists

#[test]
fn it_should_say_an_existing_local_branch_exists() {
    let fixture = fixture();

    let exists = GitProbe::new(fixture.repo.path()).branch_exists(WITHOUT_WORKTREE);

    assert_eq!(exists, Ok(true));
}

#[test]
fn it_should_say_a_missing_branch_does_not_exist() {
    let fixture = fixture();

    let exists = GitProbe::new(fixture.repo.path()).branch_exists("feat/never-created");

    assert_eq!(exists, Ok(false));
}

#[test]
fn it_should_say_a_name_git_cannot_use_as_a_branch_does_not_exist() {
    let fixture = fixture();

    let exists = GitProbe::new(fixture.repo.path()).branch_exists("feat/../x");

    assert_eq!(exists, Ok(false));
}

#[test]
fn it_should_not_take_a_tag_for_a_local_branch() {
    let fixture = fixture();
    fixture.repo.git(&["tag", "only-a-tag"]);

    let exists = GitProbe::new(fixture.repo.path()).branch_exists("only-a-tag");

    assert_eq!(exists, Ok(false));
}

#[test]
fn it_should_say_the_branch_of_a_linked_worktree_exists() {
    let fixture = fixture();

    let exists = GitProbe::new(fixture.repo.path()).branch_exists(WITH_WORKTREE);

    assert_eq!(exists, Ok(true));
}

#[test]
fn it_should_answer_branch_exists_the_same_from_inside_a_linked_worktree() {
    let fixture = fixture();

    let exists = GitProbe::new(&fixture.linked).branch_exists(WITHOUT_WORKTREE);

    assert_eq!(exists, Ok(true));
}

// worktree_branches

#[test]
fn it_should_list_the_branches_that_have_a_worktree_sorted() {
    let fixture = fixture();

    let branches = GitProbe::new(fixture.repo.path()).worktree_branches();

    assert_eq!(branches, Ok(names(&[WITH_WORKTREE, "main"])));
}

#[test]
fn it_should_not_list_an_existing_branch_that_has_no_worktree() {
    let fixture = fixture();

    let branches = GitProbe::new(fixture.repo.path())
        .worktree_branches()
        .expect("the worktrees are listed");

    assert!(
        !branches.iter().any(|name| name == WITHOUT_WORKTREE),
        "{WITHOUT_WORKTREE} has no worktree but was listed: {branches:?}"
    );
}

#[test]
fn it_should_list_the_same_worktree_branches_from_inside_a_linked_worktree() {
    let fixture = fixture();

    let branches = GitProbe::new(&fixture.linked).worktree_branches();

    assert_eq!(branches, Ok(names(&[WITH_WORKTREE, "main"])));
}

#[test]
fn it_should_not_list_a_detached_worktree_as_a_branch() {
    let fixture = fixture();
    let scratch = TempDir::new().expect("temp dir");
    let detached = scratch.path().join("detached");
    fixture.repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        &detached.to_string_lossy(),
    ]);

    let branches = GitProbe::new(fixture.repo.path()).worktree_branches();

    assert_eq!(branches, Ok(names(&[WITH_WORKTREE, "main"])));
}

// local_branches

#[test]
fn it_should_list_every_local_branch_sorted() {
    let fixture = fixture();

    let branches = GitProbe::new(fixture.repo.path()).local_branches();

    assert_eq!(
        branches,
        Ok(names(&[WITHOUT_WORKTREE, WITH_WORKTREE, "main"]))
    );
}

#[test]
fn it_should_not_list_tags_or_remote_branches_as_local_branches() {
    let fixture = fixture();
    fixture.repo.git(&["tag", "only-a-tag"]);
    fixture
        .repo
        .git(&["update-ref", "refs/remotes/origin/remote-only", "HEAD"]);

    let branches = GitProbe::new(fixture.repo.path()).local_branches();

    assert_eq!(
        branches,
        Ok(names(&[WITHOUT_WORKTREE, WITH_WORKTREE, "main"]))
    );
}
