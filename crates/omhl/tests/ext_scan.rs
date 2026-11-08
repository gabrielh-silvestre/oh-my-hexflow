mod support;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ::omhl::extensions::model::{ExtensionIssue, ExtensionKind, RawExtension};
use ::omhl::extensions::scan::{ScanOutput, scan};
use support::ext::ExtTree;

fn summary(issues: &[ExtensionIssue]) -> Vec<(String, &'static str)> {
    issues
        .iter()
        .map(|issue| (issue.path.clone(), issue.code))
        .collect()
}

fn at(path: &str, code: &'static str) -> (String, &'static str) {
    (path.to_owned(), code)
}

fn listing(output: &ScanOutput) -> Vec<(ExtensionKind, &str)> {
    output
        .files
        .iter()
        .map(|file| (file.kind, file.slug.as_str()))
        .collect()
}

fn assert_scanned_nothing(output: &ScanOutput) {
    assert!(output.files.is_empty(), "files must be empty");
    assert!(output.errors.is_empty(), "errors must be empty");
    assert!(output.warnings.is_empty(), "warnings must be empty");
}

// Árvore vazia

#[test]
fn it_should_return_empty_output_when_the_omhl_directory_is_missing() {
    let tree = ExtTree::new();

    let output = scan(tree.root());

    assert_scanned_nothing(&output);
}

#[test]
fn it_should_return_empty_output_when_the_directories_are_empty() {
    let tree = ExtTree::new();
    tree.dir(".omhl/checks").dir(".omhl/guidelines");

    let output = scan(tree.root());

    assert_scanned_nothing(&output);
}

// Leitura

#[test]
fn it_should_read_tomls_with_and_without_sibling_markdown() {
    let tree = ExtTree::new();
    tree.check("a", "blocking = true\n")
        .markdown("checks", "a", "Long text\n")
        .check("b", "blocking = false\n");

    let output = scan(tree.root());

    assert_eq!(
        output.files,
        vec![
            RawExtension {
                kind: ExtensionKind::Check,
                slug: "a".to_owned(),
                toml_path: ".omhl/checks/a.toml".to_owned(),
                toml: "blocking = true\n".to_owned(),
                markdown_path: Some(".omhl/checks/a.md".to_owned()),
                markdown: Some("Long text\n".to_owned()),
            },
            RawExtension {
                kind: ExtensionKind::Check,
                slug: "b".to_owned(),
                toml_path: ".omhl/checks/b.toml".to_owned(),
                toml: "blocking = false\n".to_owned(),
                markdown_path: None,
                markdown: None,
            },
        ]
    );
}

#[test]
fn it_should_sort_files_by_kind_then_slug() {
    let tree = ExtTree::new();
    tree.guideline("a", "text = \"x\"\n")
        .check("z", "blocking = true\n")
        .check("b", "blocking = true\n");

    let output = scan(tree.root());

    assert_eq!(
        listing(&output),
        vec![
            (ExtensionKind::Check, "b"),
            (ExtensionKind::Check, "z"),
            (ExtensionKind::Guideline, "a"),
        ]
    );
}

#[test]
fn it_should_ignore_the_projection_directory() {
    let tree = ExtTree::new();
    tree.file(".omhl/projection/x.md", "generated\n")
        .file(".omhl/projection/.lock", "");

    let output = scan(tree.root());

    assert_scanned_nothing(&output);
}

// Avisos

#[test]
fn it_should_warn_unknown_entry_in_the_omhl_root() {
    let tree = ExtTree::new();
    tree.file(".omhl/notes.txt", "notes\n").dir(".omhl/other");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.warnings),
        vec![
            at(".omhl/notes.txt", "unknown-entry"),
            at(".omhl/other", "unknown-entry"),
        ]
    );
}

#[test]
fn it_should_warn_orphan_markdown() {
    let tree = ExtTree::new();
    tree.markdown("checks", "x", "Lonely text\n");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.warnings),
        vec![at(".omhl/checks/x.md", "orphan-markdown")]
    );
    assert!(
        output.files.is_empty(),
        "an orphan markdown is not an extension"
    );
}

#[test]
fn it_should_warn_hidden_file_ignored() {
    let tree = ExtTree::new();
    tree.file(".omhl/checks/.gitkeep", "")
        .file(".omhl/checks/.a.toml.swp", "swap");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.warnings),
        vec![
            at(".omhl/checks/.a.toml.swp", "hidden-file-ignored"),
            at(".omhl/checks/.gitkeep", "hidden-file-ignored"),
        ]
    );
    assert!(output.errors.is_empty(), "hidden files are not errors");
}

// Erros de nome e de tipo de entrada

#[test]
fn it_should_reject_bad_file_names() {
    let tree = ExtTree::new();
    tree.check("Bad Name", "blocking = true\n")
        .file(".omhl/checks/x.yaml", "blocking: true\n")
        .check("-a", "blocking = true\n");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.errors),
        vec![
            at(".omhl/checks/-a.toml", "bad-file-name"),
            at(".omhl/checks/Bad Name.toml", "bad-file-name"),
            at(".omhl/checks/x.yaml", "bad-file-name"),
        ]
    );
}

#[test]
fn it_should_reject_a_symlink() {
    let tree = ExtTree::new();
    tree.file("outside.toml", "blocking = true\n")
        .symlink(".omhl/checks/a.toml", "../../outside.toml");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl/checks/a.toml", "symlink")]
    );
    assert!(output.files.is_empty(), "the link target must not be read");
}

#[test]
fn it_should_reject_a_directory_named_like_a_file() {
    let tree = ExtTree::new();
    tree.dir(".omhl/checks/x.toml");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl/checks/x.toml", "not-regular-file")]
    );
}

#[test]
fn it_should_reject_a_fifo_without_blocking() {
    let tree = ExtTree::new();
    tree.fifo(".omhl/checks/p.toml");
    let root = tree.root().to_path_buf();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(scan(&root));
    });

    let output = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("scan returned instead of blocking on the FIFO");

    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl/checks/p.toml", "not-regular-file")]
    );
}

// Conteúdo e tetos

#[test]
fn it_should_reject_a_file_over_64_kib() {
    let tree = ExtTree::new();
    tree.check("a", &"a".repeat(65_536))
        .check("b", &"a".repeat(65_537));

    let output = scan(tree.root());

    assert_eq!(listing(&output), vec![(ExtensionKind::Check, "a")]);
    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl/checks/b.toml", "file-too-large")]
    );
}

#[test]
fn it_should_reject_a_non_utf8_file() {
    let tree = ExtTree::new();
    tree.file(".omhl/checks/a.toml", b"SECRET-MARKER-9f3\xff\xfe");

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl/checks/a.toml", "not-utf8")]
    );
    let message = output.errors[0].message.to_lowercase();
    assert!(!message.contains("secret-marker-9f3"), "message: {message}");
    assert!(!message.contains('\u{fffd}'), "message: {message}");
    assert!(!message.contains("0xff"), "message: {message}");
    assert!(!message.contains("\\xff"), "message: {message}");
}

#[test]
fn it_should_report_a_root_with_more_than_100_entries() {
    let tree = ExtTree::new();
    tree.check("a", "blocking = true\n");
    for index in 0..100 {
        tree.file(&format!(".omhl/n{index:03}.txt"), "");
    }

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl", "too-many-entries")]
    );
    assert!(
        output.files.is_empty(),
        "nothing is read from an over-capped root"
    );
}

#[test]
fn it_should_reject_a_directory_with_more_than_100_entries() {
    let tree = ExtTree::new();
    for index in 0..101 {
        tree.check(&format!("c{index:03}"), "blocking = true\n");
    }
    for index in 0..100 {
        tree.guideline(&format!("g{index:03}"), "text = \"x\"\n");
    }

    let output = scan(tree.root());

    assert_eq!(
        summary(&output.errors),
        vec![at(".omhl/checks", "too-many-entries")]
    );
    assert_eq!(output.files.len(), 100);
    assert!(
        output
            .files
            .iter()
            .all(|file| file.kind == ExtensionKind::Guideline),
        "only the guidelines directory is read"
    );
}
