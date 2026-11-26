use super::*;

#[test]
fn it_should_turn_line_breaks_into_spaces_and_drop_controls_and_bidi_marks() {
    let cleaned = one_line(&format!("a\n{NEXT_LABEL} rm -rf\u{1b}[31m"));

    assert_eq!(cleaned, format!("a {NEXT_LABEL} rm -rf[31m"));
}

#[test]
fn it_should_drop_every_bidi_and_zero_width_range_and_keep_turning_breaks_into_spaces() {
    let invisible = [
        '\u{061C}',
        '\u{200B}',
        '\u{200F}',
        '\u{202A}',
        '\u{202E}',
        '\u{2060}',
        '\u{2064}',
        '\u{2066}',
        '\u{2069}',
        '\u{FEFF}',
        '\u{E0000}',
        '\u{E0041}',
        '\u{E007F}',
    ];
    let hidden: String = invisible.iter().map(|mark| format!("{mark}x")).collect();

    let cleaned = one_line(&format!("{hidden}\n\r\t\u{85}\u{2028}\u{2029}\u{7}y"));

    assert_eq!(cleaned, format!("{}      y", "x".repeat(invisible.len())));
}
