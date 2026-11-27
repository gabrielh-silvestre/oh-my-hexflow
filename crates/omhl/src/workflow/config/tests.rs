use std::collections::BTreeMap;
use std::path::Path;

use crate::project::ProjectError;
use crate::test_support::omhl::RUN_TESTS;
use crate::test_support::pr_gate::{CI_GREEN, VERIFY_ANCHORING};
use crate::workflow::config::{
    PromotionConfig, RequireAtom, SpecsConfig, WorkflowConfig, parse_workflow,
};

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
        "[prefix.feat]",
        "require = [\"business-premise\", \"technical-premise-confirmed\", \"agent-premise\", \"spec\"]",
    ]);

    let config = accepted(&text);

    assert_eq!(
        config.prefixes["feat"],
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
fn it_should_replace_the_requirements_of_a_listed_prefix_and_keep_the_others() {
    let text = lines(&["version = 1", "[prefix.feat]", "require = [\"spec\"]"]);

    let config = accepted(&text);

    let mut expected = WorkflowConfig::default();
    expected
        .prefixes
        .insert("feat".to_owned(), vec![RequireAtom::Spec]);
    assert_eq!(config, expected);
}

#[test]
fn it_should_add_a_prefix_the_defaults_do_not_have() {
    let text = lines(&["version = 1", "[prefix.perf]", "require = []"]);

    let config = accepted(&text);

    let mut expected = WorkflowConfig::default();
    expected.prefixes.insert("perf".to_owned(), Vec::new());
    assert_eq!(config, expected);
}

#[test]
fn it_should_allow_a_prefix_to_require_nothing() {
    let text = lines(&["version = 1", "[prefix.feat]", "require = []"]);

    let config = accepted(&text);

    assert_eq!(config.prefixes["feat"], Vec::<RequireAtom>::new());
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

// Ativação dos checks nativos: `[native] checks`

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
fn it_should_refuse_each_native_guideline_id_in_native_checks_with_its_position() {
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
