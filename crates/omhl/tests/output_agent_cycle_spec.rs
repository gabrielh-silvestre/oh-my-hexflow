mod support;

use serde_json::Value;
use support::child::Output;
use support::cycle::{self, WORK_ITEM};
use support::project::Project;

// ---------------------------------------------------------------- ajudantes

fn run(project: &Project, format: Option<&str>, args: &[&str]) -> Output {
    let flag = format.into_iter().flat_map(|format| ["--format", format]);
    let args: Vec<&str> = flag.chain(args.iter().copied()).collect();

    support::child::omhl(&args)
        .cwd(project.dir())
        .xdg(project.xdg())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .run()
}

/// O stdout de um comando que tem de sair 0, sem nada no stderr.
fn printed(project: &Project, format: Option<&str>, args: &[&str]) -> String {
    let output = run(project, format, args);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");

    output.stdout
}

/// O stderr de um comando que tem de sair 1, sem nada no stdout.
fn refused(project: &Project, format: Option<&str>, args: &[&str]) -> String {
    let output = run(project, format, args);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(output.stdout, "");

    output.stderr
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn assert_text_starting_with(text: &str, symbol: &str) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(first_line(text).starts_with(symbol), "{text}");
}

fn assert_single_json_line(text: &str) {
    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
    serde_json::from_str::<Value>(text.trim_end()).expect("the line is JSON");
}

fn validate(project: &Project, format: Option<&str>) -> String {
    printed(project, format, &["spec", "validate", WORK_ITEM])
}

fn load(project: &Project, format: Option<&str>) -> String {
    printed(project, format, &["load"])
}

// ---------------------------------------------------------------- CA-1: sem `--format`, com `human` e com `json`

/// Os testes de CA-1 de um comando: `agent` (sem flag), `human` e `json`, no sucesso e no erro, com o símbolo da primeira linha.
macro_rules! default_formats {
    ($command:ident, $success:expr, $success_args:expr, $symbol:expr, $failure:expr, $failure_args:expr) => {
        mod $command {
            use super::*;

            #[test]
            fn it_should_print_agent_text_for_the_success_when_no_format_is_given() {
                let project = $success;

                let output = run(&project, None, $success_args);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_text_starting_with(&output.stdout, $symbol);
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_human_text_for_the_success() {
                let project = $success;

                let output = run(&project, Some("human"), $success_args);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_text_starting_with(&output.stdout, $symbol);
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_a_single_json_line_for_the_success_with_format_json() {
                let project = $success;

                let output = run(&project, Some("json"), $success_args);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_single_json_line(&output.stdout);
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_agent_error_text_when_no_format_is_given() {
                let project = $failure;

                let output = run(&project, None, $failure_args);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_human_error_text() {
                let project = $failure;

                let output = run(&project, Some("human"), $failure_args);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_a_single_json_error_line_with_format_json() {
                let project = $failure;

                let output = run(&project, Some("json"), $failure_args);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_single_json_line(&output.stderr);
                assert_eq!(output.stdout, "");
            }
        }
    };
}

default_formats!(
    spec_validate_command,
    cycle::spec_one_part(),
    &["spec", "validate", WORK_ITEM],
    "✔",
    cycle::spec_broken(),
    &["spec", "validate", WORK_ITEM]
);
default_formats!(
    load_command,
    cycle::load_fresh(),
    &["load"],
    "✔",
    cycle::load_invalid(),
    &["load"]
);

// ---------------------------------------------------------------- `spec validate`

#[test]
fn it_should_open_with_the_warning_symbol_when_a_pending_amendment_has_a_violation() {
    let project = cycle::spec_with_amendments();

    let text = validate(&project, None);

    assert_text_starting_with(&text, "⚠");
}

#[test]
fn it_should_print_one_line_per_part_and_amendment_with_the_violation_under_its_amendment() {
    let project = cycle::spec_with_amendments();

    let text = validate(&project, None);

    assert_eq!(
        lines_of(&text),
        [
            "⚠ spec validate · 2 parts · 2 amendments",
            "- part · parte-01.md",
            "- part · parte-02.md",
            "- amendment · emenda-01.md · approved",
            "- amendment · emenda-02.md · pending",
            "  - emenda-02.md:3 · amendment-divergence-unknown",
        ]
    );
}

#[test]
fn it_should_keep_the_spec_validate_items_in_one_human_block_after_the_header() {
    let project = cycle::spec_with_amendments();

    let text = validate(&project, Some("human"));

    assert_eq!(
        text,
        "⚠ spec validate · 2 parts · 2 amendments\n\n- part · parte-01.md\n- part · parte-02.md\n- amendment · emenda-01.md · approved\n- amendment · emenda-02.md · pending\n  - emenda-02.md:3 · amendment-divergence-unknown\n"
    );
}

#[test]
fn it_should_open_with_the_ok_symbol_when_the_spec_has_no_amendment() {
    let project = cycle::spec_one_part();

    let text = validate(&project, None);

    assert_eq!(
        lines_of(&text),
        [
            "✔ spec validate · 1 part · 0 amendments",
            "- part · parte-01.md"
        ]
    );
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_spec_is_valid() {
    let project = cycle::spec_with_amendments();

    let text = validate(&project, None);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_print_the_violations_of_a_refused_spec_as_error_details() {
    let project = cycle::spec_broken();

    let text = refused(&project, None, &["spec", "validate", WORK_ITEM]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ spec-invalid · the spec is invalid: 2 violation(s) shown, 0 omitted",
            "  - parte-01.md:7 · cited-id-unknown · the spec breaks the rule cited-id-unknown",
            "  - parte-02.md:19 · placeholder-found · the spec breaks the rule placeholder-found",
        ]
    );
}

#[test]
fn it_should_keep_the_spec_validate_json_bytes() {
    let project = cycle::spec_with_amendments();

    let text = validate(&project, Some("json"));

    assert_eq!(
        text,
        "{\"amendments\":[{\"name\":\"emenda-01.md\",\"state\":\"approved\",\"violations\":[]},{\"name\":\"emenda-02.md\",\"state\":\"pending\",\"violations\":[{\"code\":\"amendment-divergence-unknown\",\"path\":\"emenda-02.md:3\"}]}],\"ok\":true,\"parts\":[\"parte-01.md\",\"parte-02.md\"]}\n"
    );
}

#[test]
fn it_should_keep_the_spec_invalid_json_bytes() {
    let project = cycle::spec_broken();

    let text = refused(&project, Some("json"), &["spec", "validate", WORK_ITEM]);

    assert_eq!(
        text,
        "{\"error\":{\"code\":\"spec-invalid\",\"details\":[{\"code\":\"cited-id-unknown\",\"message\":\"the spec breaks the rule cited-id-unknown\",\"path\":\"parte-01.md:7\"},{\"code\":\"placeholder-found\",\"message\":\"the spec breaks the rule placeholder-found\",\"path\":\"parte-02.md:19\"}],\"message\":\"the spec is invalid: 2 violation(s) shown, 0 omitted\"}}\n"
    );
}

// ---------------------------------------------------------------- `load`

#[test]
fn it_should_open_with_the_warning_symbol_when_the_load_has_a_warning() {
    let project = cycle::load_full();

    let text = load(&project, None);

    assert_text_starting_with(&text, "⚠");
}

#[test]
fn it_should_print_the_counts_and_one_line_per_group_item() {
    let project = cycle::load_full();

    let text = load(&project, None);

    assert_eq!(
        lines_of(&text),
        [
            "⚠ loaded · 1 created · 1 revised · 2 unchanged · 1 skipped · 1 relation · 1 orphan · 1 orphan relation · 1 warning",
            "- created · checks.payroll-new",
            "- revised · checks.payroll-revise",
            "- orphan · checks.payroll-orphan",
            "- skipped · checks.payroll-retire · extension-retired",
            "- relation · checks.payroll-new after checks.payroll-keep",
            "- orphan relation · checks.payroll-linked after checks.payroll-keep",
            "- warning · .omhl/checks/.gitkeep · hidden-file-ignored · hidden file ignored",
        ]
    );
}

#[test]
fn it_should_open_a_human_block_for_the_counts_and_the_items() {
    let project = cycle::load_full();

    let text = load(&project, Some("human"));

    assert_eq!(
        text,
        "⚠ loaded · 1 created · 1 revised · 2 unchanged · 1 skipped · 1 relation · 1 orphan · 1 orphan relation · 1 warning\n\n- created · checks.payroll-new\n- revised · checks.payroll-revise\n- orphan · checks.payroll-orphan\n- skipped · checks.payroll-retire · extension-retired\n- relation · checks.payroll-new after checks.payroll-keep\n- orphan relation · checks.payroll-linked after checks.payroll-keep\n- warning · .omhl/checks/.gitkeep · hidden-file-ignored · hidden file ignored\n"
    );
}

#[test]
fn it_should_say_dry_run_instead_of_loaded_when_the_load_writes_nothing() {
    let project = cycle::load_fresh();

    let text = printed(&project, None, &["load", "--dry-run"]);

    assert_eq!(
        lines_of(&text),
        [
            "✔ dry run · 2 created · 0 revised · 0 unchanged · 1 relation",
            "- created · checks.payroll-base, checks.payroll-extra",
            "- relation · checks.payroll-extra after checks.payroll-base",
        ]
    );
}

#[test]
fn it_should_print_only_the_counts_when_the_load_finds_nothing_to_do() {
    let project = cycle::load_fresh();
    load(&project, Some("json"));

    let text = load(&project, None);

    assert_eq!(
        lines_of(&text),
        ["✔ loaded · 0 created · 0 revised · 2 unchanged"]
    );
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_load_has_orphans() {
    let project = cycle::load_full();

    let text = load(&project, None);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_print_the_extension_details_and_the_next_step_when_the_load_refuses_an_invalid_file() {
    let project = cycle::load_invalid();

    let text = refused(&project, None, &["load"]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ extensions-invalid · extensions are invalid: 1 error(s) shown, 0 omitted; nothing was loaded",
            "  - .omhl/guidelines/payroll-tone.toml · kind-missing · the guideline file has no kind",
            "Next: add kind = \"business\" or kind = \"architecture\" to the file and run omhl load again.",
        ]
    );
}

#[test]
fn it_should_keep_the_load_json_bytes() {
    let project = cycle::load_fresh();

    let text = load(&project, Some("json"));

    assert_eq!(
        text,
        "{\"created\":[\"checks.payroll-base\",\"checks.payroll-extra\"],\"dry_run\":false,\"orphan_relations\":[],\"orphans\":[],\"relations_created\":[{\"from\":\"checks.payroll-extra\",\"to\":\"checks.payroll-base\"}],\"revised\":[],\"skipped\":[],\"tenant\":\"acme\",\"unchanged\":[],\"warnings\":[]}\n"
    );
}

#[test]
fn it_should_keep_the_load_error_json_bytes() {
    let project = cycle::load_invalid();

    let text = refused(&project, Some("json"), &["load"]);

    assert_eq!(
        text,
        "{\"error\":{\"code\":\"extensions-invalid\",\"details\":[{\"code\":\"kind-missing\",\"message\":\"the guideline file has no kind\",\"next_step\":\"add kind = \\\"business\\\" or kind = \\\"architecture\\\" to the file and run omhl load again\",\"path\":\".omhl/guidelines/payroll-tone.toml\"}],\"message\":\"extensions are invalid: 1 error(s) shown, 0 omitted; nothing was loaded\"}}\n"
    );
}
