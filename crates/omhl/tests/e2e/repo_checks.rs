use crate::support;

use std::fs;
use std::path::Path;

use ::omhl::delivery_gate::REQUIRED_EVENTS;
use ::omhl::mechanical::plan::{CheckNode, plan_run};
use ::omhl::mechanical::spec::snapshot_for;
use ::omhl::workflow::config::parse_workflow;
use hexlog::SnapshotKind;
use serde_json::{Value, json};
use support::child::{Output, omhl_json};
use support::delivery_gate::{CI_GREEN, CONFIG_LOOSENING_REVIEW, VERIFY_ANCHORING};
use support::git::commit_marker;
use support::omhl::typed_work_item_data;
use support::project::Project;
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

const WORKFLOW_FILE: &str = ".omhl/workflow.toml";
const WORK_ITEM: &str = "work-items.w1";
const GATE_BASE_LABELS: [&str; 3] = [
    "no-open-contradiction",
    "decisions-anchored",
    "anchors-confirmed",
];

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
    const REPO_GUIDELINES: [&str; 7] = [
        "guidelines.boundary-classification",
        "guidelines.code-in-english-no-emoji",
        "guidelines.commit-policy",
        "guidelines.diferencial",
        "guidelines.one-decision-per-turn",
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

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("the target directory is created");
    for entry in fs::read_dir(from).expect("the source directory is readable") {
        let entry = entry.expect("the entry is readable");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("the file is copied");
        }
    }
}

/// Projeto git temporário com a cópia do `.omhl/` deste repositório carregada: o diretório principal do repositório real só ganha o `workflow.toml` com o merge.
fn project_with_repo_config() -> Project {
    let project = Project::new("oh-my-hexflow");
    copy_dir(&repo_root().join(".omhl"), &project.dir().join(".omhl"));
    commit_marker(project.dir());
    project_output(&project, &["load"], "");

    project
}

fn project_output(project: &Project, args: &[&str], stdin: &str) -> Value {
    let output = omhl_json(args)
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(stdin)
        .run();

    ok_json(&output)
}

/// Os ids dos checks bloqueantes de `.omhl/checks/` num evento que o gate cobra.
fn blocking_repo_check_ids() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(repo_root().join(".omhl/checks"))
        .expect("the checks directory is readable")
        .map(|entry| entry.expect("the entry is readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("the check file is readable");
            let check: toml::Table = toml::from_str(&text).expect("the check file is TOML");
            let covered = check["when"]["event"]
                .as_str()
                .is_some_and(|event| REQUIRED_EVENTS.contains(&event));

            (check.get("blocking") == Some(&toml::Value::Boolean(true)) && covered).then(|| {
                format!(
                    "checks.{}",
                    path.file_stem().expect("a stem").to_string_lossy()
                )
            })
        })
        .collect();
    ids.sort_unstable();

    ids
}

#[test]
fn it_should_enable_ci_green_anchoring_and_the_loosening_review_in_the_repository_workflow() {
    let path = repo_root().join(WORKFLOW_FILE);
    let bytes = fs::read(&path).expect("the repository workflow file is readable");

    let config = parse_workflow(&path, &bytes).expect("the repository workflow file is accepted");

    assert_eq!(
        config.native.checks.into_iter().collect::<Vec<_>>(),
        [CI_GREEN, CONFIG_LOOSENING_REVIEW, VERIFY_ANCHORING]
    );
}

#[test]
fn it_should_render_the_repository_with_ci_green_and_anchoring_and_without_run_tests() {
    let project = project_with_repo_config();

    let render = project_output(&project, &["render"], "");

    let text = render["text"].as_str().expect("the render has a text");
    assert!(text.contains(&format!("### {CI_GREEN}\n")), "{text}");
    assert!(
        text.contains(&format!("### {VERIFY_ANCHORING}\n")),
        "{text}"
    );
    assert!(!text.contains("checks.run-tests"), "{text}");
}

#[test]
fn it_should_require_ci_green_and_the_blocking_repository_checks_in_the_gate() {
    let project = project_with_repo_config();
    let work_item = json!({ "items": [{ "body": {
        "kind": "node", "id": WORK_ITEM, "type": "work-item", "data": typed_work_item_data("t"),
    } }] });
    project_output(&project, &["append"], &work_item.to_string());

    let gate = project_output(&project, &["gate-delivery", WORK_ITEM], "");

    let mut required: Vec<&str> = gate["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .map(|question| question["label"].as_str().expect("a label"))
        .filter(|label| !GATE_BASE_LABELS.contains(label))
        .filter(|label| !label.starts_with("spec-digest-match:"))
        .collect();
    required.sort_unstable();
    let mut expected = blocking_repo_check_ids();
    expected.push(CI_GREEN.to_owned());
    // Without --base the review counts as required.
    expected.push(CONFIG_LOOSENING_REVIEW.to_owned());
    expected.sort_unstable();
    assert_eq!(required, expected);
}
