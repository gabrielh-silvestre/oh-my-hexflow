use std::fs;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use omhl::ports::specs::{FreezeMarkWrite, SpecStore};
use tempfile::TempDir;

const FOLDER: &str = "wi-w1__login";

// O FIFO nasce aqui porque `src/` não cria processo nem usa `nix` fora da porta de processo.
#[test]
fn it_should_report_already_for_a_mark_that_is_a_fifo() {
    let dir = TempDir::new().expect("temporary directory is created");
    fs::create_dir(dir.path().join(FOLDER)).expect("spec folder is created");
    let made = Command::new("mkfifo")
        .arg(dir.path().join(FOLDER).join(".frozen"))
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo failed");
    let store = SpecStore::new(dir.path().to_path_buf());
    let (sender, receiver) = mpsc::channel();
    // Abrir um FIFO sem escritor bloqueia para sempre, então a gravação roda fora da thread do teste.
    thread::spawn(move || {
        let _ = sender.send(store.write_freeze_mark(FOLDER));
    });

    let write = receiver.recv_timeout(Duration::from_secs(5));

    assert_eq!(write, Ok(Ok(FreezeMarkWrite::AlreadyMarked)));
}
