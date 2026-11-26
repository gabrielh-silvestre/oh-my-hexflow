use crate::support;

use std::fs;

use ::omhl::adapters::text::SECRET_WARNING;
use regex::Regex;
use serde_json::Value;
use support::child::Output;
use support::cycle::{
    self, ORPHAN_CHECK, ORPHAN_GUIDELINE, SPEC_SLUG, SUCCESSOR_GUIDELINE, State, WORK_ITEM,
    default_formats, first_line, lines_of,
};
use support::project::Project;

// ---------------------------------------------------------------- ajudantes

fn plain(project: Project) -> State {
    State {
        project,
        program: None,
        sweep_guard: None,
    }
}

/// Projeto sem marcador: todo comando recusa com `no-marker`.
fn no_marker() -> State {
    let project = Project::new("acme");
    fs::remove_file(project.dir().join(".omhl.toml")).expect("the marker is removed");

    plain(project)
}

fn with_format<'a>(format: Option<&'a str>, args: &[&'a str]) -> Vec<&'a str> {
    let flag = format.into_iter().flat_map(|format| ["--format", format]);

    flag.chain(args.iter().copied()).collect()
}

fn run(state: &State, format: Option<&str>, args: &[&str]) -> Output {
    state.command(&with_format(format, args)).run()
}

/// O stdout de um comando que tem de sair 0, sem nada no stderr.
fn printed(state: &State, format: Option<&str>, args: &[&str]) -> String {
    let output = run(state, format, args);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");

    output.stdout
}

/// O stderr de um comando que tem de sair 1, sem nada no stdout.
fn refused(state: &State, format: Option<&str>, args: &[&str]) -> String {
    let output = run(state, format, args);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(output.stdout, "");

    output.stderr
}

// ---------------------------------------------------------------- CA-1: os 7 comandos sem `--format`, com `human` e com `json`

default_formats!(
    project,
    plain(cycle::project_first()),
    &["project"],
    plain(cycle::project_blocked()),
    &["project"]
);
default_formats!(
    check_sweep,
    State::swept(Project::new("acme")),
    &["check", "sweep"],
    no_marker(),
    &["check", "sweep"]
);
default_formats!(
    check_retire,
    plain(cycle::retire_check()),
    &["check", "retire", ORPHAN_CHECK],
    plain(Project::new("acme")),
    &["check", "retire", "checks.ghost-one"]
);
default_formats!(
    guideline_retire,
    plain(cycle::retire_guidelines()),
    &["guideline", "retire", ORPHAN_GUIDELINE],
    plain(Project::new("acme")),
    &["guideline", "retire", "guidelines.ghost-one"]
);
default_formats!(
    hooks_install,
    cycle::hooks_clean(),
    &["hooks", "install"],
    cycle::hooks_foreign(),
    &["hooks", "install"]
);
default_formats!(
    spec_init,
    plain(cycle::spec_repo()),
    &["spec", "init", WORK_ITEM, SPEC_SLUG],
    plain(cycle::spec_repo()),
    &["spec", "init", WORK_ITEM, "Bad_Slug"]
);
default_formats!(
    byproduct,
    plain(cycle::byproduct_repo()),
    &["byproduct", cycle::CHECK_WITH_LINES, WORK_ITEM],
    plain(cycle::byproduct_repo()),
    &["byproduct", cycle::CHECK_WITH_LINES, "work-items.ghost-one"]
);

// ---------------------------------------------------------------- `project`

#[test]
fn it_should_list_the_written_files_of_a_first_projection() {
    let state = plain(cycle::project_first());

    let text = printed(&state, None, &["project"]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ project · .omhl/projection · 2 written · 0 unchanged · 0 removed",
            "- written · work-items.alpha.md, work-items.bravo.md",
        ]
    );
}

#[test]
fn it_should_count_the_unchanged_files_without_listing_them() {
    let state = plain(cycle::project_mixed());

    let text = printed(&state, None, &["project"]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ project · .omhl/projection · 1 written · 2 unchanged · 1 removed",
            "- written · work-items.carol.md",
            "- removed · stale-note.md",
        ]
    );
}

#[test]
fn it_should_print_the_project_header_alone_when_nothing_changed() {
    let state = plain(cycle::project_first());
    printed(&state, None, &["project"]);

    let text = printed(&state, None, &["project"]);

    assert_eq!(
        lines_of(&text),
        ["✔ project · .omhl/projection · 0 written · 2 unchanged · 0 removed"]
    );
}

#[test]
fn it_should_separate_the_header_from_the_project_items_with_a_blank_line_in_human() {
    let state = plain(cycle::project_mixed());

    let text = printed(&state, Some("human"), &["project"]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ project · .omhl/projection · 1 written · 2 unchanged · 1 removed",
            "",
            "- written · work-items.carol.md",
            "- removed · stale-note.md",
        ]
    );
}

#[test]
fn it_should_name_the_hand_written_file_when_the_projection_is_refused() {
    let state = plain(cycle::project_blocked());

    let text = refused(&state, None, &["project"]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ projection-failed · the projection could not be written",
            "  - .omhl/projection/work-items.alpha.md · not-generated · the file was not generated by omhl",
        ]
    );
}

// ---------------------------------------------------------------- `check sweep`

#[test]
fn it_should_list_each_swept_process_with_its_command_pid_and_owner() {
    let (project, _guard) = cycle::sweep_stray();
    let state = plain(project);
    let item = Regex::new(r"^- sleep · pid \d+ · owner \d+$").expect("the item pattern compiles");

    let text = printed(&state, None, &["check", "sweep"]);

    assert!(
        lines_of(&text).iter().any(|line| item.is_match(line)),
        "{text}"
    );
}

#[test]
fn it_should_open_the_check_sweep_text_with_the_swept_and_unreadable_counts() {
    let (project, _guard) = cycle::sweep_stray();
    let state = plain(project);
    let header = Regex::new(r"^✔ check sweep · [1-9]\d* swept · \d+ unreadable$")
        .expect("the header pattern compiles");

    let text = printed(&state, None, &["check", "sweep"]);

    assert!(header.is_match(first_line(&text)), "{text}");
}

// ---------------------------------------------------------------- `check retire`

#[test]
fn it_should_print_the_retired_check() {
    let state = plain(cycle::retire_check());

    let text = printed(&state, None, &["check", "retire", ORPHAN_CHECK]);

    assert_eq!(
        lines_of(&text),
        ["✔ check retire · retired checks.ledger-audit"]
    );
}

#[test]
fn it_should_name_the_pointer_when_the_check_is_not_stored() {
    let state = plain(Project::new("acme"));

    let text = refused(&state, None, &["check", "retire", "checks.ghost-one"]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ check-not-found · check-not-found at /check",
            "  - /check · check-not-found",
        ]
    );
}

// ---------------------------------------------------------------- `guideline retire`

#[test]
fn it_should_print_the_retired_guideline_alone_when_there_is_no_successor() {
    let state = plain(cycle::retire_guidelines());

    let text = printed(&state, None, &["guideline", "retire", ORPHAN_GUIDELINE]);

    assert_eq!(
        lines_of(&text),
        ["✔ guideline retire · retired guidelines.brief-tone"]
    );
}

#[test]
fn it_should_print_the_successor_of_the_retired_guideline() {
    let state = plain(cycle::retire_guidelines());

    let text = printed(
        &state,
        None,
        &[
            "guideline",
            "retire",
            ORPHAN_GUIDELINE,
            "--by",
            SUCCESSOR_GUIDELINE,
        ],
    );

    assert_eq!(
        lines_of(&text),
        ["✔ guideline retire · retired guidelines.brief-tone · superseded by guidelines.kind-tone"]
    );
}

#[test]
fn it_should_close_the_guideline_refusal_with_the_next_step() {
    let state = plain(Project::new("acme"));

    let text = refused(
        &state,
        None,
        &["guideline", "retire", "guidelines.ghost-one"],
    );

    assert_eq!(
        lines_of(&text),
        [
            "✘ guideline-not-found · the id is not a current guideline stored in the repository",
            "  - /id",
            "Next: pass the id of a current guideline that is not native.",
        ]
    );
}

// ---------------------------------------------------------------- `hooks install`

/// Os fatos do JSON de uma instalação, para montar o texto esperado: `hooks install` repete o mesmo resultado.
fn installed_json(state: &State) -> Value {
    let text = printed(state, Some("json"), &["hooks", "install"]);

    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn text_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().expect("the key is text")
}

#[test]
fn it_should_print_the_hooks_install_paths_without_a_warning() {
    let state = cycle::hooks_clean();
    let value = installed_json(&state);

    let text = printed(&state, None, &["hooks", "install"]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ hooks install · 4 scripts".to_owned(),
            format!("  hooks path: {}", text_at(&value, "hooks_path")),
            format!("  omhl: {}", text_at(&value, "omhl")),
            "  scripts: pre-commit, pre-merge-commit, pre-push, post-commit".to_owned(),
        ]
    );
}

#[test]
fn it_should_open_hooks_install_with_a_warning_symbol_and_the_warning_count() {
    let state = cycle::hooks_in_build_dir();

    let text = printed(&state, None, &["hooks", "install"]);

    assert_eq!(first_line(&text), "⚠ hooks install · 4 scripts · 1 warning");
}

#[test]
fn it_should_print_the_warning_code_and_binary_path_as_a_dash_item() {
    let state = cycle::hooks_in_build_dir();
    let value = installed_json(&state);

    let text = printed(&state, None, &["hooks", "install"]);

    assert!(
        lines_of(&text).contains(
            &format!(
                "- warning · binary-in-build-dir · {}",
                text_at(&value, "omhl")
            )
            .as_str()
        ),
        "{text}"
    );
}

#[test]
fn it_should_close_hooks_install_with_the_hint_of_the_warning_as_the_next_step() {
    let state = cycle::hooks_in_build_dir();
    let value = installed_json(&state);
    let hint = text_at(&value["warnings"][0], "hint");

    let text = printed(&state, None, &["hooks", "install"]);

    assert_eq!(text.lines().last(), Some(format!("Next: {hint}.").as_str()));
}

#[test]
fn it_should_not_suggest_a_next_step_when_hooks_install_has_no_warning() {
    let state = cycle::hooks_clean();

    let text = printed(&state, None, &["hooks", "install"]);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_align_the_hooks_install_labels_in_human() {
    let state = cycle::hooks_clean();
    let value = installed_json(&state);

    let text = printed(&state, Some("human"), &["hooks", "install"]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ hooks install · 4 scripts".to_owned(),
            format!("  hooks path: {}", text_at(&value, "hooks_path")),
            format!("  omhl:       {}", text_at(&value, "omhl")),
            "  scripts:    pre-commit, pre-merge-commit, pre-push, post-commit".to_owned(),
        ]
    );
}

#[test]
fn it_should_keep_the_warning_and_the_next_step_in_the_human_hooks_install() {
    let state = cycle::hooks_in_build_dir();

    let text = printed(&state, Some("human"), &["hooks", "install"]);

    assert!(first_line(&text).starts_with("⚠ hooks install"), "{text}");
    assert!(
        text.contains("- warning · binary-in-build-dir · "),
        "{text}"
    );
    assert!(text.contains("\nNext: install an omhl binary"), "{text}");
}

#[test]
fn it_should_refuse_a_foreign_hook_with_the_error_code_alone() {
    let state = cycle::hooks_foreign();

    let text = refused(&state, None, &["hooks", "install"]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ hooks-existing · the repository already has hooks that would be disabled",
            "  - hooks-existing",
        ]
    );
}

// ---------------------------------------------------------------- `spec init`

fn spec_dir_of(state: &State) -> String {
    let main = fs::canonicalize(state.project.dir()).expect("the project directory exists");

    format!("{}/.omhl/specs/wi-payroll__{SPEC_SLUG}", main.display())
}

#[test]
fn it_should_print_the_spec_folder_and_part_and_ask_to_fill_the_sections() {
    let state = plain(cycle::spec_repo());
    let dir = spec_dir_of(&state);

    let text = printed(&state, None, &["spec", "init", WORK_ITEM, SPEC_SLUG]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ spec init · work-items.payroll · export-reports".to_owned(),
            format!("  dir: {dir}"),
            format!("  path: {dir}/part-01.md"),
            "Next: fill in the five sections of the part file, then run omhl spec validate work-items.payroll."
                .to_owned(),
        ]
    );
}

#[test]
fn it_should_align_the_spec_init_labels_in_human() {
    let state = plain(cycle::spec_repo());
    let dir = spec_dir_of(&state);

    let text = printed(
        &state,
        Some("human"),
        &["spec", "init", WORK_ITEM, SPEC_SLUG],
    );

    assert!(text.contains(&format!("\n  dir:  {dir}\n")), "{text}");
    assert!(
        text.contains(&format!("\n  path: {dir}/part-01.md\n")),
        "{text}"
    );
}

#[test]
fn it_should_name_the_slug_pointer_when_the_slug_is_invalid() {
    let state = plain(cycle::spec_repo());

    let text = refused(&state, None, &["spec", "init", WORK_ITEM, "Bad_Slug"]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ spec-slug-invalid · the slug must have 1 to 64 characters of a-z, 0-9 and -, and not start with -",
            "  - /slug",
        ]
    );
}

// ---------------------------------------------------------------- `byproduct`

#[test]
fn it_should_print_the_stored_lines_and_the_secret_warning() {
    let state = plain(cycle::byproduct_repo());

    let text = printed(
        &state,
        None,
        &["byproduct", cycle::CHECK_WITH_LINES, WORK_ITEM],
    );

    assert_eq!(
        lines_of(&text),
        [
            "✔ byproduct · checks.payroll-lines · work-items.payroll · passed · 3 lines",
            "  first-line-out",
            "  second-line-out",
            "  err-line-here",
            SECRET_WARNING,
        ]
    );
}

#[test]
fn it_should_keep_the_secret_warning_in_the_agent_text_when_the_output_has_no_lines() {
    let state = plain(cycle::byproduct_repo());

    let text = printed(
        &state,
        None,
        &["byproduct", cycle::CHECK_WITHOUT_OUTPUT, WORK_ITEM],
    );

    assert_eq!(
        lines_of(&text),
        [
            "✔ byproduct · checks.payroll-quiet · work-items.payroll · passed · none",
            SECRET_WARNING,
        ]
    );
}

#[test]
fn it_should_keep_the_secret_warning_in_the_human_text_when_the_output_has_no_lines() {
    let state = plain(cycle::byproduct_repo());

    let text = printed(
        &state,
        Some("human"),
        &["byproduct", cycle::CHECK_WITHOUT_OUTPUT, WORK_ITEM],
    );

    assert!(text.contains(SECRET_WARNING), "{text}");
}

#[test]
fn it_should_keep_the_secret_warning_in_the_human_text_with_lines() {
    let state = plain(cycle::byproduct_repo());

    let text = printed(
        &state,
        Some("human"),
        &["byproduct", cycle::CHECK_WITH_LINES, WORK_ITEM],
    );

    assert!(text.trim_end().ends_with(SECRET_WARNING), "{text}");
}

#[test]
fn it_should_report_the_lines_cut_short_before_the_secret_warning() {
    let state = plain(cycle::byproduct_repo());

    let text = printed(
        &state,
        None,
        &["byproduct", cycle::CHECK_WITH_WIDE_LINE, WORK_ITEM],
    );

    let tail = lines_of(&text);
    assert_eq!(
        &tail[tail.len() - 2..],
        ["⚠ 1 line cut short", SECRET_WARNING]
    );
}

#[test]
fn it_should_not_report_a_cut_line_when_no_line_passed_the_limit() {
    let state = plain(cycle::byproduct_repo());

    let text = printed(
        &state,
        None,
        &["byproduct", cycle::CHECK_WITH_LINES, WORK_ITEM],
    );

    assert!(!text.contains("cut short"), "{text}");
}

#[test]
fn it_should_not_print_the_secret_warning_when_there_is_no_stored_output() {
    let state = plain(cycle::byproduct_repo());

    let text = refused(
        &state,
        None,
        &["byproduct", cycle::CHECK_WITH_LINES, "work-items.ghost-one"],
    );

    assert!(!text.contains("secrets"), "{text}");
}

#[test]
fn it_should_name_the_work_item_pointer_when_the_attestation_is_missing() {
    let state = plain(cycle::byproduct_repo());

    let text = refused(
        &state,
        None,
        &["byproduct", cycle::CHECK_WITH_LINES, "work-items.ghost-one"],
    );

    assert_eq!(
        lines_of(&text),
        [
            "✘ attestation-not-found · no attestation exists for this check and work item",
            "  - /work_item",
        ]
    );
}
