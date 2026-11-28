use crate::support;

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::git::{commit_marker, git_ok, write_executable};
use support::project::Project;
use tempfile::TempDir;

const WORK_FORMAT: &str = "work/<id>/<type>/<slug>";
const WORK_BRANCH: &str = "work/w1/fix/a-b";
const DEFAULT_FORMAT: &str = "<type>/wi-<id>__<slug>";
const DEFAULT_BRANCH: &str = "feat/wi-w1__login";
const FOLDER: &str = "wi-w1__login";
const DEFAULT_WORKFLOW: &str = "version = 1\n";
const INVALID_TOML: &str = "not = [valid\n";
const MARK: &str = ".frozen";
const SPECS: &str = ".omhl/specs";

// Hook de spec e `branch-created` sobre repositório git real: a trava do formato e a política estável

fn declared(format: &str) -> String {
    format!("version = 1\n\n[branch]\nformat = \"{format}\"\n")
}

/// Repositório git real com o `workflow.toml` dado commitado (se houver) e a pasta de dados fora dele.
struct Repo {
    project: Project,
    scratch: TempDir,
    main: PathBuf,
}

impl Repo {
    fn new(committed: Option<&str>) -> Repo {
        let project = Project::new("acme");
        if let Some(text) = committed {
            project.write_workflow(text);
        }
        commit_marker(project.dir());
        let main = fs::canonicalize(project.dir()).expect("the main directory exists");

        Repo {
            project,
            scratch: TempDir::new().expect("scratch directory is created"),
            main,
        }
    }

    /// Repositório com o work item `w1` já numa branch local e a pasta de spec criada.
    fn with_item(committed: Option<&str>, branch: &str) -> Repo {
        let repo = Repo::new(committed);
        git_ok(&repo.main, &["branch", branch]);
        repo.seed();

        repo
    }

    fn folder(&self) -> PathBuf {
        self.main.join(SPECS).join(FOLDER)
    }

    fn seed(&self) {
        fs::create_dir_all(self.folder()).expect("spec folder is created");
        fs::write(self.folder().join("parte-01.md"), "seed\n").expect("spec part is written");
    }

    fn spec_file(&self) -> PathBuf {
        self.folder().join("parte-01.md")
    }

    fn workflow_file(&self) -> PathBuf {
        self.main.join(".omhl/workflow.toml")
    }

    fn has_mark(&self) -> bool {
        self.folder().join(MARK).is_file()
    }

    fn linked(&self, branch: &str) -> PathBuf {
        let path = self.scratch.path().join("linked");
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

    fn hook(&self, cwd: &Path, tool: &str, tool_input: Value) -> Child {
        self.hook_command(cwd, tool, tool_input, None)
    }

    fn hook_command(
        &self,
        cwd: &Path,
        tool: &str,
        tool_input: Value,
        path: Option<&OsStr>,
    ) -> Child {
        let mut event = json!({
            "session_id": "s1",
            "hook_event_name": "PreToolUse",
            "cwd": cwd,
            "tool_name": tool,
        });
        event["tool_input"] = tool_input;
        let elsewhere = TempDir::new().expect("other directory is created");
        let mut command = omhl(&["hook", "spec-lock"])
            .cwd(elsewhere.path())
            .xdg(self.project.xdg())
            .stdin(event.to_string());
        if let Some(path) = path {
            command = command.env("PATH", path);
        }

        command.run()
    }

    /// Escreve o `workflow.toml` principal pelo hook, como a ferramenta `Write` do agente.
    fn write_workflow_by_hook(&self, content: &str) -> Child {
        self.hook(
            &self.main,
            "Write",
            json!({ "file_path": self.workflow_file(), "content": content }),
        )
    }

    fn edit_spec(&self, cwd: &Path) -> Child {
        self.hook(
            cwd,
            "Edit",
            json!({ "file_path": self.spec_file(), "old_string": "seed", "new_string": "x" }),
        )
    }

    fn branch_created(&self, cwd: &Path, branch: &str) -> Child {
        omhl(&["hook", "branch-created", "--branch", branch])
            .cwd(cwd)
            .xdg(self.project.xdg())
            .run()
    }

    fn break_main_file(&self) {
        fs::write(self.workflow_file(), INVALID_TOML).expect("the main file is broken");
    }
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn assert_allowed(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
    assert!(output.stderr.is_empty(), "stderr: {}", output.stderr);
}

fn assert_released_without_error(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
    assert!(
        !output.stderr.contains("\"error\""),
        "stderr: {}",
        output.stderr
    );
}

fn reason_of(output: &Child) -> String {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let specific = &json_line(&output.stdout)["hookSpecificOutput"];
    assert_eq!(specific["permissionDecision"], json!("deny"));

    specific["permissionDecisionReason"]
        .as_str()
        .expect("the reason is a string")
        .to_owned()
}

fn assert_frozen(output: &Child) {
    assert!(reason_of(output).contains("spec-frozen"));
}

// CA-018: a troca do formato grava a marca antes de a spec perder a branch

#[test]
fn it_should_write_the_mark_and_allow_the_edit_in_every_direction_of_the_format_change() {
    let cases: [(&str, Option<String>, &str, String); 5] = [
        (
            "first declaration",
            Some(DEFAULT_WORKFLOW.to_owned()),
            DEFAULT_BRANCH,
            declared(WORK_FORMAT),
        ),
        ("file created", None, DEFAULT_BRANCH, declared(WORK_FORMAT)),
        (
            "key removed",
            Some(declared(WORK_FORMAT)),
            WORK_BRANCH,
            DEFAULT_WORKFLOW.to_owned(),
        ),
        (
            "switched to the default",
            Some(declared(WORK_FORMAT)),
            WORK_BRANCH,
            declared(DEFAULT_FORMAT),
        ),
        (
            "swapped for another format",
            Some(declared(WORK_FORMAT)),
            WORK_BRANCH,
            declared("job/<id>/<type>/<slug>"),
        ),
    ];
    for (label, committed, branch, content) in cases {
        let repo = Repo::with_item(committed.as_deref(), branch);

        let output = repo.write_workflow_by_hook(&content);

        assert_allowed(&output);
        assert!(repo.has_mark(), "{label}: the mark is written");
    }
}

#[test]
fn it_should_allow_an_edit_that_swaps_the_format_line_and_write_the_mark() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);

    let output = repo.hook(
        &repo.main,
        "Edit",
        json!({
            "file_path": repo.workflow_file(),
            "old_string": WORK_FORMAT,
            "new_string": DEFAULT_FORMAT,
        }),
    );

    assert_allowed(&output);
    assert!(repo.has_mark());
}

#[test]
fn it_should_deny_the_next_spec_edit_as_frozen_even_with_the_branch_outside_the_new_format() {
    let repo = Repo::with_item(Some(DEFAULT_WORKFLOW), DEFAULT_BRANCH);
    repo.write_workflow_by_hook(&declared(WORK_FORMAT));
    fs::write(repo.workflow_file(), declared(WORK_FORMAT)).expect("the change is applied");

    let output = repo.edit_spec(&repo.main);

    assert_frozen(&output);
}

#[test]
fn it_should_deny_with_branch_format_unmarked_naming_the_work_item_the_folder_and_the_command_when_the_folder_cannot_be_marked()
 {
    let repo = Repo::new(Some(DEFAULT_WORKFLOW));
    git_ok(&repo.main, &["branch", DEFAULT_BRANCH]);
    let real = repo.main.join("real-folder");
    fs::create_dir_all(&real).expect("real folder is created");
    fs::write(real.join("parte-01.md"), "seed\n").expect("spec file is written");
    fs::create_dir_all(repo.main.join(SPECS)).expect("specs dir is created");
    symlink(&real, repo.folder()).expect("the folder is a link");

    let output = repo.write_workflow_by_hook(&declared(WORK_FORMAT));

    let reason = reason_of(&output);
    assert!(reason.starts_with("branch-format-unmarked: "), "{reason}");
    for fact in ["w1", FOLDER, "omhl hook branch-created --branch"] {
        assert!(reason.contains(fact), "{fact}: {reason}");
    }
    assert!(!real.join(MARK).exists());
}

#[test]
fn it_should_not_run_the_cure_when_the_edit_only_adds_an_exception() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    let with_exception = format!(
        "{}\n[[branch.exception]]\npattern = \"poc/*\"\nmode = \"dry-run\"\n",
        declared(WORK_FORMAT)
    );

    let output = repo.write_workflow_by_hook(&with_exception);

    assert_allowed(&output);
    assert!(!repo.has_mark());
}

#[test]
fn it_should_not_run_the_cure_when_the_edit_only_changes_whitespace() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    let spaced = format!("version = 1\n\n\n[branch]\nformat   =   \"{WORK_FORMAT}\"\n\n");

    let output = repo.write_workflow_by_hook(&spaced);

    assert_allowed(&output);
    assert!(!repo.has_mark());
}

// CA-019: a política vem do blob `HEAD` do diretório principal quando o arquivo está ilegível

#[test]
fn it_should_keep_the_spec_frozen_by_the_head_blob_when_the_main_file_is_invalid_toml() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    repo.break_main_file();

    let output = repo.edit_spec(&repo.main);

    assert_frozen(&output);
}

#[test]
fn it_should_mark_the_spec_folder_at_branch_created_by_the_head_blob_when_the_main_file_is_invalid_toml()
 {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    repo.break_main_file();

    let output = repo.branch_created(&repo.main, WORK_BRANCH);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [FOLDER], "already": [] })
    );
}

#[test]
fn it_should_ignore_the_version_of_a_linked_worktree_head_for_the_frozen_spec() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    repo.break_main_file();
    let linked = repo.linked("scratch-topic");
    fs::create_dir_all(linked.join(".omhl")).expect("the folder exists");
    fs::write(linked.join(".omhl/workflow.toml"), DEFAULT_WORKFLOW).expect("the copy is written");
    git_ok(&linked, &["add", "-A"]);
    git_ok(&linked, &["commit", "-q", "-m", "linked copy"]);

    let output = repo.edit_spec(&linked);

    assert_frozen(&output);
}

#[test]
fn it_should_ignore_the_version_of_a_linked_worktree_head_at_branch_created() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    repo.break_main_file();
    let linked = repo.linked("scratch-topic");
    fs::create_dir_all(linked.join(".omhl")).expect("the folder exists");
    fs::write(linked.join(".omhl/workflow.toml"), DEFAULT_WORKFLOW).expect("the copy is written");
    git_ok(&linked, &["add", "-A"]);
    git_ok(&linked, &["commit", "-q", "-m", "linked copy"]);

    let output = repo.branch_created(&linked, WORK_BRANCH);

    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [FOLDER], "already": [] })
    );
}

#[test]
fn it_should_apply_the_default_format_when_the_head_blob_is_invalid_too() {
    let repo = Repo::with_item(Some(INVALID_TOML), WORK_BRANCH);

    let edited = repo.edit_spec(&repo.main);
    let started = repo.branch_created(&repo.main, WORK_BRANCH);

    assert_released_without_error(&edited);
    assert_eq!(
        json_line(&started.stdout),
        json!({ "marked": [], "already": [] })
    );
}

#[test]
fn it_should_apply_the_default_format_when_the_head_blob_is_absent() {
    let repo = Repo::with_item(None, WORK_BRANCH);
    repo.break_main_file();

    let output = repo.edit_spec(&repo.main);

    assert_released_without_error(&output);
}

#[test]
fn it_should_apply_the_default_format_when_the_head_is_not_born() {
    let project = Project::new("acme");
    project.write_workflow(INVALID_TOML);
    git_ok(project.dir(), &["init", "-q", "-b", "main"]);
    let main = fs::canonicalize(project.dir()).expect("the main directory exists");
    let repo = Repo {
        project,
        scratch: TempDir::new().expect("scratch directory is created"),
        main,
    };
    repo.seed();

    let output = repo.edit_spec(&repo.main);

    assert_released_without_error(&output);
}

#[test]
fn it_should_apply_the_default_format_when_git_fails_to_read_the_head_blob() {
    for word in ["cat-file", "ls-tree"] {
        let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
        repo.break_main_file();
        let fakebin = repo.scratch.path().join("fakebin");
        fs::create_dir_all(&fakebin).expect("fakebin is created");
        let inherited = std::env::var_os("PATH").expect("the test process has a PATH");
        let real_git = std::env::split_paths(&inherited)
            .map(|dir| dir.join("git"))
            .find(|candidate| candidate.is_file())
            .expect("git is on the PATH");
        write_executable(
            &fakebin.join("git"),
            &format!(
                "#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = {word} ] && exit 1; done\nexec {} \"$@\"\n",
                real_git.display()
            ),
        );
        let path =
            std::env::join_paths(std::iter::once(fakebin).chain(std::env::split_paths(&inherited)))
                .expect("PATH is joined");

        let output = repo.hook_command(
            &repo.main,
            "Edit",
            json!({ "file_path": repo.spec_file(), "old_string": "seed", "new_string": "x" }),
            Some(&path),
        );

        assert_released_without_error(&output);
    }
}

#[test]
fn it_should_apply_the_default_format_without_reading_the_blob_when_the_main_file_is_absent() {
    let repo = Repo::with_item(Some(&declared(WORK_FORMAT)), WORK_BRANCH);
    fs::remove_file(repo.workflow_file()).expect("the main file is removed");

    let edited = repo.edit_spec(&repo.main);
    let started = repo.branch_created(&repo.main, WORK_BRANCH);

    assert_allowed(&edited);
    assert_eq!(
        json_line(&started.stdout),
        json!({ "marked": [], "already": [] })
    );
}
