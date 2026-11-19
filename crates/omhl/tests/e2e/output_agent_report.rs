use crate::support;
use serde_json::Value;
use std::fs;
use support::child::{Output, omhl as binary};
use support::output_agent_report::{ANSWERS, RECURRENCES, context_text, retro_args, with_format};
use support::project::Project;
use support::query::query_project;
use support::report::{
    ABSENT_WORK_ITEM, answers_project, bare_project, recurrences_project, retro_project,
};

// ---------------------------------------------------------------- ajudantes

fn binary_in(project: &Project, args: &[&str]) -> Output {
    binary(args).cwd(project.dir()).xdg(project.xdg()).run()
}

/// Projeto sem marcador: todo comando recusa com `no-marker`.
fn no_marker_project() -> Project {
    let project = Project::new("acme");
    fs::remove_file(project.dir().join(".omhl.toml")).expect("the marker is removed");

    project
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn assert_text_starting_with(text: &str, start: &str) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(first_line(text).starts_with(start), "{text}");
}

fn assert_single_json_line(text: &str) {
    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
    serde_json::from_str::<Value>(text.trim_end()).expect("the line is JSON");
}

const RENDER: &[&str] = &["render"];

// ---------------------------------------------------------------- CA-1: `render`

#[test]
fn it_should_print_the_context_text_for_render_when_no_format_is_given() {
    let project = query_project();

    let output = binary_in(&project, &with_format(None, RENDER));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "# omhl context for tenant acme");
}

#[test]
fn it_should_print_the_context_text_for_render_with_format_human() {
    let project = query_project();

    let output = binary_in(&project, &with_format(Some("human"), RENDER));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "# omhl context for tenant acme");
}

#[test]
fn it_should_print_a_single_json_line_for_render_with_format_json() {
    let project = query_project();

    let output = binary_in(&project, &with_format(Some("json"), RENDER));

    assert_eq!(output.code, Some(0));
    assert_single_json_line(&output.stdout);
}

#[test]
fn it_should_print_agent_error_text_for_render_when_no_format_is_given() {
    let project = no_marker_project();

    let output = binary_in(&project, &with_format(None, RENDER));

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_human_error_text_for_render() {
    let project = no_marker_project();

    let output = binary_in(&project, &with_format(Some("human"), RENDER));

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_a_single_json_error_line_for_render_with_format_json() {
    let project = no_marker_project();

    let output = binary_in(&project, &with_format(Some("json"), RENDER));

    assert_eq!(output.code, Some(1));
    assert_single_json_line(&output.stderr);
}

// ---------------------------------------------------------------- CA-1: `report answers`

#[test]
fn it_should_print_agent_text_for_report_answers_when_no_format_is_given() {
    let project = answers_project();

    let output = binary_in(&project, &with_format(None, ANSWERS));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_human_text_for_report_answers() {
    let project = answers_project();

    let output = binary_in(&project, &with_format(Some("human"), ANSWERS));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_a_single_json_line_for_report_answers_with_format_json() {
    let project = answers_project();

    let output = binary_in(&project, &with_format(Some("json"), ANSWERS));

    assert_eq!(output.code, Some(0));
    assert_single_json_line(&output.stdout);
}

fn answers_unknown_work_item() -> Vec<&'static str> {
    vec!["report", "answers", "--work-item", ABSENT_WORK_ITEM]
}

#[test]
fn it_should_print_agent_error_text_for_report_answers_when_no_format_is_given() {
    let project = bare_project();

    let output = binary_in(&project, &with_format(None, &answers_unknown_work_item()));

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_human_error_text_for_report_answers() {
    let project = bare_project();

    let output = binary_in(
        &project,
        &with_format(Some("human"), &answers_unknown_work_item()),
    );

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_a_single_json_error_line_for_report_answers_with_format_json() {
    let project = bare_project();

    let output = binary_in(
        &project,
        &with_format(Some("json"), &answers_unknown_work_item()),
    );

    assert_eq!(output.code, Some(1));
    assert_single_json_line(&output.stderr);
}

// ---------------------------------------------------------------- CA-1: `report recurrences`

#[test]
fn it_should_print_agent_text_for_report_recurrences_when_no_format_is_given() {
    let project = recurrences_project();

    let output = binary_in(&project, &with_format(None, RECURRENCES));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_human_text_for_report_recurrences() {
    let project = recurrences_project();

    let output = binary_in(&project, &with_format(Some("human"), RECURRENCES));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_a_single_json_line_for_report_recurrences_with_format_json() {
    let project = recurrences_project();

    let output = binary_in(&project, &with_format(Some("json"), RECURRENCES));

    assert_eq!(output.code, Some(0));
    assert_single_json_line(&output.stdout);
}

#[test]
fn it_should_print_agent_error_text_for_report_recurrences_when_no_format_is_given() {
    let project = no_marker_project();

    let output = binary_in(&project, &with_format(None, RECURRENCES));

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_human_error_text_for_report_recurrences() {
    let project = no_marker_project();

    let output = binary_in(&project, &with_format(Some("human"), RECURRENCES));

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_a_single_json_error_line_for_report_recurrences_with_format_json() {
    let project = no_marker_project();

    let output = binary_in(&project, &with_format(Some("json"), RECURRENCES));

    assert_eq!(output.code, Some(1));
    assert_single_json_line(&output.stderr);
}

// ---------------------------------------------------------------- CA-1: `report retro`

#[test]
fn it_should_print_agent_text_for_report_retro_when_no_format_is_given() {
    let project = retro_project();

    let output = binary_in(&project, &with_format(None, &retro_args()));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_human_text_for_report_retro() {
    let project = retro_project();

    let output = binary_in(&project, &with_format(Some("human"), &retro_args()));

    assert_eq!(output.code, Some(0));
    assert_text_starting_with(&output.stdout, "✔");
}

#[test]
fn it_should_print_a_single_json_line_for_report_retro_with_format_json() {
    let project = retro_project();

    let output = binary_in(&project, &with_format(Some("json"), &retro_args()));

    assert_eq!(output.code, Some(0));
    assert_single_json_line(&output.stdout);
}

fn retro_unknown_work_item() -> Vec<&'static str> {
    vec!["report", "retro", ABSENT_WORK_ITEM]
}

#[test]
fn it_should_print_agent_error_text_for_report_retro_when_no_format_is_given() {
    let project = bare_project();

    let output = binary_in(&project, &with_format(None, &retro_unknown_work_item()));

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_human_error_text_for_report_retro() {
    let project = bare_project();

    let output = binary_in(
        &project,
        &with_format(Some("human"), &retro_unknown_work_item()),
    );

    assert_eq!(output.code, Some(1));
    assert_text_starting_with(&output.stderr, "✘");
}

#[test]
fn it_should_print_a_single_json_error_line_for_report_retro_with_format_json() {
    let project = bare_project();

    let output = binary_in(
        &project,
        &with_format(Some("json"), &retro_unknown_work_item()),
    );

    assert_eq!(output.code, Some(1));
    assert_single_json_line(&output.stderr);
}

#[test]
fn it_should_print_the_context_text_through_the_binary_without_an_extra_line_break() {
    let project = query_project();
    let text = context_text(&project);

    let output = binary_in(&project, &with_format(None, RENDER));

    assert_eq!(output.stdout, text);
}
