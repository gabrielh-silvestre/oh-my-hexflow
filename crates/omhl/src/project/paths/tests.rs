use crate::project::paths::prepare_db_file;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

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
