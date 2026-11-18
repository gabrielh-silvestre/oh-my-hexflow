use crate::project::ProjectError;
use crate::project::marker::{Located, Marker, find_marker, parse_marker};
use crate::test_support::tenant;
use std::fs;
use std::path::Path;
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
