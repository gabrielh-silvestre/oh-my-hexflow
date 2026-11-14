mod support;

use omhl::extensions::model::{Extension, ExtensionIssue, ExtensionKind, RawExtension};
use omhl::extensions::parse::parse;
use serde_json::{Value, json};
use support::omhl::check_data;

const CHECK_TOML_PATH: &str = ".omhl/checks/lint.toml";
const CHECK_MARKDOWN_PATH: &str = ".omhl/checks/lint.md";
const GUIDELINE_TOML_PATH: &str = ".omhl/guidelines/tone.toml";
const GUIDELINE_MARKDOWN_PATH: &str = ".omhl/guidelines/tone.md";

const CHECK_TOP: &str = "blocking = true\nhow = \"Run lint\"\n";
const CHECK_WHEN: &str = "event = \"before-commit\"\n";

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
        "event = \"before-commit\"\nmatcher = \"Bash\"\n",
    );

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.kind, ExtensionKind::Check);
    assert_eq!(extension.id, "checks.lint");
    assert_eq!(
        extension.data,
        json!({
            "blocking": true,
            "how": "Run lint",
            "when": {"vocabulary": "closed", "event": "before-commit", "matcher": "Bash"}
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
            "when": {"vocabulary": "closed", "event": "before-commit"}
        })
    );
    assert!(extension.data["when"].get("matcher").is_none());
}

#[test]
fn it_should_accept_vocabulary_closed_explicitly() {
    let toml = check_toml(
        CHECK_TOP,
        "event = \"before-commit\"\nvocabulary = \"closed\"\n",
    );

    let extension = parsed(&raw_check(&toml, None));

    assert_eq!(extension.data["when"]["vocabulary"], json!("closed"));
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
    let matcher_toml = check_toml(
        CHECK_TOP,
        "event = \"before-commit\"\nmatcher = \"c\\rd\"\n",
    );
    let event_toml = check_toml(CHECK_TOP, "event = \"a\\rb\"\n");

    let extension = parsed(&raw_check(&matcher_toml, None));

    assert_eq!(extension.data["when"]["matcher"], json!("c\nd"));
    assert_eq!(
        codes(&raw_check(&event_toml, None)),
        vec!["event-unsupported"]
    );
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
    let extension = parsed(&raw_guideline(
        "kind = \"negocio\"\ntext = \"Be brief\"\n",
        None,
    ));

    assert_eq!(extension.kind, ExtensionKind::Guideline);
    assert_eq!(extension.id, "guidelines.tone");
    assert_eq!(
        extension.data,
        json!({"text": "Be brief", "kind": "negocio"})
    );
    assert!(extension.after.is_empty());
}

#[test]
fn it_should_take_guideline_text_from_the_markdown() {
    let extension = parsed(&raw_guideline("kind = \"negocio\"\n", Some("Long")));

    assert_eq!(extension.data, json!({"text": "Long", "kind": "negocio"}));
}

#[test]
fn it_should_parse_a_guideline_with_each_kind() {
    let kinds: Vec<Value> = ["negocio", "arquitetura"]
        .iter()
        .map(|kind| {
            let toml = format!("kind = \"{kind}\"\ntext = \"x\"\n");

            parsed(&raw_guideline(&toml, None)).data["kind"].clone()
        })
        .collect();

    assert_eq!(kinds, vec![json!("negocio"), json!("arquitetura")]);
}

#[test]
fn it_should_reject_a_guideline_without_kind() {
    let found = issues(&raw_guideline("text = \"x\"\n", None));

    assert_eq!(
        found.iter().map(|issue| issue.code).collect::<Vec<_>>(),
        vec!["kind-missing"]
    );
    assert!(!found[0].message.is_empty());
    assert!(
        found[0]
            .next_step
            .is_some_and(|guidance| !guidance.next_step.is_empty())
    );
}

#[test]
fn it_should_reject_a_guideline_with_an_unknown_kind() {
    let found = issues(&raw_guideline("kind = \"banana\"\ntext = \"x\"\n", None));

    assert_eq!(
        found.iter().map(|issue| issue.code).collect::<Vec<_>>(),
        vec!["kind-invalid"]
    );
    assert!(!found[0].message.is_empty());
    assert!(
        found[0]
            .next_step
            .is_some_and(|guidance| !guidance.next_step.is_empty())
    );
}

#[test]
fn it_should_not_echo_the_unknown_kind_in_the_issue() {
    let found = issues(&raw_guideline("kind = \"banana\"\ntext = \"x\"\n", None));

    let echoed = found[0].message.contains("banana")
        || found[0].next_step.is_some_and(|guidance| {
            guidance.cause.contains("banana") || guidance.next_step.contains("banana")
        });
    assert!(!echoed);
}

#[test]
fn it_should_reject_a_guideline_without_text() {
    let found = codes(&raw_guideline("kind = \"negocio\"\n", None));

    assert_eq!(found, vec!["text-missing"]);
}

#[test]
fn it_should_reject_after_in_a_guideline() {
    let found = codes(&raw_guideline(
        "kind = \"negocio\"\ntext = \"x\"\nafter = [\"checks.a\"]\n",
        None,
    ));

    assert_eq!(found, vec!["invalid-toml"]);
}

#[test]
fn it_should_reject_blocking_in_a_guideline() {
    let found = codes(&raw_guideline(
        "kind = \"negocio\"\ntext = \"x\"\nblocking = true\n",
        None,
    ));

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
    let toml = check_toml(CHECK_TOP, "event = \"before-commit\"\nfoo = 1\n");

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
fn it_should_reject_a_vocabulary_other_than_closed() {
    let toml = check_toml(
        CHECK_TOP,
        "event = \"before-commit\"\nvocabulary = \"free\"\n",
    );

    let found = codes(&raw_check(&toml, None));

    assert_eq!(found, vec!["value-not-allowed"]);
}

#[test]
fn it_should_reject_empty_event_and_matcher() {
    let empty_event = check_toml(CHECK_TOP, "event = \"\"\n");
    let empty_matcher = check_toml(CHECK_TOP, "event = \"before-commit\"\nmatcher = \"\"\n");

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
        "event = \"before-commit\"\nmatcher = \"a\\u007fb\"\n",
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
            "event = \"before-commit\"\nmatcher = \"{}\"\n",
            "x".repeat(201)
        ),
    );
    let long_text = format!("kind = \"negocio\"\ntext = \"{}\"\n", "x".repeat(2_001));
    let at_limit_how = check_toml(
        &format!("blocking = true\nhow = \"{}\"\n", "é".repeat(8_000)),
        CHECK_WHEN,
    );
    let at_limit_event = check_toml(CHECK_TOP, &format!("event = \"{}\"\n", "é".repeat(100)));
    let at_limit_matcher = check_toml(
        CHECK_TOP,
        &format!(
            "event = \"before-commit\"\nmatcher = \"{}\"\n",
            "é".repeat(200)
        ),
    );
    let at_limit_text = format!("kind = \"negocio\"\ntext = \"{}\"\n", "é".repeat(2_000));

    let over = [
        issues(&raw_check(&long_how, None)),
        issues(&raw_check(&long_event, None)),
        issues(&raw_check(&long_matcher, None)),
        issues(&raw_guideline(&long_text, None)),
    ];
    let at_limit = [
        parse(&raw_check(&at_limit_how, None)).is_ok(),
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
    assert_eq!(at_limit, [true, true, true]);
    assert_eq!(
        codes(&raw_check(&at_limit_event, None)),
        vec!["event-unsupported"]
    );
}

#[test]
fn it_should_derive_ids_from_kind_and_slug() {
    let check_raw = raw_check(&check_toml(CHECK_TOP, CHECK_WHEN), None);
    let guideline_raw = raw_guideline("kind = \"negocio\"\ntext = \"Be brief\"\n", None);

    let check = parsed(&check_raw);
    let guideline = parsed(&guideline_raw);

    assert_eq!(check.id, "checks.lint");
    assert_eq!(check.path, check_raw.toml_path);
    assert_eq!(guideline.id, "guidelines.tone");
    assert_eq!(guideline.path, guideline_raw.toml_path);
}

#[test]
fn it_should_reject_a_malformed_after_target_with_the_file_path() {
    let long_segment = format!("checks.{}", "a".repeat(64));
    let targets = [
        "checks.Bad Name",
        "checks.a..b",
        "checks.",
        long_segment.as_str(),
    ];

    let found: Vec<Vec<(String, &'static str)>> = targets
        .iter()
        .map(|target| {
            let top = format!("blocking = true\nhow = \"Run lint\"\nafter = [\"{target}\"]\n");
            issues(&raw_check(&check_toml(&top, CHECK_WHEN), None))
                .into_iter()
                .map(|issue| (issue.path, issue.code))
                .collect()
        })
        .collect();

    assert_eq!(
        found,
        vec![vec![(CHECK_TOML_PATH.to_owned(), "value-not-allowed")]; 4]
    );
}

#[test]
fn it_should_reject_more_than_sixteen_after_targets() {
    let after_of = |count: usize| {
        let targets: Vec<String> = (1..=count).map(|n| format!("\"checks.t{n:02}\"")).collect();
        let top = format!(
            "blocking = true\nhow = \"Run lint\"\nafter = [{}]\n",
            targets.join(", ")
        );
        raw_check(&check_toml(&top, CHECK_WHEN), None)
    };
    let over = after_of(17);
    let at_limit = after_of(16);

    let over_codes = codes(&over);
    let at_limit_parsed = parse(&at_limit).is_ok();

    assert_eq!(over_codes, vec!["value-too-long"]);
    assert!(at_limit_parsed, "sixteen targets must parse");
}

#[test]
fn it_should_reject_invisible_format_characters() {
    let code_points: [u32; 13] = [
        0x200B, 0x200F, 0x2028, 0x202A, 0x202E, 0x2060, 0x2064, 0xFEFF, 0xE0000, 0xE0001, 0xE0020,
        0xE007F, 0x2063,
    ];
    let literal = |code_point: u32| format!("\\U{code_point:08X}");

    let mut unexpected: Vec<String> = Vec::new();
    for code_point in code_points {
        let escaped = literal(code_point);
        let how = check_toml(
            &format!("blocking = true\nhow = \"a{escaped}b\"\n"),
            CHECK_WHEN,
        );
        let event = check_toml(CHECK_TOP, &format!("event = \"a{escaped}b\"\n"));
        let matcher = check_toml(
            CHECK_TOP,
            &format!("event = \"before-commit\"\nmatcher = \"a{escaped}b\"\n"),
        );
        let text = format!("kind = \"negocio\"\ntext = \"a{escaped}b\"\n");
        let outcomes = [
            ("how", issues(&raw_check(&how, None))),
            ("event", issues(&raw_check(&event, None))),
            ("matcher", issues(&raw_check(&matcher, None))),
            ("text", issues(&raw_guideline(&text, None))),
        ];
        for (field, found) in outcomes {
            let found_codes: Vec<&str> = found.iter().map(|issue| issue.code).collect();
            let echoed = found.iter().any(|issue| {
                issue
                    .message
                    .contains(char::from_u32(code_point).expect("valid code point"))
            });
            if found_codes != ["value-not-allowed"] || echoed {
                unexpected.push(format!("U+{code_point:04X} in {field}: {found_codes:?}"));
            }
        }
    }

    assert!(unexpected.is_empty(), "unexpected outcomes: {unexpected:?}");
}

#[test]
fn it_should_write_the_closed_vocabulary_when_parsing_an_extension() {
    let events = ["before-commit", "before-pr", "before-merge"];

    let written: Vec<_> = events
        .iter()
        .map(|event| {
            let toml = check_toml(CHECK_TOP, &format!("event = \"{event}\"\n"));
            parsed(&raw_check(&toml, None)).data["when"].clone()
        })
        .collect();

    let expected: Vec<_> = events
        .iter()
        .map(|event| json!({ "vocabulary": "closed", "event": event }))
        .collect();
    assert_eq!(written, expected);
}

#[test]
fn it_should_reject_a_text_only_extension_whose_event_is_outside_the_closed_list() {
    let toml = check_toml(CHECK_TOP, "event = \"before-deploy\"\n");

    let found = issues(&raw_check(&toml, None));

    assert_eq!(
        found,
        vec![ExtensionIssue {
            path: ".omhl/checks/lint.toml".to_owned(),
            code: "event-unsupported",
            message: "`event` is not one of before-commit, before-pr, before-merge".to_owned(),
            next_step: None,
        }]
    );
    assert!(!found[0].message.contains("before-deploy"));
}

#[test]
fn it_should_reject_the_native_review_event_in_an_extension() {
    let toml = check_toml(CHECK_TOP, "event = \"before-publishing-review-comments\"\n");

    let found = issues(&raw_check(&toml, None));

    assert_eq!(
        found.iter().map(|issue| issue.code).collect::<Vec<_>>(),
        vec!["event-unsupported"]
    );
    assert!(!found[0].message.contains("review"));
}

#[test]
fn it_should_keep_the_runnable_events_equal_to_the_catalog_enum() {
    let catalog = omhl::graphs::catalog().expect("the embedded catalogs build");
    let check = catalog
        .type_def(&hexlog::TypeName::parse("check").expect("valid type name"))
        .expect("the check type exists");

    let listed = &check.schema["properties"]["when"]["properties"]["event"]["enum"];

    assert_eq!(
        omhl::mechanical::RUNNABLE_EVENTS,
        ["before-commit", "before-pr", "before-merge"]
    );
    assert_eq!(listed, &json!(omhl::mechanical::RUNNABLE_EVENTS));
}

#[test]
fn it_should_report_the_form_issue_of_an_event_before_event_unsupported() {
    let empty = check_toml(CHECK_TOP, "event = \"\"\n");
    let too_long = check_toml(CHECK_TOP, &format!("event = \"{}\"\n", "x".repeat(101)));
    let control = check_toml(CHECK_TOP, "event = \"a\\u001bb\"\n");
    let both = check_toml(
        CHECK_TOP,
        "event = \"before-deploy\"\nvocabulary = \"free\"\n",
    );

    let found = (
        codes(&raw_check(&empty, None)),
        codes(&raw_check(&too_long, None)),
        codes(&raw_check(&control, None)),
        codes(&raw_check(&both, None)),
    );

    assert_eq!(
        found,
        (
            vec!["value-not-allowed"],
            vec!["value-too-long"],
            vec!["value-not-allowed"],
            vec!["event-unsupported", "value-not-allowed"]
        )
    );
}

// Campos mecânicos do check

const REPORT_LINT_TOML: &str = r#"concern = "style"
variant = "lint"
when = { event = "before-commit" }
blocking = true
run = ["cargo", "clippy", "--workspace", "--all-targets", "--locked"]
timeout_secs = 600
expect = [
  { exit = 0, stderr = "(?m)^warning", outcome = "passed_with_warning" },
  { exit = 0, outcome = "passed" },
]
"#;
const REPORT_LINT_MD: &str = "Run clippy on the workspace.\n";

fn report_lint_data() -> Value {
    let mut data = check_data("before-commit", "Run clippy on the workspace.", true);
    data["concern"] = json!("style");
    data["variant"] = json!("lint");
    data["run"] = json!([
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked"
    ]);
    data["timeout_secs"] = json!(600);
    data["expect"] = json!([
        {"exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning"},
        {"exit": 0, "outcome": "passed"},
    ]);

    data
}

/// Check mecânico mínimo; `(chave, Some(v))` troca ou insere a linha, `(chave, None)` apaga.
fn mech_toml(over: &[(&str, Option<&str>)]) -> String {
    let mut lines: Vec<String> = [
        "blocking = true",
        "how = \"Run lint\"",
        "run = [\"true\"]",
        "timeout_secs = 60",
        "expect = [{ exit = 0, outcome = \"passed\" }]",
        "[when]",
        "event = \"before-pr\"",
    ]
    .map(str::to_owned)
    .to_vec();

    for (key, value) in over {
        let prefix = format!("{key} = ");
        let found = lines.iter().position(|line| line.starts_with(&prefix));
        match (found, value) {
            (Some(index), Some(value)) => lines[index] = format!("{prefix}{value}"),
            (Some(index), None) => {
                lines.remove(index);
            }
            (None, Some(value)) => {
                let when = lines
                    .iter()
                    .position(|line| line == "[when]")
                    .expect("the base has a [when] table");
                lines.insert(when, format!("{prefix}{value}"));
            }
            (None, None) => {}
        }
    }

    format!("{}\n", lines.join("\n"))
}

fn mech_raw(over: &[(&str, Option<&str>)]) -> RawExtension {
    raw_check(&mech_toml(over), None)
}

fn code_list(found: &[ExtensionIssue]) -> Vec<&'static str> {
    found.iter().map(|issue| issue.code).collect()
}

fn echoes(found: &[ExtensionIssue], needle: &str) -> bool {
    found.iter().any(|issue| issue.message.contains(needle))
}

const TWO_STREAMS: &[(&str, Option<&str>)] = &[(
    "expect",
    Some("[{ exit = 0, stdout = \"a\", stderr = \"b\", outcome = \"passed\" }]"),
)];

#[test]
fn it_should_parse_the_lint_check_of_the_report_into_node_data() {
    let raw = raw_check(REPORT_LINT_TOML, Some(REPORT_LINT_MD));

    let extension = parsed(&raw);

    assert_eq!(extension.id, "checks.lint");
    assert_eq!(extension.data, report_lint_data());
    assert_eq!(extension.after, Vec::<String>::new());
    assert_eq!(extension.path, CHECK_TOML_PATH);
    assert!(extension.data.get("snapshot_kind").is_none());
}

#[test]
fn it_should_reject_a_run_with_an_empty_argv() {
    let found = issues(&mech_raw(&[("run", Some("[]"))]));

    assert_eq!(code_list(&found), vec!["run-empty"]);
    assert_eq!(found[0].path, CHECK_TOML_PATH);
    assert_eq!(found[0].message, "run-empty at /run");
}

#[test]
fn it_should_reject_a_timeout_of_zero() {
    let found = issues(&mech_raw(&[("timeout_secs", Some("0"))]));

    assert_eq!(code_list(&found), vec!["timeout-out-of-range"]);
    assert_eq!(found[0].message, "timeout-out-of-range at /timeout_secs");
}

#[test]
fn it_should_reject_a_timeout_above_the_limit() {
    let over = issues(&mech_raw(&[("timeout_secs", Some("7201"))]));
    let at_limit = parsed(&mech_raw(&[("timeout_secs", Some("7200"))]));

    assert_eq!(code_list(&over), vec!["timeout-out-of-range"]);
    assert_eq!(at_limit.data["timeout_secs"], json!(7200));
}

#[test]
fn it_should_reject_an_invalid_regex_without_echoing_it() {
    let over = [(
        "expect",
        Some("[{ exit = 0, stdout = \"(unclosed\", outcome = \"passed\" }]"),
    )];

    let found = issues(&mech_raw(&over));

    assert_eq!(code_list(&found), vec!["regex-invalid"]);
    assert_eq!(found[0].message, "regex-invalid at /expect/0/stdout");
    assert!(!echoes(&found, "unclosed"));
}

#[test]
fn it_should_reject_an_unknown_concern() {
    let over = [
        ("concern", Some("\"security\"")),
        ("variant", Some("\"lint\"")),
    ];

    let found = issues(&mech_raw(&over));

    assert_eq!(code_list(&found), vec!["value-not-allowed"]);
    assert_eq!(
        found[0].message,
        "`concern` is not one of style, test, compile"
    );
    assert!(!echoes(&found, "security"));
}

#[test]
fn it_should_reject_a_variant_that_does_not_belong_to_the_concern() {
    let over = [
        ("concern", Some("\"style\"")),
        ("variant", Some("\"suite\"")),
    ];

    let found = issues(&mech_raw(&over));

    assert_eq!(code_list(&found), vec!["value-not-allowed"]);
    assert_eq!(found[0].message, "`variant` does not belong to `concern`");
}

#[test]
fn it_should_reject_run_without_expect() {
    let found = issues(&mech_raw(&[("expect", None)]));

    assert_eq!(code_list(&found), vec!["expect-missing"]);
    assert_eq!(found[0].message, "expect-missing at /expect");
}

#[test]
fn it_should_reject_an_empty_expect() {
    let found = issues(&mech_raw(&[("expect", Some("[]"))]));

    assert_eq!(code_list(&found), vec!["expect-empty"]);
    assert_eq!(found[0].message, "expect-empty at /expect");
}

#[test]
fn it_should_reject_a_check_over_the_aggregate_size_limit() {
    let how = format!("\"{}\"", "h".repeat(7000));
    let item = format!("\"{}\"", "r".repeat(1000));
    let run = format!("[{}]", vec![item; 5].join(", "));
    let expect = format!(
        "[{{ exit = 0, stdout = \"{}\", outcome = \"passed\" }}]",
        "a".repeat(2830)
    );
    let over = [
        ("how", Some(how.as_str())),
        ("run", Some(run.as_str())),
        ("expect", Some(expect.as_str())),
    ];

    let found = issues(&mech_raw(&over));

    assert_eq!(code_list(&found), vec!["data-too-large"]);
    assert_eq!(found[0].message, "data-too-large");
    assert!(!echoes(&found, "hhhh") && !echoes(&found, "rrrr") && !echoes(&found, "aaaa"));
}

#[test]
fn it_should_reject_a_snapshot_kind_key_as_unknown() {
    let found = issues(&mech_raw(&[("snapshot_kind", Some("\"git-tree\""))]));

    assert_eq!(code_list(&found), vec!["invalid-toml"]);
    assert!(has_toml_position(&found[0].message));
    assert!(!echoes(&found, "snapshot_kind"));
    assert!(
        parse(&mech_raw(&[])).is_ok(),
        "the control without the key parses"
    );
}

#[test]
fn it_should_keep_a_text_only_check_unchanged() {
    let toml = "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";

    let extension = parsed(&raw_check(toml, None));

    assert_eq!(
        extension.data,
        check_data("before-commit", "Run lint", true)
    );
    assert_eq!(extension.data.as_object().map(|keys| keys.len()), Some(3));
}

#[test]
fn it_should_reject_an_unknown_key_in_an_expect_rule() {
    let over = [(
        "expect",
        Some("[{ exit = 0, outcome = \"passed\", extra = 1 }]"),
    )];

    let found = issues(&mech_raw(&over));

    assert_eq!(code_list(&found), vec!["invalid-toml"]);
    assert!(has_toml_position(&found[0].message));
    assert!(
        parse(&mech_raw(&[])).is_ok(),
        "the control without the key parses"
    );
}

#[test]
fn it_should_reject_an_event_outside_the_closed_list_for_a_check_with_run() {
    let found = issues(&mech_raw(&[("event", Some("\"before-deploy\""))]));

    assert_eq!(code_list(&found), vec!["event-unsupported"]);
    assert_eq!(found[0].path, CHECK_TOML_PATH);
    assert!(!echoes(&found, "before-deploy"));
}

#[test]
fn it_should_reject_a_rule_with_both_a_stdout_and_a_stderr_pattern() {
    let found = issues(&mech_raw(TWO_STREAMS));

    assert_eq!(code_list(&found), vec!["rule-two-streams"]);
    assert!(
        found[0]
            .message
            .starts_with("rule-two-streams at /expect/0")
    );
}

#[test]
fn it_should_name_the_one_command_per_check_practice_in_the_two_streams_error() {
    let found = issues(&mech_raw(TWO_STREAMS));

    assert_eq!(
        found[0].message,
        "rule-two-streams at /expect/0: a rule matches stdout or stderr, not both; keep one command per check"
    );
    assert!(found[0].message.contains("one command per check"));
}
