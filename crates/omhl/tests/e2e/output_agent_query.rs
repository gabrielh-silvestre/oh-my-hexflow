use crate::support;
use serde_json::Value;
use support::child::{Output, omhl as binary};
use support::output_agent_query::first_line;
use support::project::Project;
use support::query::{DECISION, query_project};

fn binary_in(project: &Project, args: &[&str]) -> Output {
    binary(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn assert_text_starting_with(text: &str, symbol: &str) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(first_line(text).starts_with(symbol), "{text}");
}

// ---------------------------------------------------------------- CA-1: sem `--format` (agent)

#[test]
fn it_should_print_agent_text_for_list_when_no_format_is_given() {
    let project = query_project();

    let output = binary_in(&project, &["list"]);

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_agent_text_for_get_when_no_format_is_given() {
    let project = query_project();

    let output = binary_in(&project, &["get", DECISION]);

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_a_warning_and_exit_zero_for_get_of_a_missing_node_when_no_format_is_given() {
    let project = query_project();

    let output = binary_in(&project, &["get", "checks.absent-probe"]);

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "⚠");
}

#[test]
fn it_should_print_agent_error_text_for_list_when_no_format_is_given() {
    let project = query_project();

    let output = binary_in(&project, &["list", "--limit", "0"]);

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_agent_error_text_for_get_when_no_format_is_given() {
    let project = query_project();

    let output = binary_in(&project, &["get", "Bad_Id"]);

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

// ---------------------------------------------------------------- CA-1: `human`

#[test]
fn it_should_print_human_text_for_list() {
    let project = query_project();

    let output = binary_in(&project, &["--format", "human", "list"]);

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_human_text_for_get() {
    let project = query_project();

    let output = binary_in(&project, &["--format", "human", "get", DECISION]);

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_a_human_warning_and_exit_zero_for_get_of_a_missing_node() {
    let project = query_project();

    let output = binary_in(
        &project,
        &["--format", "human", "get", "checks.absent-probe"],
    );

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "⚠");
}

#[test]
fn it_should_print_human_error_text_for_get() {
    let project = query_project();

    let output = binary_in(&project, &["--format", "human", "get", "Bad_Id"]);

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

// ---------------------------------------------------------------- CA-1: `json`

#[test]
fn it_should_print_a_single_json_line_for_list_with_format_json() {
    let project = query_project();

    let output = binary_in(&project, &["--format", "json", "list"]);

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stdout.trim_end()).expect("the line is JSON")["items"]
            .as_array()
            .map(Vec::len),
        Some(11)
    );
}

#[test]
fn it_should_print_a_single_json_line_for_get_with_format_json() {
    let project = query_project();

    let output = binary_in(&project, &["--format", "json", "get", DECISION]);

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stdout.trim_end()).expect("the line is JSON")["node"]
            ["id"],
        DECISION
    );
}

#[test]
fn it_should_print_a_null_node_in_json_for_get_of_a_missing_node() {
    let project = query_project();

    let output = binary_in(
        &project,
        &["--format", "json", "get", "checks.absent-probe"],
    );

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, "{\"node\":null}\n");
}

#[test]
fn it_should_print_a_single_json_error_line_for_get_with_format_json() {
    let project = query_project();

    let output = binary_in(&project, &["--format", "json", "get", "Bad_Id"]);

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stderr.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stderr.trim_end()).expect("the line is JSON")["error"]
            ["code"],
        "INVALID_INPUT"
    );
}
