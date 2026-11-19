mod support;

use std::fs;

use serde_json::{Value, json};
use support::agent_author;
use support::child::{Output, omhl as binary};
use support::forge::forge;
use support::inproc::run_inproc;
use support::omhl::{node_item, work_item_data};
use support::project::Project;
use support::query::query_project;
use support::report::{
    ABSENT_WORK_ITEM, WORK_ITEM, answers_project, bare_project, recurrences_project, retro_project,
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

/// O stdout do comando em processo; o stderr tem de estar vazio.
fn stdout_of(project: &Project, args: &[&str]) -> String {
    let run = run_inproc(project, args, "");

    assert_eq!(run.stderr, "", "unexpected stderr for {args:?}");

    run.stdout
}

fn lines_of(project: &Project, args: &[&str]) -> Vec<String> {
    stdout_of(project, args)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn assert_line_printed(lines: &[String], expected: &str) {
    assert!(
        lines.iter().any(|line| line == expected),
        "`{expected}` is not in:\n{}",
        lines.join("\n")
    );
}

const RENDER: &[&str] = &["render"];
const ANSWERS: &[&str] = &["report", "answers"];
const RECURRENCES: &[&str] = &["report", "recurrences"];

fn retro_args() -> Vec<&'static str> {
    vec!["report", "retro", WORK_ITEM]
}

fn with_format<'a>(format: Option<&'a str>, args: &[&'a str]) -> Vec<&'a str> {
    let flag = format.into_iter().flat_map(|format| ["--format", format]);

    flag.chain(args.iter().copied()).collect()
}

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

// ---------------------------------------------------------------- `render`: os bytes do `text`

/// O `text` do contexto no `--format json`.
fn context_text(project: &Project) -> String {
    let json = stdout_of(project, &["--format", "json", "render"]);
    let value: Value = serde_json::from_str(json.trim_end()).expect("the output is JSON");

    value["text"]
        .as_str()
        .expect("the value has the text")
        .to_owned()
}

#[test]
fn it_should_print_the_json_text_in_the_agent_render_when_it_has_no_controls() {
    let project = query_project();
    let text = context_text(&project);

    let printed = stdout_of(&project, &["--format", "agent", "render"]);

    assert_eq!(printed, text);
}

#[test]
fn it_should_print_the_json_text_in_the_human_render_when_it_has_no_controls() {
    let project = query_project();
    let text = context_text(&project);

    let printed = stdout_of(&project, &["--format", "human", "render"]);

    assert_eq!(printed, text);
}

#[test]
fn it_should_print_the_same_bytes_in_the_agent_and_human_render() {
    let project = query_project();

    let agent = stdout_of(&project, &["--format", "agent", "render"]);
    let human = stdout_of(&project, &["--format", "human", "render"]);

    assert_eq!(agent, human);
}

#[test]
fn it_should_print_the_context_text_through_the_binary_without_an_extra_line_break() {
    let project = query_project();
    let text = context_text(&project);

    let output = binary_in(&project, &with_format(None, RENDER));

    assert_eq!(output.stdout, text);
}

// ---------------------------------------------------------------- `report answers`: layout

#[test]
fn it_should_print_one_agent_line_per_gap_with_its_state_and_work_item() {
    let project = answers_project();

    let lines = lines_of(&project, ANSWERS);

    assert_line_printed(&lines, "✔ report answers · 4 gaps");
    assert_line_printed(&lines, "gaps.rounding · open · of work-items.payroll");
}

#[test]
fn it_should_print_the_answering_premise_the_answerer_and_the_missing_criterion_in_the_agent_line()
{
    let project = answers_project();

    let lines = lines_of(&project, ANSWERS);

    assert_line_printed(
        &lines,
        "gaps.currency · answered · by premises.cents · human answer · no criterion · of work-items.payroll",
    );
}

#[test]
fn it_should_print_criterion_found_in_the_agent_line_of_a_gap_backed_by_a_directive() {
    let project = answers_project();

    let lines = lines_of(&project, ANSWERS);

    assert_line_printed(
        &lines,
        "gaps.timezone · answered · by premises.utc-rule · autonomous answer · criterion found · of work-items.payroll",
    );
}

#[test]
fn it_should_print_needs_human_without_an_answerer_in_the_agent_line_of_a_contested_gap() {
    let project = answers_project();

    let lines = lines_of(&project, ANSWERS);

    assert_line_printed(
        &lines,
        "gaps.locale · needs-human · by premises.dst-rule · of work-items.payroll",
    );
}

#[test]
fn it_should_print_an_empty_agent_answers_report_as_a_single_line() {
    let project = bare_project();

    let printed = stdout_of(&project, ANSWERS);

    assert_eq!(printed, "✔ report answers · none\n");
}

#[test]
fn it_should_not_print_a_next_step_in_the_agent_answers_report() {
    let project = answers_project();

    let printed = stdout_of(&project, ANSWERS);

    assert!(!printed.contains("Next:"), "{printed}");
}

#[test]
fn it_should_print_each_gap_of_the_human_answers_report_as_an_aligned_block() {
    let project = answers_project();

    let printed = stdout_of(&project, &with_format(Some("human"), ANSWERS));

    assert!(
        printed.contains(
            "gaps.currency\n  state:       answered\n  answered by: premises.cents\n  answerer:    human\n  criterion:   none\n  work item:   work-items.payroll\n"
        ),
        "{printed}"
    );
}

#[test]
fn it_should_print_criterion_found_in_the_human_block_of_a_gap_backed_by_a_directive() {
    let project = answers_project();

    let printed = stdout_of(&project, &with_format(Some("human"), ANSWERS));

    assert!(printed.contains("  criterion:   found\n"), "{printed}");
}

// ---------------------------------------------------------------- `report recurrences`: layout

#[test]
fn it_should_print_the_group_kind_and_text_in_the_agent_recurrences_report() {
    let project = recurrences_project();

    let lines = lines_of(&project, RECURRENCES);

    assert_line_printed(&lines, "✔ report recurrences · min 3 · 1 group");
    assert_line_printed(
        &lines,
        "- business · money lives in cents, see decisions.journal-mode",
    );
}

#[test]
fn it_should_print_the_work_items_and_premises_of_a_group_as_indented_comma_lists() {
    let project = recurrences_project();

    let lines = lines_of(&project, RECURRENCES);

    assert_line_printed(
        &lines,
        "  work items: work-items.payroll, work-items.invoice, work-items.ledger",
    );
    assert_line_printed(
        &lines,
        "  premises: premises.cents-payroll, premises.cents-invoice, premises.cents-ledger",
    );
}

#[test]
fn it_should_print_an_empty_agent_recurrences_report_as_a_single_line() {
    let project = bare_project();

    let printed = stdout_of(&project, RECURRENCES);

    assert_eq!(printed, "✔ report recurrences · min 3 · none\n");
}

#[test]
fn it_should_not_print_a_next_step_in_the_agent_recurrences_report() {
    let project = recurrences_project();

    let printed = stdout_of(&project, RECURRENCES);

    assert!(!printed.contains("Next:"), "{printed}");
}

#[test]
fn it_should_align_the_labels_of_a_group_in_the_human_recurrences_report() {
    let project = recurrences_project();

    let printed = stdout_of(&project, &with_format(Some("human"), RECURRENCES));

    assert!(
        printed.contains(
            "\n  work items: work-items.payroll, work-items.invoice, work-items.ledger\n  premises:   premises.cents-payroll, premises.cents-invoice, premises.cents-ledger\n"
        ),
        "{printed}"
    );
}

#[test]
fn it_should_separate_the_header_from_the_groups_with_a_blank_line_in_the_human_recurrences_report()
{
    let project = recurrences_project();

    let printed = stdout_of(&project, &with_format(Some("human"), RECURRENCES));

    assert!(
        printed.starts_with("✔ report recurrences · min 3 · 1 group\n\n- business"),
        "{printed}"
    );
}

// ---------------------------------------------------------------- `report retro`: layout

const RETRO_HEADER: &str = "✔ report retro · 3 divergences · 1 approved amendment · 1 pending amendment · 1 no-criterion gap · 1 open gap";
const RETRO_NEXT: &str =
    "Next: propose one improvement per item to the owner and change nothing without the answer.";

#[test]
fn it_should_summarize_every_kind_of_finding_in_the_agent_retro_header() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_eq!(lines[0], RETRO_HEADER);
}

#[test]
fn it_should_print_one_agent_line_per_divergence_group_with_kind_resolution_and_ids() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_line_printed(&lines, "- divergence · missing · open · divergences.audit");
    assert_line_printed(&lines, "- divergence · partial · open · divergences.report");
    assert_line_printed(
        &lines,
        "- divergence · partial · spec-stale · divergences.layout",
    );
}

#[test]
fn it_should_print_the_approved_amendment_with_its_divergence_in_the_agent_retro() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_line_printed(&lines, "- amendment · emenda-01.md · divergences.layout");
}

#[test]
fn it_should_print_the_pending_amendment_in_the_agent_retro() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_line_printed(&lines, "- pending amendment · emenda-02.md");
}

#[test]
fn it_should_print_the_gap_answered_without_criterion_with_its_premise_in_the_agent_retro() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_line_printed(
        &lines,
        "- no criterion · gaps.currency · answered by premises.utc-rule",
    );
}

#[test]
fn it_should_print_the_open_gap_with_its_state_in_the_agent_retro() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_line_printed(&lines, "- open gap · gaps.rounding · open");
}

#[test]
fn it_should_end_the_agent_retro_with_the_next_step() {
    let project = retro_project();

    let lines = lines_of(&project, &retro_args());

    assert_eq!(lines.last().map(String::as_str), Some(RETRO_NEXT));
}

#[test]
fn it_should_print_a_null_agent_retro_as_a_single_line_without_a_next_step() {
    let project = bare_project();

    let printed = stdout_of(&project, &retro_args());

    assert_eq!(printed, "✔ report retro · none\n");
}

#[test]
fn it_should_separate_the_header_from_the_findings_with_a_blank_line_in_the_human_retro() {
    let project = retro_project();

    let printed = stdout_of(&project, &with_format(Some("human"), &retro_args()));

    assert!(
        printed.starts_with(&format!("{RETRO_HEADER}\n\n- divergence")),
        "{printed}"
    );
}

#[test]
fn it_should_separate_the_findings_from_the_next_step_with_a_blank_line_in_the_human_retro() {
    let project = retro_project();

    let printed = stdout_of(&project, &with_format(Some("human"), &retro_args()));

    assert!(
        printed.ends_with(&format!(
            "- open gap · gaps.rounding · open\n\n{RETRO_NEXT}\n"
        )),
        "{printed}"
    );
}

const CONTROLLED_DECISION: &str = "decisions.controlled";

/// Um contexto cujo texto de decisão traz um OSC (`ESC ] 0 ; x BEL`) e uma quebra de linha.
fn controlled_project() -> Project {
    let project = Project::new("acme");
    let items = vec![
        node_item(WORK_ITEM, "work-item", work_item_data("Payroll rollout")),
        node_item(
            CONTROLLED_DECISION,
            "decision",
            json!({
                "text": "before\u{1b}]0;x\u{7}after\nsecond line",
                "evidence": [{ "source": "docs/storage.md", "summary": "The storage doc recommends it." }],
                "work_item": WORK_ITEM,
            }),
        ),
    ];
    forge(&project, agent_author(), items);

    project
}

fn without_controls(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

fn assert_render_drops_controls(format: &str) {
    let project = controlled_project();
    let text = context_text(&project);

    let printed = stdout_of(&project, &["--format", format, "render"]);

    assert!(text.contains("\u{1b}]0;x\u{7}"), "{text:?}");
    assert!(
        !printed
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "{printed:?}"
    );
    assert!(
        printed.contains("before]0;xafter\n  second line"),
        "{printed:?}"
    );
    assert_eq!(printed, without_controls(&text));
}

#[test]
fn it_should_drop_control_bytes_from_the_agent_render() {
    assert_render_drops_controls("agent");
}

#[test]
fn it_should_drop_control_bytes_from_the_human_render() {
    assert_render_drops_controls("human");
}
