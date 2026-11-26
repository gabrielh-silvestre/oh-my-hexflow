use crate::support;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use hexlog::Config;
use omhl::app::runtime::Runtime;
use omhl::facade::{Omhl, OmhlDeps};
use omhl::project::Env;
use omhl::project::marker::find_marker;
use omhl::project::paths::prepare_db_file;

use support::omhl::{acme, seed_pull_request};
use support::project::Project;
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
        session: &crate::support::NO_SESSION,
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

// Banco dentro do projeto (`Runtime::discover` com o sistema de arquivos real)

/// Código e mensagem do erro de `discover` para o banco em `db`, ou `None` quando o banco abre.
fn discover_error(project: &Project, db: &Path) -> Option<(String, String)> {
    let env = Env {
        xdg_data_home: Some(project.xdg().as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
        claude_code: false,
        role: None,
        plugin_root: None,
        session: None,
    };

    Runtime::discover(project.dir(), Some(db), &env)
        .err()
        .map(|error| (error.code().to_owned(), error.message().to_owned()))
}

fn error_code(error: Option<(String, String)>) -> Option<String> {
    error.map(|(code, _)| code)
}

#[cfg(unix)]
#[test]
fn it_should_refuse_a_db_reached_through_a_missing_folder_and_dot_dot() {
    let project = Project::new("acme");
    let outside = TempDir::new().expect("temporary directory is created");
    let project_name = project.dir().file_name().expect("the project has a name");
    // `missing/..` some por texto e deixa `outside/<projeto>/x.db`, que é um link para o projeto.
    std::os::unix::fs::symlink(project.dir(), outside.path().join(project_name))
        .expect("the symlink is created");
    let db = outside
        .path()
        .join("missing")
        .join("..")
        .join(project_name)
        .join("x.db");

    let error = discover_error(&project, &db);

    assert_eq!(error_code(error).as_deref(), Some("db-inside-project"));
    assert!(!project.dir().join("x.db").exists());
}

#[cfg(unix)]
#[test]
fn it_should_refuse_a_db_file_that_is_a_symlink_into_the_project() {
    let project = Project::new("acme");
    let outside = TempDir::new().expect("temporary directory is created");
    let db = outside.path().join("x.db");
    std::os::unix::fs::symlink(project.dir().join("real.db"), &db).expect("the symlink is created");

    let error = discover_error(&project, &db);

    assert_eq!(error_code(error).as_deref(), Some("db-inside-project"));
    assert!(!project.dir().join("real.db").exists());
}

#[cfg(unix)]
#[test]
fn it_should_refuse_a_db_folder_that_is_a_symlink_into_the_project() {
    let project = Project::new("acme");
    let outside = TempDir::new().expect("temporary directory is created");
    std::os::unix::fs::symlink(project.dir(), outside.path().join("folder"))
        .expect("the symlink is created");

    let error = discover_error(&project, &outside.path().join("folder").join("x.db"));

    assert_eq!(error_code(error).as_deref(), Some("db-inside-project"));
    assert!(!project.dir().join("x.db").exists());
}

#[cfg(unix)]
#[test]
fn it_should_report_database_file_when_the_db_path_is_a_symlink_loop() {
    let project = Project::new("acme");
    let outside = TempDir::new().expect("temporary directory is created");
    let (first, second) = (outside.path().join("a"), outside.path().join("b"));
    std::os::unix::fs::symlink(&second, &first).expect("the symlink is created");
    std::os::unix::fs::symlink(&first, &second).expect("the symlink is created");

    let error = discover_error(&project, &first.join("x.db"));

    assert_eq!(
        error,
        Some((
            "database-file".to_owned(),
            "the database path cannot be resolved".to_owned()
        ))
    );
}

#[test]
fn it_should_report_database_file_when_the_db_path_ends_in_dot_dot() {
    let project = Project::new("acme");
    let outside = TempDir::new().expect("temporary directory is created");
    fs::create_dir(outside.path().join("sub")).expect("the folder is created");

    let error = discover_error(&project, &outside.path().join("sub").join(".."));

    assert_eq!(error_code(error).as_deref(), Some("database-file"));
}

#[test]
fn it_should_open_a_db_outside_the_project() {
    let project = Project::new("acme");

    let error = discover_error(&project, &project.db_path());

    assert_eq!(error, None);
}
