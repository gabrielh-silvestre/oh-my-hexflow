use super::*;

#[test]
fn it_should_turn_line_breaks_into_spaces_and_drop_controls_and_bidi_marks() {
    let cleaned = one_line(&format!("a\n{NEXT_LABEL} rm -rf\u{1b}[31m"));

    assert_eq!(cleaned, format!("a {NEXT_LABEL} rm -rf[31m"));
}
