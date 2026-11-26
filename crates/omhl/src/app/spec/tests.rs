use std::path::Path;

use super::{is_lock_candidate, needs_git};

#[test]
fn it_should_take_a_note_under_the_omhl_notes_folder_as_a_lock_candidate() {
    let note = Path::new("/p/.omhl/notes/a.md");

    assert!(is_lock_candidate(note));
}

#[test]
fn it_should_not_take_a_path_that_only_resembles_a_note_as_a_lock_candidate() {
    for path in [
        "/p/.omhl/notes-x/a.md",
        "/p/.omhl/note/a.md",
        "/p/.omhl/notes/../a.md",
    ] {
        assert!(!is_lock_candidate(Path::new(path)), "{path}");
    }
}

#[test]
fn it_should_not_spend_git_on_a_note() {
    let note = Path::new("/p/.omhl/notes/a.md");

    assert!(!needs_git(note));
}
