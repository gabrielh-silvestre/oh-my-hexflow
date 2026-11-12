mod support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::ext::ExtTree;
use support::omhl::decision_data;
use support::project::Project;
use tempfile::TempDir;

const NATIVE_CHECKS: [&str; 3] = [
    "checks.run-tests",
    "checks.ci-green-before-merge",
    "checks.verify-review-comment-anchoring",
];
const NATIVE_GUIDELINES: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];
const CONTEXT_CEILING: usize = 10_000;
const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const INVALID_TOML: &str = "this is = [not valid";
const PENDING_NOTE: &str = "NOTE: 1 extension change(s) not loaded into tenant acme; run omhl load";
const ORPHAN_NOTE: &str = "NOTE: 1 check(s) lost their file and their loaded command no longer runs: checks.lint; restore the file or run omhl check retire <id>";

// Apoio

fn session_start(cwd: &Path) -> Value {
    json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": cwd,
        "source": "startup",
    })
}

/// Roda `omhl hook session-start` com o stdin dado, de uma pasta que não é a do projeto,
/// para que o `cwd` só possa vir do stdin.
fn run_hook(xdg: &Path, stdin: &str) -> Child {
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["hook", "session-start"])
        .cwd(elsewhere.path())
        .xdg(xdg)
        .stdin(stdin)
        .run()
}

fn hook_for(project: &Project) -> Child {
    run_hook(project.xdg(), &session_start(project.dir()).to_string())
}

fn in_project(project: &Project, args: &[&str]) -> Child {
    omhl(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn append_decisions(project: &Project, decisions: &[(String, String)]) {
    let items: Vec<Value> = decisions
        .iter()
        .map(|(id, text)| {
            json!({
                "body": {
                    "kind": "node",
                    "id": id,
                    "type": "decision",
                    "data": decision_data(text, None),
                }
            })
        })
        .collect();

    let appended = omhl(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(json!({ "items": items }).to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn append_use_wal(project: &Project) {
    append_decisions(
        project,
        &[("decisions.use-wal".to_owned(), "Use WAL".to_owned())],
    );
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// O `additionalContext` da saída do hook; falha se a saída não for o objeto esperado.
fn context_of(output: &Child) -> String {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    json_line(&output.stdout)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_owned()
}

fn data_files(xdg: &Path) -> usize {
    fs::read_dir(xdg.join("omhl")).map_or(0, Iterator::count)
}

// Sem marcador

#[test]
fn it_should_print_nothing_and_create_nothing_without_a_marker() {
    let bare = TempDir::new().expect("bare directory is created");
    let xdg = TempDir::new().expect("data directory is created");

    let output = run_hook(xdg.path(), &session_start(bare.path()).to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
    assert_eq!(data_files(xdg.path()), 0, "no database file is created");
}

// Saída

#[test]
fn it_should_print_one_json_object_with_the_session_start_context() {
    let project = Project::new("acme");

    let output = hook_for(&project);

    let value = json_line(&output.stdout);
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stdout.matches('\n').count(), 1, "one line");
    assert!(output.stdout.ends_with('\n'), "the line ends with LF");
    assert_eq!(
        value["hookSpecificOutput"]["hookEventName"],
        json!("SessionStart")
    );
    assert!(value["hookSpecificOutput"]["additionalContext"].is_string());
    assert_eq!(
        value.as_object().expect("the root is an object").len(),
        1,
        "hookSpecificOutput is the only root key"
    );
}

#[test]
fn it_should_inject_the_native_base_and_the_project_nodes() {
    let project = Project::new("acme");
    append_use_wal(&project);

    let context = context_of(&hook_for(&project));

    for id in NATIVE_CHECKS.iter().chain(&NATIVE_GUIDELINES) {
        assert!(context.contains(&format!("### {id}\n")), "missing {id}");
    }
    assert!(context.contains("### decisions.use-wal\n"));
}

#[test]
fn it_should_keep_the_context_within_10000_characters() {
    let project = Project::new("acme");
    let text = "x".repeat(1_900);
    let decisions: Vec<(String, String)> = (0..200)
        .map(|n| (format!("decisions.d{n:03}"), text.clone()))
        .collect();
    for batch in decisions.chunks(50) {
        append_decisions(&project, batch);
    }

    let context = context_of(&hook_for(&project));

    let omitted = json_line(&in_project(&project, &["render"]).stdout)["omitted"]
        .as_u64()
        .expect("omitted is a number");
    let footer = format!("[{omitted} more nodes omitted for size; run `omhl list` to read them]");
    assert!(omitted > 0, "the budget forces an omission");
    assert!(context.chars().count() <= CONTEXT_CEILING);
    assert!(context.ends_with('\n'), "the context ends with LF");
    assert_eq!(context.lines().last(), Some(footer.as_str()));
}

// Descoberta do marcador e tolerância da entrada

#[test]
fn it_should_find_the_marker_from_a_subdirectory_cwd() {
    let project = Project::new("acme");
    let nested = project.dir().join("a").join("b").join("c");
    fs::create_dir_all(&nested).expect("nested directory is created");

    let output = run_hook(project.xdg(), &session_start(&nested).to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(json_line(&output.stdout)["hookSpecificOutput"].is_object());
}

#[test]
fn it_should_tolerate_extra_stdin_fields_and_an_unknown_source() {
    let project = Project::new("acme");
    let stdin = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": project.dir(),
        "source": "future",
        "model": "x",
        "extra": { "a": 1 },
    });

    let output = run_hook(project.xdg(), &stdin.to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(json_line(&output.stdout)["hookSpecificOutput"].is_object());
}

// Erros

#[test]
fn it_should_exit_one_with_no_stdout_on_invalid_stdin() {
    let project = Project::new("acme");

    let output = run_hook(project.xdg(), "{");

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(output.stderr.matches('\n').count(), 1, "one error line");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("input-invalid-json")
    );
}

#[test]
fn it_should_refuse_a_relative_cwd() {
    let project = Project::new("acme");
    let stdin = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": "relative/project",
        "source": "startup",
    });

    let output = run_hook(project.xdg(), &stdin.to_string());

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("invalid-hook-input")
    );
}

#[test]
fn it_should_exit_one_with_no_stdout_on_an_invalid_marker() {
    let project = Project::new("acme");
    fs::write(
        project.dir().join(".omhl.toml"),
        "version = 1\ntenant = \"acme\"\nextra = 1\n",
    )
    .expect("marker is overwritten");

    let output = hook_for(&project);

    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error["code"], json!("invalid-marker"));
    assert!(
        error["message"]
            .as_str()
            .expect("message is a string")
            .contains(".omhl.toml")
    );
}

// Idempotência e igualdade com o render

#[test]
fn it_should_print_identical_output_and_write_nothing_on_a_second_run() {
    let project = Project::new("acme");
    let total = |project: &Project| {
        json_line(&in_project(project, &["verify"]).stdout)["total_records"].clone()
    };
    let before = total(&project);

    let first = hook_for(&project);
    let second = hook_for(&project);
    let after = total(&project);

    assert!(context_of(&first).contains("### checks.run-tests\n"));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(before, after);
}

#[test]
fn it_should_inject_the_same_text_as_the_render_action() {
    let project = Project::new("acme");
    append_use_wal(&project);

    let context = context_of(&hook_for(&project));
    let rendered = in_project(&project, &["render"]);

    assert_eq!(rendered.code, Some(0), "stderr: {}", rendered.stderr);
    assert_eq!(json_line(&rendered.stdout)["text"], json!(context));
}

// Aviso de extensions

fn warning_line(context: &str) -> &str {
    context
        .lines()
        .find(|line| line.starts_with("WARNING:"))
        .expect("the context has a WARNING line")
}

#[test]
fn it_should_append_a_note_when_extension_changes_are_pending() {
    let project = Project::new("acme");
    let baseline = context_of(&hook_for(&project));
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let context = context_of(&hook_for(&project));

    let appended = context
        .strip_prefix(baseline.as_str())
        .expect("the rendered text comes first");
    assert_eq!(appended.trim_end(), format!("\n{PENDING_NOTE}"));
}

#[test]
fn it_should_list_the_orphaned_checks_in_a_note() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    let loaded = in_project(&project, &["load"]);
    fs::remove_file(project.dir().join(".omhl/checks/lint.toml")).expect("the file is deleted");

    let context = context_of(&hook_for(&project));

    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    assert!(context.contains(ORPHAN_NOTE), "context: {context}");
    assert!(!context.contains(PENDING_NOTE), "context: {context}");
}

#[test]
fn it_should_not_append_anything_when_extensions_are_synced() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    let loaded = in_project(&project, &["load"]);

    let context = context_of(&hook_for(&project));
    let rendered = in_project(&project, &["render"]);

    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    assert!(context.contains("### checks.lint\n"), "the check is loaded");
    assert_eq!(json_line(&rendered.stdout)["text"], json!(context));
    assert!(!context.contains("NOTE:"), "no note");
    assert!(!context.contains("WARNING:"), "no warning");
}

#[test]
fn it_should_not_write_to_the_database_when_extensions_are_pending() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let hooked = hook_for(&project);
    let listed = in_project(&project, &["list", "--type", "check"]);

    let ids: Vec<Value> = json_line(&listed.stdout)["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|item| item["id"].clone())
        .collect();
    assert_eq!(hooked.code, Some(0), "stderr: {}", hooked.stderr);
    assert_eq!(listed.code, Some(0), "stderr: {}", listed.stderr);
    assert!(
        !ids.contains(&json!("checks.lint")),
        "the check stays unloaded"
    );
}

#[test]
fn it_should_warn_with_codes_and_counts_but_never_paths() {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .check("alpha-check", INVALID_TOML)
        .check("beta-check", INVALID_TOML)
        .symlink(".omhl/checks/gamma-link.toml", "alpha-check.toml");

    let context = context_of(&hook_for(&project));

    let line = warning_line(&context);
    assert!(line.contains("invalid-toml x2"), "counts the invalid files");
    assert!(line.contains("symlink x1"), "counts the symlink");
    for leaked in [".omhl/", "alpha-check", "beta-check", "gamma-link"] {
        assert!(!line.contains(leaked), "the warning leaks {leaked}");
    }
}

#[test]
fn it_should_keep_the_rendered_text_when_the_extension_check_fails() {
    let project = Project::new("acme");
    let baseline = context_of(&hook_for(&project));
    ExtTree::at(project.dir()).check("lint", INVALID_TOML);

    let context = context_of(&hook_for(&project));

    let appended = context
        .strip_prefix(baseline.as_str())
        .expect("the rendered text comes first");
    assert!(
        appended.starts_with("\nWARNING:"),
        "the warning follows a blank line"
    );
}

#[test]
fn it_should_keep_the_warning_line_within_480_characters() {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .file(".omhl/checks/UPPER.toml", LINT_TOML)
        .check("broken", INVALID_TOML)
        .symlink(".omhl/checks/linked.toml", "broken.toml")
        .check("huge", &"#".repeat(65_537))
        .file(".omhl/checks/binary.toml", [0xff, 0xfe, 0xfd])
        .check(
            "nohow",
            "blocking = true\n\n[when]\nevent = \"before-commit\"\n",
        );

    let context = context_of(&hook_for(&project));

    assert!(warning_line(&context).chars().count() <= 480);
}
