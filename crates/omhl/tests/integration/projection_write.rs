use crate::support;

use std::fs;

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ::omhl::projection::write::write;
use ::omhl::projection::{ProjectedFile, ProjectionError, WriteReport};
use support::ext::ExtTree;

const HEADER: &str = "<!-- omhl: generated file, do not edit -->";

const PROJECTION: &str = ".omhl/projection";

fn generated(name: &str, body: &str) -> ProjectedFile {
    ProjectedFile {
        name: name.to_owned(),
        content: format!("{HEADER}\n{body}"),
    }
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
