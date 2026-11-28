use super::{SCRIPTS, double_quoted};
use crate::mechanical::RUNNABLE_EVENTS;

#[test]
fn it_should_escape_a_double_quote() {
    let quoted = double_quoted(r#"a"b"#);

    assert_eq!(quoted, r#"a\"b"#);
}

#[test]
fn it_should_escape_a_dollar_sign() {
    let quoted = double_quoted("a$b");

    assert_eq!(quoted, r"a\$b");
}

#[test]
fn it_should_escape_a_backtick() {
    let quoted = double_quoted("a`b");

    assert_eq!(quoted, "a\\`b");
}

#[test]
fn it_should_escape_a_backslash() {
    let quoted = double_quoted(r"a\b");

    assert_eq!(quoted, r"a\\b");
}

#[test]
fn it_should_keep_the_characters_the_shell_does_not_interpret_inside_double_quotes() {
    let quoted = double_quoted("/a b/c'd");

    assert_eq!(quoted, "/a b/c'd");
}

#[test]
fn it_should_bind_each_hook_event_to_an_event_of_the_runnable_list() {
    let events: Vec<&str> = SCRIPTS
        .iter()
        .filter_map(|(_, command)| command.split("--event ").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();

    assert_eq!(events, ["before-commit", "before-commit", "before-push"]);
    assert!(events.iter().all(|event| RUNNABLE_EVENTS.contains(event)));
}

#[test]
fn it_should_run_the_pre_push_hook_on_before_push() {
    let command = SCRIPTS
        .iter()
        .find(|(name, _)| *name == "pre-push")
        .map(|(_, command)| *command);

    assert_eq!(command, Some("check run --event before-push --pre-push"));
}

#[test]
fn it_should_install_a_post_merge_script_that_passes_the_squash_flag() {
    let command = SCRIPTS
        .iter()
        .find(|(name, _)| *name == "post-merge")
        .map(|(_, command)| *command);

    assert_eq!(command, Some("hooks post-merge \"$1\" >/dev/null"));
}

#[test]
fn it_should_install_five_scripts() {
    let names: Vec<&str> = SCRIPTS.iter().map(|(name, _)| *name).collect();

    assert_eq!(
        names,
        [
            "pre-commit",
            "pre-merge-commit",
            "pre-push",
            "post-commit",
            "post-merge"
        ]
    );
}
