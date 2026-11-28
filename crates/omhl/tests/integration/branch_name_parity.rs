//! Paridade do validador de nome de branch com o `git check-ref-format --branch` (CA-013); roda o git, por isso fica fora dos testes de unidade.

use std::path::Path;
use std::process::Command;

use omhl::workflow::branch_policy::is_valid_branch_name;

const INVALID_NAMES: [&str; 25] = [
    "",
    "-x",
    "/a",
    "refs/heads/x",
    "HEAD",
    "a..b",
    "a//b",
    "a@{b",
    "@",
    "a/",
    "a.",
    ".a",
    "a/.b",
    "a.lock",
    "a.lock/b",
    "a b",
    "a\u{1}b",
    "a\u{7f}b",
    "a~b",
    "a^b",
    "a:b",
    "a?b",
    "a*b",
    "a[b",
    "a\\b",
];

const VALID_NAMES: [&str; 5] = ["develop", "a/b", "a.b", "a-b", "x.lockx"];

// Os dois nomes que o git aceita e o omhl recusa de propósito
const OMHL_ONLY_REFUSALS: [&str; 2] = ["refs/heads/x", "@"];

fn git_accepts(repo: &Path, name: &str) -> bool {
    Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .current_dir(repo)
        .output()
        .expect("git runs")
        .status
        .success()
}

fn fixture_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git runs");
    assert!(status.success());
    dir
}

#[test]
fn it_should_agree_with_git_check_ref_format_on_every_name_but_the_two_omhl_refusals() {
    let repo = fixture_repo();
    let names: Vec<&str> = INVALID_NAMES
        .iter()
        .chain(VALID_NAMES.iter())
        .copied()
        .filter(|name| !OMHL_ONLY_REFUSALS.contains(name))
        .collect();

    let disagreements: Vec<&str> = names
        .into_iter()
        .filter(|name| git_accepts(repo.path(), name) != is_valid_branch_name(name))
        .collect();

    assert!(
        disagreements.is_empty(),
        "the validator disagrees with git: {disagreements:?}"
    );
}

#[test]
fn it_should_refuse_refs_heads_x_and_at_although_git_accepts_them() {
    let repo = fixture_repo();

    let git_halves: Vec<bool> = OMHL_ONLY_REFUSALS
        .iter()
        .map(|name| git_accepts(repo.path(), name))
        .collect();
    let omhl_halves: Vec<bool> = OMHL_ONLY_REFUSALS
        .iter()
        .map(|name| is_valid_branch_name(name))
        .collect();

    assert_eq!(git_halves, vec![true, true]);
    assert_eq!(omhl_halves, vec![false, false]);
}
