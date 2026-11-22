use super::double_quoted;

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
