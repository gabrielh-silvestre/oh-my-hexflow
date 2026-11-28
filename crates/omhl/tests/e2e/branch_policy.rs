use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Output as Child, omhl, omhl_json};
use support::ext::ExtTree;
use support::git::git_ok;
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const WORK_FORMAT: &str = "work/<id>/<type>/<slug>";
const WORK_BRANCH: &str = "work/w1/fix/a-b";
const DEFAULT_FORMAT: &str = "<type>/wi-<id>__<slug>";
const SPEC_FOLDER: &str = "wi-w1__login";
const SPEC_PART: &str = "parte-01.md";
const MARK_LINE: &str = "omhl-commit-mark v1\n";
const BROKEN: &str = "version = 1\ncolour = true\n";
const INVALID_CONFIG: &str = "invalid-workflow-config";
const EVENTS: [&str; 2] = ["before-commit", "before-push"];

// CLI sobre repositório git real: a política de branch declarada no `workflow.toml`

/// Repositório git real (`main`, `.omhl.toml`, o `workflow.toml` dado e dois checks que gravam um arquivo ao rodar), com o work item `w1` sem tipo e os checks carregados.
struct Repo {
    project: Project,
    scratch: TempDir,
    main: PathBuf,
    record: String,
}

impl Repo {
    fn new(workflow: &str) -> Repo {
        let scratch = TempDir::new().expect("scratch directory is created");
        let project = Project::new("acme");
        let tree = ExtTree::at(project.dir());
        for event in EVENTS {
            let ran = scratch.path().join(format!("ran-{event}"));
            tree.check(&format!("gate-{event}"), &touch_check(event, &ran));
        }
        project.write_workflow(workflow);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Test Author"],
            &["config", "user.email", "author@example.com"],
            &["add", "-A"],
            &["commit", "-q", "-m", "base"],
        ] {
            git_ok(project.dir(), args);
        }
        let main = fs::canonicalize(project.dir()).expect("the main directory exists");
        let mut repo = Repo {
            project,
            scratch,
            main,
            record: String::new(),
        };
        let appended = repo
            .json(&repo.main, &["append"])
            .stdin(json!({ "items": [work_item(None)] }).to_string())
            .run();
        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
        repo.record = json_line(&appended.stdout)["records"][0]["id"]
            .as_str()
            .expect("the record has an id")
            .to_owned();
        let loaded = repo.json(&repo.main, &["load"]).run();
        assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);

        repo
    }

    fn with_branch_table(format: Option<&str>, exceptions: &[(&str, &str)]) -> Repo {
        Repo::new(&workflow_with(format, exceptions))
    }

    fn json(&self, cwd: &Path, args: &[&str]) -> support::child::Cmd {
        omhl_json(args).cwd(cwd).xdg(self.project.xdg())
    }

    fn text(&self, cwd: &Path, args: &[&str]) -> support::child::Cmd {
        omhl(args).cwd(cwd).xdg(self.project.xdg())
    }

    /// Branch nova numa worktree ligada; a política continua sendo a do diretório principal.
    fn linked(&self, branch: &str) -> PathBuf {
        let path = self
            .scratch
            .path()
            .join(format!("linked-{}", branch.replace('/', "-")));
        git_ok(
            &self.main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                branch,
                &path.to_string_lossy(),
            ],
        );

        fs::canonicalize(path).expect("the worktree exists")
    }

    /// Grava e commita o `workflow.toml` na worktree ligada: é a cópia da branch, que não vale como política.
    fn commit_linked_workflow(&self, linked: &Path, toml: &str) {
        let target = linked.join(".omhl/workflow.toml");
        fs::create_dir_all(target.parent().expect("the file has a parent")).expect("parent");
        fs::write(target, toml).expect("the linked workflow is written");
        git_ok(linked, &["add", "-A"]);
        git_ok(linked, &["commit", "-q", "-m", "linked workflow"]);
    }

    /// Estraga só a cópia de trabalho do diretório principal; o blob do `HEAD` fica como estava.
    fn break_main_file(&self) {
        fs::write(self.main.join(".omhl/workflow.toml"), BROKEN).expect("the file is broken");
    }

    fn ran(&self, event: &str) -> bool {
        self.scratch.path().join(format!("ran-{event}")).exists()
    }

    fn check_run(&self, cwd: &Path, event: &str) -> Child {
        self.json(cwd, &["check", "run", "--event", event]).run()
    }

    fn gate_worktree(&self, cwd: &Path, branch: &str) -> Child {
        self.json(cwd, &["gate-worktree", "--branch", branch]).run()
    }

    fn attestations(&self) -> Vec<Value> {
        let listed = self
            .json(&self.main, &["list", "--type", "attestation"])
            .run();
        assert_eq!(listed.code, Some(0), "stderr: {}", listed.stderr);

        json_line(&listed.stdout)["items"]
            .as_array()
            .expect("items is an array")
            .clone()
    }

    fn put_mark(&self, cwd: &Path, name: &str, content: &str) -> PathBuf {
        let mark = git_dir(cwd).join(name);
        fs::write(&mark, content).expect("the mark is written");

        mark
    }

    fn post_commit(&self, cwd: &Path) -> Child {
        self.json(cwd, &["hooks", "post-commit"]).run()
    }

    fn append_type(&self, cwd: &Path, type_value: &str) -> Child {
        self.json(cwd, &["append"])
            .stdin(json!({ "items": [work_item(Some((type_value, &self.record)))] }).to_string())
            .run()
    }

    fn spec_folder(&self) -> PathBuf {
        self.main.join(".omhl/specs").join(SPEC_FOLDER)
    }

    fn seed_spec(&self) -> PathBuf {
        fs::create_dir_all(self.spec_folder()).expect("spec folder is created");
        let part = self.spec_folder().join(SPEC_PART);
        fs::write(&part, "seed\n").expect("spec part is written");

        part
    }

    fn spec_lock(&self, tool_input: &Value, tool: &str) -> Child {
        let event = json!({
            "session_id": "s1",
            "hook_event_name": "PreToolUse",
            "cwd": self.main,
            "tool_name": tool,
            "tool_input": tool_input,
        });
        let elsewhere = TempDir::new().expect("other directory is created");

        omhl(&["hook", "spec-lock"])
            .cwd(elsewhere.path())
            .xdg(self.project.xdg())
            .stdin(event.to_string())
            .run()
    }
}

fn touch_check(event: &str, ran: &Path) -> String {
    let ran = ran.to_str().expect("the path is UTF-8");
    let run = serde_json::to_string(&["sh", "-c", "printf ran > \"$1\"", "sh", ran])
        .expect("the argv serializes");

    format!(
        "blocking = true\nhow = \"Always passes.\"\nrun = {run}\ntimeout_secs = 60\nexpect = [{{ exit = 0, outcome = \"passed\" }}]\n\n[when]\nevent = \"{event}\"\n"
    )
}

fn workflow_with(format: Option<&str>, exceptions: &[(&str, &str)]) -> String {
    let mut text = String::from("version = 1\n\n[branch]\n");
    if let Some(format) = format {
        text.push_str(&format!("format = \"{format}\"\n"));
    }
    for (pattern, mode) in exceptions {
        text.push_str(&format!(
            "\n[[branch.exception]]\npattern = \"{pattern}\"\nmode = \"{mode}\"\n"
        ));
    }

    text
}

fn work_item(typed: Option<(&str, &str)>) -> Value {
    let mut item = json!({ "body": {
        "kind": "node", "id": W1, "type": "work-item",
        "data": { "title": "t", "status": "open" },
    }});
    if let Some((type_value, base)) = typed {
        item["body"]["data"]["type"] = json!(type_value);
        item["base"] = json!(base);
    }

    item
}

fn git_dir(cwd: &Path) -> PathBuf {
    PathBuf::from(git_ok(cwd, &["rev-parse", "--absolute-git-dir"]).trim())
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_of(output: &Child) -> Value {
    json_line(&output.stderr)["error"].clone()
}

fn error_code(output: &Child) -> Value {
    error_of(output)["code"].clone()
}

fn warning_codes(output: &Child) -> Vec<Value> {
    [&output.stdout, &output.stderr]
        .into_iter()
        .filter_map(|text| serde_json::from_str::<Value>(text.trim_end()).ok())
        .find_map(|body| {
            body["warnings"]
                .as_array()
                .or_else(|| body["error"]["warnings"].as_array())
                .cloned()
        })
        .unwrap_or_default()
        .iter()
        .map(|warning| warning["code"].clone())
        .collect()
}

fn hint_of(output: &Child) -> String {
    error_of(output)["details"][0]["context"]["hint"]
        .as_str()
        .expect("the refusal has a hint")
        .to_owned()
}

fn assert_ran_unrecorded(repo: &Repo, output: &Child, event: &str) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["recorded"], json!(false));
    assert!(repo.ran(event), "the check command ran");
    assert_eq!(repo.attestations(), Vec::<Value>::new());
}

fn assert_refused(output: &Child, code: &str) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(error_code(output), json!(code), "stderr: {}", output.stderr);
}

fn assert_allowed(output: &Child, reason: &str) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["allowed"], json!(true));
    assert_eq!(json_line(&output.stdout)["reason"], json!(reason));
}

fn denied_reason(output: &Child) -> String {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let specific = &json_line(&output.stdout)["hookSpecificOutput"];
    assert_eq!(specific["permissionDecision"], json!("deny"));

    specific["permissionDecisionReason"]
        .as_str()
        .expect("the reason is a string")
        .to_owned()
}

fn gate_delivery_labels(repo: &Repo) -> Vec<String> {
    let gate = repo.json(&repo.main, &["gate-delivery", W1]).run();
    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);

    json_line(&gate.stdout)["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .map(|question| question["label"].as_str().expect("a label").to_owned())
        .collect()
}

// CA-006: o formato declarado reconhece o work item em cada chamador

#[test]
fn it_should_freeze_the_spec_by_a_branch_named_with_the_declared_format() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    git_ok(&repo.main, &["branch", WORK_BRANCH]);
    let part = repo.seed_spec();

    let output = repo.spec_lock(&json!({ "file_path": part, "content": "x" }), "Write");

    assert!(denied_reason(&output).contains("spec-frozen"));
}

#[test]
fn it_should_not_freeze_the_spec_by_a_branch_that_the_default_format_does_not_recognize() {
    let repo = Repo::new("version = 1\n");
    git_ok(&repo.main, &["branch", WORK_BRANCH]);
    let part = repo.seed_spec();

    let output = repo.spec_lock(&json!({ "file_path": part, "content": "x" }), "Write");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
}

#[test]
fn it_should_find_the_frozen_work_item_by_a_branch_named_with_the_declared_format_when_the_specs_dir_moves()
 {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    git_ok(&repo.main, &["branch", WORK_BRANCH]);
    let moved = format!(
        "{}\n[specs]\ndir = \"docs/specs\"\n",
        workflow_with(Some(WORK_FORMAT), &[])
    );

    let output = repo.spec_lock(
        &json!({ "file_path": repo.main.join(".omhl/workflow.toml"), "content": moved }),
        "Write",
    );

    let reason = denied_reason(&output);
    assert!(reason.starts_with("spec-dir-frozen: "), "reason: {reason}");
    assert!(reason.contains("wi-w1"), "reason: {reason}");
}

#[test]
fn it_should_write_the_freeze_mark_for_a_branch_named_with_the_declared_format() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    git_ok(&repo.main, &["branch", WORK_BRANCH]);
    let part = repo.seed_spec();

    repo.spec_lock(&json!({ "file_path": part, "content": "x" }), "Write");

    assert!(repo.spec_folder().join(".frozen").is_file());
}

#[test]
fn it_should_mark_the_spec_folder_at_wt_post_start_for_a_branch_named_with_the_declared_format() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    repo.seed_spec();

    let output = repo
        .text(
            &repo.main,
            &["hook", "wt-post-start", "--branch", WORK_BRANCH],
        )
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [SPEC_FOLDER], "already": [] })
    );
}

#[test]
fn it_should_recognize_the_work_item_of_the_declared_format_in_gate_worktree_with_the_type_of_the_format()
 {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);

    let output = repo.gate_worktree(&repo.main, WORK_BRANCH);

    assert_refused(&output, "worktree-gate-refused");
    let context = &error_of(&output)["details"][0]["context"];
    assert_eq!(context["work_item"], json!(W1));
    assert_eq!(context["prefix"], json!("fix"));
}

#[test]
fn it_should_record_the_check_run_on_a_branch_named_with_the_declared_format() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    let linked = repo.linked(WORK_BRANCH);

    let output = repo.check_run(&linked, "before-push");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let body = json_line(&output.stdout);
    assert_eq!(body["recorded"], json!(true));
    assert_eq!(body["work_item"], json!(W1));
}

// CA-007 e EMD-042: a política vem do diretório principal

#[test]
fn it_should_ignore_an_exception_declared_only_in_the_linked_worktree_in_check_run() {
    let repo = Repo::new("version = 1\n");
    let linked = repo.linked("poc/x");
    repo.commit_linked_workflow(&linked, &workflow_with(None, &[("poc/*", "dry-run")]));

    let output = repo.check_run(&linked, "before-push");

    assert_refused(&output, "branch-name-invalid");
    assert!(!repo.ran("before-push"));
}

#[test]
fn it_should_ignore_a_format_declared_only_in_the_linked_worktree_in_check_run() {
    let repo = Repo::new("version = 1\n");
    let linked = repo.linked(WORK_BRANCH);
    repo.commit_linked_workflow(&linked, &workflow_with(Some(WORK_FORMAT), &[]));

    let output = repo.check_run(&linked, "before-push");

    assert_refused(&output, "branch-name-invalid");
}

#[test]
fn it_should_ignore_an_exception_declared_only_in_the_linked_worktree_in_gate_worktree() {
    let repo = Repo::new("version = 1\n");
    let linked = repo.linked("poc/x");
    repo.commit_linked_workflow(&linked, &workflow_with(None, &[("poc/*", "skip")]));

    // `poc/x` já tem worktree e sairia `worktree-exists`; `poc/y` só passa pela exceção, que a cópia ligada não declara.
    let output = repo.gate_worktree(&linked, "poc/y");

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_ignore_a_format_declared_only_in_the_linked_worktree_in_gate_worktree() {
    let repo = Repo::new("version = 1\n");
    let linked = repo.linked("feat/wi-w1__x");
    repo.commit_linked_workflow(&linked, &workflow_with(Some(WORK_FORMAT), &[]));

    let output = repo.gate_worktree(&linked, WORK_BRANCH);

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_refuse_gate_worktree_with_invalid_workflow_config_when_the_main_file_is_unreadable() {
    let repo = Repo::new(BROKEN);

    let output = repo.gate_worktree(&repo.main, "feat/wi-w1__x");

    assert_refused(&output, INVALID_CONFIG);
    assert!(output.stdout.is_empty(), "stdout stays empty");
}

#[test]
fn it_should_apply_an_exception_of_the_head_blob_in_check_run_when_the_main_file_is_unreadable() {
    for event in EVENTS {
        let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run")]);
        repo.break_main_file();
        let linked = repo.linked("poc/x");

        let output = repo.check_run(&linked, event);

        assert_ran_unrecorded(&repo, &output, event);
        assert!(
            warning_codes(&output).contains(&json!(INVALID_CONFIG)),
            "{event}: {}",
            output.stdout
        );
    }
}

#[test]
fn it_should_keep_the_unreadable_main_file_warning_on_a_skip_exception_of_the_head_blob() {
    for event in EVENTS {
        let repo = Repo::with_branch_table(None, &[("poc/*", "skip")]);
        repo.break_main_file();
        let linked = repo.linked("poc/x");

        let output = repo.check_run(&linked, event);

        assert_eq!(output.code, Some(0), "{event}: {}", output.stderr);
        assert!(!repo.ran(event), "{event}: the check command did not run");
        assert!(
            warning_codes(&output).contains(&json!(INVALID_CONFIG)),
            "{event}: {}",
            output.stdout
        );
    }
}

#[test]
fn it_should_apply_the_default_in_check_run_when_the_main_file_and_its_head_blob_are_unreadable() {
    for event in EVENTS {
        let repo = Repo::new(BROKEN);
        let linked = repo.linked("poc/x");

        let output = repo.check_run(&linked, event);

        assert_refused(&output, "branch-name-invalid");
    }
}

#[test]
fn it_should_ignore_an_exception_of_the_linked_copy_in_check_run_when_the_main_file_is_unreadable()
{
    for event in EVENTS {
        let repo = Repo::new(BROKEN);
        let linked = repo.linked("poc/x");
        repo.commit_linked_workflow(&linked, &workflow_with(None, &[("poc/*", "dry-run")]));

        let output = repo.check_run(&linked, event);

        assert_refused(&output, "branch-name-invalid");
    }
}

// CA-008: o `check run` age pelo que a classificadora devolve

#[test]
fn it_should_refuse_develop_without_a_declaration_with_the_default_format_in_the_hint() {
    let repo = Repo::new("version = 1\n");
    let linked = repo.linked("develop");

    let output = repo.check_run(&linked, "before-commit");

    assert_refused(&output, "branch-name-invalid");
    assert!(
        hint_of(&output).contains(DEFAULT_FORMAT),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_show_the_declared_format_in_the_hint_of_a_branch_outside_it() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    let linked = repo.linked("topic");

    let output = repo.check_run(&linked, "before-push");

    assert_refused(&output, "branch-name-invalid");
    assert!(hint_of(&output).contains(WORK_FORMAT), "{}", output.stderr);
}

#[test]
fn it_should_show_the_declared_format_in_the_hint_of_the_protected_main() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);

    let output = repo.check_run(&repo.main, "before-push");

    assert_refused(&output, "branch-protected");
    assert!(hint_of(&output).contains(WORK_FORMAT), "{}", output.stderr);
}

#[test]
fn it_should_refuse_main_with_branch_protected_whatever_the_exceptions() {
    let repo = Repo::with_branch_table(None, &[("develop", "skip")]);

    let output = repo.check_run(&repo.main, "before-commit");

    assert_refused(&output, "branch-protected");
}

#[test]
fn it_should_run_a_dry_run_exception_without_recording() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run")]);
    let linked = repo.linked("poc/x");

    let output = repo.check_run(&linked, "before-commit");

    assert_ran_unrecorded(&repo, &output, "before-commit");
}

#[test]
fn it_should_run_a_dry_run_with_mark_exception_without_recording_when_the_mark_is_present() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run-with-mark")]);
    let linked = repo.linked("poc/x");
    repo.put_mark(&linked, "omhl-commit-mark", MARK_LINE);

    let output = repo.check_run(&linked, "before-commit");

    assert_ran_unrecorded(&repo, &output, "before-commit");
}

#[test]
fn it_should_run_nothing_and_say_the_event_the_branch_and_the_mode_for_a_skip_exception() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "skip")]);
    let linked = repo.linked("poc/x");

    let output = repo
        .text(&linked, &["check", "run", "--event", "before-commit"])
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!repo.ran("before-commit"), "the check command did not run");
    for word in ["before-commit", "poc/x", "skip"] {
        assert!(output.stdout.contains(word), "{word}: {}", output.stdout);
    }
}

#[test]
fn it_should_record_a_work_item_branch_that_also_matches_an_exception() {
    let repo = Repo::with_branch_table(None, &[("feat/*", "skip")]);
    let linked = repo.linked("feat/wi-w1__x");

    let output = repo.check_run(&linked, "before-push");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["recorded"], json!(true));
    assert!(repo.ran("before-push"));
}

// CA-009: a marca é genérica e só vale em `dry-run-with-mark`

#[test]
fn it_should_refuse_before_commit_without_the_mark_on_a_dry_run_with_mark_exception() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run-with-mark")]);
    let linked = repo.linked("poc/x");

    let output = repo.check_run(&linked, "before-commit");

    assert_refused(&output, "commit-mark-missing");
    assert!(!repo.ran("before-commit"));
}

#[test]
fn it_should_not_ask_for_the_mark_on_before_push_on_a_dry_run_with_mark_exception() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run-with-mark")]);
    let linked = repo.linked("poc/x");

    let output = repo.check_run(&linked, "before-push");

    assert_ran_unrecorded(&repo, &output, "before-push");
}

#[test]
fn it_should_not_ask_for_the_mark_on_before_commit_on_a_dry_run_exception() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run")]);
    let linked = repo.linked("poc/x");

    let output = repo.check_run(&linked, "before-commit");

    assert_ran_unrecorded(&repo, &output, "before-commit");
}

#[test]
fn it_should_not_ask_for_the_mark_on_before_commit_on_a_skip_exception() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "skip")]);
    let linked = repo.linked("poc/x");

    let output = repo.check_run(&linked, "before-commit");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_name_the_absolute_mark_path_and_first_line_in_the_commit_mark_missing_hint() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run-with-mark")]);
    let linked = repo.linked("poc/x");

    let output = repo.check_run(&linked, "before-commit");

    let path = git_dir(&linked).join("omhl-commit-mark");
    assert!(
        output.stderr.contains(&path.display().to_string()),
        "stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("omhl-commit-mark v1"),
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_ignore_a_mark_that_only_has_the_old_name() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run-with-mark")]);
    let linked = repo.linked("poc/x");
    repo.put_mark(&linked, "omhl-develop-mark", "omhl-develop-mark v1\n");

    let output = repo.check_run(&linked, "before-commit");

    assert_refused(&output, "commit-mark-missing");
}

// CA-010 e EMD-055: o `post-commit` consome a marca só na exceção `dry-run-with-mark`

fn post_commit_on(workflow_exceptions: &[(&str, &str)], branch: &str) -> (Repo, PathBuf, Child) {
    let repo = Repo::with_branch_table(None, workflow_exceptions);
    let cwd = if branch == "main" {
        repo.main.clone()
    } else {
        repo.linked(branch)
    };
    let mark = repo.put_mark(&cwd, "omhl-commit-mark", MARK_LINE);
    let output = repo.post_commit(&cwd);

    (repo, mark, output)
}

#[test]
fn it_should_delete_the_mark_on_a_dry_run_with_mark_exception_at_post_commit() {
    let (_repo, mark, output) = post_commit_on(&[("poc/*", "dry-run-with-mark")], "poc/x");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout), json!({ "consumed": true }));
    assert!(!mark.exists(), "the mark is consumed");
}

#[test]
fn it_should_find_no_mark_on_the_second_post_commit() {
    let (repo, _mark, _first) = post_commit_on(&[("poc/*", "dry-run-with-mark")], "poc/x");
    let linked = repo.scratch.path().join("linked-poc-x");

    let second = repo.post_commit(&linked);

    assert_eq!(json_line(&second.stdout), json!({ "consumed": false }));
}

#[test]
fn it_should_keep_the_mark_on_a_work_item_branch_at_post_commit() {
    let (_repo, mark, output) = post_commit_on(&[("poc/*", "dry-run-with-mark")], "feat/wi-w1__x");

    assert_eq!(json_line(&output.stdout), json!({ "consumed": false }));
    assert!(mark.exists(), "the mark stays");
}

#[test]
fn it_should_keep_the_mark_on_main_at_post_commit() {
    let (_repo, mark, output) = post_commit_on(&[("poc/*", "dry-run-with-mark")], "main");

    assert_eq!(json_line(&output.stdout), json!({ "consumed": false }));
    assert!(mark.exists(), "the mark stays");
}

#[test]
fn it_should_keep_the_mark_on_a_dry_run_exception_at_post_commit() {
    let (_repo, mark, output) = post_commit_on(&[("poc/*", "dry-run")], "poc/x");

    assert_eq!(json_line(&output.stdout), json!({ "consumed": false }));
    assert!(mark.exists(), "the mark stays");
}

#[test]
fn it_should_keep_the_mark_on_a_skip_exception_at_post_commit() {
    let (_repo, mark, output) = post_commit_on(&[("poc/*", "skip")], "poc/x");

    assert_eq!(json_line(&output.stdout), json!({ "consumed": false }));
    assert!(mark.exists(), "the mark stays");
}

#[test]
fn it_should_consume_the_mark_at_post_commit_by_the_head_blob_when_the_main_file_is_unreadable() {
    let repo = Repo::with_branch_table(None, &[("poc/*", "dry-run-with-mark")]);
    repo.break_main_file();
    let linked = repo.linked("poc/x");
    let mark = repo.put_mark(&linked, "omhl-commit-mark", MARK_LINE);

    let output = repo.post_commit(&linked);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout), json!({ "consumed": true }));
    assert!(!mark.exists(), "the mark is consumed");
}

#[test]
fn it_should_ship_the_commit_mark_plugin_command_and_not_the_develop_one() {
    let commands = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/commands");

    assert!(commands.join("omhl-commit-mark.md").is_file());
    assert!(!commands.join("omhl-develop-mark.md").exists());
}

#[test]
fn it_should_say_commit_mark_in_the_help_of_the_post_commit_command() {
    let repo = Repo::new("version = 1\n");

    let help = repo
        .text(&repo.main, &["hooks", "post-commit", "--help"])
        .run();

    assert_eq!(help.code, Some(0), "stderr: {}", help.stderr);
    assert!(help.stdout.contains("commit mark"), "{}", help.stdout);
}

// CA-011: o `gate-worktree` sobre a classificadora

#[test]
fn it_should_exempt_main_in_gate_worktree() {
    let repo = Repo::new("version = 1\n");

    let output = repo.gate_worktree(&repo.main, "main");

    assert_allowed(&output, "exempt-branch");
}

#[test]
fn it_should_exempt_an_exception_of_every_mode_in_gate_worktree() {
    for mode in ["skip", "dry-run", "dry-run-with-mark"] {
        let repo = Repo::with_branch_table(None, &[("poc/*", mode)]);

        let output = repo.gate_worktree(&repo.main, "poc/x");

        assert_allowed(&output, "exempt-branch");
    }
}

#[test]
fn it_should_decide_the_matrix_by_the_type_of_the_declared_format_in_gate_worktree() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);

    let output = repo.gate_worktree(&repo.main, "work/w1/chore/a-b");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_refuse_develop_without_a_declaration_with_prefix_not_allowed_in_gate_worktree() {
    let repo = Repo::new("version = 1\n");

    let output = repo.gate_worktree(&repo.main, "develop");

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_exempt_develop_declared_as_an_exception_in_gate_worktree() {
    let repo = Repo::with_branch_table(None, &[("develop", "dry-run-with-mark")]);

    let output = repo.gate_worktree(&repo.main, "develop");

    assert_allowed(&output, "exempt-branch");
}

// CA-012 e EMD-045/046: o tipo sem `type` gravado vem do `<type>` do formato

const NO_TYPE_FORMAT: &str = "<id>/<slug>";

fn open_status(repo: &Repo) -> Value {
    let fetched = repo.json(&repo.main, &["get", W1]).run();

    json_line(&fetched.stdout)["node"]["data"]["status"].clone()
}

#[test]
fn it_should_close_with_the_type_of_a_branch_in_the_default_format() {
    let repo = Repo::new("version = 1\n");
    git_ok(&repo.main, &["branch", "chore/wi-w1__x"]);

    let output = repo.json(&repo.main, &["close", W1]).run();

    // sem attestations o gate recusa; chegar ao gate prova que o tipo saiu do nome da branch
    assert_refused(&output, "close-refused");
}

#[test]
fn it_should_close_with_the_type_of_a_branch_in_the_declared_format() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    git_ok(&repo.main, &["branch", "work/w1/chore/x"]);

    let output = repo.json(&repo.main, &["close", W1]).run();

    // sem attestations o gate recusa; chegar ao gate prova que o tipo saiu do nome da branch
    assert_refused(&output, "close-refused");
}

#[test]
fn it_should_refuse_close_without_a_type_and_record_nothing_when_the_format_has_no_type() {
    let repo = Repo::with_branch_table(Some(NO_TYPE_FORMAT), &[]);
    git_ok(&repo.main, &["branch", "w1/x"]);

    let output = repo.json(&repo.main, &["close", W1]).run();

    assert_refused(&output, "work-item-type-unknown");
    assert_eq!(open_status(&repo), json!("open"));
}

#[test]
fn it_should_take_the_type_of_a_branch_in_the_default_format_in_gate_delivery() {
    let repo = Repo::new("version = 1\n");
    git_ok(&repo.main, &["branch", "feat/wi-w1__x"]);

    let labels = gate_delivery_labels(&repo);

    assert!(labels.contains(&"requires:business-premise".to_owned()));
}

#[test]
fn it_should_take_the_type_of_a_branch_in_the_declared_format_in_gate_delivery() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    git_ok(&repo.main, &["branch", "work/w1/feat/x"]);

    let labels = gate_delivery_labels(&repo);

    assert!(labels.contains(&"requires:business-premise".to_owned()));
}

#[test]
fn it_should_refuse_gate_delivery_without_a_type_when_the_format_has_no_type() {
    let repo = Repo::with_branch_table(Some(NO_TYPE_FORMAT), &[]);
    git_ok(&repo.main, &["branch", "w1/x"]);

    let output = repo.json(&repo.main, &["gate-delivery", W1]).run();

    assert_refused(&output, "work-item-type-unknown");
}

#[test]
fn it_should_refuse_an_append_that_loosens_the_type_read_from_a_default_format_branch() {
    let repo = Repo::new("version = 1\n");
    git_ok(&repo.main, &["branch", "feat/wi-w1__x"]);

    let output = repo.append_type(&repo.main, "chore");

    assert_refused(&output, "type-change-loosens");
}

#[test]
fn it_should_refuse_an_append_that_loosens_the_type_read_from_a_declared_format_branch() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);
    git_ok(&repo.main, &["branch", "work/w1/feat/x"]);

    let output = repo.append_type(&repo.main, "chore");

    assert_refused(&output, "type-change-loosens");
}

#[test]
fn it_should_treat_an_append_type_change_as_one_without_a_recorded_type_when_the_format_has_no_type()
 {
    let repo = Repo::with_branch_table(Some(NO_TYPE_FORMAT), &[]);
    git_ok(&repo.main, &["branch", "w1/x"]);

    let output = repo.append_type(&repo.main, "chore");

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_refuse_gate_worktree_with_prefix_not_allowed_naming_the_format_without_a_type() {
    let repo = Repo::with_branch_table(Some(NO_TYPE_FORMAT), &[]);

    let output = repo.gate_worktree(&repo.main, "w1/x");

    assert_refused(&output, "prefix-not-allowed");
    assert!(
        output.stderr.contains(NO_TYPE_FORMAT),
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_name_the_declared_format_in_prefix_not_allowed_instead_of_the_default() {
    let repo = Repo::with_branch_table(Some(WORK_FORMAT), &[]);

    let output = repo.gate_worktree(&repo.main, "work/w1/perf/x");

    assert_refused(&output, "prefix-not-allowed");
    assert!(
        output.stderr.contains(WORK_FORMAT),
        "stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains(DEFAULT_FORMAT),
        "stderr: {}",
        output.stderr
    );
}
