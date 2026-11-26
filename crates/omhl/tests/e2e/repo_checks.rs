use crate::support;

use std::path::Path;

use ::omhl::mechanical::plan::{CheckNode, plan_run};
use ::omhl::mechanical::spec::snapshot_for;
use hexlog::SnapshotKind;
use serde_json::{Value, json};
use support::child::{Output, omhl_json};
use support::repo_checks::repo_root;
use support::sid;
use tempfile::TempDir;

const REPO_CHECKS: [&str; 6] = [
    "checks.compile-build",
    "checks.compile-validate",
    "checks.doc-review",
    "checks.fmt",
    "checks.lint",
    "checks.test-suite",
];

// Apoio

/// Roda `omhl` na raiz do repositório com o banco em `xdg`.
fn load(args: &[&str], xdg: &Path) -> Output {
    omhl_json(args).cwd(&repo_root()).xdg(xdg).run()
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
fn it_should_list_the_checks_and_the_guidelines_in_the_dry_run() {
    const REPO_GUIDELINES: [&str; 4] = [
        "guidelines.code-in-english-no-emoji",
        "guidelines.diferencial",
        "guidelines.premissa-so-vale-confirmada",
        "guidelines.proposito",
    ];
    let expected: Vec<&str> = REPO_CHECKS.into_iter().chain(REPO_GUIDELINES).collect();
    let xdg = TempDir::new().expect("data directory is created");

    let output = load(&["load", "--dry-run"], xdg.path());
    let listed = checks(xdg.path());

    let report = ok_json(&output);
    assert_eq!(report["dry_run"], json!(true));
    assert_eq!(report["created"], json!(expected));
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
fn it_should_declare_a_timeout_for_every_repository_check_that_runs_a_command() {
    let (_, items) = loaded_repo();

    let mut ids: Vec<&str> = items.iter().map(item_id).collect();
    ids.sort_unstable();
    let below_floor: Vec<&str> = items
        .iter()
        .filter(|item| item["data"].get("run").is_some())
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
            ("checks.doc-review", SnapshotKind::GitCommit),
            ("checks.fmt", SnapshotKind::GitTree),
            ("checks.lint", SnapshotKind::GitTree),
            ("checks.test-suite", SnapshotKind::GitCommit),
        ]
    );
}
