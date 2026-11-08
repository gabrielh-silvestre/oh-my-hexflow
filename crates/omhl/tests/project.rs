mod support;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use hexlog::Config;
use omhl::facade::{Omhl, OmhlDeps};
use omhl::project::marker::{Located, Marker, find_marker, parse_marker};
use omhl::project::paths::{default_db_path, prepare_db_file};
use omhl::project::{Env, ProjectError};
use support::omhl::{acme, seed_pull_request};
use support::{FakeProbe, FixedClock, SeqIds, snap, tenant};
use tempfile::TempDir;

const MARKER_FILE: &str = ".omhl.toml";

fn marker_path() -> &'static Path {
    Path::new("/project/.omhl.toml")
}

fn marker_text(tenant_name: &str) -> String {
    format!("version = 1\ntenant = \"{tenant_name}\"\n")
}

fn write_marker(dir: &Path, content: &str) {
    fs::create_dir_all(dir).expect("directory is created");
    fs::write(dir.join(MARKER_FILE), content).expect("marker is written");
}

fn refusal(bytes: &[u8]) -> ProjectError {
    parse_marker(marker_path(), bytes).expect_err("the marker is refused")
}

// Leitura do marcador (pura)

#[test]
fn it_should_parse_a_valid_marker() {
    let bytes = marker_text("acme");

    let marker = parse_marker(marker_path(), bytes.as_bytes());

    assert_eq!(
        marker,
        Ok(Marker {
            version: 1,
            tenant: tenant("acme"),
        })
    );
}

#[test]
fn it_should_refuse_an_unknown_key() {
    let bytes = format!("{}extra = 1\n", marker_text("acme"));

    let error = refusal(bytes.as_bytes());

    assert_eq!(error.code, "invalid-marker");
    assert!(
        error.message.contains("/project/.omhl.toml"),
        "the message names the marker path: {}",
        error.message
    );
    assert!(
        error.message.contains("extra"),
        "the message names the unknown key: {}",
        error.message
    );
}

#[test]
fn it_should_refuse_a_future_version_with_its_own_message() {
    let bytes = "version = 2\ntenant = \"acme\"\n";

    let error = refusal(bytes.as_bytes());

    assert_eq!(error.code, "invalid-marker");
    assert!(
        error.message.contains("does not understand version = 2"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn it_should_refuse_a_tenant_that_breaks_the_name_rule() {
    let uppercase = marker_text("Acme");
    let empty = marker_text("");
    let too_long = marker_text(&"a".repeat(64));

    let codes: Vec<&str> = [uppercase, empty, too_long]
        .iter()
        .map(|text| refusal(text.as_bytes()).code)
        .collect();

    assert_eq!(codes, vec!["invalid-marker"; 3]);
}

#[test]
fn it_should_refuse_a_marker_above_4_kib() {
    let mut bytes = b"version = 1\ntenant = \"acme\"\n#".to_vec();
    bytes.resize(4097, b'x');

    let error = refusal(&bytes);

    assert_eq!(error.code, "invalid-marker");
    assert!(
        error.message.contains("larger than 4096 bytes"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn it_should_report_path_line_and_column_on_a_syntax_error() {
    let bytes = "tenant = acme\n";

    let error = refusal(bytes.as_bytes());

    assert_eq!(error.code, "invalid-marker");
    assert!(
        error.message.contains("/project/.omhl.toml"),
        "the message names the marker path: {}",
        error.message
    );
    assert!(
        error.message.contains("line 1"),
        "the message carries the line: {}",
        error.message
    );
    assert!(
        error.message.contains("column"),
        "the message carries the column: {}",
        error.message
    );
}

#[test]
fn it_should_not_echo_marker_content_in_the_toml_error() {
    let bytes = "version = 1\ntenant = secret-value-123\n";

    let error = refusal(bytes.as_bytes());

    assert_eq!(error.code, "invalid-marker");
    assert!(
        !error.message.contains("secret-value-123"),
        "the message repeats the marker content: {}",
        error.message
    );
    assert!(
        error.message.contains("not valid TOML at line 2, column "),
        "the message carries only line and column: {}",
        error.message
    );
}

// Busca do marcador (sistema de arquivos real)

#[test]
fn it_should_find_the_marker_in_the_start_directory() {
    let dir = TempDir::new().expect("temporary directory is created");
    write_marker(dir.path(), &marker_text("acme"));

    let located = find_marker(dir.path());

    assert_eq!(
        located,
        Ok(Some(Located {
            root: dir.path().to_path_buf(),
            marker: Marker {
                version: 1,
                tenant: tenant("acme"),
            },
        }))
    );
}

#[test]
fn it_should_find_the_marker_in_an_ancestor() {
    let dir = TempDir::new().expect("temporary directory is created");
    let start = dir.path().join("a/b/c");
    fs::create_dir_all(&start).expect("directories are created");
    write_marker(&dir.path().join("a"), &marker_text("acme"));

    let located = find_marker(&start)
        .expect("the search succeeds")
        .expect("a marker is found");

    assert_eq!(located.root, dir.path().join("a"));
}

#[test]
fn it_should_use_the_nearest_marker() {
    let dir = TempDir::new().expect("temporary directory is created");
    write_marker(&dir.path().join("a"), &marker_text("outer"));
    write_marker(&dir.path().join("a/b"), &marker_text("inner"));

    let located = find_marker(&dir.path().join("a/b"))
        .expect("the search succeeds")
        .expect("a marker is found");

    assert_eq!(located.marker.tenant, tenant("inner"));
    assert_eq!(located.root, dir.path().join("a/b"));
}

#[test]
fn it_should_report_no_marker_as_none() {
    let dir = TempDir::new().expect("temporary directory is created");
    let start = dir.path().join("a/b");
    fs::create_dir_all(&start).expect("directories are created");
    assert!(
        start
            .ancestors()
            .all(|ancestor| !ancestor.join(MARKER_FILE).exists()),
        "the premise fails: a marker already exists above the temporary directory"
    );

    let located = find_marker(&start);

    assert_eq!(located, Ok(None));
}

#[test]
fn it_should_stop_at_an_invalid_nearest_marker() {
    let dir = TempDir::new().expect("temporary directory is created");
    write_marker(&dir.path().join("a"), &marker_text("acme"));
    write_marker(&dir.path().join("a/b"), "tenant = acme\n");

    let error = find_marker(&dir.path().join("a/b")).expect_err("the nearest marker is invalid");

    assert_eq!(error.code, "invalid-marker");
}

#[cfg(unix)]
#[test]
fn it_should_refuse_a_marker_that_is_a_symlink() {
    let dir = TempDir::new().expect("temporary directory is created");
    let elsewhere = TempDir::new().expect("other directory is created");
    let target = elsewhere.path().join("real-marker.toml");
    fs::write(&target, marker_text("acme")).expect("target marker is written");
    std::os::unix::fs::symlink(&target, dir.path().join(MARKER_FILE))
        .expect("the symlink is created");

    let error = find_marker(dir.path()).expect_err("the symlink marker is refused");

    assert_eq!(error.code, "invalid-marker");
}

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

// Caminho padrão do banco (puro)

#[test]
fn it_should_use_xdg_data_home_when_absolute() {
    let env = Env {
        xdg_data_home: Some(OsStr::new("/x")),
        home: Some(OsStr::new("/h")),
    };

    let path = default_db_path(&env);

    assert_eq!(path, Ok(PathBuf::from("/x/omhl/omhl.db")));
}

#[test]
fn it_should_ignore_a_relative_or_empty_xdg_data_home() {
    let relative = Env {
        xdg_data_home: Some(OsStr::new("rel")),
        home: Some(OsStr::new("/h")),
    };
    let empty = Env {
        xdg_data_home: Some(OsStr::new("")),
        home: Some(OsStr::new("/h")),
    };

    let paths = [default_db_path(&relative), default_db_path(&empty)];

    let expected = PathBuf::from("/h/.local/share/omhl/omhl.db");
    assert_eq!(paths, [Ok(expected.clone()), Ok(expected)]);
}

#[test]
fn it_should_fail_without_xdg_and_home() {
    let env = Env {
        xdg_data_home: None,
        home: None,
    };

    let error = default_db_path(&env).expect_err("no location can be derived");

    assert_eq!(error.code, "no-database-path");
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
fn it_should_create_the_directory_as_0700_and_the_database_as_0600() {
    let dir = TempDir::new().expect("temporary directory is created");
    let db = dir.path().join("n/omhl/omhl.db");

    prepare_db_file(&db).expect("the database file is prepared");

    assert_eq!(mode_of(&dir.path().join("n")), 0o700);
    assert_eq!(mode_of(&dir.path().join("n/omhl")), 0o700);
    assert_eq!(mode_of(&db), 0o600);
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

#[cfg(unix)]
#[test]
fn it_should_leave_an_existing_database_untouched() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("temporary directory is created");
    let db = dir.path().join("omhl.db");
    fs::write(&db, b"dbbytes").expect("the database file is written");
    fs::set_permissions(&db, fs::Permissions::from_mode(0o644)).expect("the mode is set");

    prepare_db_file(&db).expect("the database file is prepared");

    assert_eq!(fs::read(&db).expect("the file is readable"), b"dbbytes");
    assert_eq!(mode_of(&db), 0o644);
}

#[test]
fn it_should_refuse_a_database_path_that_is_a_directory() {
    let dir = TempDir::new().expect("temporary directory is created");
    let db = dir.path().join("omhl.db");
    fs::create_dir(&db).expect("the directory is created");

    let error = prepare_db_file(&db).expect_err("a directory is not a database file");

    assert_eq!(error.code, "database-file");
    assert!(
        error.message.starts_with(&format!("{}: ", db.display())),
        "the message names the path and the cause: {}",
        error.message
    );
}
