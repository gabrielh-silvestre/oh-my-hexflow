use crate::support;

use std::fs;

use support::repo_checks::repo_root;

const REVIEW: &str = "plugin/skills/review/references/";
const FLOW: &str = "plugin/skills/directives-flow/references/";

fn page(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

fn missing<'a>(text: &str, anchors: &[&'a str]) -> Vec<&'a str> {
    anchors
        .iter()
        .copied()
        .filter(|anchor| !text.contains(anchor))
        .collect()
}

fn line_starting<'a>(text: &'a str, prefix: &str) -> &'a str {
    text.lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("no line starts with {prefix}"))
}

// Códigos na página do comando que os produz

#[test]
fn it_should_list_the_close_codes_on_the_close_page() {
    let text = page(&format!("{REVIEW}close.md"));

    let absent = missing(
        &text,
        &[
            "`close-refused`",
            "`close-warnings`",
            "`work-item-type-unknown`",
            "`review-base-invalid`",
            "`failed-delivery-unneeded`",
            "`delivery-not-on-work-item-branch`",
            "`delivery-not-integrated`",
        ],
    );

    assert_eq!(absent, Vec::<&str>::new());
}

#[test]
fn it_should_list_the_type_and_base_codes_on_the_gate_delivery_page() {
    let text = page(&format!("{REVIEW}gate-delivery.md"));

    let absent = missing(
        &text,
        &["`work-item-type-unknown`", "`review-base-invalid`"],
    );

    assert_eq!(absent, Vec::<&str>::new());
}

#[test]
fn it_should_list_the_loosening_code_on_the_load_page() {
    let text = page(&format!("{REVIEW}load.md"));

    assert!(text.contains("`check-loosening-refused`"));
}

#[test]
fn it_should_list_the_loosening_code_on_the_check_retire_page() {
    let text = page(&format!("{REVIEW}check-retire.md"));

    assert!(text.contains("`check-loosening-refused`"));
}

#[test]
fn it_should_list_the_type_change_and_loosening_codes_on_the_append_page() {
    let text = page(&format!("{FLOW}append.md"));

    let absent = missing(
        &text,
        &["`type-change-loosens`", "`check-loosening-refused`"],
    );

    assert_eq!(absent, Vec::<&str>::new());
}

// Exceções da recusa por workflow.toml inválido

#[test]
fn it_should_name_abandon_and_the_repair_pr_in_the_readme_native_checks_entry() {
    let readme = page("README.md");

    let line = line_starting(&readme, "5. O binário anterior recusa");

    let absent = missing(line, &["`--abandon`", "PR de reparo"]);
    assert_eq!(absent, Vec::<&str>::new(), "{line}");
}

#[test]
fn it_should_name_abandon_and_the_repair_pull_request_in_the_native_checks_refusal_bullet() {
    let text = page(&format!("{FLOW}native-checks.md"));

    let line = line_starting(
        &text,
        "- `gate-delivery` and `close` refuse with `invalid-workflow-config`",
    );

    let absent = missing(line, &["`--abandon`", "repair pull request"]);
    assert_eq!(absent, Vec::<&str>::new(), "{line}");
}

#[test]
fn it_should_name_abandon_and_the_repair_pull_request_in_the_close_invalid_config_entry() {
    let text = page(&format!("{REVIEW}close.md"));

    let line = line_starting(&text, "- `invalid-workflow-config`");

    let absent = missing(line, &["`--abandon`", "repair pull request"]);
    assert_eq!(absent, Vec::<&str>::new(), "{line}");
}

#[test]
fn it_should_name_abandon_and_the_repair_pull_request_in_the_gate_delivery_invalid_config_entry() {
    let text = page(&format!("{REVIEW}gate-delivery.md"));

    let line = line_starting(&text, "- `invalid-workflow-config`");

    let absent = missing(line, &["`--abandon`", "repair pull request"]);
    assert_eq!(absent, Vec::<&str>::new(), "{line}");
}

#[test]
fn it_should_not_say_in_the_readme_update_entry_that_an_invalid_workflow_stops_blocking_work() {
    let readme = page("README.md");

    assert!(!readme.contains("não trava mais o trabalho"));
}

#[test]
fn it_should_not_say_in_the_readme_update_entry_that_abandon_refuses_a_failed_gate() {
    let readme = page("README.md");

    let line = line_starting(&readme, "1. `close` agora recusa");

    assert!(!line.contains("`--abandon`"), "{line}");
}

// Texto do close por código e a retro

#[test]
fn it_should_tell_close_to_resolve_each_warning_before_asking_the_owner() {
    let text = page(&format!("{REVIEW}close.md"));

    assert!(text.contains("try to resolve each warning"));
}

#[test]
fn it_should_say_the_authorization_request_differs_by_error_code_on_the_close_page() {
    let text = page(&format!("{REVIEW}close.md"));

    assert!(text.contains("differs by error code"));
}

#[test]
fn it_should_ask_for_how_each_failed_refuse_question_got_past_the_brake_on_the_close_page() {
    let text = page(&format!("{REVIEW}close.md"));

    assert!(text.contains("how it got past the brake"));
}

#[test]
fn it_should_say_close_takes_the_merge_base_that_gate_delivery_prints() {
    let text = page(&format!("{REVIEW}close.md"));

    assert!(text.contains("the merge-base that `gate-delivery` prints"));
}
