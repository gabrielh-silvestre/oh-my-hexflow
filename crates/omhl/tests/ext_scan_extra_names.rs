//! `specs.dir` customizado (decisão g): o nome da pasta sob `.omhl/` chega ao scan como nome extra.
//! Fica fora de `ext_scan.rs` para que aquele arquivo siga compilando com o `scan(root)` atual.

mod support;

use ::omhl::extensions::model::{ExtensionIssue, ExtensionKind};
use ::omhl::extensions::scan::{ScanOutput, scan, scan_with};
use support::ext::ExtTree;

fn summary(issues: &[ExtensionIssue]) -> Vec<(String, &'static str)> {
    issues
        .iter()
        .map(|issue| (issue.path.clone(), issue.code))
        .collect()
}

fn listing(output: &ScanOutput) -> Vec<(ExtensionKind, &str)> {
    output
        .files
        .iter()
        .map(|file| (file.kind, file.slug.as_str()))
        .collect()
}

#[test]
fn it_should_ignore_a_custom_specs_directory_named_by_the_caller() {
    let tree = ExtTree::new();
    tree.file(".omhl/my-specs/wi-dir0__x/part-1.md", "spec text\n");

    let output = scan_with(tree.root(), &["my-specs"]);

    assert_eq!(
        (
            output.files.len(),
            output.errors.len(),
            output.warnings.len()
        ),
        (0, 0, 0)
    );
}

#[test]
fn it_should_not_apply_the_entry_cap_to_a_custom_specs_directory() {
    let tree = ExtTree::new();
    for n in 0..101 {
        tree.dir(&format!(".omhl/my-specs/wi-dir0__{n}"));
    }

    let output = scan_with(tree.root(), &["my-specs"]);

    assert_eq!(
        (
            output.files.len(),
            output.errors.len(),
            output.warnings.len()
        ),
        (0, 0, 0)
    );
}

#[test]
fn it_should_warn_about_a_custom_specs_directory_the_caller_did_not_name() {
    let tree = ExtTree::new();
    tree.dir(".omhl/my-specs");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.warnings),
        vec![(".omhl/my-specs".to_owned(), "unknown-entry")]
    );
}

#[test]
fn it_should_still_warn_about_other_unknown_entries_when_extra_names_are_given() {
    let tree = ExtTree::new();
    tree.dir(".omhl/my-specs")
        .file(".omhl/notes.txt", "notes\n");

    let output = scan_with(tree.root(), &["my-specs"]);

    assert_eq!(
        summary(&output.warnings),
        vec![(".omhl/notes.txt".to_owned(), "unknown-entry")]
    );
}

#[test]
fn it_should_keep_scanning_extensions_when_extra_names_are_given() {
    let tree = ExtTree::new();
    tree.check("a", "blocking = true\n").dir(".omhl/my-specs");

    let output = scan_with(tree.root(), &["my-specs"]);

    assert_eq!(listing(&output), vec![(ExtensionKind::Check, "a")]);
}

#[test]
fn it_should_scan_the_same_with_no_extra_names_as_plain_scan() {
    let tree = ExtTree::new();
    tree.check("a", "blocking = true\n")
        .file(".omhl/notes.txt", "notes\n");

    let plain = scan(tree.root());
    let with_none = scan_with(tree.root(), &[]);

    assert_eq!(
        (listing(&plain), summary(&plain.warnings)),
        (listing(&with_none), summary(&with_none.warnings))
    );
}
