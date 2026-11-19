use crate::app::input::parse_json_text;

// Entrada

#[test]
fn it_should_refuse_an_input_above_1_mib_before_parsing() {
    let bytes = vec![b' '; 1_048_576 + 1];

    let error = parse_json_text(&bytes).expect_err("the input is refused");

    assert_eq!(error.code(), "input-too-large");
}

#[test]
fn it_should_refuse_json_nested_beyond_the_parser_limit() {
    let bytes = "[".repeat(130);

    let error = parse_json_text(bytes.as_bytes()).expect_err("the input is refused");

    assert_eq!(error.code(), "input-too-deep");
}

#[test]
fn it_should_refuse_input_that_is_not_utf8() {
    let bytes = [0xff, 0xfe];

    let error = parse_json_text(&bytes).expect_err("the input is refused");

    assert_eq!(error.code(), "input-not-utf8");
}

#[test]
fn it_should_report_the_position_of_invalid_json_without_echoing_the_input() {
    let text = r#"{"a": SENTINEL_VALUE}"#;

    let error = parse_json_text(text.as_bytes()).expect_err("the input is refused");

    assert_eq!(error.code(), "input-invalid-json");
    assert!(
        error
            .message()
            .starts_with("input is not valid JSON at line 1 column "),
        "unexpected message: {}",
        error.message()
    );
    assert!(!error.to_json().to_string().contains("SENTINEL_VALUE"));
}
