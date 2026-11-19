use crate::support;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ::omhl::extensions::model::ExtensionIssue;
use ::omhl::extensions::scan::scan;
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
