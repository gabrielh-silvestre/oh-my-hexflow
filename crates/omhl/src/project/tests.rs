use crate::project::Env;
use crate::project::paths::default_db_path;
use std::ffi::OsStr;
use std::path::PathBuf;

// Caminho padrão do banco (puro)

#[test]
fn it_should_use_xdg_data_home_when_absolute() {
    let env = Env {
        xdg_data_home: Some(OsStr::new("/x")),
        home: Some(OsStr::new("/h")),
        git_index_file: None,
        parent_pid: 1,
        claude_code: false,
        role: None,
        plugin_root: None,
        claude: Default::default(),
        session: None,
    };

    let path = default_db_path(&env);

    assert_eq!(path, Ok(PathBuf::from("/x/omhl/omhl.db")));
}

#[test]
fn it_should_ignore_a_relative_or_empty_xdg_data_home() {
    let relative = Env {
        xdg_data_home: Some(OsStr::new("rel")),
        home: Some(OsStr::new("/h")),
        git_index_file: None,
        parent_pid: 1,
        claude_code: false,
        role: None,
        plugin_root: None,
        claude: Default::default(),
        session: None,
    };
    let empty = Env {
        xdg_data_home: Some(OsStr::new("")),
        home: Some(OsStr::new("/h")),
        git_index_file: None,
        parent_pid: 1,
        claude_code: false,
        role: None,
        plugin_root: None,
        claude: Default::default(),
        session: None,
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
        git_index_file: None,
        parent_pid: 1,
        claude_code: false,
        role: None,
        plugin_root: None,
        claude: Default::default(),
        session: None,
    };

    let error = default_db_path(&env).expect_err("no location can be derived");

    assert_eq!(error.code, "no-database-path");
}
