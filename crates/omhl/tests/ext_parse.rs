use omhl::extensions::model::{Extension, ExtensionIssue, ExtensionKind, RawExtension};
use omhl::extensions::parse::parse;
use serde_json::json;

const CHECK_TOML_PATH: &str = ".omhl/checks/lint.toml";
const CHECK_MARKDOWN_PATH: &str = ".omhl/checks/lint.md";
const GUIDELINE_TOML_PATH: &str = ".omhl/guidelines/tone.toml";
const GUIDELINE_MARKDOWN_PATH: &str = ".omhl/guidelines/tone.md";

const CHECK_TOP: &str = "blocking = true\nhow = \"Run lint\"\n";
const CHECK_WHEN: &str = "event = \"pre-commit\"\n";

fn check_toml(top: &str, when: &str) -> String {
    format!("{top}\n[when]\n{when}\n")
}

fn raw_check(toml: &str, markdown: Option<&str>) -> RawExtension {
    RawExtension {
        kind: ExtensionKind::Check,
        slug: "lint".to_owned(),
        toml_path: CHECK_TOML_PATH.to_owned(),
        toml: toml.to_owned(),
        markdown_path: markdown.map(|_| CHECK_MARKDOWN_PATH.to_owned()),
        markdown: markdown.map(str::to_owned),
    }
}

fn raw_guideline(toml: &str, markdown: Option<&str>) -> RawExtension {
    RawExtension {
        kind: ExtensionKind::Guideline,
        slug: "tone".to_owned(),
        toml_path: GUIDELINE_TOML_PATH.to_owned(),
        toml: toml.to_owned(),
        markdown_path: markdown.map(|_| GUIDELINE_MARKDOWN_PATH.to_owned()),
        markdown: markdown.map(str::to_owned),
    }
}

fn parsed(raw: &RawExtension) -> Extension {
    parse(raw).expect("the extension parses")
}

fn issues(raw: &RawExtension) -> Vec<ExtensionIssue> {
    parse(raw).expect_err("the extension is refused")
}

fn codes(raw: &RawExtension) -> Vec<&'static str> {
    issues(raw).iter().map(|issue| issue.code).collect()
}

fn has_toml_position(message: &str) -> bool {
    let Some(rest) = message.strip_prefix("not valid TOML at line ") else {
        return false;
    };
    let Some((line, column)) = rest.split_once(", column ") else {
        return false;
    };

    let digits = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());

    digits(line) && digits(column)
}

#[test]
fn it_should_parse_a_complete_check() {
    let toml = check_toml(
        "blocking = true\nhow = \"Run lint\"\nafter = [\"checks.build\"]\n",
        "event = \"pre-commit\"\nmatcher = \"Bash\"\n",
    );

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.kind, ExtensionKind::Check);
    assert_eq!(extension.id, "checks.lint");
    assert_eq!(
        extension.data,
        json!({
            "blocking": true,
            "how": "Run lint",
            "when": {"vocabulary": "free", "event": "pre-commit", "matcher": "Bash"}
        })
    );
    assert_eq!(extension.after, vec!["checks.build".to_owned()]);
    assert_eq!(extension.path, CHECK_TOML_PATH);
}

#[test]
fn it_should_parse_a_check_without_matcher() {
    let toml = check_toml(CHECK_TOP, CHECK_WHEN);

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(
        extension.data,
        json!({
            "blocking": true,
            "how": "Run lint",
            "when": {"vocabulary": "free", "event": "pre-commit"}
        })
    );
    assert!(extension.data["when"].get("matcher").is_none());
}

#[test]
fn it_should_accept_vocabulary_free_explicitly() {
    let toml = check_toml(CHECK_TOP, "event = \"pre-commit\"\nvocabulary = \"free\"\n");

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.data["when"]["vocabulary"], json!("free"));
}

#[test]
fn it_should_take_how_from_the_markdown_sibling() {
    let toml = check_toml("blocking = true\n", CHECK_WHEN);

    let extension = parsed(&raw_check(&toml, Some("Long\ntext")));

    assert_eq!(extension.data["how"], json!("Long\ntext"));
}

#[test]
fn it_should_reject_how_in_toml_and_markdown_together() {
    let toml = check_toml(CHECK_TOP, CHECK_WHEN);

    let found = codes(&raw_check(&toml, Some("Long text")));

    assert_eq!(found, vec!["text-conflict"]);
}

#[test]
fn it_should_reject_a_check_without_how_anywhere() {
    let toml = check_toml("blocking = true\n", CHECK_WHEN);

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["text-missing"]);
}

#[test]
fn it_should_reject_a_markdown_that_is_only_line_breaks() {
    let toml = check_toml("blocking = true\n", CHECK_WHEN);

    let found = codes(&raw_check(&toml, Some("\n\r\n")));

    assert_eq!(found, vec!["text-missing"]);
}

#[test]
fn it_should_strip_trailing_newlines_from_the_markdown() {
    let toml = check_toml("blocking = true\n", CHECK_WHEN);

    let extension = parsed(&raw_check(&toml, Some("text\n\n")));

    assert_eq!(extension.data["how"], json!("text"));
}

#[test]
fn it_should_normalize_crlf_in_toml_text() {
    let toml = check_toml("blocking = true\nhow = \"a\\r\\nb\"\n", CHECK_WHEN);

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.data["how"], json!("a\nb"));
}

#[test]
fn it_should_normalize_crlf_in_the_markdown() {
    let toml = check_toml("blocking = true\n", CHECK_WHEN);

    let extension = parsed(&raw_check(&toml, Some("a\r\nb\rc")));

    assert_eq!(extension.data["how"], json!("a\nb\nc"));
}

#[test]
fn it_should_normalize_lone_cr_in_event_and_matcher() {
    let toml = check_toml(CHECK_TOP, "event = \"a\\rb\"\nmatcher = \"c\\rd\"\n");

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.data["when"]["event"], json!("a\nb"));
    assert_eq!(extension.data["when"]["matcher"], json!("c\nd"));
}

#[test]
fn it_should_parse_after_into_the_after_list() {
    let toml = check_toml(
        "blocking = true\nhow = \"Run lint\"\nafter = [\"checks.a\", \"checks.b\"]\n",
        CHECK_WHEN,
    );

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(
        extension.after,
        vec!["checks.a".to_owned(), "checks.b".to_owned()]
    );
}

#[test]
fn it_should_reject_duplicate_after() {
    let toml = check_toml(
        "blocking = true\nhow = \"Run lint\"\nafter = [\"checks.a\", \"checks.a\"]\n",
        CHECK_WHEN,
    );

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["value-not-allowed"]);
}

#[test]
fn it_should_reject_after_that_is_not_a_check_id() {
    let other_kind = check_toml(
        "blocking = true\nhow = \"Run lint\"\nafter = [\"guidelines.x\"]\n",
        CHECK_WHEN,
    );
    let no_prefix = check_toml(
        "blocking = true\nhow = \"Run lint\"\nafter = [\"nope\"]\n",
        CHECK_WHEN,
    );

    let other_kind_codes = codes(&raw_check(&other_kind, None));
    let no_prefix_codes = codes(&raw_check(&no_prefix, None));

    assert_eq!(other_kind_codes, vec!["value-not-allowed"]);
    assert_eq!(no_prefix_codes, vec!["value-not-allowed"]);
}

#[test]
fn it_should_reject_a_check_depending_on_itself() {
    let toml = check_toml(
        "blocking = true\nhow = \"Run lint\"\nafter = [\"checks.lint\"]\n",
        CHECK_WHEN,
    );

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["value-not-allowed"]);
}

#[test]
fn it_should_parse_a_guideline() {
    let extension = parsed(&raw_guideline("text = \"Be brief\"\n", None));

    assert_eq!(extension.kind, ExtensionKind::Guideline);
    assert_eq!(extension.id, "guidelines.tone");
    assert_eq!(extension.data, json!({"text": "Be brief"}));
    assert!(extension.after.is_empty());
}

#[test]
fn it_should_take_guideline_text_from_the_markdown() {
    let extension = parsed(&raw_guideline("", Some("Long")));

    assert_eq!(extension.data, json!({"text": "Long"}));
}

#[test]
fn it_should_reject_a_guideline_without_text() {
    let found = codes(&raw_guideline("", None));

    assert_eq!(found, vec!["text-missing"]);
}

#[test]
fn it_should_reject_after_in_a_guideline() {
    let found = codes(&raw_guideline(
        "text = \"x\"\nafter = [\"checks.a\"]\n",
        None,
    ));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_blocking_in_a_guideline() {
    let found = codes(&raw_guideline("text = \"x\"\nblocking = true\n", None));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_invalid_toml_with_line_and_column() {
    let raw = raw_check("blocking = \n", None);

    let found = issues(&raw);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code, "invalid-toml");
    assert_eq!(found[0].path, CHECK_TOML_PATH);
    assert!(
        has_toml_position(&found[0].message),
        "unexpected message: {}",
        found[0].message
    );
    assert!(
        found[0]
            .message
            .starts_with("not valid TOML at line 1, column ")
    );
}

#[test]
fn it_should_not_echo_toml_content_in_the_error() {
    let raw = raw_check("blocking = SECRET-MARKER-9f3\n", None);

    let found = issues(&raw);

    assert!(found.iter().any(|issue| issue.code == "invalid-toml"));
    assert!(
        found
            .iter()
            .all(|issue| !issue.message.contains("SECRET-MARKER-9f3"))
    );
}

#[test]
fn it_should_reject_an_unknown_key() {
    let toml = check_toml(
        "blocking = true\nhow = \"Run lint\"\nextra = 1\n",
        CHECK_WHEN,
    );

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_an_unknown_when_key() {
    let toml = check_toml(CHECK_TOP, "event = \"pre-commit\"\nfoo = 1\n");

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_a_wrong_type() {
    let toml = check_toml("blocking = \"yes\"\nhow = \"Run lint\"\n", CHECK_WHEN);

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_a_missing_blocking() {
    let toml = check_toml("how = \"Run lint\"\n", CHECK_WHEN);

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_a_missing_event() {
    let toml = check_toml(CHECK_TOP, "matcher = \"Bash\"\n");

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_a_vocabulary_other_than_free() {
    let toml = check_toml(
        CHECK_TOP,
        "event = \"pre-commit\"\nvocabulary = \"closed\"\n",
    );

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["value-not-allowed"]);
}

#[test]
fn it_should_reject_empty_event_and_matcher() {
    let empty_event = check_toml(CHECK_TOP, "event = \"\"\n");
    let empty_matcher = check_toml(CHECK_TOP, "event = \"pre-commit\"\nmatcher = \"\"\n");

    let event_codes = codes(&raw_check(&empty_event, None));
    let matcher_codes = codes(&raw_check(&empty_matcher, None));

    assert_eq!(event_codes, vec!["value-not-allowed"]);
    assert_eq!(matcher_codes, vec!["value-not-allowed"]);
}

#[test]
fn it_should_reject_control_characters_in_how() {
    let escape = check_toml("blocking = true\nhow = \"a\\u001bb\"\n", CHECK_WHEN);
    let c1 = check_toml("blocking = true\nhow = \"a\\u0085b\"\n", CHECK_WHEN);

    let escape_issues = issues(&raw_check(&escape, None));
    let c1_issues = issues(&raw_check(&c1, None));

    for found in [escape_issues, c1_issues] {
        assert_eq!(
            found.iter().map(|issue| issue.code).collect::<Vec<_>>(),
            vec!["value-not-allowed"]
        );
        assert!(found.iter().all(|issue| {
            !issue.message.contains('\u{1b}') && !issue.message.contains('\u{85}')
        }));
    }
}

#[test]
fn it_should_reject_control_characters_in_event_and_matcher() {
    let escape_in_event = check_toml(CHECK_TOP, "event = \"a\\u001bb\"\n");
    let del_in_matcher = check_toml(
        CHECK_TOP,
        "event = \"pre-commit\"\nmatcher = \"a\\u007fb\"\n",
    );

    let event_codes = codes(&raw_check(&escape_in_event, None));
    let matcher_codes = codes(&raw_check(&del_in_matcher, None));

    assert_eq!(event_codes, vec!["value-not-allowed"]);
    assert_eq!(matcher_codes, vec!["value-not-allowed"]);
}

#[test]
fn it_should_accept_tab_and_newline_in_text() {
    let toml = check_toml("blocking = true\nhow = \"a\\tb\\nc\"\n", CHECK_WHEN);

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.data["how"], json!("a\tb\nc"));
}

#[test]
fn it_should_reject_text_over_the_schema_limits() {
    let long_how = check_toml(
        &format!("blocking = true\nhow = \"{}\"\n", "x".repeat(8_001)),
        CHECK_WHEN,
    );
    let long_event = check_toml(CHECK_TOP, &format!("event = \"{}\"\n", "x".repeat(101)));
    let long_matcher = check_toml(
        CHECK_TOP,
        &format!(
            "event = \"pre-commit\"\nmatcher = \"{}\"\n",
            "x".repeat(201)
        ),
    );
    let long_text = format!("text = \"{}\"\n", "x".repeat(2_001));
    let at_limit_how = check_toml(
        &format!("blocking = true\nhow = \"{}\"\n", "é".repeat(8_000)),
        CHECK_WHEN,
    );
    let at_limit_event = check_toml(CHECK_TOP, &format!("event = \"{}\"\n", "é".repeat(100)));
    let at_limit_matcher = check_toml(
        CHECK_TOP,
        &format!(
            "event = \"pre-commit\"\nmatcher = \"{}\"\n",
            "é".repeat(200)
        ),
    );
    let at_limit_text = format!("text = \"{}\"\n", "é".repeat(2_000));

    let over = [
        issues(&raw_check(&long_how, None)),
        issues(&raw_check(&long_event, None)),
        issues(&raw_check(&long_matcher, None)),
        issues(&raw_guideline(&long_text, None)),
    ];
    let at_limit = [
        parse(&raw_check(&at_limit_how, None)).is_ok(),
        parse(&raw_check(&at_limit_event, None)).is_ok(),
        parse(&raw_check(&at_limit_matcher, None)).is_ok(),
        parse(&raw_guideline(&at_limit_text, None)).is_ok(),
    ];

    for found in &over {
        assert_eq!(
            found.iter().map(|issue| issue.code).collect::<Vec<_>>(),
            vec!["value-too-long"]
        );
        assert!(
            found
                .iter()
                .all(|issue| !issue.message.contains("xxxxxxxxxx"))
        );
    }
    assert_eq!(at_limit, [true, true, true, true]);
}

#[test]
fn it_should_derive_ids_from_kind_and_slug() {
    let check_raw = raw_check(&check_toml(CHECK_TOP, CHECK_WHEN), None);
    let guideline_raw = raw_guideline("text = \"Be brief\"\n", None);

    let check = parsed(&check_raw);
    let guideline = parsed(&guideline_raw);

    assert_eq!(check.id, "checks.lint");
    assert_eq!(check.path, check_raw.toml_path);
    assert_eq!(guideline.id, "guidelines.tone");
    assert_eq!(guideline.path, guideline_raw.toml_path);
}
