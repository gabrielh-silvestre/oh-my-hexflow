use std::fs;
use std::path::{Path, PathBuf};

use super::{plugin_files, scan_files};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn seeded_file(root: &Path, name: &str, content: &str) -> Vec<String> {
    fs::write(root.join(name), content).expect("write file");

    vec![name.to_owned()]
}

#[test]
fn it_should_find_no_omc_label_outside_the_origin_line_in_the_plugin_files() {
    let root = repo_root();
    let files = plugin_files(&root);

    let found = scan_files(&root, &files);

    assert!(!files.is_empty(), "plugin/ has tracked files");
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_report_the_file_and_line_of_each_seeded_omc_label() {
    let cases = [
        "see oh-my-claudecode for details",
        "the RALPLAN-DR summary",
        "a ralplan review",
        "implemented by the executor",
        "written by the test-engineer",
        "judged by the quality-reviewer",
    ];
    let dir = tempfile::tempdir().expect("temp dir");

    for case in cases {
        let files = seeded_file(dir.path(), "seed.md", &format!("clean\n{case}\n"));

        let found = scan_files(dir.path(), &files);

        assert_eq!(found, ["seed.md:2"], "case: {case}");
    }
}

#[test]
fn it_should_not_report_the_origin_line_of_an_agent() {
    let dir = tempfile::tempdir().expect("temp dir");
    let files = seeded_file(
        dir.path(),
        "agent.md",
        "Ported from oh-my-claudecode 5.7.19 (MIT), agents/critic.md. Adapted.\n",
    );

    let found = scan_files(dir.path(), &files);

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_not_report_a_word_that_only_contains_a_label() {
    let dir = tempfile::tempdir().expect("temp dir");
    let files = seeded_file(dir.path(), "x.md", "the comic and the executors\n");

    let found = scan_files(dir.path(), &files);

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_not_ship_a_license_file_inside_plugin() {
    let files = plugin_files(&repo_root());

    let licenses: Vec<&String> = files
        .iter()
        .filter(|file| {
            let name = file.rsplit('/').next().unwrap_or(file);
            ["LICENSE", "NOTICE", "THIRD_PARTY"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .collect();

    assert!(licenses.is_empty(), "{licenses:?}");
}
