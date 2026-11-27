use crate::support;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use serde_json::{Value, json};
use support::child::{Cmd, omhl, omhl_json};
use support::cycle::{check_file, check_repo};
use support::ext::ExtTree;
use support::git::git_ok;
use support::omhl::RUN_TESTS;
use support::pr_gate::{ALL_NATIVE_CHECKS, CI_GREEN, VERIFY_ANCHORING};
use support::project::Project;

const GUIDELINES: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];
const LINT: &str = "checks.lint";
const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const WORKFLOW: &str = ".omhl/workflow.toml";

// Apoio

fn in_dir(project: &Project, dir: &Path, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(dir).xdg(project.xdg())
}

fn in_project(project: &Project, args: &[&str]) -> Cmd {
    in_dir(project, project.dir(), args)
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// O texto do `render` da pasta dada; o `render` nunca recusa, então sai 0.
fn render_in(project: &Project, dir: &Path) -> String {
    let output = in_dir(project, dir, &["render"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    json_line(&output.stdout)["text"]
        .as_str()
        .expect("the render has a text")
        .to_owned()
}

fn render_of(project: &Project) -> String {
    render_in(project, project.dir())
}

/// O `additionalContext` do `session-start` do projeto.
fn session_context(project: &Project) -> String {
    let stdin = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": project.dir(),
        "source": "startup",
    });
    let output = omhl(&["hook", "session-start"])
        .xdg(project.xdg())
        .stdin(stdin.to_string())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    json_line(&output.stdout)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_owned()
}

/// Os dois textos que a spec iguala: o `render` e o contexto do `session-start`.
fn both_texts(project: &Project) -> [String; 2] {
    [render_of(project), session_context(project)]
}

fn headers(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .collect()
}

fn native_checks_in(text: &str) -> Vec<&str> {
    headers(text)
        .into_iter()
        .filter(|id| ALL_NATIVE_CHECKS.contains(id))
        .collect()
}

fn block<'t>(text: &'t str, id: &str) -> Vec<&'t str> {
    let header = format!("### {id}");

    text.lines()
        .skip_while(|line| *line != header)
        .take_while(|line| !line.is_empty())
        .collect()
}

fn warnings_in(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("WARNING:"))
        .collect()
}

fn repo_enabling(ids: &[&str]) -> Project {
    let project = Project::new("acme");
    project.enable_natives(ids);
    support::git::commit_marker(project.dir());

    project
}

fn append_after(project: &Project, from: &str, to: &str) -> support::child::Output {
    let request = json!({ "items": [{ "body": {
        "kind": "relation", "relation_type": "after", "from": from, "to": to,
    } }] });

    in_project(project, &["append"])
        .stdin(request.to_string())
        .run()
}

/// Repositório git com o check de projeto `lint` carregado e as ativações dadas.
fn repo_with_lint(natives: &[&str]) -> Project {
    let project = Project::new("acme");
    project.enable_natives(natives);
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    support::git::commit_marker(project.dir());
    let loaded = in_project(&project, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);

    project
}

// CA-003: o `render` e o `additionalContext` imprimem só o bloco dos checks ligados

#[test]
fn it_should_print_no_native_check_block_and_keep_the_guidelines_without_a_workflow_file() {
    let project = Project::with_workflow(None);

    for text in both_texts(&project) {
        assert!(native_checks_in(&text).is_empty(), "{text}");
        assert_eq!(headers(&text), GUIDELINES, "{text}");
        assert!(text.contains("## Checks\n"), "{text}");
    }
}

#[test]
fn it_should_print_no_native_check_block_with_an_empty_list() {
    let project = repo_enabling(&[]);

    for text in both_texts(&project) {
        assert!(native_checks_in(&text).is_empty(), "{text}");
    }
}

#[test]
fn it_should_print_only_the_run_tests_block_when_only_it_is_enabled() {
    let project = repo_enabling(&[RUN_TESTS]);

    for text in both_texts(&project) {
        assert_eq!(native_checks_in(&text), [RUN_TESTS], "{text}");
    }
}

#[test]
fn it_should_print_only_the_ci_green_block_when_only_it_is_enabled() {
    let project = repo_enabling(&[CI_GREEN]);

    for text in both_texts(&project) {
        assert_eq!(native_checks_in(&text), [CI_GREEN], "{text}");
    }
}

// CA-021 e CA-022: ligado, o bloco da ancoragem sai como instrução

#[test]
fn it_should_print_only_the_anchoring_block_when_only_it_is_enabled() {
    let project = repo_enabling(&[VERIFY_ANCHORING]);

    for text in both_texts(&project) {
        assert_eq!(native_checks_in(&text), [VERIFY_ANCHORING], "{text}");
    }
}

#[test]
fn it_should_print_the_three_blocks_when_the_three_are_enabled() {
    let project = repo_enabling(&ALL_NATIVE_CHECKS);

    for text in both_texts(&project) {
        assert_eq!(native_checks_in(&text), ALL_NATIVE_CHECKS, "{text}");
    }
}

// CA-004: a linha `after` some quando o destino é um nativo desligado

#[test]
fn it_should_print_the_after_line_of_ci_green_when_run_tests_is_enabled_too() {
    let project = repo_enabling(&[RUN_TESTS, CI_GREEN]);

    for text in both_texts(&project) {
        assert!(
            block(&text, CI_GREEN).contains(&"- after: checks.run-tests"),
            "{text}"
        );
    }
}

#[test]
fn it_should_print_no_after_line_for_ci_green_when_only_it_is_enabled() {
    let project = repo_enabling(&[CI_GREEN]);

    for text in both_texts(&project) {
        assert!(!text.contains("- after:"), "{text}");
    }
}

#[test]
fn it_should_not_print_the_after_line_of_a_project_check_to_a_disabled_native() {
    let project = repo_with_lint(&[CI_GREEN]);
    let appended = append_after(&project, LINT, RUN_TESTS);

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    for text in both_texts(&project) {
        assert!(
            !block(&text, LINT)
                .iter()
                .any(|line| line.starts_with("- after:")),
            "{text}"
        );
    }
}

#[test]
fn it_should_print_the_after_line_of_a_project_check_to_an_enabled_native() {
    let project = repo_with_lint(&[RUN_TESTS]);
    let appended = append_after(&project, LINT, RUN_TESTS);

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    for text in both_texts(&project) {
        assert!(
            block(&text, LINT).contains(&"- after: checks.run-tests"),
            "{text}"
        );
    }
}

#[test]
fn it_should_print_the_after_line_of_a_project_check_to_another_project_check() {
    let project = repo_with_lint(&[]);
    ExtTree::at(project.dir()).check("docs", LINT_TOML);
    let loaded = in_project(&project, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    let appended = append_after(&project, LINT, "checks.docs");

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    for text in both_texts(&project) {
        assert!(
            block(&text, LINT).contains(&"- after: checks.docs"),
            "{text}"
        );
    }
}

#[test]
fn it_should_keep_running_the_check_of_the_event_with_an_after_to_a_disabled_native() {
    let toml = check_file("style", "format", "exit 0");
    let project = check_repo(&[("payroll-format", &toml)]);
    let appended = append_after(&project, "checks.payroll-format", RUN_TESTS);

    let run = in_project(&project, &["check", "run", "--event", "before-pr"]).run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let results = &json_line(&run.stdout)["results"];
    assert_eq!(results[0]["check"], json!("checks.payroll-format"));
    assert_eq!(results[0]["outcome"], json!("passed"));
}

#[test]
fn it_should_keep_refusing_an_after_that_closes_a_cycle_with_the_native_pair_while_all_are_disabled()
 {
    let project = repo_with_lint(&[]);
    let first = append_after(&project, LINT, CI_GREEN);

    let closing = append_after(&project, RUN_TESTS, LINT);

    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);
    assert_eq!(closing.code, Some(1), "stderr: {}", closing.stderr);
    assert_eq!(
        json_line(&closing.stderr)["error"]["code"],
        json!("CYCLE_REJECTED")
    );
}

// CA-008: a ativação vem do diretório principal, de qualquer worktree

fn linked_worktree(project: &Project) -> std::path::PathBuf {
    let path = project.dir().with_extension("linked");
    git_ok(
        project.dir(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/wi-w1__x",
            path.to_str().expect("utf-8"),
        ],
    );

    path
}

fn write_workflow_in(dir: &Path, text: &str) {
    fs::create_dir_all(dir.join(".omhl")).expect("the .omhl directory is created");
    fs::write(dir.join(WORKFLOW), text).expect("the workflow file is written");
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", "workflow copy"]);
}

#[test]
fn it_should_print_the_block_from_a_worktree_when_the_main_directory_enables_it_and_the_copy_disables_it()
 {
    let project = repo_enabling(&[RUN_TESTS]);
    let linked = linked_worktree(&project);
    write_workflow_in(&linked, "version = 1\n[native]\nchecks = []\n");

    let text = render_in(&project, &linked);

    assert_eq!(native_checks_in(&text), [RUN_TESTS], "{text}");
    fs::remove_dir_all(linked).expect("the worktree is removed");
}

#[test]
fn it_should_print_no_block_from_a_worktree_when_the_main_directory_disables_it_and_the_copy_enables_it()
 {
    let project = repo_enabling(&[]);
    let linked = linked_worktree(&project);
    write_workflow_in(
        &linked,
        "version = 1\n[native]\nchecks = [\"checks.run-tests\"]\n",
    );

    let text = render_in(&project, &linked);

    assert!(native_checks_in(&text).is_empty(), "{text}");
    assert!(warnings_in(&text).is_empty(), "{text}");
    fs::remove_dir_all(linked).expect("the worktree is removed");
}

#[test]
fn it_should_not_use_the_copy_of_the_current_directory_when_there_is_no_git() {
    let project = Project::new("acme");
    project.enable_natives(&ALL_NATIVE_CHECKS);

    for text in both_texts(&project) {
        assert!(native_checks_in(&text).is_empty(), "{text}");
    }
}

// CA-009 e CA-010: o arquivo que o `omhl` não aceita

fn repo_with_invalid(setup: impl FnOnce(&Path)) -> Project {
    let project = Project::with_workflow(None);
    setup(project.dir());

    project
}

fn write_invalid(dir: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(dir.join(".omhl")).expect("the .omhl directory is created");
    fs::write(dir.join(WORKFLOW), bytes).expect("the workflow file is written");
}

/// Sem recusar, sem bloco nativo, com o aviso `invalid-workflow-config` uma vez em cada texto.
fn assert_not_accepted(project: &Project) {
    for text in both_texts(project) {
        let warnings = warnings_in(&text);
        assert!(native_checks_in(&text).is_empty(), "{text}");
        assert_eq!(headers(&text), GUIDELINES, "{text}");
        assert_eq!(warnings.len(), 1, "{text}");
        assert!(warnings[0].contains("invalid-workflow-config"), "{text}");
        assert_eq!(text.matches("invalid-workflow-config").count(), 1, "{text}");
    }
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_has_a_syntax_error() {
    let project = repo_with_invalid(|dir| write_invalid(dir, "version = 1\n[native\n"));

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_has_an_unknown_top_level_key() {
    let project = repo_with_invalid(|dir| write_invalid(dir, "version = 1\ncolour = true\n"));

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_lists_a_wrong_native_id() {
    let project = repo_with_invalid(|dir| {
        write_invalid(dir, "version = 1\n[native]\nchecks = [\"checks.lint\"]\n");
    });

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_has_an_unsupported_version() {
    let project = repo_with_invalid(|dir| write_invalid(dir, "version = 2\n"));

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_is_not_utf8() {
    let project = repo_with_invalid(|dir| write_invalid(dir, [0xff, 0xfe, 0x00]));

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_is_a_symlink() {
    let project = repo_with_invalid(|dir| {
        fs::write(dir.join("elsewhere.toml"), "version = 1\n").expect("the target is written");
        fs::create_dir_all(dir.join(".omhl")).expect("the .omhl directory is created");
        symlink(dir.join("elsewhere.toml"), dir.join(WORKFLOW)).expect("the link is created");
    });

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_file_is_above_the_size_cap() {
    let project = repo_with_invalid(|dir| {
        write_invalid(dir, format!("version = 1\n#{}\n", "x".repeat(65_536)));
    });

    assert_not_accepted(&project);
}

#[test]
fn it_should_warn_and_print_no_block_when_the_workflow_path_is_a_directory() {
    let project = repo_with_invalid(|dir| {
        fs::create_dir_all(dir.join(WORKFLOW)).expect("the directory is created");
    });

    assert_not_accepted(&project);
}

#[test]
fn it_should_print_no_block_even_when_the_invalid_file_listed_a_native_check() {
    let project = repo_with_invalid(|dir| {
        write_invalid(
            dir,
            "version = 1\ncolour = true\n[native]\nchecks = [\"checks.run-tests\"]\n",
        );
    });

    for text in both_texts(&project) {
        assert!(native_checks_in(&text).is_empty(), "{text}");
    }
}

#[test]
fn it_should_name_the_fix_of_the_file_as_the_next_step_of_the_invalid_workflow_line() {
    let project = repo_with_invalid(|dir| write_invalid(dir, "version = 2\n"));

    for text in both_texts(&project) {
        let line = warnings_in(&text)[0];
        assert!(line.contains("omhl load --dry-run"), "{line}");
        assert!(
            line.contains("gate-pr and close refuse until it is fixed"),
            "{line}"
        );
        assert!(line.contains("no native check is printed"), "{line}");
        assert!(!line.contains("git init"), "{line}");
    }
}

#[test]
fn it_should_print_the_invalid_workflow_line_in_the_preamble_before_the_checks() {
    let project = repo_with_invalid(|dir| write_invalid(dir, "version = 2\n"));

    for text in both_texts(&project) {
        let warning_at = text.find("WARNING:").expect("the warning is printed");
        let checks_at = text.find("## Checks").expect("the checks are printed");
        assert!(warning_at < checks_at, "{text}");
    }
}

#[test]
fn it_should_warn_about_an_invalid_workflow_once_in_the_session_context_with_pending_extensions() {
    let project = repo_with_invalid(|dir| write_invalid(dir, "version = 2\n"));
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let context = session_context(&project);

    assert_eq!(
        context.matches("invalid-workflow-config").count(),
        1,
        "{context}"
    );
    assert!(
        context.contains("NOTE: 1 extension change(s) not loaded"),
        "{context}"
    );
}

#[test]
fn it_should_print_no_warning_with_an_accepted_workflow_file() {
    let project = repo_enabling(&[RUN_TESTS]);

    for text in both_texts(&project) {
        assert!(warnings_in(&text).is_empty(), "{text}");
    }
}

#[test]
fn it_should_print_no_warning_without_a_workflow_file() {
    let project = Project::with_workflow(None);

    for text in both_texts(&project) {
        assert!(warnings_in(&text).is_empty(), "{text}");
    }
}

#[test]
fn it_should_warn_with_main_dir_unknown_and_git_init_in_a_project_without_git() {
    let project = Project::new("acme");

    for text in both_texts(&project) {
        let warnings = warnings_in(&text);
        assert_eq!(warnings.len(), 1, "{text}");
        assert!(warnings[0].contains("main-dir-unknown"), "{text}");
        assert!(warnings[0].contains("git init"), "{text}");
        assert!(!warnings[0].contains("omhl load --dry-run"), "{text}");
        assert!(!text.contains("invalid-workflow-config"), "{text}");
        assert!(native_checks_in(&text).is_empty(), "{text}");
    }
}

#[test]
fn it_should_print_the_main_dir_unknown_line_in_the_preamble_before_the_checks() {
    let project = Project::new("acme");

    for text in both_texts(&project) {
        let warning_at = text.find("WARNING:").expect("the warning is printed");
        let checks_at = text.find("## Checks").expect("the checks are printed");
        assert!(warning_at < checks_at, "{text}");
    }
}

// CA-024: sem git, o `render` muda só pela linha `main-dir-unknown` do preâmbulo

#[test]
fn it_should_render_the_same_text_without_git_except_for_the_main_dir_unknown_line() {
    let without_git = render_of(&Project::new("acme"));
    let with_git = render_of(&Project::with_git("acme"));

    let line = warnings_in(&without_git)[0];
    assert_eq!(without_git.replacen(&format!("{line}\n"), "", 1), with_git);
}
