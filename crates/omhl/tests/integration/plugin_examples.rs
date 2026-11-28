use crate::support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde_json::Value;
use support::child::omhl_json;
use support::git::{expect_git_success, git_in};
use support::project::Project;
use support::repo_checks::repo_root;

const EXAMPLES_DIR: &str = "plugin/examples";
const EXAMPLES_README: &str = "README.md";
const WORKFLOW_FILE: &str = "workflow.toml";
const LAYOUT: [&str; 3] = ["checks", "guidelines", WORKFLOW_FILE];
const NATIVE_WORKFLOW_EXAMPLE: &str = "workflows/interview-native-run.toml";
const RUST_CHECKS: [&str; 3] = [
    "checks/rust-fmt.toml",
    "checks/rust-clippy.toml",
    "checks/rust-test.toml",
];
const NODE_CHECKS: [&str; 1] = ["checks/node-test.toml"];
const PYTHON_CHECKS: [&str; 1] = ["checks/python-test.toml"];
const TOOLCHAIN_VARIABLES: [&str; 5] = [
    "RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
];
const FORBIDDEN_KEYS: [&str; 4] = ["branch", "github", "exception", "integration"];
const INTEGRATION_KEYS: [(&str, &str); 6] = [
    ("git", "hooks"),
    ("creation_gate", "enabled"),
    ("provider", "name"),
    ("provider", "host"),
    ("provider", "require_ci"),
    ("worktree", "dir"),
];
const FORBIDDEN_PROGRAMS: [&str; 3] = ["gh", "wt", "git"];
const LINT_CHECK: &str = "checks/lint-sources.toml";

type Tree = BTreeMap<String, String>;

// Apoio

fn tracked_examples() -> Tree {
    let args = ["ls-files", EXAMPLES_DIR];
    let listing = expect_git_success(&git_in(&repo_root(), &args), &args);
    let prefix = format!("{EXAMPLES_DIR}/");

    listing
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .filter(|path| *path != EXAMPLES_README)
        .filter(|path| !path.starts_with("workflows/"))
        .map(|path| {
            let text = fs::read_to_string(repo_root().join(EXAMPLES_DIR).join(path))
                .unwrap_or_else(|error| panic!("{path} is readable: {error}"));

            (path.to_owned(), text)
        })
        .collect()
}

/// Lê do disco, não do git, e junto o `.md` com o `how` que o `load` exige.
fn from_disk(paths: &[&str]) -> Tree {
    let mut tree = Tree::new();
    for path in paths {
        let text = fs::read_to_string(repo_root().join(EXAMPLES_DIR).join(path))
            .unwrap_or_else(|error| panic!("{path} is readable: {error}"));
        tree.insert((*path).to_owned(), text);
        if let Some(stem) = path.strip_suffix(".toml") {
            let sibling = format!("{stem}.md");
            if let Ok(how) = fs::read_to_string(repo_root().join(EXAMPLES_DIR).join(&sibling)) {
                tree.insert(sibling, how);
            }
        }
    }

    tree
}

fn run_words(tree: &Tree, path: &str) -> Vec<String> {
    let table: toml::Table = toml::from_str(&tree[path]).expect("valid TOML");

    table["run"]
        .as_array()
        .expect("run is an array")
        .iter()
        .filter_map(|word| word.as_str().map(str::to_owned))
        .collect()
}

fn edited(tree: &Tree, path: &str, old: &str, new: &str) -> Tree {
    let mut copy = tree.clone();
    let text = copy.get_mut(path).expect("the example file exists");
    assert!(text.contains(old), "{path} has no `{old}` to edit");
    *text = text.replace(old, new);

    copy
}

fn with_file(tree: &Tree, path: &str, text: &str) -> Tree {
    let mut copy = tree.clone();
    copy.insert(path.to_owned(), text.to_owned());

    copy
}

/// Os ids que o carregador daria às pastas `<pasta>/<slug>.toml` da árvore.
fn scanned_ids(tree: &Tree) -> BTreeMap<String, String> {
    tree.keys()
        .filter_map(|path| {
            let (folder, name) = path.split_once('/')?;
            let slug = name.strip_suffix(".toml")?;

            (!slug.contains('/')).then(|| (path.clone(), format!("{folder}.{slug}")))
        })
        .collect()
}

fn problems_of(report: &Value) -> Vec<String> {
    if let Some(error) = report.get("error") {
        let details = error["details"].as_array().cloned().unwrap_or_default();
        let shown = if details.is_empty() {
            vec![error.clone()]
        } else {
            details
        };

        return shown.iter().map(|item| format!("load: {item}")).collect();
    }

    ["skipped", "warnings"]
        .iter()
        .flat_map(|key| report[*key].as_array().cloned().unwrap_or_default())
        .map(|item| format!("load: {item}"))
        .collect()
}

fn not_created(tree: &Tree, report: &Value) -> Vec<String> {
    let created: BTreeSet<&str> = report["created"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    scanned_ids(tree)
        .into_iter()
        .filter(|(_, id)| !created.contains(id.as_str()))
        .map(|(path, id)| format!("{path}: id {id} is not in created"))
        .collect()
}

/// O carregador aceita um `after` para um id nativo: só esta conferência o recusa num exemplo.
fn dangling_after(tree: &Tree) -> Vec<String> {
    let ids: BTreeSet<String> = scanned_ids(tree).into_values().collect();

    scanned_ids(tree)
        .into_iter()
        .filter_map(|(path, id)| {
            let table: toml::Table = toml::from_str(&tree[&path]).ok()?;
            let after = table.get("after")?.as_array()?;

            Some(
                after
                    .iter()
                    .filter_map(toml::Value::as_str)
                    .filter(|target| *target != id && !ids.contains(*target))
                    .map(|target| {
                        format!("{path}: after names {target}, which is not another example")
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

fn failures_of(tree: &Tree) -> Vec<String> {
    let project = Project::new("examples");
    for (path, text) in tree {
        let target = project.dir().join(".omhl").join(path);
        fs::create_dir_all(target.parent().expect("the file has a folder"))
            .expect("the folder is created");
        fs::write(&target, text).expect("the example is copied");
    }

    let output = omhl_json(&["load", "--dry-run"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .run();
    // Um erro do `omhl` sai no stderr; o relatório de um `load` aceito, no stdout.
    let body = if output.code == Some(0) {
        &output.stdout
    } else {
        &output.stderr
    };
    let report: Value = serde_json::from_str(body.trim_end()).expect("the load report is JSON");

    [
        problems_of(&report),
        not_created(tree, &report),
        dangling_after(tree),
    ]
    .concat()
}

fn parsed(tree: &Tree, folder: &str) -> Vec<(String, toml::Table)> {
    tree.iter()
        .filter(|(path, _)| path.starts_with(&format!("{folder}/")) && path.ends_with(".toml"))
        .map(|(path, text)| (path.clone(), toml::from_str(text).expect("valid TOML")))
        .collect()
}

fn has_markdown_sibling(tree: &Tree, toml_path: &str) -> bool {
    tree.contains_key(&toml_path.replace(".toml", ".md"))
}

fn collect_keys(value: &toml::Value, found: &mut Vec<String>) {
    match value {
        toml::Value::Table(table) => {
            for (key, item) in table {
                found.push(key.clone());
                collect_keys(item, found);
            }
        }
        toml::Value::Array(items) => items.iter().for_each(|item| collect_keys(item, found)),
        _ => {}
    }
}

// Conjunto dos exemplos

#[test]
fn it_should_load_every_example_together_without_problems() {
    let tree = tracked_examples();

    let failures = failures_of(&tree);

    assert!(!tree.is_empty(), "plugin/examples has no tracked file");
    assert_eq!(failures, Vec::<String>::new());
}

#[test]
fn it_should_reject_a_workflow_file_with_an_unknown_key() {
    let tree = tracked_examples();
    let seeded = edited(
        &tree,
        WORKFLOW_FILE,
        "version = 1",
        "unknown = 1\nversion = 1",
    );

    let failures = failures_of(&seeded);

    assert!(
        failures.iter().any(|found| found.contains(WORKFLOW_FILE)),
        "{failures:?}"
    );
}

#[test]
fn it_should_accept_a_check_with_a_variant_outside_the_former_vocabulary_of_its_concern() {
    let tree = tracked_examples();
    let seeded = edited(
        &tree,
        LINT_CHECK,
        "variant = \"lint\"",
        "variant = \"suite\"",
    );

    let failures = failures_of(&seeded);

    assert!(
        !failures.iter().any(|found| found.contains(LINT_CHECK)),
        "{failures:?}"
    );
}

#[test]
fn it_should_reject_a_check_that_comes_after_a_native_check() {
    let tree = tracked_examples();
    let seeded = edited(
        &tree,
        LINT_CHECK,
        "checks.format-sources",
        "checks.run-tests",
    );

    let failures = failures_of(&seeded);

    assert_eq!(
        failures,
        vec![format!(
            "{LINT_CHECK}: after names checks.run-tests, which is not another example"
        )]
    );
}

#[test]
fn it_should_reject_a_toml_file_the_loader_skips_silently() {
    let tree = tracked_examples();
    let skipped = tree["checks/unit-tests.toml"].clone();
    let seeded = with_file(&tree, "notes/x.toml", &skipped);

    let failures = failures_of(&seeded);

    assert_eq!(
        failures,
        vec!["notes/x.toml: id notes.x is not in created".to_owned()]
    );
}

// Layout e variantes

#[test]
fn it_should_keep_the_examples_in_the_layout_of_the_omhl_folder() {
    let tree = tracked_examples();

    let first_level: BTreeSet<&str> = tree
        .keys()
        .map(|path| path.split('/').next().expect("the path has a part"))
        .collect();

    assert_eq!(first_level, BTreeSet::from(LAYOUT));
}

#[test]
fn it_should_place_the_examples_next_to_the_skills_and_not_inside_them() {
    let plugin = repo_root().join("plugin");

    let examples = plugin.join("examples");

    assert!(examples.is_dir());
    assert!(!plugin.join("skills").join("examples").exists());
}

#[test]
fn it_should_tell_the_reader_to_check_a_copy_with_the_dry_run() {
    let readme = fs::read_to_string(repo_root().join(EXAMPLES_DIR).join(EXAMPLES_README))
        .expect("the examples README is readable");

    assert!(readme.contains("omhl load --dry-run"));
}

#[test]
fn it_should_have_a_mechanical_check_with_its_command_limit_rule_and_event() {
    let tree = tracked_examples();

    let checks = parsed(&tree, "checks");

    assert!(checks.iter().any(|(_, table)| {
        ["run", "timeout_secs", "expect", "when"]
            .iter()
            .all(|key| table.contains_key(*key))
    }));
}

#[test]
fn it_should_have_a_text_check_with_its_how_in_a_markdown_sibling() {
    let tree = tracked_examples();

    let checks = parsed(&tree, "checks");

    assert!(
        checks.iter().any(|(path, table)| {
            !table.contains_key("run") && has_markdown_sibling(&tree, path)
        })
    );
}

#[test]
fn it_should_have_a_check_that_comes_after_another_example() {
    let tree = tracked_examples();

    let checks = parsed(&tree, "checks");

    assert!(checks.iter().any(|(_, table)| table.contains_key("after")));
}

#[test]
fn it_should_have_a_business_guideline_with_its_text_in_the_toml() {
    let tree = tracked_examples();

    let guidelines = parsed(&tree, "guidelines");

    assert!(guidelines.iter().any(|(_, table)| {
        table.get("kind").and_then(toml::Value::as_str) == Some("business")
            && table.contains_key("text")
    }));
}

#[test]
fn it_should_have_an_architecture_guideline_with_its_text_in_a_markdown_sibling() {
    let tree = tracked_examples();

    let guidelines = parsed(&tree, "guidelines");

    assert!(guidelines.iter().any(|(path, table)| {
        table.get("kind").and_then(toml::Value::as_str) == Some("architecture")
            && !table.contains_key("text")
            && has_markdown_sibling(&tree, path)
    }));
}

#[test]
fn it_should_have_a_workflow_file_with_only_the_settings_the_loader_accepts_today() {
    let tree = tracked_examples();

    let workflow: toml::Table = toml::from_str(&tree[WORKFLOW_FILE]).expect("valid TOML");

    let keys: BTreeSet<&str> = workflow.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "version",
            "specs",
            "promotion",
            "prefix",
            "git",
            "creation_gate",
            "provider",
            "worktree",
        ])
    );
}

// CA-005: cada chave das integrações aparece no exemplo com a linha do efeito logo acima

#[test]
fn it_should_give_each_integration_key_of_the_workflow_example_an_effect_comment() {
    let tree = tracked_examples();
    let text = &tree[WORKFLOW_FILE];
    let rows: Vec<&str> = text.lines().map(str::trim).collect();

    let unexplained: Vec<String> = INTEGRATION_KEYS
        .iter()
        .filter(|(table, key)| {
            let header = format!("[{table}]");
            let start = rows.iter().position(|row| *row == header);
            let at = start.and_then(|start| {
                rows[start..]
                    .iter()
                    .position(|row| row.starts_with(&format!("{key} =")))
                    .map(|offset| start + offset)
            });

            !matches!(at, Some(at) if at > 0 && rows[at - 1].starts_with('#'))
        })
        .map(|(table, key)| format!("{table}.{key}"))
        .collect();

    assert_eq!(unexplained, Vec::<String>::new());
}

#[test]
fn it_should_not_use_a_key_or_a_program_that_belongs_to_a_later_front() {
    let tree = tracked_examples();
    let tables: Vec<toml::Table> = parsed(&tree, "checks")
        .into_iter()
        .chain(parsed(&tree, "guidelines"))
        .map(|(_, table)| table)
        .chain([toml::from_str(&tree[WORKFLOW_FILE]).expect("valid TOML")])
        .collect();

    let mut keys = Vec::new();
    tables
        .iter()
        .for_each(|table| collect_keys(&toml::Value::Table(table.clone()), &mut keys));
    let programs: Vec<String> = tables
        .iter()
        .filter_map(|table| table.get("run")?.as_array()?.first()?.as_str())
        .map(str::to_owned)
        .collect();

    let forbidden_keys: Vec<&String> = keys
        .iter()
        .filter(|key| FORBIDDEN_KEYS.contains(&key.as_str()))
        .collect();
    let forbidden_programs: Vec<&String> = programs
        .iter()
        .filter(|program| FORBIDDEN_PROGRAMS.contains(&program.as_str()))
        .collect();
    assert_eq!(forbidden_keys, Vec::<&String>::new());
    assert_eq!(forbidden_programs, Vec::<&String>::new());
}

// Exemplos de linguagem e do workflow

#[test]
fn it_should_load_the_rust_examples_without_problems() {
    let tree = from_disk(&RUST_CHECKS);

    assert_eq!(failures_of(&tree), Vec::<String>::new());
}

#[test]
fn it_should_load_the_node_example_without_problems() {
    let tree = from_disk(&NODE_CHECKS);

    assert_eq!(failures_of(&tree), Vec::<String>::new());
}

#[test]
fn it_should_load_the_python_example_without_problems() {
    let tree = from_disk(&PYTHON_CHECKS);

    assert_eq!(failures_of(&tree), Vec::<String>::new());
}

#[test]
fn it_should_load_the_workflow_example_with_interview_and_native_run_without_problems() {
    let tree = from_disk(&[NATIVE_WORKFLOW_EXAMPLE]);
    let as_workflow = Tree::from([(
        WORKFLOW_FILE.to_owned(),
        tree[NATIVE_WORKFLOW_EXAMPLE].clone(),
    )]);

    assert_eq!(failures_of(&as_workflow), Vec::<String>::new());
}

#[test]
fn it_should_turn_the_native_run_tests_on_in_the_workflow_example() {
    let tree = from_disk(&[NATIVE_WORKFLOW_EXAMPLE]);

    let workflow: toml::Table = toml::from_str(&tree[NATIVE_WORKFLOW_EXAMPLE]).expect("valid TOML");

    assert!(workflow.contains_key("interview"));
    assert_eq!(
        workflow["native"]["checks"]
            .as_array()
            .expect("native.checks is an array")
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>(),
        vec!["checks.run-tests"]
    );
    assert!(workflow["native"]["run"]["checks.run-tests"].is_table());
}

#[test]
fn it_should_start_every_rust_example_run_by_clearing_the_five_toolchain_variables() {
    let tree = from_disk(&RUST_CHECKS);
    let expected: Vec<String> = ["env"]
        .into_iter()
        .chain(TOOLCHAIN_VARIABLES.iter().flat_map(|name| ["-u", *name]))
        .map(str::to_owned)
        .collect();

    for path in RUST_CHECKS {
        let words = run_words(&tree, path);

        assert_eq!(words[..expected.len()], expected[..], "{path}");
    }
}

#[test]
fn it_should_bind_every_example_check_to_one_of_the_four_events() {
    let tree = tracked_examples();

    let outside: Vec<String> = parsed(&tree, "checks")
        .into_iter()
        .filter(|(_, table)| {
            !table["when"]["event"]
                .as_str()
                .is_some_and(|event| omhl::app::EVENTS.contains(&event))
        })
        .map(|(path, _)| path)
        .collect();

    assert_eq!(outside, Vec::<String>::new());
}

#[test]
fn it_should_tell_the_reader_that_each_example_is_to_copy_and_adjust() {
    let readme = fs::read_to_string(repo_root().join(EXAMPLES_DIR).join(EXAMPLES_README))
        .expect("the examples README is readable");

    assert!(readme.to_lowercase().contains("copy and adjust"));
}

#[test]
fn it_should_list_every_new_example_in_the_readme() {
    let readme = fs::read_to_string(repo_root().join(EXAMPLES_DIR).join(EXAMPLES_README))
        .expect("the examples README is readable");

    let missing: Vec<&str> = RUST_CHECKS
        .iter()
        .chain(&NODE_CHECKS)
        .chain(&PYTHON_CHECKS)
        .chain(&[NATIVE_WORKFLOW_EXAMPLE])
        .copied()
        .filter(|path| !readme.contains(path))
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}
