use crate::support;

use std::fs;

use serde_json::{Value, json};
use support::child::omhl_json;
use support::repo_checks::repo_root;
use tempfile::TempDir;

const BOUNDARY_DOC: &str = "docs/plugin-and-project-boundary.md";
const GUIDELINE_FILE: &str = ".omhl/guidelines/boundary-classification.toml";
const CLASSES: [&str; 3] = [
    "Method",
    "Optional default configuration",
    "Exclusive to this repository",
];
const COLUMNS: [&str; 4] = ["Behavior", "Class", "Where it lives", "Today"];
const NAMED_BEHAVIORS: [&str; 13] = [
    "lock of the spec",
    "role hook",
    "session start",
    "native checks",
    "native guidelines",
    "branch name",
    "prefix",
    "`develop`",
    "hooks of `wt`",
    "git hooks",
    "github",
    "language",
    "conventions of this repository",
];
const TEXT_HOMES: [&str; 5] = [
    "plugin/skills/*/references/",
    "plugin/examples/",
    "docs/",
    "README.md",
    "CONTRIBUTING.md",
];
const ROLES_LINK: &str = "https://github.com/gabrielh-silvestre/oh-my-hexflow/blob/main/docs/directives-flow.md#a-matriz-por-papel";

// Apoio

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

fn section(page: &str, heading: &str) -> String {
    let start = format!("## {heading}");

    page.lines()
        .skip_while(|line| *line != start)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn cells(row: &str) -> Vec<String> {
    row.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// A tabela da seção `Classification`: o cabeçalho e as linhas de dados.
fn classification_table() -> (Vec<String>, Vec<Vec<String>>) {
    let page = read_repo_file(BOUNDARY_DOC);
    let rows: Vec<Vec<String>> = section(&page, "Classification")
        .lines()
        .filter(|line| line.starts_with('|'))
        .map(cells)
        .collect();
    let (header, body) = rows.split_first().expect("the section has a table");

    (header.clone(), body.iter().skip(1).cloned().collect())
}

fn guideline() -> toml::Table {
    toml::from_str(&read_repo_file(GUIDELINE_FILE)).expect("the guideline is valid TOML")
}

// Documento da fronteira

#[test]
fn it_should_have_the_three_classes_and_the_text_rule_as_sections() {
    let page = read_repo_file(BOUNDARY_DOC);

    let missing: Vec<&str> = [CLASSES.as_slice(), &["Where text lives"]]
        .concat()
        .into_iter()
        .filter(|heading| !page.lines().any(|line| line == format!("## {heading}")))
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn it_should_classify_with_the_columns_behavior_class_where_it_lives_and_today() {
    let (header, _) = classification_table();

    assert_eq!(header, COLUMNS);
}

#[test]
fn it_should_give_every_row_of_the_table_one_of_the_three_classes() {
    let (_, rows) = classification_table();

    let outside: Vec<&String> = rows
        .iter()
        .map(|row| &row[1])
        .filter(|class| !CLASSES.contains(&class.as_str()))
        .collect();

    assert!(!rows.is_empty());
    assert_eq!(outside, Vec::<&String>::new());
}

#[test]
fn it_should_fill_the_today_cell_of_every_row() {
    let (_, rows) = classification_table();

    let empty: Vec<&String> = rows
        .iter()
        .map(|row| &row[0])
        .zip(rows.iter().map(|row| &row[3]))
        .filter(|(_, today)| today.is_empty())
        .map(|(behavior, _)| behavior)
        .collect();

    assert_eq!(empty, Vec::<&String>::new());
}

#[test]
fn it_should_cover_every_behavior_the_boundary_names() {
    let (_, rows) = classification_table();
    let behaviors: Vec<String> = rows.iter().map(|row| row[0].to_lowercase()).collect();

    let uncovered: Vec<&str> = NAMED_BEHAVIORS
        .into_iter()
        .filter(|wanted| !behaviors.iter().any(|behavior| behavior.contains(wanted)))
        .collect();

    assert_eq!(uncovered, Vec::<&str>::new());
}

#[test]
fn it_should_link_the_boundary_document_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains(&format!("]({BOUNDARY_DOC})")));
}

#[test]
fn it_should_name_the_five_homes_of_the_text_and_the_rule_that_the_other_side_links() {
    let page = read_repo_file(BOUNDARY_DOC);

    let rule = section(&page, "Where text lives");

    let missing: Vec<&str> = TEXT_HOMES
        .into_iter()
        .filter(|home| !rule.contains(&format!("`{home}`")))
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
    assert!(rule.contains("The other side links to it and does not copy it."));
}

// Diretiva

#[test]
fn it_should_declare_the_boundary_guideline_as_architecture_with_an_english_text() {
    let table = guideline();

    let text = table["text"].as_str().expect("the guideline has a text");

    assert_eq!(table["kind"].as_str(), Some("architecture"));
    assert!(!text.trim().is_empty());
    assert!(text.is_ascii());
}

#[test]
fn it_should_cite_the_boundary_document_in_the_guideline() {
    let table = guideline();

    let text = table["text"].as_str().expect("the guideline has a text");

    assert!(text.contains(BOUNDARY_DOC));
}

#[test]
fn it_should_load_the_repository_guideline_in_the_dry_run_without_problems() {
    let xdg = TempDir::new().expect("data directory is created");

    let output = omhl_json(&["load", "--dry-run"])
        .cwd(&repo_root())
        .xdg(xdg.path())
        .run();

    let report: Value = serde_json::from_str(output.stdout.trim_end()).expect("JSON report");
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(report["skipped"], json!([]));
    assert_eq!(report["warnings"], json!([]));
    assert!(
        report["created"]
            .to_string()
            .contains("guidelines.boundary-classification")
    );
}

// Ponteiros

#[test]
fn it_should_point_the_directives_flow_skill_to_a_readme_of_examples_inside_the_plugin() {
    let skill = "plugin/skills/directives-flow/SKILL.md";
    let link = "../../examples/README.md";

    let target = repo_root().join("plugin/skills/directives-flow").join(link);

    assert!(read_repo_file(skill).contains(&format!("]({link})")));
    assert!(target.is_file());
    assert!(
        target
            .canonicalize()
            .expect("the target resolves")
            .starts_with(
                repo_root()
                    .join("plugin")
                    .canonicalize()
                    .expect("plugin resolves")
            )
    );
}

#[test]
fn it_should_link_the_examples_from_the_docs_that_describe_optional_files() {
    let pages = [
        "docs/directives-flow.md",
        "docs/extensions-and-projection.md",
    ];

    let without_link: Vec<&str> = pages
        .into_iter()
        .filter(|page| !read_repo_file(page).contains("plugin/examples/"))
        .collect();

    assert_eq!(without_link, Vec::<&str>::new());
}

#[test]
fn it_should_link_the_examples_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains("](plugin/examples/README.md)"));
}

#[test]
fn it_should_keep_the_absolute_link_of_the_roles_page_to_the_role_matrix() {
    let roles = read_repo_file("plugin/skills/directives-flow/references/roles.md");

    assert!(roles.contains(ROLES_LINK));
}

// Glossário

#[test]
fn it_should_define_boundary_in_the_glossary_with_an_avoid_line() {
    let glossary = read_repo_file("GLOSSARY.md");

    let entry = glossary
        .split("\n\n")
        .find(|block| block.starts_with("**Boundary**:"))
        .expect("the glossary has a Boundary entry");

    assert!(entry.lines().any(|line| line.starts_with("_Avoid_:")));
}
