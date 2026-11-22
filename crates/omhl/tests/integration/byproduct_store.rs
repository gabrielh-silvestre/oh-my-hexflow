use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::Duration;

use omhl::byproduct::{ByproductError, ByproductErrorKind, ByproductHash, ByproductStore};
use omhl::ports::blob::BlobStore;
use tempfile::TempDir;

// NIST "abc"
const ABC: &[u8] = b"abc";
const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const WITHIN: Duration = Duration::from_secs(5);

fn blob_path(db: &Path) -> PathBuf {
    db.join("blobs").join(&ABC_SHA256[..2]).join(ABC_SHA256)
}

fn abc_hash() -> ByproductHash {
    ByproductHash::parse(ABC_SHA256).expect("a valid hash")
}

/// Abrir um FIFO sem escritor trava para sempre: o `get`/`put` roda fora da thread do teste.
fn within_deadline<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, RecvTimeoutError> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });

    receiver.recv_timeout(WITHIN)
}

fn make_fifo_at_blob_path(db: &Path) {
    let path = blob_path(db);
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    let made = Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo failed");
}

fn kind_of<T>(result: Result<T, ByproductError>) -> Result<(), ByproductErrorKind> {
    result.map(|_| ()).map_err(|error| error.kind)
}

#[test]
fn it_should_report_a_blob_that_is_a_fifo_as_corrupt_without_blocking() {
    let db = TempDir::new().expect("temporary directory is created");
    make_fifo_at_blob_path(db.path());
    let store = BlobStore::new(db.path());

    let result = within_deadline(move || kind_of(store.get(&abc_hash())));

    assert_eq!(result, Ok(Err(ByproductErrorKind::Corrupt)));
}

#[test]
fn it_should_replace_a_fifo_at_the_blob_path_with_a_regular_file_without_blocking() {
    let db = TempDir::new().expect("temporary directory is created");
    make_fifo_at_blob_path(db.path());
    let store = BlobStore::new(db.path());

    let result = within_deadline(move || store.put(ABC).map(|hash| hash.as_str().to_owned()));

    let stored = fs::symlink_metadata(blob_path(db.path())).expect("the blob exists");
    assert_eq!(
        (
            result,
            stored.is_file(),
            fs::read(blob_path(db.path())).ok()
        ),
        (Ok(Ok(ABC_SHA256.to_owned())), true, Some(ABC.to_vec()))
    );
}

#[test]
fn it_should_report_a_symlink_at_the_blob_path_as_corrupt() {
    let db = TempDir::new().expect("temporary directory is created");
    let elsewhere = TempDir::new().expect("temporary directory is created");
    let real = elsewhere.path().join("real");
    fs::write(&real, ABC).expect("the real file is written");
    let path = blob_path(db.path());
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    std::os::unix::fs::symlink(&real, &path).expect("the symlink is created");
    let store = BlobStore::new(db.path());

    let result = kind_of(store.get(&abc_hash()));

    assert_eq!(result, Err(ByproductErrorKind::Corrupt));
}

#[test]
fn it_should_replace_a_symlink_at_the_blob_path_without_writing_through_it() {
    let db = TempDir::new().expect("temporary directory is created");
    let elsewhere = TempDir::new().expect("temporary directory is created");
    let real = elsewhere.path().join("real");
    fs::write(&real, b"untouched").expect("the real file is written");
    let path = blob_path(db.path());
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    std::os::unix::fs::symlink(&real, &path).expect("the symlink is created");
    let store = BlobStore::new(db.path());

    let stored = store.put(ABC).map(|hash| hash.as_str().to_owned());

    let blob = fs::symlink_metadata(&path).expect("the blob exists");
    assert_eq!(
        (
            stored,
            blob.is_file(),
            fs::read(&real).expect("the real file is read")
        ),
        (Ok(ABC_SHA256.to_owned()), true, b"untouched".to_vec())
    );
}

#[test]
fn it_should_return_none_for_a_hash_that_was_never_stored() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = BlobStore::new(db.path());

    let result = store.get(&abc_hash());

    assert_eq!(result, Ok(None));
}

#[test]
fn it_should_read_back_a_stored_blob() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = BlobStore::new(db.path());
    let hash = store.put(ABC).expect("the blob is stored");

    let result = store.get(&hash);

    assert_eq!(result, Ok(Some(ABC.to_vec())));
}
