use crate::app::git_version::{notice, parse_version};
use crate::probe::GitVersion;

fn reported(text: &str) -> GitVersion {
    GitVersion::Reported(text.to_owned())
}

// Leitura da versão

#[test]
fn it_should_read_the_first_two_numeric_components() {
    assert_eq!(parse_version("git version 2.30.2\n"), Some((2, 30)));
    assert_eq!(parse_version("git version 1.8.5"), Some((1, 8)));
    assert_eq!(parse_version("git version 2.31"), Some((2, 31)));
}

#[test]
fn it_should_ignore_a_vendor_suffix_after_the_numbers() {
    assert_eq!(
        parse_version("git version 2.39.2 (Apple Git-143)"),
        Some((2, 39))
    );
    assert_eq!(parse_version("git version 2.43.0.windows.1"), Some((2, 43)));
}

#[test]
fn it_should_not_read_a_text_without_two_numeric_components() {
    assert_eq!(parse_version("git version banana"), None);
    assert_eq!(parse_version("git version 2"), None);
    assert_eq!(parse_version("2.31.0"), None);
    assert_eq!(parse_version(""), None);
}

// Aviso

#[test]
fn it_should_warn_when_the_git_is_missing() {
    let line = notice(&GitVersion::NotFound).expect("a warning");

    assert!(
        line.contains("not found") && line.contains("2.31"),
        "{line}"
    );
    assert!(!line.contains('\n'));
}

#[test]
fn it_should_warn_when_the_git_is_older_than_2_31() {
    for text in [
        "git version 2.30.2",
        "git version 1.8.5",
        "git version 2.9.0",
    ] {
        let line = notice(&reported(text)).expect("a warning");

        assert!(line.contains("too old") && line.contains("2.31"), "{line}");
    }
}

#[test]
fn it_should_not_warn_from_2_31_on() {
    for text in [
        "git version 2.31.0",
        "git version 2.39.2 (Apple Git-143)",
        "git version 2.43.0.windows.1",
        "git version 3.0.0",
    ] {
        assert_eq!(notice(&reported(text)), None, "{text}");
    }
}

#[test]
fn it_should_stay_silent_when_the_version_cannot_be_read() {
    assert_eq!(notice(&GitVersion::Unreadable), None);
    assert_eq!(notice(&reported("git version banana")), None);
}
