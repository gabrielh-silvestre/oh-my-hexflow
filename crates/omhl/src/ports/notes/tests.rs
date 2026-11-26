use crate::ports::notes::birth_notes_folder;
use crate::project::layout::NOTES_GITIGNORE_CONTENT;
use std::fs;

#[test]
fn it_should_write_the_gitignore_when_the_notes_folder_is_born() {
    let root = tempfile::tempdir().unwrap();

    birth_notes_folder(root.path()).unwrap();

    let written = fs::read_to_string(root.path().join(".omhl/notes/.gitignore")).unwrap();
    assert_eq!(written, NOTES_GITIGNORE_CONTENT);
}

#[test]
fn it_should_write_nothing_when_the_notes_folder_already_exists() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".omhl/notes")).unwrap();

    birth_notes_folder(root.path()).unwrap();

    assert!(!root.path().join(".omhl/notes/.gitignore").exists());
}

#[test]
fn it_should_not_recreate_the_gitignore_deleted_between_two_events() {
    let root = tempfile::tempdir().unwrap();
    birth_notes_folder(root.path()).unwrap();
    fs::remove_file(root.path().join(".omhl/notes/.gitignore")).unwrap();

    birth_notes_folder(root.path()).unwrap();

    assert!(!root.path().join(".omhl/notes/.gitignore").exists());
}

#[test]
fn it_should_fail_when_omhl_is_a_regular_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".omhl"), "").unwrap();

    let result = birth_notes_folder(root.path());

    assert!(result.is_err());
}
