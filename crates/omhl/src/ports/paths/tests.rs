//! `resolve_physical`: o caminho sem link simbólico, com ou sem o alvo existente.

use crate::ports::paths::resolve_physical;
use std::fs;
use std::os::unix::fs::symlink;

const HOPS_BEYOND_THE_LIMIT: usize = 41;

#[test]
fn it_should_resolve_the_existing_ancestor_and_keep_the_missing_tail() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir(base.join("real")).unwrap();
    symlink(base.join("real"), base.join("alias")).unwrap();

    let resolved = resolve_physical(&base.join("alias/missing/file.md"));

    assert_eq!(resolved, Ok(base.join("real/missing/file.md")));
}

#[test]
fn it_should_follow_a_dangling_final_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir(base.join("real")).unwrap();
    symlink(base.join("real/not-yet.md"), base.join("alias.md")).unwrap();

    let resolved = resolve_physical(&base.join("alias.md"));

    assert_eq!(resolved, Ok(base.join("real/not-yet.md")));
}

#[test]
fn it_should_stop_after_forty_hops() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    for hop in 0..HOPS_BEYOND_THE_LIMIT {
        symlink(
            base.join(format!("link-{}", hop + 1)),
            base.join(format!("link-{hop}")),
        )
        .unwrap();
    }

    let resolved = resolve_physical(&base.join("link-0"));

    assert_eq!(resolved, Err("too-many-hops"));
}
