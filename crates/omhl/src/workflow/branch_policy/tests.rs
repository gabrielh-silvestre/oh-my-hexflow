use std::path::Path;

use crate::project::ProjectError;
use crate::test_support::sid;
use crate::workflow::branch_policy::{
    BranchConfig, BranchFormat, BranchMatch, ExceptionMode, ExceptionRule, FormatError,
    is_valid_branch_name,
};
use crate::workflow::config::{WorkflowConfig, parse_workflow};

const DEFAULT_TEMPLATE: &str = "<type>/wi-<id>__<slug>";

const INVALID_NAMES: [&str; 25] = [
    "",
    "-x",
    "/a",
    "refs/heads/x",
    "HEAD",
    "a..b",
    "a//b",
    "a@{b",
    "@",
    "a/",
    "a.",
    ".a",
    "a/.b",
    "a.lock",
    "a.lock/b",
    "a b",
    "a\u{1}b",
    "a\u{7f}b",
    "a~b",
    "a^b",
    "a:b",
    "a?b",
    "a*b",
    "a[b",
    "a\\b",
];

const VALID_NAMES: [&str; 5] = ["develop", "a/b", "a.b", "a-b", "x.lockx"];

fn format_of(template: &str) -> BranchFormat {
    BranchFormat::parse(template).expect("the template is valid")
}

fn config_with(rules: &[(&str, ExceptionMode)]) -> BranchConfig {
    BranchConfig {
        exceptions: rules
            .iter()
            .map(|(pattern, mode)| ExceptionRule {
                pattern: (*pattern).to_owned(),
                mode: *mode,
            })
            .collect(),
        ..BranchConfig::default()
    }
}

fn pattern_text(pattern: &str) -> String {
    format!("version = 1\n[[branch.exception]]\npattern = \"{pattern}\"\nmode = \"skip\"\n")
}

fn parse(text: &str) -> Result<WorkflowConfig, ProjectError> {
    parse_workflow(Path::new("/project/.omhl/workflow.toml"), text.as_bytes())
}

// Formato (CA-002)

#[test]
fn it_should_accept_the_default_and_the_three_documented_templates() {
    let templates = [
        DEFAULT_TEMPLATE,
        "<id>/<slug>",
        "<type>/<id>.<slug>",
        "work/<id>/<type>/<slug>",
    ];

    let refused: Vec<&str> = templates
        .iter()
        .copied()
        .filter(|template| BranchFormat::parse(template).is_err())
        .collect();

    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[test]
fn it_should_name_a_distinct_cause_for_each_refused_template() {
    let table = [
        ("", FormatError::Empty),
        ("<foo>/<id>", FormatError::UnknownField),
        ("<type>/<slug>", FormatError::IdMissing),
        ("<id>/<id>", FormatError::IdRepeated),
        ("<type>/<type>/<id>", FormatError::TypeRepeated),
        ("<id>~<slug>", FormatError::InvalidLiteral),
        ("<type>/<id>-<slug>", FormatError::AmbiguousField),
        ("<type><id>__<slug>", FormatError::AmbiguousField),
        ("<type>-wi-<id>__<slug>", FormatError::AmbiguousField),
        ("<slug>/<id>", FormatError::AmbiguousField),
    ];

    let causes: Vec<Option<FormatError>> = table
        .iter()
        .map(|(template, _)| BranchFormat::parse(template).err())
        .collect();

    let expected: Vec<Option<FormatError>> = table.iter().map(|(_, cause)| Some(*cause)).collect();
    assert_eq!(causes, expected);
}

#[test]
fn it_should_expose_the_template_text_and_whether_it_has_a_type() {
    let with_type = format_of(DEFAULT_TEMPLATE);
    let without_type = format_of("<id>/<slug>");

    assert_eq!(with_type.template(), DEFAULT_TEMPLATE);
    assert!(with_type.has_type());
    assert!(!without_type.has_type());
}

#[test]
fn it_should_default_the_format_to_the_work_item_template() {
    let format = BranchFormat::default();

    assert_eq!(format.template(), DEFAULT_TEMPLATE);
}

// Casamento (CA-003, parte do formato)

#[test]
fn it_should_match_a_work_item_branch_with_the_work_slash_id_slash_type_slash_slug_format() {
    let format = format_of("work/<id>/<type>/<slug>");

    let matched = format.match_branch("work/w1/fix/a-b");

    assert_eq!(
        matched,
        Some(BranchMatch {
            id: sid("work-items.w1"),
            kind: Some("fix".to_owned()),
        })
    );
}

#[test]
fn it_should_match_a_work_item_branch_with_the_type_slash_id_dot_slug_format() {
    let format = format_of("<type>/<id>.<slug>");

    let matched = format.match_branch("fix/w1.a-b");

    assert_eq!(
        matched,
        Some(BranchMatch {
            id: sid("work-items.w1"),
            kind: Some("fix".to_owned()),
        })
    );
}

#[test]
fn it_should_give_no_kind_when_the_format_has_no_type() {
    let format = format_of("<id>/<slug>");

    let matched = format.match_branch("w1/a-b");

    assert_eq!(
        matched,
        Some(BranchMatch {
            id: sid("work-items.w1"),
            kind: None,
        })
    );
}

// Pattern da exceção (CA-004)

#[test]
fn it_should_refuse_each_invalid_exception_pattern_through_the_workflow_parse() {
    let patterns = [
        "main", "*", "/*", "poc*", "a/*/b", "", "-x", "refs/x", "a..b", "-x/*", "refs/*",
    ];

    let not_refused: Vec<&str> = patterns
        .iter()
        .copied()
        .filter(|pattern| !matches!(parse(&pattern_text(pattern)), Err(e) if e.code == "invalid-workflow-config"))
        .collect();

    assert!(not_refused.is_empty(), "not refused: {not_refused:?}");
}

#[test]
fn it_should_refuse_a_repeated_exception_pattern() {
    let text = "version = 1\n[[branch.exception]]\npattern = \"poc/*\"\nmode = \"skip\"\n[[branch.exception]]\npattern = \"poc/*\"\nmode = \"dry-run\"\n";

    let error = parse(text).expect_err("the repeated pattern is refused");

    assert_eq!(error.code, "invalid-workflow-config");
}

#[test]
fn it_should_accept_an_exact_name_and_a_prefix_pattern() {
    let patterns = ["develop", "poc/*", "a/b/*"];

    let refused: Vec<&str> = patterns
        .iter()
        .copied()
        .filter(|pattern| parse(&pattern_text(pattern)).is_err())
        .collect();

    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[test]
fn it_should_match_nested_names_under_a_prefix_pattern() {
    let policy = config_with(&[("poc/*", ExceptionMode::Skip)]);

    let shallow = policy.exception_for("poc/x");
    let deep = policy.exception_for("poc/x/y");

    assert_eq!(shallow, Some(ExceptionMode::Skip));
    assert_eq!(deep, Some(ExceptionMode::Skip));
}

#[test]
fn it_should_not_match_the_bare_prefix_or_an_empty_rest() {
    let policy = config_with(&[("poc/*", ExceptionMode::Skip)]);

    let bare = policy.exception_for("poc");
    let empty_rest = policy.exception_for("poc/");

    assert_eq!(bare, None);
    assert_eq!(empty_rest, None);
}

#[test]
fn it_should_match_an_exact_pattern_only_to_that_name() {
    let policy = config_with(&[("develop", ExceptionMode::DryRun)]);

    let exact = policy.exception_for("develop");
    let longer = policy.exception_for("develop2");
    let nested = policy.exception_for("develop/x");

    assert_eq!(exact, Some(ExceptionMode::DryRun));
    assert_eq!(longer, None);
    assert_eq!(nested, None);
}

#[test]
fn it_should_prefer_the_exact_name_over_a_prefix() {
    let policy = config_with(&[
        ("poc/*", ExceptionMode::Skip),
        ("poc/keep", ExceptionMode::DryRunWithMark),
    ]);

    let mode = policy.exception_for("poc/keep");

    assert_eq!(mode, Some(ExceptionMode::DryRunWithMark));
}

#[test]
fn it_should_prefer_the_longest_prefix() {
    let policy = config_with(&[
        ("poc/*", ExceptionMode::Skip),
        ("poc/deep/*", ExceptionMode::DryRun),
    ]);

    let deep = policy.exception_for("poc/deep/x");
    let shallow = policy.exception_for("poc/other");

    assert_eq!(deep, Some(ExceptionMode::DryRun));
    assert_eq!(shallow, Some(ExceptionMode::Skip));
}

#[test]
fn it_should_give_the_same_modes_whatever_the_order_of_the_entries() {
    let entries = [
        ("poc/*", ExceptionMode::Skip),
        ("poc/deep/*", ExceptionMode::DryRun),
        ("poc/keep", ExceptionMode::DryRunWithMark),
    ];
    let orders: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let names = ["poc/keep", "poc/deep/x", "poc/other", "poc", "main"];

    let results: Vec<Vec<Option<ExceptionMode>>> = orders
        .iter()
        .map(|order| {
            let shuffled: Vec<_> = order.iter().map(|index| entries[*index]).collect();
            let policy = config_with(&shuffled);
            names
                .iter()
                .map(|name| policy.exception_for(name))
                .collect()
        })
        .collect();

    assert!(
        results.iter().all(|row| row == &results[0]),
        "the mode depends on the order: {results:?}"
    );
}

// Validador de nome (CA-013)

#[test]
fn it_should_refuse_every_invalid_branch_name() {
    let accepted: Vec<&str> = INVALID_NAMES
        .iter()
        .copied()
        .filter(|name| is_valid_branch_name(name))
        .collect();

    assert!(accepted.is_empty(), "accepted: {accepted:?}");
}

#[test]
fn it_should_accept_every_valid_branch_name() {
    let refused: Vec<&str> = VALID_NAMES
        .iter()
        .copied()
        .filter(|name| !is_valid_branch_name(name))
        .collect();

    assert!(refused.is_empty(), "refused: {refused:?}");
}
