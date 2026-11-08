mod support;

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ::omhl::projection::write::write;
use ::omhl::projection::{ProjectedFile, ProjectionError, WriteReport};
use support::ext::ExtTree;

const HEADER: &str = "<!-- omhl: generated file, do not edit -->";
const GITIGNORE: &str = "# omhl: generated file, do not edit\n*\n";
const PROJECTION: &str = ".omhl/projection";

fn generated(name: &str, body: &str) -> ProjectedFile {
    ProjectedFile {
        name: name.to_owned(),
        content: format!("{HEADER}\n{body}"),
    }
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn refused(root: &Path, files: &[ProjectedFile]) -> ProjectionError {
    write(root, files).expect_err("write is refused")
}

fn write_within_deadline(
    root: &Path,
    files: &[ProjectedFile],
) -> Result<WriteReport, ProjectionError> {
    let root = root.to_path_buf();
    let files = files.to_vec();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(write(&root, &files));
    });

    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("write returned instead of blocking")
}

// Criação e conteúdo

#[test]
fn it_should_create_the_projection_directory_and_files() {
    let tree = ExtTree::new();
    let files = [generated("a.md", "first\n"), generated("b.md", "second\n")];

    let report = write(tree.root(), &files).expect("write succeeds");

    let projection = tree.root().join(PROJECTION);
    assert!(projection.join("a.md").is_file(), "a.md must exist");
    assert!(projection.join("b.md").is_file(), "b.md must exist");
    assert_eq!(report.written, names(&["a.md", "b.md"]));
}

#[test]
fn it_should_write_the_exact_bytes() {
    let tree = ExtTree::new();
    let file = generated("a.md", "# Café naïve — ship it ✓\n\nsecond line\n");

    write(tree.root(), std::slice::from_ref(&file)).expect("write succeeds");

    let on_disk = tree.read(".omhl/projection/a.md");
    assert_eq!(on_disk, file.content.as_bytes());
}

#[test]
fn it_should_write_the_internal_gitignore() {
    let tree = ExtTree::new();

    write(tree.root(), &[generated("a.md", "body\n")]).expect("write succeeds");

    let gitignore = tree.read(".omhl/projection/.gitignore");
    assert_eq!(gitignore, b"# omhl: generated file, do not edit\n*\n");
}

#[test]
fn it_should_rewrite_a_missing_or_different_gitignore() {
    let tree = ExtTree::new();
    let files = [generated("a.md", "body\n")];
    write(tree.root(), &files).expect("first write succeeds");
    let gitignore = tree.root().join(PROJECTION).join(".gitignore");
    fs::remove_file(&gitignore).expect("gitignore is removed");
    fs::write(&gitignore, "something else\n").expect("gitignore is replaced");
    write(tree.root(), &files).expect("write over a different gitignore succeeds");
    let after_different = fs::read_to_string(&gitignore).expect("gitignore is read");
    fs::remove_file(&gitignore).expect("gitignore is removed again");

    write(tree.root(), &files).expect("write over a missing gitignore succeeds");

    let after_missing = fs::read_to_string(&gitignore).expect("gitignore is read again");
    assert_eq!(
        [after_different, after_missing],
        [GITIGNORE.to_owned(), GITIGNORE.to_owned()]
    );
}

#[test]
fn it_should_make_files_read_only_on_unix() {
    let tree = ExtTree::new();

    write(tree.root(), &[generated("a.md", "body\n")]).expect("write succeeds");

    let metadata = fs::metadata(tree.root().join(PROJECTION).join("a.md")).expect("a.md exists");
    assert_eq!(metadata.permissions().mode() & 0o777, 0o444);
}

// Relatório

#[test]
fn it_should_report_unchanged_when_the_bytes_are_equal() {
    let tree = ExtTree::new();
    let files = [generated("a.md", "body\n")];
    let target = tree.root().join(PROJECTION).join("a.md");
    write(tree.root(), &files).expect("first write succeeds");
    let before = fs::metadata(&target).expect("a.md exists");

    let report = write(tree.root(), &files).expect("second write succeeds");

    let after = fs::metadata(&target).expect("a.md still exists");
    assert_eq!(
        report,
        WriteReport {
            unchanged: names(&["a.md"]),
            ..WriteReport::default()
        }
    );
    assert_eq!(
        (before.ino(), before.modified().expect("mtime is readable")),
        (after.ino(), after.modified().expect("mtime is readable"))
    );
}

#[test]
fn it_should_replace_a_changed_read_only_file() {
    let tree = ExtTree::new();
    write(tree.root(), &[generated("a.md", "old\n")]).expect("first write succeeds");
    let updated = generated("a.md", "new\n");

    let report = write(tree.root(), std::slice::from_ref(&updated)).expect("second write succeeds");

    assert_eq!(report.written, names(&["a.md"]));
    assert_eq!(
        tree.read(".omhl/projection/a.md"),
        updated.content.as_bytes()
    );
}

#[test]
fn it_should_report_written_unchanged_and_removed_sorted() {
    let tree = ExtTree::new();
    write(
        tree.root(),
        &[
            generated("y.md", "orphan\n"),
            generated("t.md", "old\n"),
            generated("m.md", "same\n"),
            generated("p.md", "old\n"),
            generated("d.md", "orphan\n"),
            generated("c.md", "same\n"),
        ],
    )
    .expect("first write succeeds");

    let report = write(
        tree.root(),
        &[
            generated("t.md", "new\n"),
            generated("c.md", "same\n"),
            generated("e.md", "brand new\n"),
            generated("m.md", "same\n"),
            generated("p.md", "new\n"),
        ],
    )
    .expect("second write succeeds");

    assert_eq!(
        report,
        WriteReport {
            written: names(&["e.md", "p.md", "t.md"]),
            unchanged: names(&["c.md", "m.md"]),
            removed: names(&["d.md", "y.md"]),
        }
    );
}

// Órfãos e limpeza

#[test]
fn it_should_remove_generated_orphans() {
    let tree = ExtTree::new();
    write(
        tree.root(),
        &[generated("a.md", "first\n"), generated("b.md", "second\n")],
    )
    .expect("first write succeeds");

    let report =
        write(tree.root(), &[generated("a.md", "first\n")]).expect("second write succeeds");

    assert_eq!(report.removed, names(&["b.md"]));
    assert!(
        !tree.root().join(PROJECTION).join("b.md").exists(),
        "b.md must be gone"
    );
}

#[test]
fn it_should_keep_markdown_files_without_the_header() {
    let tree = ExtTree::new();
    tree.file(".omhl/projection/notes.md", "my own notes\n");

    let report = write(tree.root(), &[generated("a.md", "body\n")]).expect("write succeeds");

    assert!(report.removed.is_empty(), "nothing must be removed");
    assert_eq!(tree.read(".omhl/projection/notes.md"), b"my own notes\n");
}

#[test]
fn it_should_never_remove_gitignore_or_lock() {
    let tree = ExtTree::new();

    write(tree.root(), &[]).expect("write with an empty list succeeds");

    let projection = tree.root().join(PROJECTION);
    assert!(
        projection.join(".gitignore").is_file(),
        ".gitignore must stay"
    );
    assert!(projection.join(".lock").is_file(), ".lock must stay");
}

#[test]
fn it_should_sweep_stale_temporary_files() {
    let tree = ExtTree::new();
    tree.file(".omhl/projection/.omhl-xyz.tmp", "half written");

    write(tree.root(), &[generated("a.md", "body\n")]).expect("write succeeds");

    assert!(
        !tree.root().join(PROJECTION).join(".omhl-xyz.tmp").exists(),
        "the stale temporary file must be gone"
    );
}

// Recusas

#[test]
fn it_should_refuse_a_symlinked_target() {
    let tree = ExtTree::new();
    let outside = tree.root().join("outside.txt");
    tree.file("outside.txt", "precious\n");
    tree.dir(PROJECTION).symlink(
        ".omhl/projection/a.md",
        outside.to_str().expect("utf-8 path"),
    );

    let error = refused(
        tree.root(),
        &[generated("a.md", "body\n"), generated("b.md", "body\n")],
    );

    assert_eq!(error.code, "symlink");
    assert_eq!(tree.read("outside.txt"), b"precious\n");
    assert!(
        !tree.root().join(PROJECTION).join("b.md").exists(),
        "b.md must not be created"
    );
}

#[test]
fn it_should_refuse_symlinked_directories() {
    let omhl_linked = ExtTree::new();
    let elsewhere = omhl_linked.root().join("elsewhere");
    omhl_linked
        .dir("elsewhere")
        .symlink(".omhl", elsewhere.to_str().expect("utf-8 path"));
    let projection_linked = ExtTree::new();
    let inner = projection_linked.root().join("inner");
    projection_linked
        .dir(".omhl")
        .dir("inner")
        .symlink(PROJECTION, inner.to_str().expect("utf-8 path"));
    let files = [generated("a.md", "body\n")];

    let errors = [&omhl_linked, &projection_linked].map(|tree| refused(tree.root(), &files));

    let outcome: Vec<(&str, &str)> = errors
        .iter()
        .map(|error| (error.code, error.path.as_str()))
        .collect();
    assert_eq!(
        outcome,
        vec![("symlink", ".omhl"), ("symlink", ".omhl/projection")]
    );
    assert_eq!(
        fs::read_dir(&elsewhere).expect("elsewhere exists").count(),
        0
    );
    assert_eq!(fs::read_dir(&inner).expect("inner exists").count(), 0);
}

#[test]
fn it_should_refuse_a_target_without_the_header_and_write_nothing() {
    let tree = ExtTree::new();
    tree.file(".omhl/projection/a.md", "written by hand\n");

    let error = refused(
        tree.root(),
        &[generated("a.md", "body\n"), generated("b.md", "body\n")],
    );

    assert_eq!(error.code, "not-generated");
    assert_eq!(tree.read(".omhl/projection/a.md"), b"written by hand\n");
    assert!(
        !tree.root().join(PROJECTION).join("b.md").exists(),
        "b.md must not be created"
    );
}

#[test]
fn it_should_refuse_a_lock_that_is_a_fifo_or_symlink_without_hanging() {
    let fifo_lock = ExtTree::new();
    fifo_lock.fifo(".omhl/projection/.lock");
    let symlink_lock = ExtTree::new();
    let lock_target = symlink_lock.root().join("lock-target");
    symlink_lock.file("lock-target", "").symlink(
        ".omhl/projection/.lock",
        lock_target.to_str().expect("utf-8 path"),
    );
    let files = [generated("a.md", "body\n")];

    let codes = [&fifo_lock, &symlink_lock].map(|tree| {
        write_within_deadline(tree.root(), &files)
            .expect_err("write is refused")
            .code
    });

    assert_eq!(codes, ["not-regular-file", "symlink"]);
}

#[test]
fn it_should_report_projection_busy_when_the_lock_is_held() {
    let tree = ExtTree::new();
    tree.dir(PROJECTION);
    let held = fs::File::create(tree.root().join(PROJECTION).join(".lock")).expect("lock opens");
    held.try_lock().expect("the test takes the lock");

    let error = write_within_deadline(tree.root(), &[generated("a.md", "body\n")])
        .expect_err("write is refused");

    assert_eq!(error.code, "projection-busy");
    assert!(
        !tree.root().join(PROJECTION).join("a.md").exists(),
        "a.md must not be created"
    );
}

#[test]
fn it_should_refuse_a_directory_with_more_than_1100_entries() {
    let tree = ExtTree::new();
    for index in 0..1_101 {
        tree.file(&format!(".omhl/projection/e{index:04}.txt"), "");
    }

    let error = refused(tree.root(), &[generated("a.md", "body\n")]);

    assert_eq!(error.code, "too-many-entries");
}
