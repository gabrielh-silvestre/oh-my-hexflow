use std::fs;

use tempfile::TempDir;

use super::{ExcludeOutcome, ensure_worktree_exclude};

#[test]
fn it_should_write_nothing_for_a_worktree_dir_outside_the_main_directory() {
    let main = TempDir::new().expect("main directory is created");
    let outside = TempDir::new().expect("outside directory is created");
    let common = main.path().join(".git");

    let outcome = ensure_worktree_exclude(main.path(), &common, outside.path());

    assert!(matches!(outcome, Ok(ExcludeOutcome::OutsideMainDir)));
    assert!(
        fs::metadata(common.join("info/exclude")).is_err(),
        "nothing is written"
    );
}
