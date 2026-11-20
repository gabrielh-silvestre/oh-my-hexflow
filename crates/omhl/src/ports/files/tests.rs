//! `open_regular_nonblocking` e `read_regular_utf8`.

use crate::ports::files::{FILE_TOO_LARGE, open_regular_nonblocking, read_regular_utf8};
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[test]
fn it_should_refuse_a_fifo_without_a_writer_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("pipe");
    mkfifo(&fifo, Mode::from_bits_truncate(0o600)).unwrap();
    let (sender, receiver) = mpsc::channel();
    // A abertura de um FIFO sem escritor trava para sempre sem `O_NONBLOCK`: por isso ela roda numa
    // thread e o teste espera com prazo. A thread presa NÃO recebe `join` depois do prazo e morre com
    // o processo de teste; não "corrigir" isso, um `join` penduraria a suíte inteira.
    thread::spawn(move || {
        let _ = sender.send(open_regular_nonblocking(&fifo).map(|_| ()));
    });

    let result = receiver.recv_timeout(Duration::from_secs(5));

    assert_eq!(result, Ok(Err("not-regular-file")));
}

#[test]
fn it_should_refuse_a_symlink_as_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.txt");
    let link = dir.path().join("link.txt");
    fs::write(&target, "text").unwrap();
    symlink(&target, &link).unwrap();

    let result = open_regular_nonblocking(&link);

    assert_eq!(result.err(), Some("symlink"));
}

#[test]
fn it_should_refuse_a_directory_as_not_regular() {
    let dir = tempfile::tempdir().unwrap();

    let result = open_regular_nonblocking(dir.path());

    assert_eq!(result.err(), Some("not-regular-file"));
}

#[test]
fn it_should_open_a_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    fs::write(&path, "text").unwrap();

    let mut file = open_regular_nonblocking(&path).unwrap();

    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "text");
}

#[test]
fn it_should_refuse_a_file_over_the_cap_by_the_descriptor_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    fs::write(&path, "12345").unwrap();

    let result = read_regular_utf8(&path, 4);

    assert_eq!(result, Err(FILE_TOO_LARGE));
}
