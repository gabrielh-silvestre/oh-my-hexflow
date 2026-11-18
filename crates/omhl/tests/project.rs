mod support;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use hexlog::Config;
use omhl::facade::{Omhl, OmhlDeps};
use omhl::project::marker::find_marker;
use omhl::project::paths::prepare_db_file;

use support::omhl::{acme, seed_pull_request};
use support::{FakeProbe, FixedClock, SeqIds, snap};
use tempfile::TempDir;

const MARKER_FILE: &str = ".omhl.toml";

#[cfg(unix)]
#[test]
fn it_should_refuse_a_marker_that_is_a_fifo() {
    let dir = TempDir::new().expect("temporary directory is created");
    let fifo = dir.path().join(MARKER_FILE);
    let made = Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo failed");
    let start = dir.path().to_path_buf();
    let (sender, receiver) = mpsc::channel();
    // Opening a FIFO without a writer blocks forever, so the search runs off the test thread.
    thread::spawn(move || {
        let _ = sender.send(find_marker(&start).map_err(|error| error.code));
    });

    let result = receiver.recv_timeout(Duration::from_secs(5));

    assert_eq!(
        result,
        Ok(Err("invalid-marker")),
        "the FIFO marker is refused without being opened"
    );
}

// Preparo do arquivo do banco (sistema de arquivos real)

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .expect("metadata is readable")
        .permissions()
        .mode()
        & 0o777
}

#[cfg(unix)]
#[test]
fn it_should_create_the_wal_files_as_0600_after_a_write() {
    let dir = TempDir::new().expect("temporary directory is created");
    let db = dir.path().join("n/omhl/omhl.db");
    prepare_db_file(&db).expect("the database file is prepared");
    let clock = FixedClock::default();
    let ids = SeqIds::new();
    let probe = FakeProbe::clean(snap('a'));
    let deps = OmhlDeps {
        clock: &clock,
        ids: &ids,
        probe: &probe,
    };
    let mut omhl = Omhl::open(&Config::new(db.clone()), deps).expect("omhl opens");
    omhl.ensure_tenant(&acme()).expect("the tenant is created");
    seed_pull_request(&mut omhl);

    let wal = sibling_with_suffix(&db, "-wal");
    let shm = sibling_with_suffix(&db, "-shm");

    assert_eq!(mode_of(&wal), 0o600);
    assert_eq!(mode_of(&shm), 0o600);
    drop(omhl);
}

#[cfg(unix)]
fn sibling_with_suffix(db: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = db.as_os_str().to_owned();
    name.push(suffix);

    PathBuf::from(name)
}
