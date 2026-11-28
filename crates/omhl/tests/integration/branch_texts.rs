use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use ::omhl::workflow::config::parse_workflow;
use support::repo_checks::repo_root;

const OLD_NAMES: [&str; 3] = [
    "omhl-develop-mark",
    "develop-mark-missing",
    "develop-mark-unavailable",
];
const COMMIT_MARK_COMMAND: &str = "plugin/commands/omhl-commit-mark.md";
const CHECK_RUN_PAGE: &str = "plugin/skills/review/references/check-run.md";
const CLOSURE_PAGE: &str = "plugin/skills/review/references/fechamento.md";
const REVIEW_SKILL: &str = "plugin/skills/review/SKILL.md";
const CLOSE_PAGE: &str = "plugin/skills/review/references/close.md";
const DIRECTIVES_FLOW_DOC: &str = "docs/directives-flow.md";
const MARK_LIMIT_FRAGMENTS: [&str; 4] = [
    "dry-run-with-mark",
    "pre-merge-commit",
    "fast-forward",
    "conflict",
];
const README_ENTRY_FRAGMENTS: [&str; 11] = [
    "omhl-commit-mark v1",
    "commit-mark-missing",
    "commit-mark-unavailable",
    "omhl-develop-mark",
    "branch.integration",
    "branch.format",
    "[branch]",
    "poc/*",
    "binário anterior",
    "release",
    "à mão",
];

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

fn files_under(dir: &Path, skipped: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("directory is readable") {
        let path = entry.expect("entry is readable").path();
        if path == skipped {
            continue;
        }
        if path.is_dir() {
            files_under(&path, skipped, found);
        } else {
            found.push(path);
        }
    }
}

/// A entrada de aviso mais nova do README: a primeira `###` depois de `## Avisos de atualização`.
fn split_readme(readme: &str) -> (String, String) {
    let section = readme
        .find("\n## Avisos de atualização")
        .expect("the README has the warnings section");
    let entry_start = section
        + readme[section..]
            .find("\n### ")
            .expect("the warnings section has an entry")
        + 1;
    let entry_end = readme[entry_start + 1..]
        .find("\n##")
        .map_or(readme.len(), |at| entry_start + 1 + at + 1);
    let entry = readme[entry_start..entry_end].to_owned();
    let outside = format!("{}{}", &readme[..entry_start], &readme[entry_end..]);
    (entry, outside)
}

fn errors_section(page: &str) -> String {
    let after = page
        .split("\n## Errors")
        .nth(1)
        .expect("the page has an Errors section");
    after.split("\n## ").next().unwrap_or(after).to_owned()
}

fn missing_from<'a>(text: &str, fragments: &[&'a str]) -> Vec<&'a str> {
    fragments
        .iter()
        .copied()
        .filter(|fragment| !text.contains(fragment))
        .collect()
}

fn scanned_texts() -> Vec<(String, String)> {
    let root = repo_root();
    let mut paths = Vec::new();
    files_under(&root.join("plugin"), Path::new(""), &mut paths);
    files_under(&root.join("docs"), &root.join("docs/adr"), &mut paths);
    paths.push(root.join("GLOSSARY.md"));
    paths.push(root.join("AGENTS.md"));
    let mut texts: Vec<(String, String)> = paths
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            Some((path.display().to_string(), text))
        })
        .collect();
    texts.push((
        "README.md".to_owned(),
        split_readme(&read_repo_file("README.md")).1,
    ));
    texts
}

fn files_naming(wanted: &str) -> Vec<String> {
    scanned_texts()
        .into_iter()
        .filter(|(_, text)| text.contains(wanted))
        .map(|(path, _)| path)
        .collect()
}

#[test]
fn it_should_keep_the_old_mark_name_and_codes_out_of_plugin_docs_and_readme_outside_the_warning() {
    for old in OLD_NAMES {
        assert_eq!(files_naming(old), Vec::<String>::new(), "{old}");
    }
}

#[test]
fn it_should_say_in_the_readme_entry_everything_the_update_needs() {
    let (entry, _) = split_readme(&read_repo_file("README.md"));

    assert_eq!(
        missing_from(&entry, &README_ENTRY_FRAGMENTS),
        Vec::<&str>::new()
    );
}

#[test]
fn it_should_accept_the_branch_example_of_the_readme_entry_in_parse_workflow() {
    let (entry, _) = split_readme(&read_repo_file("README.md"));
    let block = entry
        .split("```toml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the entry has a toml block");
    let text = if block.contains("version") {
        block.to_owned()
    } else {
        format!("version = 1\n{block}")
    };

    let config = parse_workflow(Path::new(".omhl/workflow.toml"), text.as_bytes())
        .expect("the example is a valid workflow.toml");

    assert_eq!(
        config
            .branch
            .exceptions
            .iter()
            .map(|rule| (rule.pattern.as_str(), rule.mode.as_str()))
            .collect::<Vec<_>>(),
        vec![("poc/*", "skip"), ("develop", "dry-run-with-mark")]
    );
}

#[test]
fn it_should_carry_the_mark_limit_in_the_commit_mark_command() {
    let command = read_repo_file(COMMIT_MARK_COMMAND);

    assert_eq!(
        missing_from(&command, &MARK_LIMIT_FRAGMENTS),
        Vec::<&str>::new()
    );
}

#[test]
fn it_should_carry_the_mark_limit_in_the_errors_section_of_check_run() {
    let errors = errors_section(&read_repo_file(CHECK_RUN_PAGE));

    assert_eq!(
        missing_from(&errors, &MARK_LIMIT_FRAGMENTS),
        Vec::<&str>::new()
    );
}

#[test]
fn it_should_list_both_mark_codes_in_the_errors_section_of_check_run() {
    let errors = errors_section(&read_repo_file(CHECK_RUN_PAGE));

    assert_eq!(
        missing_from(&errors, &["commit-mark-missing", "commit-mark-unavailable"]),
        Vec::<&str>::new()
    );
}

#[test]
fn it_should_list_branch_format_unmarked_in_the_hook_code_table_of_directives_flow() {
    let page = read_repo_file(DIRECTIVES_FLOW_DOC);

    let row = page
        .lines()
        .find(|line| line.starts_with('|') && line.contains("branch-format-unmarked"));

    assert!(row.is_some(), "a table row names branch-format-unmarked");
    assert!(page.contains("spec-dir-frozen"));
}

#[test]
fn it_should_carry_the_pr_base_check_and_the_base_rule_after_merge_in_the_closure_page() {
    let page = read_repo_file(CLOSURE_PAGE);

    assert_eq!(
        missing_from(&page, &["branch.integration", "--base"]),
        Vec::<&str>::new()
    );
}

#[test]
fn it_should_carry_the_pr_base_check_in_the_review_skill() {
    let skill = read_repo_file(REVIEW_SKILL);

    assert_eq!(
        missing_from(&skill, &["branch.integration", "--base"]),
        Vec::<&str>::new()
    );
}

#[test]
fn it_should_say_a_project_without_branch_integration_in_the_close_page() {
    let page = read_repo_file(CLOSE_PAGE);

    assert!(page.contains("a project without `branch.integration`"));
}

#[test]
fn it_should_not_say_a_project_without_develop_anywhere_in_the_plugin() {
    let root = repo_root();
    let mut paths = Vec::new();
    files_under(&root.join("plugin"), Path::new(""), &mut paths);

    let found: Vec<_> = paths
        .iter()
        .filter(|path| {
            fs::read_to_string(path).is_ok_and(|text| text.contains("a project without `develop`"))
        })
        .collect();

    assert_eq!(found, Vec::<&PathBuf>::new());
}
