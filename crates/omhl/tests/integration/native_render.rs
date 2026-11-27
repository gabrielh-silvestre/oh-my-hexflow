use crate::support;

use omhl::probe::NativeRefusal;
use omhl::project::ProjectError;
use serde_json::json;
use support::FakeNative;
use support::appfx::AppFixture;
use support::pr_gate::{ALL_NATIVE_CHECKS, CI_GREEN, VERIFY_ANCHORING};

const RUN_TESTS: &str = "checks.run-tests";
const GUIDELINES: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];

fn text_of(fixture: &AppFixture) -> String {
    fixture.render()["text"]
        .as_str()
        .expect("the render has a text")
        .to_owned()
}

fn headers(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .collect()
}

fn native_checks_in(text: &str) -> Vec<&str> {
    headers(text)
        .into_iter()
        .filter(|id| ALL_NATIVE_CHECKS.contains(id))
        .collect()
}

fn warnings_in(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("WARNING:"))
        .collect()
}

fn invalid_workflow() -> NativeRefusal {
    NativeRefusal::InvalidWorkflow(ProjectError {
        code: "invalid-workflow-config",
        message: "/secret/path/.omhl/workflow.toml: invalid".to_owned(),
    })
}

// CA-003: o `render` imprime só o bloco dos checks nativos ligados

#[test]
fn it_should_print_no_native_check_block_when_the_resolver_enables_none() {
    let fixture = AppFixture::new().with_native(FakeNative::enabling(&[]));

    let text = text_of(&fixture);

    assert!(native_checks_in(&text).is_empty(), "{text}");
    assert!(text.contains("## Checks\n"), "{text}");
    assert!(warnings_in(&text).is_empty(), "{text}");
}

#[test]
fn it_should_keep_the_native_guidelines_when_the_resolver_enables_none() {
    let fixture = AppFixture::new().with_native(FakeNative::enabling(&[]));

    let text = text_of(&fixture);

    assert_eq!(headers(&text), GUIDELINES);
}

#[test]
fn it_should_print_only_the_block_of_the_check_the_resolver_enables() {
    for id in ALL_NATIVE_CHECKS {
        let fixture = AppFixture::new().with_native(FakeNative::enabling(&[id]));

        let text = text_of(&fixture);

        assert_eq!(native_checks_in(&text), [id], "{text}");
    }
}

#[test]
fn it_should_print_the_three_blocks_when_the_resolver_enables_the_three() {
    let fixture = AppFixture::new().with_native(FakeNative::all_enabled());

    let text = text_of(&fixture);

    assert_eq!(
        native_checks_in(&text),
        [RUN_TESTS, CI_GREEN, VERIFY_ANCHORING]
    );
    assert!(warnings_in(&text).is_empty(), "{text}");
}

// CA-004: a linha `after` do par nativo depende dos dois ligados

#[test]
fn it_should_print_the_native_after_line_when_both_checks_are_enabled() {
    let fixture = AppFixture::new().with_native(FakeNative::enabling(&[RUN_TESTS, CI_GREEN]));

    let text = text_of(&fixture);

    assert!(text.contains("- after: checks.run-tests\n"), "{text}");
}

#[test]
fn it_should_print_no_after_line_when_only_ci_green_is_enabled() {
    let fixture = AppFixture::new().with_native(FakeNative::enabling(&[CI_GREEN]));

    let text = text_of(&fixture);

    assert!(!text.contains("- after:"), "{text}");
}

// CA-004: só o texto é filtrado; a relação gravada segue lida por `list` e `get`

const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";

fn fixture_with_lint_after_run_tests(native: FakeNative) -> AppFixture {
    let fixture = AppFixture::new().with_native(native);
    fixture.ext().check("lint", LINT_TOML);
    fixture.load(false).expect("the check is loaded");
    let relation = json!({ "items": [{ "body": {
        "kind": "relation", "relation_type": "after", "from": "checks.lint", "to": RUN_TESTS,
    } }] });
    fixture
        .append_as_user(relation)
        .expect("the relation is appended");

    fixture
}

#[test]
fn it_should_not_print_the_after_line_to_a_disabled_native_check() {
    let fixture = fixture_with_lint_after_run_tests(FakeNative::enabling(&[CI_GREEN]));

    let text = text_of(&fixture);

    assert!(!text.contains("- after:"), "{text}");
}

#[test]
fn it_should_print_the_after_line_to_an_enabled_native_check() {
    let fixture = fixture_with_lint_after_run_tests(FakeNative::enabling(&[RUN_TESTS]));

    let text = text_of(&fixture);

    assert!(text.contains("- after: checks.run-tests\n"), "{text}");
}

#[test]
fn it_should_keep_the_stored_relation_to_a_disabled_native_check_readable() {
    let fixture = fixture_with_lint_after_run_tests(FakeNative::enabling(&[]));

    let edges = fixture.relation_edges("after");
    let listed = fixture.list("check");
    let node = fixture.get("checks.lint");

    assert!(edges.contains(&("checks.lint".to_owned(), RUN_TESTS.to_owned())));
    assert!(listed.to_string().contains("checks.lint"), "{listed}");
    assert_eq!(node["id"], json!("checks.lint"));
}

// CA-009 e CA-010

#[test]
fn it_should_render_without_a_native_block_and_never_refuse_when_the_main_directory_is_unknown() {
    let fixture =
        AppFixture::new().with_native(FakeNative::refusing(NativeRefusal::MainDirUnknown));

    let text = text_of(&fixture);

    assert!(native_checks_in(&text).is_empty(), "{text}");
    assert_eq!(headers(&text), GUIDELINES);
}

#[test]
fn it_should_render_without_a_native_block_and_never_refuse_when_the_workflow_is_not_accepted() {
    let fixture = AppFixture::new().with_native(FakeNative::refusing(invalid_workflow()));

    let text = text_of(&fixture);

    assert!(native_checks_in(&text).is_empty(), "{text}");
    assert_eq!(headers(&text), GUIDELINES);
}

#[test]
fn it_should_render_with_the_main_dir_unknown_notice_when_no_resolver_was_injected() {
    let fixture = AppFixture::new().without_native();

    let text = text_of(&fixture);

    assert!(native_checks_in(&text).is_empty(), "{text}");
    assert_eq!(warnings_in(&text).len(), 1, "{text}");
    assert!(warnings_in(&text)[0].contains("main-dir-unknown"), "{text}");
}

#[test]
fn it_should_warn_once_with_the_main_dir_unknown_code_and_git_init() {
    let fixture =
        AppFixture::new().with_native(FakeNative::refusing(NativeRefusal::MainDirUnknown));

    let text = text_of(&fixture);

    let [line] = warnings_in(&text)[..] else {
        panic!("one warning line expected: {text}");
    };
    assert!(line.contains("main-dir-unknown"), "{line}");
    assert!(line.contains("git init"), "{line}");
    assert!(!line.contains("omhl load --dry-run"), "{line}");
}

#[test]
fn it_should_warn_once_with_the_invalid_workflow_code_and_the_fix_as_next_step() {
    let fixture = AppFixture::new().with_native(FakeNative::refusing(invalid_workflow()));

    let text = text_of(&fixture);

    let [line] = warnings_in(&text)[..] else {
        panic!("one warning line expected: {text}");
    };
    assert!(line.contains("invalid-workflow-config"), "{line}");
    assert!(line.contains("gate-pr and close refuse until"), "{line}");
    assert!(line.contains("no native check is printed"), "{line}");
    assert!(line.contains("omhl load --dry-run"), "{line}");
    assert!(!line.contains("git init"), "{line}");
}

#[test]
fn it_should_not_leak_the_path_of_the_refused_file_into_the_warning() {
    let fixture = AppFixture::new().with_native(FakeNative::refusing(invalid_workflow()));

    let text = text_of(&fixture);

    assert!(!text.contains("/secret/path"), "{text}");
}

#[test]
fn it_should_print_no_warning_when_the_resolver_accepts_the_file() {
    let fixture = AppFixture::new().with_native(FakeNative::enabling(&[RUN_TESTS]));

    let text = text_of(&fixture);

    assert!(warnings_in(&text).is_empty(), "{text}");
}
