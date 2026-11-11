mod support;

use std::fs;
use std::path::{Path, PathBuf};

use ::omhl::mechanical::plan::{CheckNode, plan_run};
use ::omhl::mechanical::spec::snapshot_for;
use hexlog::SnapshotKind;
use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::sid;
use tempfile::TempDir;

const REPO_CHECKS: [&str; 5] = [
    "checks.compile-build",
    "checks.compile-validate",
    "checks.fmt",
    "checks.lint",
    "checks.test-suite",
];
const FAILURE_TABLE_CODES: [&str; 16] = [
    "check-not-run",
    "attestation-refused",
    "attestation-not-recorded",
    "checks-failed",
    "unsupported-platform",
    "spawn-failed",
    "outcome-immutable",
    "command-changed-tree",
    "command-changed-worktree",
    "not-run",
    "spec-invalid",
    "snapshot-tree-mismatch",
    "snapshot-dirty",
    "snapshot-not-head",
    "work-item-closed",
    "probe-failed",
];
const FAILURE_TABLE_KINDS: [&str; 10] = [
    "spawn-not-found",
    "spawn-permission-denied",
    "spawn-failed",
    "busy-exhausted",
    "guard-refused",
    "blob-failed",
    "probe-failed",
    "tree-changed",
    "push-to-main",
    "parent-gone",
];
const MARK_LINE: &str =
    r#"printf 'omhl-develop-mark v1\n' > "$(git rev-parse --absolute-git-dir)/omhl-develop-mark""#;
const MECHANICAL_GATES_PAGE: &str = "docs/mechanical-gates.md";
const DEVELOP_MARK_COMMAND: &str = "plugin/commands/omhl-develop-mark.md";

// Apoio

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Roda `omhl` na raiz do repositório com o banco em `xdg`.
fn load(args: &[&str], xdg: &Path) -> Output {
    omhl(args).cwd(&repo_root()).xdg(xdg).run()
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn ok_json(output: &Output) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    json_line(&output.stdout)
}

/// Os `items` de `omhl list --type check` no banco de `xdg`.
fn checks(xdg: &Path) -> Vec<Value> {
    let listed = ok_json(&load(&["list", "--type", "check"], xdg));

    listed["items"]
        .as_array()
        .expect("items is an array")
        .clone()
}

/// Carrega os checks do repositório num banco temporário: o relatório do `load` e os checks gravados.
fn loaded_repo() -> (Value, Vec<Value>) {
    let xdg = TempDir::new().expect("data directory is created");

    let report = ok_json(&load(&["load"], xdg.path()));

    (report, checks(xdg.path()))
}

fn item_id(item: &Value) -> &str {
    item["id"].as_str().expect("the check has an id")
}

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

fn missing_from<'a>(page: &str, wanted: &[&'a str]) -> Vec<&'a str> {
    wanted
        .iter()
        .copied()
        .filter(|item| !page.contains(&format!("`{item}`")))
        .collect()
}

// load e dry-run

#[test]
fn it_should_load_the_repository_checks_without_errors() {
    let xdg = TempDir::new().expect("data directory is created");

    let output = load(&["load"], xdg.path());

    let report = ok_json(&output);
    assert_eq!(output.stderr, "");
    assert_eq!(report["tenant"], json!("oh-my-hexflow"));
    assert_eq!(report["dry_run"], json!(false));
    assert_eq!(report["skipped"], json!([]));
    assert_eq!(report["warnings"], json!([]));
    assert_eq!(report["orphans"], json!([]));
    assert_eq!(report["orphan_relations"], json!([]));
}

#[test]
fn it_should_list_five_created_checks_in_the_dry_run() {
    let xdg = TempDir::new().expect("data directory is created");

    let output = load(&["load", "--dry-run"], xdg.path());
    let listed = checks(xdg.path());

    let report = ok_json(&output);
    assert_eq!(report["dry_run"], json!(true));
    assert_eq!(report["created"], json!(REPO_CHECKS));
    assert_eq!(report["revised"], json!([]));
    assert_eq!(listed, Vec::<Value>::new());
}

// Plano e campos dos checks

#[test]
fn it_should_order_lint_after_fmt_in_the_repository_plan() {
    let (report, items) = loaded_repo();
    let nodes: Vec<CheckNode> = items
        .iter()
        .map(|item| CheckNode {
            id: sid(item_id(item)),
            data: item["data"].clone(),
            shadowed: false,
        })
        .collect();
    let after: Vec<_> = report["relations_created"]
        .as_array()
        .expect("relations_created is an array")
        .iter()
        .map(|pair| {
            (
                sid(pair["from"].as_str().expect("from is text")),
                sid(pair["to"].as_str().expect("to is text")),
            )
        })
        .collect();

    let plan = plan_run("before-commit", &nodes, &after);

    assert_eq!(
        report["relations_created"],
        json!([
            {"from": "checks.compile-build", "to": "checks.compile-validate"},
            {"from": "checks.lint", "to": "checks.fmt"},
            {"from": "checks.test-suite", "to": "checks.compile-build"},
        ])
    );
    assert_eq!(
        plan.into_iter()
            .map(|planned| planned.id)
            .collect::<Vec<_>>(),
        vec![sid("checks.fmt"), sid("checks.lint")]
    );
}

#[test]
fn it_should_declare_a_timeout_for_every_repository_check() {
    let (_, items) = loaded_repo();

    let mut ids: Vec<&str> = items.iter().map(item_id).collect();
    ids.sort_unstable();
    let below_floor: Vec<&str> = items
        .iter()
        .filter(|item| {
            !item["data"]["timeout_secs"]
                .as_u64()
                .is_some_and(|n| n >= 10)
        })
        .map(item_id)
        .collect();

    assert_eq!(ids, REPO_CHECKS);
    assert_eq!(below_floor, Vec::<&str>::new());
}

#[test]
fn it_should_derive_git_tree_only_for_the_before_commit_checks() {
    let (_, items) = loaded_repo();

    let mut kinds: Vec<(&str, SnapshotKind)> = items
        .iter()
        .map(|item| (item_id(item), snapshot_for(&item["data"])))
        .collect();
    kinds.sort_unstable_by_key(|(id, _)| *id);

    assert_eq!(
        kinds,
        vec![
            ("checks.compile-build", SnapshotKind::GitCommit),
            ("checks.compile-validate", SnapshotKind::GitCommit),
            ("checks.fmt", SnapshotKind::GitTree),
            ("checks.lint", SnapshotKind::GitTree),
            ("checks.test-suite", SnapshotKind::GitCommit),
        ]
    );
}

// Documentação

#[test]
fn it_should_link_the_mechanical_gates_page_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains("](docs/mechanical-gates.md)"));
    assert!(repo_root().join(MECHANICAL_GATES_PAGE).is_file());
}

#[test]
fn it_should_document_every_error_code_and_kind_of_the_failure_table_in_the_mechanical_gates_page()
{
    let page = read_repo_file(MECHANICAL_GATES_PAGE);

    let missing_codes = missing_from(&page, &FAILURE_TABLE_CODES);
    let missing_kinds = missing_from(&page, &FAILURE_TABLE_KINDS);

    assert_eq!(missing_codes, Vec::<&str>::new());
    assert_eq!(missing_kinds, Vec::<&str>::new());
}

#[test]
fn it_should_provide_the_develop_mark_command_with_the_marker_line() {
    let command = read_repo_file(DEVELOP_MARK_COMMAND);
    let page = read_repo_file(MECHANICAL_GATES_PAGE);

    assert!(command.lines().any(|line| line == MARK_LINE));
    assert!(command.contains("omhl-develop-mark v1"));
    assert!(page.contains("](../plugin/commands/omhl-develop-mark.md)"));
}
