//! `SpecStore`: as pastas `wi-<id>__<slug>` de um diretório de specs.

use crate::ports::specs::{SpecStore, SpecStoreError};
use std::fs;
use std::path::Path;

const FOLDER: &str = "wi-w1__login";
const ENTRIES_MAX: usize = 10_000;
const BYTES_MAX: usize = 65_536;
const TWO_FOLDERS: SpecStoreError =
    SpecStoreError::Unusable("the work item has more than one spec folder");
const TOO_MANY_ENTRIES: SpecStoreError =
    SpecStoreError::Unusable("the specs folder has too many entries");

fn store() -> (tempfile::TempDir, SpecStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = SpecStore::new(dir.path().to_owned());

    (dir, store)
}

fn seed(dir: &Path, folder: &str, files: &[&str]) {
    let folder = dir.join(folder);
    fs::create_dir_all(&folder).unwrap();
    for file in files {
        fs::write(folder.join(file), "text").unwrap();
    }
}

fn seed_entries(dir: &Path, count: usize) {
    for index in 0..count {
        fs::write(dir.join(format!("entry-{index}")), "").unwrap();
    }
}

#[test]
fn it_should_list_only_the_amendments_of_the_work_item_folder_by_name() {
    let (dir, store) = store();
    seed(
        dir.path(),
        FOLDER,
        &["parte-01.md", "emenda-02.md", "emenda-01.md", "notas.md"],
    );

    let names = store.amendment_names("w1");

    assert_eq!(
        names,
        Ok(vec!["emenda-01.md".to_owned(), "emenda-02.md".to_owned()])
    );
}

#[test]
fn it_should_list_an_amendment_without_reading_its_content() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    fs::write(dir.path().join(FOLDER).join("emenda-01.md"), [0xff, 0xfe]).unwrap();
    fs::create_dir(dir.path().join(FOLDER).join("emenda-02.md")).unwrap();

    let names = store.amendment_names("w1");

    assert_eq!(
        names,
        Ok(vec!["emenda-01.md".to_owned(), "emenda-02.md".to_owned()])
    );
}

#[test]
fn it_should_list_no_amendments_when_the_work_item_has_no_folder() {
    let (dir, store) = store();
    seed(dir.path(), "wi-w2__other", &["emenda-01.md"]);

    let names = store.amendment_names("w1");

    assert_eq!(names, Ok(Vec::new()));
}

#[test]
fn it_should_refuse_to_list_amendments_when_the_work_item_has_two_folders() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["emenda-01.md"]);
    seed(dir.path(), "wi-w1__other", &[]);

    let names = store.amendment_names("w1");

    assert_eq!(names, Err(TWO_FOLDERS));
}

#[test]
fn it_should_refuse_a_second_folder_of_the_work_item_even_when_it_is_empty() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    seed(dir.path(), "wi-w1__zzz", &[]);

    let has_spec = store.has_spec("w1");

    assert_eq!(has_spec, Err(TWO_FOLDERS));
}

#[test]
fn it_should_not_confuse_the_folder_of_another_work_item_with_a_second_folder() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    seed(dir.path(), "wi-w10__login", &["parte-01.md"]);

    let has_spec = store.has_spec("w1");

    assert_eq!(has_spec, Ok(true));
}

#[test]
fn it_should_list_the_folder_files_in_name_order() {
    let (dir, store) = store();
    seed(
        dir.path(),
        FOLDER,
        &["parte-02.md", "emenda-01.md", "parte-01.md"],
    );

    let files = store.folder_files(FOLDER);

    assert_eq!(
        files,
        Ok(vec![
            "emenda-01.md".to_owned(),
            "parte-01.md".to_owned(),
            "parte-02.md".to_owned()
        ])
    );
}

#[test]
fn it_should_accept_a_specs_dir_with_exactly_the_entries_cap() {
    let (dir, store) = store();
    seed_entries(dir.path(), ENTRIES_MAX);

    let has_spec = store.has_spec("w1");

    assert_eq!(has_spec, Ok(false));
}

#[test]
fn it_should_refuse_a_specs_dir_with_one_entry_over_the_cap() {
    let (dir, store) = store();
    seed_entries(dir.path(), ENTRIES_MAX + 1);

    let has_spec = store.has_spec("w1");

    assert_eq!(has_spec, Err(TOO_MANY_ENTRIES));
}

#[test]
fn it_should_flag_a_part_over_the_byte_limit_without_reading_it() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    fs::write(
        dir.path().join(FOLDER).join("parte-02.md"),
        "a".repeat(BYTES_MAX + 1),
    )
    .unwrap();

    let folder = store.read_folder("w1").unwrap().unwrap();

    let flags: Vec<(&str, bool, bool)> = folder
        .parts
        .iter()
        .map(|part| (part.name.as_str(), part.oversized, part.text.is_empty()))
        .collect();
    assert_eq!(
        flags,
        vec![("parte-01.md", false, false), ("parte-02.md", true, true)]
    );
}

#[test]
fn it_should_refuse_a_part_that_is_a_directory() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &[]);
    fs::create_dir(dir.path().join(FOLDER).join("parte-01.md")).unwrap();

    let folder = store.read_folder("w1");

    assert_eq!(
        folder.err(),
        Some(SpecStoreError::Unusable("a spec file cannot be read"))
    );
}

#[test]
fn it_should_tell_whether_a_spec_folder_exists() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &[]);

    let existing = store.folder_exists(FOLDER);
    let missing = store.folder_exists("wi-w1__other");

    assert_eq!((existing, missing), (Ok(true), Ok(false)));
}
