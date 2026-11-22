use crate::ports::specs::{FreezeMarkWrite, SpecStore, SpecStoreError};
use std::fs;
use std::os::unix::fs::symlink;
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

fn seed_parts_of_the_per_file_cap(dir: &Path, count: usize) {
    let folder = dir.join(FOLDER);
    fs::create_dir_all(&folder).unwrap();
    let content = "a".repeat(BYTES_MAX);
    for index in 0..count {
        fs::write(folder.join(format!("parte-{index:03}.md")), &content).unwrap();
    }
}

#[test]
fn it_should_refuse_a_spec_folder_over_the_aggregate_byte_cap() {
    let (dir, store) = store();
    // 257 * 64 KiB passa de 16 MiB por um arquivo.
    seed_parts_of_the_per_file_cap(dir.path(), 257);

    let folder = store.read_folder("w1");

    assert_eq!(
        folder.err(),
        Some(SpecStoreError::Unusable("the spec folder is too large"))
    );
}

#[test]
fn it_should_accept_198_files_of_the_per_file_cap() {
    let (dir, store) = store();
    seed_parts_of_the_per_file_cap(dir.path(), 198);

    let folder = store.read_folder("w1").unwrap().unwrap();

    assert_eq!(folder.parts.len(), 198);
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

#[test]
fn it_should_find_the_mark_in_any_folder_of_the_work_item() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    seed(dir.path(), "wi-w1__other", &[".frozen"]);

    let marked = store.has_freeze_mark("w1");

    assert!(marked);
}

#[test]
fn it_should_not_refuse_the_mark_lookup_when_the_work_item_has_two_folders() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    seed(dir.path(), "wi-w1__other", &["parte-01.md"]);

    let marked = store.has_freeze_mark("w1");

    assert!(!marked);
}

#[test]
fn it_should_treat_a_non_directory_entry_as_without_mark() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    fs::write(dir.path().join("wi-w1__0"), "").unwrap();
    let target = dir.path().join("plain-file");
    fs::write(&target, "").unwrap();
    symlink(&target, dir.path().join("wi-w1__1")).unwrap();

    let marked = store.has_freeze_mark("w1");

    assert!(!marked);
}

#[test]
fn it_should_treat_a_mark_lookup_error_other_than_missing_as_marked() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    symlink("wi-w1__0", dir.path().join("wi-w1__0")).unwrap();

    let marked = store.has_freeze_mark("w1");

    assert!(marked);
}

#[test]
fn it_should_treat_an_unreadable_specs_dir_as_marked() {
    let (dir, store) = store();
    seed_entries(dir.path(), ENTRIES_MAX + 1);

    let marked = store.has_freeze_mark("w1");

    assert!(marked);
}

#[test]
fn it_should_list_only_directories_and_symlinks_as_markable_folders() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);
    fs::write(dir.path().join("wi-w1__0"), "").unwrap();
    symlink(dir.path().join(FOLDER), dir.path().join("wi-w1__1")).unwrap();

    let folders = store.markable_folders("w1");

    assert_eq!(folders, Ok(vec!["wi-w1__1".to_owned(), FOLDER.to_owned()]));
}

#[test]
fn it_should_write_the_mark_once_and_report_already_on_the_second_write() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md"]);

    let writes = [
        store.write_freeze_mark(FOLDER),
        store.write_freeze_mark(FOLDER),
    ];

    assert_eq!(
        writes,
        [
            Ok(FreezeMarkWrite::Written),
            Ok(FreezeMarkWrite::AlreadyMarked)
        ]
    );
    assert!(dir.path().join(FOLDER).join(".frozen").is_file());
}

#[test]
fn it_should_report_already_for_a_mark_that_is_a_symlink_or_a_directory() {
    let (dir, store) = store();
    let folders = ["wi-w1__link", "wi-w2__dir"];
    for folder in folders {
        seed(dir.path(), folder, &[]);
    }
    symlink("nowhere", dir.path().join(folders[0]).join(".frozen")).unwrap();
    fs::create_dir(dir.path().join(folders[1]).join(".frozen")).unwrap();

    let writes = folders.map(|folder| store.write_freeze_mark(folder));

    assert_eq!(writes, [Ok(FreezeMarkWrite::AlreadyMarked); 2]);
}

#[test]
fn it_should_not_write_a_mark_into_a_symlinked_folder() {
    let (dir, store) = store();
    seed(dir.path(), "real", &[]);
    symlink(dir.path().join("real"), dir.path().join(FOLDER)).unwrap();

    let write = store.write_freeze_mark(FOLDER);

    assert_eq!(
        write,
        Err(SpecStoreError::Unusable("the spec folder is a symlink"))
    );
    assert!(!dir.path().join("real").join(".frozen").exists());
}

#[test]
fn it_should_not_count_the_mark_as_a_spec_file() {
    let (dir, store) = store();
    seed(dir.path(), FOLDER, &["parte-01.md", ".frozen"]);

    let spec = store.read_folder("w1").unwrap().unwrap();
    let amendments = store.amendment_names("w1");

    assert_eq!(spec.parts.len(), 1);
    assert!(spec.amendments.is_empty());
    assert_eq!(amendments, Ok(Vec::new()));
}
