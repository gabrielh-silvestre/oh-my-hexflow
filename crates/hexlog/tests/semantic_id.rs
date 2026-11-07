use hexlog::{ErrorCode, SemanticId};

fn dotted(segments: usize, segment: &str) -> String {
    vec![segment; segments].join(".")
}

#[test]
fn it_should_accept_valid_semantic_ids() {
    let at_the_limit = format!("{}.aa", dotted(99, "a"));
    let long_segment = "a".repeat(63);
    let cases = [
        "area.sub.item",
        "a",
        "a.b-c.d1",
        at_the_limit.as_str(),
        long_segment.as_str(),
    ];

    for text in cases {
        let parsed = SemanticId::parse(text).unwrap();

        assert_eq!(parsed.as_str(), text);
    }
}

#[test]
fn it_should_reject_invalid_semantic_ids() {
    let segment_of_64 = "a".repeat(64);
    let too_long = format!("{}.aaa", dotted(99, "a"));
    let cases = [
        "",
        ".a",
        "a.",
        "a..b",
        "A.b",
        "a_b",
        "-a",
        segment_of_64.as_str(),
        too_long.as_str(),
        "á.b",
        "a b",
    ];

    for text in cases {
        let error = SemanticId::parse(text).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidInput, "code for {text:?}");
        assert_eq!(error.details.len(), 1, "details for {text:?}");
        assert_eq!(error.details[0].code, "semantic-id-format", "{text:?}");
        assert_eq!(error.details[0].path, "", "path for {text:?}");
        if text.chars().count() >= 8 {
            assert!(!error.message.contains(text), "message echoes {text:?}");
        }
    }
}
