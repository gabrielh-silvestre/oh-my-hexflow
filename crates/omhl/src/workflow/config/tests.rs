use std::collections::BTreeMap;
use std::path::Path;

use crate::project::ProjectError;
use crate::test_support::delivery_gate::{CI_GREEN, VERIFY_ANCHORING};
use crate::test_support::omhl::RUN_TESTS;
use crate::workflow::branch_policy::{BranchConfig, BranchFormat, ExceptionMode, ExceptionRule};
use crate::workflow::config::{
    InterviewConfig, InterviewWeights, PromotionConfig, RequireAtom, SpecsConfig, WorkflowConfig,
    native_run_digest, parse_evaluated_workflow, parse_workflow, raw_native_run_digest,
};

const CONFIG_LOOSENING_REVIEW: &str = "checks.config-loosening-review";
const WORKFLOW_PATH: &str = "/project/.omhl/workflow.toml";
const CANARY: &str = "CANARY-7f3a91";
const ERROR_CODE: &str = "invalid-workflow-config";

fn workflow_path() -> &'static Path {
    Path::new(WORKFLOW_PATH)
}

fn lines(rows: &[&str]) -> String {
    format!("{}\n", rows.join("\n"))
}

fn parse(text: &str) -> Result<WorkflowConfig, ProjectError> {
    parse_workflow(workflow_path(), text.as_bytes())
}

fn accepted(text: &str) -> WorkflowConfig {
    parse(text).expect("the workflow file is accepted")
}

fn refusal(text: &str) -> ProjectError {
    parse(text).expect_err("the workflow file is refused")
}

fn assert_refused_at_line(error: &ProjectError, line: usize) {
    assert_eq!(error.code, ERROR_CODE);
    assert!(
        error.message.contains(WORKFLOW_PATH),
        "the message names the file path: {}",
        error.message
    );
    assert!(
        error.message.contains(&format!("line {line}, column ")),
        "the message carries line {line} and a column: {}",
        error.message
    );
}

fn prefix_table(rows: &[(&str, &[RequireAtom])]) -> BTreeMap<String, Vec<RequireAtom>> {
    rows.iter()
        .map(|(prefix, atoms)| ((*prefix).to_owned(), atoms.to_vec()))
        .collect()
}

// Padrões (decisão g): sem arquivo, valem os valores compilados

#[test]
fn it_should_default_the_specs_section_to_the_oh_my_hexflow_values() {
    let config = WorkflowConfig::default();

    assert_eq!(
        config.specs,
        SpecsConfig {
            dir: ".omhl/specs".to_owned(),
            max_lines: 300,
            placeholders: ["TODO", "TBD", "FIXME", "???"].map(str::to_owned).to_vec(),
            cite_business_premise: true,
        }
    );
}

#[test]
fn it_should_default_the_promotion_threshold_to_three_work_items() {
    let config = WorkflowConfig::default();

    assert_eq!(config.promotion, PromotionConfig { min_work_items: 3 });
}

#[test]
fn it_should_default_the_prefix_table_to_the_seven_documented_prefixes() {
    use RequireAtom::{AgentPremise, BusinessPremise, Spec, TechnicalPremiseConfirmed};

    let config = WorkflowConfig::default();

    assert_eq!(
        config.prefixes,
        prefix_table(&[
            ("feat", &[BusinessPremise, Spec]),
            ("fix", &[BusinessPremise, Spec]),
            ("refactor", &[TechnicalPremiseConfirmed]),
            ("tests", &[TechnicalPremiseConfirmed]),
            ("bugfix", &[AgentPremise]),
            ("chore", &[]),
            ("docs", &[]),
        ])
    );
}

#[test]
fn it_should_read_a_file_with_only_the_version_as_the_defaults() {
    let config = accepted("version = 1\n");

    assert_eq!(config, WorkflowConfig::default());
}

#[test]
fn it_should_read_the_four_require_atoms_by_their_kebab_case_names() {
    let text = lines(&[
        "version = 1",
        "[prefix.chore]",
        "require = [\"business-premise\", \"technical-premise-confirmed\", \"agent-premise\", \"spec\"]",
    ]);

    let config = accepted(&text);

    assert_eq!(
        config.prefixes["chore"],
        vec![
            RequireAtom::BusinessPremise,
            RequireAtom::TechnicalPremiseConfirmed,
            RequireAtom::AgentPremise,
            RequireAtom::Spec,
        ]
    );
}

// Arquivo parcial: sobrescreve só o que traz

#[test]
fn it_should_override_only_the_keys_the_file_carries() {
    let text = lines(&["version = 1", "[specs]", "max_lines = 150"]);

    let config = accepted(&text);

    let mut expected = WorkflowConfig::default();
    expected.specs.max_lines = 150;
    assert_eq!(config, expected);
}

#[test]
fn it_should_replace_the_placeholder_list_instead_of_extending_it() {
    let text = lines(&["version = 1", "[specs]", "placeholders = [\"WIP\"]"]);

    let config = accepted(&text);

    assert_eq!(config.specs.placeholders, vec!["WIP".to_owned()]);
}

#[test]
fn it_should_read_a_custom_specs_dir() {
    let text = lines(&["version = 1", "[specs]", "dir = \"docs/specs\""]);

    let config = accepted(&text);

    assert_eq!(config.specs.dir, "docs/specs");
}

#[test]
fn it_should_read_a_custom_promotion_threshold() {
    let text = lines(&["version = 1", "[promotion]", "min_work_items = 5"]);

    let config = accepted(&text);

    assert_eq!(config.promotion, PromotionConfig { min_work_items: 5 });
}

#[test]
fn it_should_add_the_requirements_of_a_listed_default_prefix_to_its_defaults() {
    let text = lines(&["version = 1", "[prefix.refactor]", "require = [\"spec\"]"]);

    let config = accepted(&text);

    assert_eq!(
        config.prefixes["refactor"],
        vec![RequireAtom::TechnicalPremiseConfirmed, RequireAtom::Spec]
    );
    assert_eq!(
        config.prefixes["feat"],
        WorkflowConfig::default().prefixes["feat"]
    );
}

#[test]
fn it_should_keep_the_defaults_when_a_listed_prefix_requires_nothing_more() {
    let text = lines(&["version = 1", "[prefix.feat]", "require = []"]);

    let config = accepted(&text);

    assert_eq!(
        config.prefixes["feat"],
        vec![RequireAtom::BusinessPremise, RequireAtom::Spec]
    );
}

#[test]
fn it_should_give_a_default_prefix_with_no_defaults_exactly_the_listed_atoms() {
    let text = lines(&["version = 1", "[prefix.chore]", "require = [\"spec\"]"]);

    let config = accepted(&text);

    assert_eq!(config.prefixes["chore"], vec![RequireAtom::Spec]);
}

#[test]
fn it_should_give_a_new_prefix_exactly_the_listed_atoms() {
    let text = lines(&["version = 1", "[prefix.release]", "require = [\"spec\"]"]);

    let config = accepted(&text);

    assert_eq!(config.prefixes["release"], vec![RequireAtom::Spec]);
}

#[test]
fn it_should_add_a_prefix_the_defaults_do_not_have() {
    let text = lines(&["version = 1", "[prefix.perf]", "require = []"]);

    let config = accepted(&text);

    let mut expected = WorkflowConfig::default();
    expected.prefixes.insert("perf".to_owned(), Vec::new());
    assert_eq!(config, expected);
}

// AC 14: o valor vem do texto, não do binário

#[test]
fn it_should_read_a_different_max_lines_when_the_text_changes() {
    let with = |value: u32| lines(&["version = 1", "[specs]", &format!("max_lines = {value}")]);

    let read = (
        accepted(&with(300)).specs.max_lines,
        accepted(&with(120)).specs.max_lines,
    );

    assert_eq!(read, (300, 120));
}

// Recusas: código estável, caminho e posição (linha e coluna)

#[test]
fn it_should_refuse_an_unknown_top_level_key_with_its_position() {
    let text = lines(&["version = 1", "colour = true"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 2);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_specs_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "colour = true"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_promotion_with_its_position() {
    let text = lines(&["version = 1", "[promotion]", "colour = true"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_a_prefix_table_with_its_position() {
    let text = lines(&["version = 1", "[prefix.feat]", "colour = true"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_an_unknown_top_level_table_with_its_position() {
    let text = lines(&["version = 1", "[bogus]", "x = 1"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 2);
}

#[test]
fn it_should_refuse_max_lines_zero_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "max_lines = 0"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_min_work_items_zero_with_its_position() {
    let text = lines(&["version = 1", "[promotion]", "min_work_items = 0"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_a_negative_max_lines_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "max_lines = -5"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_an_absolute_specs_dir_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "dir = \"/etc/specs\""]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_a_specs_dir_that_leaves_the_main_directory_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "dir = \"../outside\""]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_a_specs_dir_with_a_parent_component_in_the_middle_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "dir = \"docs/../specs\""]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

fn specs_dir_text(dir: &str) -> String {
    lines(&["version = 1", "[specs]", &format!("dir = \"{dir}\"")])
}

#[test]
fn it_should_refuse_a_specs_dir_without_a_folder_name_with_its_position() {
    for dir in ["", ".", "./"] {
        let text = specs_dir_text(dir);

        let error = refusal(&text);

        assert_refused_at_line(&error, 3);
    }
}

#[test]
fn it_should_refuse_a_specs_dir_that_is_the_git_or_the_omhl_root_with_its_position() {
    for dir in [".git", ".omhl"] {
        let text = specs_dir_text(dir);

        let error = refusal(&text);

        assert_refused_at_line(&error, 3);
    }
}

#[test]
fn it_should_refuse_a_specs_dir_that_is_a_subtree_read_by_the_extensions_scan_with_its_position() {
    for dir in [
        ".omhl/checks",
        ".omhl/guidelines",
        ".omhl/projection",
        ".omhl/workflow.toml",
    ] {
        let text = specs_dir_text(dir);

        let error = refusal(&text);

        assert_refused_at_line(&error, 3);
    }
}

#[test]
fn it_should_refuse_a_specs_dir_nested_inside_the_git_dir_or_a_reserved_subtree_with_its_position()
{
    for dir in [".git/specs", ".omhl/checks/specs", ".omhl/guidelines/specs"] {
        let text = specs_dir_text(dir);

        let error = refusal(&text);

        assert_refused_at_line(&error, 3);
    }
}

#[test]
fn it_should_accept_the_default_specs_dir_written_out() {
    let text = specs_dir_text(".omhl/specs");

    let config = accepted(&text);

    assert_eq!(config.specs.dir, ".omhl/specs");
}

#[test]
fn it_should_accept_a_specs_dir_with_a_folder_name_outside_the_omhl_directory() {
    let text = specs_dir_text("docs/specs");

    let config = accepted(&text);

    assert_eq!(config.specs.dir, "docs/specs");
}

#[test]
fn it_should_refuse_an_unknown_require_atom_with_its_position() {
    let text = lines(&[
        "version = 1",
        "[prefix.feat]",
        "require = [\"spec\", \"no-such-atom\"]",
    ]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_a_value_of_the_wrong_type_with_its_position() {
    let text = lines(&["version = 1", "[specs]", "max_lines = \"many\""]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_a_version_other_than_one_with_its_position() {
    let error = refusal("version = 2\n");

    assert_refused_at_line(&error, 1);
}

#[test]
fn it_should_refuse_a_file_without_a_version() {
    let text = lines(&["[specs]", "max_lines = 150"]);

    let error = refusal(&text);

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_report_the_position_of_a_toml_syntax_error() {
    let text = lines(&["version = 1", "[specs]", "max_lines = = 3"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_bytes_that_are_not_utf8() {
    let error = parse_workflow(workflow_path(), b"version = 1\n\xff\xfe")
        .expect_err("the bytes are refused");

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_not_echo_the_file_content_in_any_refusal() {
    let refused = [
        lines(&["version = 1", &format!("colour = \"{CANARY}\"")]),
        lines(&[
            "version = 1",
            "[prefix.feat]",
            &format!("require = [\"{CANARY}\"]"),
        ]),
        lines(&[
            "version = 1",
            "[specs]",
            &format!("max_lines = \"{CANARY}\""),
        ]),
        lines(&["version = 1", "[specs]", &format!("dir = {CANARY}")]),
        lines(&["version = 1", "[specs]", &format!("dir = \"/{CANARY}\"")]),
        lines(&["version = 1", "[specs]", &format!("dir = \"../{CANARY}\"")]),
    ];

    let messages: Vec<String> = refused.iter().map(|text| refusal(text).message).collect();

    let leaked: Vec<&String> = messages
        .iter()
        .filter(|message| message.contains(CANARY))
        .collect();
    assert!(
        leaked.is_empty(),
        "messages repeat the file content: {leaked:?}"
    );
}

fn native_checks_text(list: &str) -> String {
    lines(&["version = 1", "[native]", &format!("checks = {list}")])
}

fn enabled(text: &str) -> Vec<&'static str> {
    accepted(text).native.checks.into_iter().collect()
}

#[test]
fn it_should_enable_no_native_check_without_the_native_table() {
    let config = accepted("version = 1\n");

    assert!(config.native.checks.is_empty());
}

#[test]
fn it_should_enable_no_native_check_with_an_empty_native_table() {
    let text = lines(&["version = 1", "[native]"]);

    let config = accepted(&text);

    assert!(config.native.checks.is_empty());
}

#[test]
fn it_should_enable_no_native_check_with_an_empty_list() {
    let found = enabled(&native_checks_text("[]"));

    assert!(found.is_empty());
}

#[test]
fn it_should_enable_only_the_listed_native_check_when_each_id_is_alone() {
    let found: Vec<Vec<&str>> = [RUN_TESTS, CI_GREEN, VERIFY_ANCHORING]
        .iter()
        .map(|id| enabled(&native_checks_text(&format!("[\"{id}\"]"))))
        .collect();

    assert_eq!(
        found,
        vec![vec![RUN_TESTS], vec![CI_GREEN], vec![VERIFY_ANCHORING]]
    );
}

#[test]
fn it_should_enable_the_three_native_checks_when_all_ids_are_listed() {
    let text = native_checks_text(&format!(
        "[\"{VERIFY_ANCHORING}\", \"{RUN_TESTS}\", \"{CI_GREEN}\"]"
    ));

    let mut found = enabled(&text);

    found.sort_unstable();
    assert_eq!(found, vec![CI_GREEN, RUN_TESTS, VERIFY_ANCHORING]);
}

#[test]
fn it_should_enable_the_config_loosening_review_when_its_id_is_listed() {
    let text = native_checks_text(&format!("[\"{CONFIG_LOOSENING_REVIEW}\"]"));

    let found = enabled(&text);

    assert_eq!(found, vec![CONFIG_LOOSENING_REVIEW]);
}

#[test]
fn it_should_keep_the_other_native_checks_off_when_only_the_config_loosening_review_is_listed() {
    let text = native_checks_text(&format!("[\"{CONFIG_LOOSENING_REVIEW}\"]"));

    let found = enabled(&text);

    assert!(!found.contains(&RUN_TESTS) && !found.contains(&CI_GREEN));
}

#[test]
fn it_should_accept_a_repeated_native_check_id_as_a_single_activation() {
    let text = native_checks_text(&format!("[\"{RUN_TESTS}\", \"{RUN_TESTS}\"]"));

    let found = enabled(&text);

    assert_eq!(found, vec![RUN_TESTS]);
}

#[test]
fn it_should_keep_the_other_keys_when_native_checks_are_enabled() {
    let text = lines(&[
        "version = 1",
        "[specs]",
        "max_lines = 150",
        "[native]",
        &format!("checks = [\"{CI_GREEN}\"]"),
    ]);

    let config = accepted(&text);

    let mut expected = WorkflowConfig::default();
    expected.specs.max_lines = 150;
    expected.native.checks.insert(CI_GREEN);
    assert_eq!(config, expected);
}

#[test]
fn it_should_refuse_an_unknown_native_check_id_with_its_position() {
    let error = refusal(&native_checks_text("[\"checks.no-such-check\"]"));

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_each_former_native_guideline_id_in_native_checks_as_unknown_with_its_position()
{
    let errors: Vec<ProjectError> = [
        "guidelines.one-decision-per-turn",
        "guidelines.commit-policy",
    ]
    .iter()
    .map(|id| refusal(&native_checks_text(&format!("[\"{id}\"]"))))
    .collect();

    for error in &errors {
        assert_refused_at_line(error, 3);
    }
}

#[test]
fn it_should_refuse_a_project_check_id_in_native_checks_with_its_position() {
    let error = refusal(&native_checks_text("[\"checks.my-project-check\"]"));

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_a_wrong_id_after_a_valid_one_in_native_checks() {
    let text = native_checks_text(&format!("[\"{RUN_TESTS}\", \"checks.no-such-check\"]"));

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_native_checks_that_is_not_a_list_with_its_position() {
    let error = refusal(&native_checks_text(&format!("\"{RUN_TESTS}\"")));

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_native_checks_with_a_non_text_item_with_its_position() {
    let error = refusal(&native_checks_text("[1]"));

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_native_with_its_position() {
    let text = lines(&["version = 1", "[native]", "guidelines = []"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_not_echo_a_refused_native_check_id() {
    let error = refusal(&native_checks_text(&format!("[\"{CANARY}\"]")));

    assert!(
        !error.message.contains(CANARY),
        "the message repeats the file content: {}",
        error.message
    );
}

// Comando do nativo executável: [native.run."checks.run-tests"]

const RUN_TABLE: &str = "[native.run.\"checks.run-tests\"]";

fn run_text(rows: &[&str]) -> String {
    let mut all = vec!["version = 1", RUN_TABLE];
    all.extend_from_slice(rows);
    lines(&all)
}

fn run_text_for(id: &str, rows: &[&str]) -> String {
    let header = format!("[native.run.\"{id}\"]");
    let mut all = vec!["version = 1", header.as_str()];
    all.extend_from_slice(rows);
    lines(&all)
}

fn run_digest(rows: &[&str]) -> String {
    let config = accepted(&run_text(rows));
    native_run_digest(&config.native.run[RUN_TESTS])
}

#[test]
fn it_should_accept_the_run_command_table_of_the_run_tests_native() {
    let config = accepted(&run_text(&[
        "run = [\"cargo\", \"test\"]",
        "timeout_secs = 600",
        "expect = [{ exit = 0, outcome = \"passed\" }]",
    ]));

    let entry = &config.native.run[RUN_TESTS];
    assert_eq!(entry.run, ["cargo", "test"]);
    assert_eq!(entry.timeout_secs, 600);
}

#[test]
fn it_should_have_no_run_command_table_by_default() {
    let config = WorkflowConfig::default();

    assert!(config.native.run.is_empty());
}

#[test]
fn it_should_apply_the_default_expect_when_the_table_omits_it() {
    let omitted = run_digest(&["run = [\"cargo\", \"test\"]", "timeout_secs = 600"]);

    let explicit = run_digest(&[
        "run = [\"cargo\", \"test\"]",
        "timeout_secs = 600",
        "expect = [{ exit = 0, outcome = \"passed\" }]",
    ]);

    assert_eq!(omitted, explicit);
}

#[test]
fn it_should_refuse_the_run_table_of_each_instruction_only_native() {
    for id in [
        CI_GREEN,
        VERIFY_ANCHORING,
        CONFIG_LOOSENING_REVIEW,
        "checks.no-such-check",
    ] {
        let text = run_text_for(id, &["run = [\"true\"]", "timeout_secs = 60"]);

        assert_eq!(refusal(&text).code, ERROR_CODE, "{id}");
    }
}

#[test]
fn it_should_refuse_a_run_table_with_an_empty_run() {
    let error = refusal(&run_text(&["run = []", "timeout_secs = 60"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_run_table_with_sixty_five_run_items() {
    let items = vec!["\"x\""; 65].join(", ");

    let error = refusal(&run_text(&[
        &format!("run = [{items}]"),
        "timeout_secs = 60",
    ]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_run_table_without_timeout_secs() {
    let error = refusal(&run_text(&["run = [\"true\"]"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_run_table_with_a_malformed_expect() {
    let error = refusal(&run_text(&[
        "run = [\"true\"]",
        "timeout_secs = 60",
        "expect = [{ exit = 0, outcome = \"no-such-outcome\" }]",
    ]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_run_table_with_an_unknown_key() {
    let error = refusal(&run_text(&[
        "run = [\"true\"]",
        "timeout_secs = 60",
        "shell = true",
    ]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_give_a_different_native_digest_when_run_changes() {
    let base = run_digest(&["run = [\"cargo\", \"test\"]", "timeout_secs = 600"]);

    let changed = run_digest(&[
        "run = [\"cargo\", \"test\", \"--lib\"]",
        "timeout_secs = 600",
    ]);

    assert_ne!(base, changed);
}

#[test]
fn it_should_give_a_different_native_digest_when_timeout_secs_changes() {
    let base = run_digest(&["run = [\"cargo\", \"test\"]", "timeout_secs = 600"]);

    let changed = run_digest(&["run = [\"cargo\", \"test\"]", "timeout_secs = 601"]);

    assert_ne!(base, changed);
}

#[test]
fn it_should_give_a_different_native_digest_when_the_effective_expect_changes() {
    let base = run_digest(&["run = [\"cargo\", \"test\"]", "timeout_secs = 600"]);

    let changed = run_digest(&[
        "run = [\"cargo\", \"test\"]",
        "timeout_secs = 600",
        "expect = [{ exit = 1, outcome = \"failed\" }]",
    ]);

    assert_ne!(base, changed);
}

fn raw_table(rows: &[&str]) -> toml::Table {
    rows.join("\n").parse().expect("the table parses")
}

const INVALID_ROWS: [&str; 2] = ["run = []", "timeout_secs = 60"];
const VALID_ROWS: [&str; 2] = ["run = [\"true\"]", "timeout_secs = 60"];
const DEFAULT_EXPECT_ROW: &str = "expect = [{ exit = 0, outcome = \"passed\" }]";

#[test]
fn it_should_give_the_raw_digest_a_different_value_when_the_default_expect_is_written_out() {
    let mut spelled = INVALID_ROWS.to_vec();
    spelled.push(DEFAULT_EXPECT_ROW);

    let omitted = raw_native_run_digest(&raw_table(&INVALID_ROWS));
    let written = raw_native_run_digest(&raw_table(&spelled));

    assert_ne!(omitted, written);
}

#[test]
fn it_should_give_the_raw_digest_a_different_value_from_the_parsed_digest_when_expect_is_omitted() {
    let raw = raw_native_run_digest(&raw_table(&VALID_ROWS));

    let parsed = run_digest(&VALID_ROWS);

    assert_ne!(raw, parsed);
}

#[test]
fn it_should_give_the_raw_digest_the_same_value_for_the_same_table() {
    let first = raw_native_run_digest(&raw_table(&INVALID_ROWS));

    let second = raw_native_run_digest(&raw_table(&INVALID_ROWS));

    assert_eq!(first, second);
}

// Entrevista: [interview] com threshold e [interview.weights]

const WEIGHTS_TABLE: &str = "[interview.weights]";

fn interview_text(threshold: &str, weights: [&str; 4]) -> String {
    let [objective, constraints, success, context] = weights;
    lines(&[
        "version = 1",
        "[interview]",
        &format!("threshold = {threshold}"),
        WEIGHTS_TABLE,
        &format!("objective = {objective}"),
        &format!("constraints = {constraints}"),
        &format!("success = {success}"),
        &format!("context = {context}"),
    ])
}

fn with_threshold(threshold: &str) -> String {
    interview_text(threshold, ["0.35", "0.25", "0.25", "0.15"])
}

fn with_weights(weights: [&str; 4]) -> String {
    interview_text("0.2", weights)
}

fn default_interview() -> InterviewConfig {
    InterviewConfig {
        threshold: 0.20,
        weights: InterviewWeights {
            objective: 0.35,
            constraints: 0.25,
            success: 0.25,
            context: 0.15,
        },
    }
}

#[test]
fn it_should_default_the_interview_to_the_compiled_threshold_and_weights() {
    let config = WorkflowConfig::default();

    assert_eq!(config.interview, default_interview());
}

#[test]
fn it_should_keep_the_default_interview_when_the_file_has_no_interview_table() {
    let config = accepted(&lines(&["version = 1"]));

    assert_eq!(config.interview, default_interview());
}

#[test]
fn it_should_accept_the_default_values_written_out_in_the_file() {
    let config = accepted(&with_threshold("0.20"));

    assert_eq!(config.interview, default_interview());
}

#[test]
fn it_should_accept_a_threshold_of_0_35() {
    let config = accepted(&with_threshold("0.35"));

    assert_eq!(config.interview.threshold, 0.35);
}

#[test]
fn it_should_accept_a_threshold_of_1() {
    let config = accepted(&with_threshold("1"));

    assert_eq!(config.interview.threshold, 1.0);
}

#[test]
fn it_should_refuse_a_threshold_of_0() {
    assert_eq!(refusal(&with_threshold("0")).code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_negative_threshold() {
    assert_eq!(refusal(&with_threshold("-0.1")).code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_threshold_of_1_01() {
    assert_eq!(refusal(&with_threshold("1.01")).code, ERROR_CODE);
}

#[test]
fn it_should_accept_weights_that_sum_to_1() {
    let config = accepted(&with_weights(["0.4", "0.3", "0.2", "0.1"]));

    assert_eq!(config.interview.weights.objective, 0.4);
    assert_eq!(config.interview.weights.context, 0.1);
}

#[test]
fn it_should_refuse_weights_that_sum_to_0_9() {
    let error = refusal(&with_weights(["0.35", "0.25", "0.25", "0.05"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_weights_that_sum_to_1_1() {
    let error = refusal(&with_weights(["0.35", "0.25", "0.25", "0.25"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_accept_a_weight_of_0() {
    let config = accepted(&with_weights(["0.5", "0.25", "0.25", "0"]));

    assert_eq!(config.interview.weights.context, 0.0);
}

#[test]
fn it_should_accept_a_weight_of_1() {
    let config = accepted(&with_weights(["1", "0", "0", "0"]));

    assert_eq!(config.interview.weights.objective, 1.0);
}

#[test]
fn it_should_refuse_a_weight_of_1_2() {
    let error = refusal(&with_weights(["1.2", "-0.2", "0", "0"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_weight_of_minus_0_1_even_when_the_sum_is_1() {
    let error = refusal(&with_weights(["0.6", "0.25", "0.25", "-0.1"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_interview() {
    let error = refusal(&lines(&["version = 1", "[interview]", "limit = 0.2"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_the_interview_weights() {
    let error = refusal(&lines(&["version = 1", WEIGHTS_TABLE, "tone = 1"]));

    assert_eq!(error.code, ERROR_CODE);
}

#[test]
fn it_should_refuse_a_repeated_interview_table() {
    let text = lines(&[
        "version = 1",
        "[interview]",
        "threshold = 0.3",
        "[interview]",
        "threshold = 0.4",
    ]);

    assert_eq!(refusal(&text).code, ERROR_CODE);
}

// Tabela [branch] (brn1 CA-001)

fn with_branch(rows: &[&str]) -> String {
    let mut all = vec!["version = 1"];
    all.extend_from_slice(rows);
    lines(&all)
}

fn assert_refused_with_position(error: &ProjectError) {
    assert_eq!(error.code, ERROR_CODE);
    assert!(
        error.message.contains(WORKFLOW_PATH),
        "the message names the file path: {}",
        error.message
    );
    assert!(
        error.message.contains("line ") && error.message.contains(", column "),
        "the message carries a line and a column: {}",
        error.message
    );
}

fn toml_string(raw: &str) -> String {
    let mut out = String::from("\"");
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn integration_text(name: &str) -> String {
    with_branch(&["[branch]", &format!("integration = {}", toml_string(name))])
}

#[test]
fn it_should_accept_every_branch_key_into_the_expected_fields() {
    let text = with_branch(&[
        "[branch]",
        "format = \"work/<id>/<type>/<slug>\"",
        "integration = \"develop\"",
        "[[branch.exception]]",
        "pattern = \"poc/*\"",
        "mode = \"skip\"",
        "[[branch.exception]]",
        "pattern = \"develop\"",
        "mode = \"dry-run-with-mark\"",
    ]);

    let config = accepted(&text);

    assert_eq!(
        config.branch,
        BranchConfig {
            format: BranchFormat::parse("work/<id>/<type>/<slug>").expect("valid format"),
            integration: Some("develop".to_owned()),
            exceptions: vec![
                ExceptionRule {
                    pattern: "poc/*".to_owned(),
                    mode: ExceptionMode::Skip,
                },
                ExceptionRule {
                    pattern: "develop".to_owned(),
                    mode: ExceptionMode::DryRunWithMark,
                },
            ],
        }
    );
}

#[test]
fn it_should_accept_the_dry_run_mode() {
    let text = with_branch(&[
        "[[branch.exception]]",
        "pattern = \"spike/*\"",
        "mode = \"dry-run\"",
    ]);

    let config = accepted(&text);

    assert_eq!(config.branch.exceptions[0].mode, ExceptionMode::DryRun);
}

#[test]
fn it_should_default_the_branch_policy_when_the_table_is_absent() {
    let config = accepted("version = 1\n");

    assert_eq!(config.branch, BranchConfig::default());
}

#[test]
fn it_should_refuse_an_unknown_key_inside_branch_with_its_position() {
    let text = with_branch(&["[branch]", "colour = true"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_the_mark_key_inside_branch_with_its_position() {
    let text = with_branch(&["[branch]", "mark = true"]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 3);
}

#[test]
fn it_should_refuse_an_unknown_key_inside_an_exception_with_its_position() {
    let text = with_branch(&[
        "[[branch.exception]]",
        "pattern = \"poc/*\"",
        "mode = \"skip\"",
        "colour = true",
    ]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 5);
}

#[test]
fn it_should_refuse_the_mark_key_inside_an_exception_with_its_position() {
    let text = with_branch(&[
        "[[branch.exception]]",
        "pattern = \"poc/*\"",
        "mode = \"skip\"",
        "mark = true",
    ]);

    let error = refusal(&text);

    assert_refused_at_line(&error, 5);
}

#[test]
fn it_should_refuse_an_exception_without_mode() {
    let text = with_branch(&["[[branch.exception]]", "pattern = \"poc/*\""]);

    let error = refusal(&text);

    assert_refused_with_position(&error);
}

#[test]
fn it_should_refuse_an_exception_without_pattern() {
    let text = with_branch(&["[[branch.exception]]", "mode = \"skip\""]);

    let error = refusal(&text);

    assert_refused_with_position(&error);
}

#[test]
fn it_should_refuse_a_mode_outside_the_three_values() {
    let text = with_branch(&[
        "[[branch.exception]]",
        "pattern = \"poc/*\"",
        "mode = \"mark\"",
    ]);

    let error = refusal(&text);

    assert_refused_with_position(&error);
}

#[test]
fn it_should_refuse_a_format_that_is_not_a_valid_template() {
    let text = with_branch(&["[branch]", "format = \"<type>/<id>-<slug>\""]);

    let error = refusal(&text);

    assert_refused_with_position(&error);
}

#[test]
fn it_should_refuse_a_wrong_type_for_each_branch_key() {
    let texts = [
        with_branch(&["[branch]", "format = 1"]),
        with_branch(&["[branch]", "integration = 1"]),
        with_branch(&["[[branch.exception]]", "pattern = 1", "mode = \"skip\""]),
        with_branch(&["[[branch.exception]]", "pattern = \"poc/*\"", "mode = 1"]),
    ];

    let accepted_by_mistake: Vec<&String> = texts
        .iter()
        .filter(|text| !matches!(parse(text), Err(error) if error.code == ERROR_CODE))
        .collect();

    assert!(
        accepted_by_mistake.is_empty(),
        "not refused: {accepted_by_mistake:?}"
    );
}

#[test]
fn it_should_refuse_an_exception_that_is_not_a_list() {
    let text = with_branch(&[
        "[branch.exception]",
        "pattern = \"poc/*\"",
        "mode = \"skip\"",
    ]);

    let error = refusal(&text);

    assert_refused_with_position(&error);
}

#[test]
fn it_should_not_echo_the_file_content_in_a_branch_refusal() {
    let refused = [
        with_branch(&["[branch]", &format!("{CANARY} = true")]),
        with_branch(&["[branch]", &format!("format = \"<{CANARY}>\"")]),
        with_branch(&["[branch]", &format!("integration = \"{CANARY} x\"")]),
        with_branch(&[
            "[[branch.exception]]",
            "pattern = \"poc/*\"",
            &format!("mode = \"{CANARY}\""),
        ]),
    ];

    let messages: Vec<String> = refused.iter().map(|text| refusal(text).message).collect();

    let leaked: Vec<&String> = messages
        .iter()
        .filter(|message| message.contains(CANARY))
        .collect();
    assert!(
        leaked.is_empty(),
        "messages repeat the file content: {leaked:?}"
    );
}

#[test]
fn it_should_refuse_an_invalid_branch_table_in_the_lenient_evaluated_parse() {
    let text = with_branch(&["[branch]", "mark = true"]);

    let error = parse_evaluated_workflow(workflow_path(), text.as_bytes())
        .expect_err("the evaluated copy is refused");

    assert_eq!(error.code, ERROR_CODE);
}

// branch.integration (brn1 CA-013, EMD-061)

#[test]
fn it_should_refuse_each_invalid_integration_name_with_its_position() {
    let names = [
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

    let not_refused: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| {
            !matches!(parse(&integration_text(name)), Err(error)
                if error.code == ERROR_CODE
                    && error.message.contains("line ")
                    && error.message.contains(", column "))
        })
        .collect();

    assert!(not_refused.is_empty(), "not refused: {not_refused:?}");
}

#[test]
fn it_should_accept_an_integration_equal_to_an_exception_pattern() {
    let text = with_branch(&[
        "[branch]",
        "integration = \"develop\"",
        "[[branch.exception]]",
        "pattern = \"develop\"",
        "mode = \"dry-run-with-mark\"",
    ]);

    let config = accepted(&text);

    assert_eq!(config.branch.integration.as_deref(), Some("develop"));
}

#[test]
fn it_should_accept_main_as_the_integration() {
    let config = accepted(&integration_text("main"));

    assert_eq!(config.branch.integration.as_deref(), Some("main"));
}

#[test]
fn it_should_refuse_an_integration_of_201_characters() {
    let error = refusal(&integration_text(&"a".repeat(201)));

    assert_refused_with_position(&error);
}

#[test]
fn it_should_accept_an_integration_of_200_characters() {
    let name = "a".repeat(200);

    let config = accepted(&integration_text(&name));

    assert_eq!(config.branch.integration, Some(name));
}

// CA-001: as tabelas das integrações ([git], [creation_gate], [provider], [worktree])

mod integrations {
    use std::path::PathBuf;

    use super::{CANARY, ERROR_CODE, accepted, assert_refused_at_line, lines, refusal};
    use crate::workflow::config::{
        CreationGateConfig, DEFAULT_PROVIDER_HOST, DEFAULT_WORKTREE_DIR, GitConfig, ProviderConfig,
        ProviderName, WorktreeConfig,
    };

    const FULL: &str = "version = 1\n\n[git]\nhooks = true\n\n[creation_gate]\nenabled = true\n\n[provider]\nname = \"github\"\nhost = \"ghe.example.com\"\nrequire_ci = true\n\n[worktree]\ndir = \"build/wt\"\n";

    fn refused_at_line_three(table: &str, row: &str) {
        let text = lines(&["version = 1", table, row]);

        let error = refusal(&text);

        assert_refused_at_line(&error, 3);
    }

    #[test]
    fn it_should_read_the_git_table_when_all_four_tables_are_given() {
        let config = accepted(FULL);

        assert_eq!(config.git, GitConfig { hooks: true });
    }

    #[test]
    fn it_should_read_the_creation_gate_table_when_all_four_tables_are_given() {
        let config = accepted(FULL);

        assert_eq!(config.creation_gate, CreationGateConfig { enabled: true });
    }

    #[test]
    fn it_should_read_the_provider_table_when_all_four_tables_are_given() {
        let config = accepted(FULL);

        assert_eq!(
            config.provider,
            Some(ProviderConfig {
                name: ProviderName::Github,
                host: "ghe.example.com".to_owned(),
                require_ci: true,
            })
        );
    }

    #[test]
    fn it_should_read_the_worktree_table_when_all_four_tables_are_given() {
        let config = accepted(FULL);

        assert_eq!(
            config.worktree,
            WorktreeConfig {
                dir: PathBuf::from("build/wt")
            }
        );
    }

    #[test]
    fn it_should_accept_an_absolute_worktree_dir() {
        let config = accepted(&lines(&["version = 1", "[worktree]", "dir = \"/srv/wt\""]));

        assert_eq!(config.worktree.dir, PathBuf::from("/srv/wt"));
    }

    #[test]
    fn it_should_spell_the_github_provider_name_as_github() {
        assert_eq!(ProviderName::Github.as_str(), "github");
    }

    #[test]
    fn it_should_default_git_hooks_to_false_without_the_git_table() {
        let config = accepted("version = 1\n");

        assert!(!config.git.hooks);
    }

    #[test]
    fn it_should_default_the_creation_gate_to_off_without_its_table() {
        let config = accepted("version = 1\n");

        assert!(!config.creation_gate.enabled);
    }

    #[test]
    fn it_should_leave_the_integration_off_without_the_provider_table() {
        let config = accepted("version = 1\n");

        assert_eq!(config.provider, None);
    }

    #[test]
    fn it_should_default_the_worktree_dir_to_dot_worktrees_without_its_table() {
        let config = accepted("version = 1\n");

        assert_eq!(config.worktree.dir, PathBuf::from(DEFAULT_WORKTREE_DIR));
        assert_eq!(DEFAULT_WORKTREE_DIR, ".worktrees");
    }

    #[test]
    fn it_should_default_the_provider_host_to_github_com_and_require_ci_to_false() {
        let config = accepted(&lines(&["version = 1", "[provider]", "name = \"github\""]));

        let provider = config.provider.expect("the integration is on");
        assert_eq!(provider.host, DEFAULT_PROVIDER_HOST);
        assert_eq!(DEFAULT_PROVIDER_HOST, "github.com");
        assert!(!provider.require_ci);
    }

    #[test]
    fn it_should_refuse_an_unknown_key_in_each_new_table_with_its_position() {
        for table in ["[git]", "[creation_gate]", "[provider]", "[worktree]"] {
            refused_at_line_three(table, "colour = true");
        }
    }

    #[test]
    fn it_should_refuse_a_wrong_type_for_each_new_key_with_its_position() {
        for (table, row) in [
            ("[git]", "hooks = \"yes\""),
            ("[creation_gate]", "enabled = 1"),
            ("[provider]", "name = 1"),
            ("[worktree]", "dir = true"),
        ] {
            refused_at_line_three(table, row);
        }
    }

    #[test]
    fn it_should_refuse_a_wrong_type_for_host_and_require_ci_with_their_position() {
        for row in ["host = 1", "require_ci = \"yes\""] {
            let text = lines(&["version = 1", "[provider]", "name = \"github\"", row]);

            let error = refusal(&text);

            assert_refused_at_line(&error, 4);
        }
    }

    #[test]
    fn it_should_refuse_a_provider_name_other_than_github_and_list_github() {
        let text = lines(&["version = 1", "[provider]", "name = \"gitlab\""]);

        let error = refusal(&text);

        assert_refused_at_line(&error, 3);
        assert!(
            error.message.contains("github"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn it_should_refuse_an_empty_host_with_its_position() {
        let text = lines(&[
            "version = 1",
            "[provider]",
            "name = \"github\"",
            "host = \"\"",
        ]);

        let error = refusal(&text);

        assert_refused_at_line(&error, 4);
    }

    #[test]
    fn it_should_refuse_a_host_with_a_slash_a_colon_or_a_space() {
        for host in ["github.com/x", "https:github.com", "git hub.com"] {
            let text = lines(&[
                "version = 1",
                "[provider]",
                "name = \"github\"",
                &format!("host = \"{host}\""),
            ]);

            let error = refusal(&text);

            assert_refused_at_line(&error, 4);
        }
    }

    #[test]
    fn it_should_refuse_host_or_require_ci_without_a_provider_name() {
        for row in ["host = \"github.com\"", "require_ci = true"] {
            let text = lines(&["version = 1", "[provider]", row]);

            let error = refusal(&text);

            assert_eq!(error.code, ERROR_CODE, "{row}");
            assert!(error.message.contains("line "), "{row}: {}", error.message);
        }
    }

    #[test]
    fn it_should_refuse_an_empty_worktree_dir_with_its_position() {
        refused_at_line_three("[worktree]", "dir = \"\"");
    }

    #[test]
    fn it_should_refuse_a_worktree_dir_with_a_parent_component() {
        refused_at_line_three("[worktree]", "dir = \"wt/../..\"");
    }

    #[test]
    fn it_should_refuse_a_worktree_dir_equal_to_dot_git() {
        refused_at_line_three("[worktree]", "dir = \".git\"");
    }

    #[test]
    fn it_should_not_repeat_the_file_text_in_the_refusals_of_the_new_tables() {
        let refused = [
            lines(&["version = 1", "[git]", &format!("hooks = \"{CANARY}\"")]),
            lines(&["version = 1", "[provider]", &format!("name = \"{CANARY}\"")]),
            lines(&[
                "version = 1",
                "[provider]",
                "name = \"github\"",
                &format!("host = \"{CANARY}/x\""),
            ]),
            lines(&[
                "version = 1",
                "[worktree]",
                &format!("dir = \"../{CANARY}/..\""),
            ]),
        ];

        let leaked: Vec<String> = refused
            .iter()
            .map(|text| refusal(text).message)
            .filter(|message| message.contains(CANARY))
            .collect();

        assert_eq!(leaked, Vec::<String>::new());
    }
}
