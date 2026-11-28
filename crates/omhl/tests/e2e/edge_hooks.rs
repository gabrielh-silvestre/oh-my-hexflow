use crate::support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use support::child::{Cmd, Output as Child, omhl, omhl_json};
use support::git::{git_in, git_ok, write_executable};
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const W1_BRANCH: &str = "feat/wi-w1__x";
const W1_SPEC: &str = "wi-w1__x";
const W2_SPEC: &str = "wi-w2__y";
const LINKED: &str = "feat/wi-x1__login";
const LINKED_DIR: &str = ".worktrees/feat-wi-x1__login";
const EXCLUDE_LINE: &str = "/.worktrees/\n";
const BROKEN: &str = "version = 1\ncolour = true\n";
const GATE_ON: &str = "version = 1\n\n[creation_gate]\nenabled = true\n";
const GIT_HOOKS_ON: &str = "version = 1\n\n[git]\nhooks = true\n";
const PROVIDER_ON: &str = "version = 1\n\n[provider]\nname = \"github\"\n";
const PUSH_WARNING: &str = "protected-branch-push";
const GATE_UNAVAILABLE: &str = "creation-gate-unavailable";
const INSTALL: &str = "omhl hooks install";

// Hooks novos do Claude Code sobre um repositório git real (CMP-004, CMP-008 a CMP-016)

/// Repositório git real em `main` com `.omhl.toml` commitado; o `workflow.toml` é opcional.
struct Edge {
    project: Project,
    scratch: TempDir,
    main: PathBuf,
}

impl Edge {
    fn new() -> Edge {
        Edge::build(Project::new("acme"))
    }

    /// Sem marcador: o aviso de push vale em todo projeto com o plugin.
    fn unmarked() -> Edge {
        let edge = Edge::build(Project::new("acme"));
        fs::remove_file(edge.main.join(".omhl.toml")).expect("the marker is removed");
        edge.git(&["add", "-A"]);
        edge.git(&["commit", "-q", "-m", "drop marker"]);

        edge
    }

    fn build(project: Project) -> Edge {
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

        Edge {
            project,
            scratch: TempDir::new().expect("scratch directory is created"),
            main,
        }
    }

    fn git(&self, args: &[&str]) -> String {
        git_ok(&self.main, args)
    }

    /// Grava e commita o `workflow.toml` do diretório principal.
    fn workflow(&self, text: &str) -> &Edge {
        self.project.write_workflow(text);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "workflow"]);

        self
    }

    /// Estraga só a cópia de trabalho, sem commit.
    fn break_workflow(&self) {
        self.project.write_workflow(BROKEN);
    }

    fn seed_spec(&self, folder: &str) -> PathBuf {
        let dir = self.main.join(".omhl/specs").join(folder);
        fs::create_dir_all(&dir).expect("spec folder is created");
        fs::write(dir.join("parte-01.md"), "seed\n").expect("spec part is written");

        dir
    }

    fn frozen(&self, folder: &str) -> bool {
        self.main
            .join(".omhl/specs")
            .join(folder)
            .join(".frozen")
            .is_file()
    }

    fn exclude_path(&self) -> PathBuf {
        self.main.join(".git/info/exclude")
    }

    fn exclude_text(&self) -> String {
        fs::read_to_string(self.exclude_path()).unwrap_or_default()
    }

    fn linked_under_worktrees(&self) {
        self.git(&["worktree", "add", "-q", "-b", LINKED, LINKED_DIR]);
    }

    fn append(&self, items: &[Value]) {
        let appended = omhl_json(&["append"])
            .cwd(&self.main)
            .xdg(self.project.xdg())
            .stdin(json!({ "items": items }).to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    fn work_item(&self, type_value: Option<&str>) -> &Edge {
        let mut data = json!({ "title": "t", "status": "open" });
        if let Some(type_value) = type_value {
            data["type"] = json!(type_value);
        }
        self.append(&[node(W1, "work-item", &data)]);

        self
    }

    fn technical_premise(&self) -> &Edge {
        let data = json!({
            "text": "a premise",
            "status": "confirmed",
            "kind": "technical",
            "work_item": W1,
            "confirmed_by": { "by": "person:tester", "question": "Adopt it?", "answer": "Yes." },
        });
        self.append(&[node("premises.t1", "premise", &data)]);

        self
    }

    /// `omhl hook <name>` de uma pasta fora do projeto: o `cwd` vem do evento.
    fn hook_cmd(&self, name: &str, event: &Value) -> Cmd {
        omhl(&["hook", name])
            .cwd(self.scratch.path())
            .xdg(self.project.xdg())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(event.to_string())
    }

    fn hook(&self, name: &str, event: &Value) -> Child {
        self.hook_cmd(name, event).run()
    }

    fn birth(&self, command: &str) -> Child {
        self.hook(
            "branch-birth",
            &bash_event("PostToolUse", &self.main, command),
        )
    }

    fn guard(&self, command: &str) -> Child {
        self.guard_from(&self.main, command)
    }

    fn guard_from(&self, cwd: &Path, command: &str) -> Child {
        self.hook("command-guard", &bash_event("PreToolUse", cwd, command))
    }

    fn session_start(&self) -> Child {
        self.session_start_from(&self.main)
    }

    fn session_start_from(&self, cwd: &Path) -> Child {
        let event = json!({
            "hook_event_name": "SessionStart",
            "session_id": "s1",
            "cwd": cwd,
            "source": "startup",
        });

        self.hook("session-start", &event)
    }

    fn gate_worktree(&self, branch: &str) -> Child {
        omhl_json(&["gate-worktree", "--branch", branch])
            .cwd(&self.main)
            .xdg(self.project.xdg())
            .run()
    }

    fn install_hooks(&self) {
        let output = omhl_json(&["hooks", "install"])
            .cwd(&self.main)
            .xdg(self.project.xdg())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .run();

        assert_eq!(output.code, Some(0), "install stderr: {}", output.stderr);
    }

    fn hooks_dir(&self) -> PathBuf {
        self.project.xdg().join("omhl").join("hooks")
    }

    /// Pasta com um `git` que grava o argv em `git.log` e repassa ao git real; `check-ignore` sai com `ignore_exit` quando dado.
    fn logging_git(&self, ignore_exit: Option<i32>) -> PathBuf {
        let bin = self.scratch.path().join("bin");
        fs::create_dir_all(&bin).expect("bin folder is created");
        let log = self.scratch.path().join("git.log");
        let fail = ignore_exit
            .map(|code| format!("case \"$*\" in *check-ignore*) exit {code};; esac\n"))
            .unwrap_or_default();
        write_executable(
            &bin.join("git"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\n{fail}exec \"{}\" \"$@\"\n",
                log.display(),
                real_git().display()
            ),
        );

        bin
    }

    fn git_log(&self) -> String {
        fs::read_to_string(self.scratch.path().join("git.log")).unwrap_or_default()
    }

    /// Pasta com um `gh` falso que grava cada chamada em `gh.log` e responde `trunk`.
    fn fake_gh(&self) -> PathBuf {
        let bin = self.scratch.path().join("gh-bin");
        fs::create_dir_all(&bin).expect("gh folder is created");
        write_executable(
            &bin.join("gh"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\necho '{{\"defaultBranchRef\":{{\"name\":\"trunk\"}}}}'\n",
                self.scratch.path().join("gh.log").display()
            ),
        );

        bin
    }

    fn gh_log(&self) -> String {
        fs::read_to_string(self.scratch.path().join("gh.log")).unwrap_or_default()
    }
}

fn real_git() -> PathBuf {
    let found = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("sh runs");

    PathBuf::from(String::from_utf8_lossy(&found.stdout).trim())
}

fn path_with(first: &Path) -> String {
    format!(
        "{}:{}",
        first.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn node(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn bash_event(event: &str, cwd: &Path, command: &str) -> Value {
    json!({
        "session_id": "s1",
        "hook_event_name": event,
        "cwd": cwd,
        "tool_name": "Bash",
        "tool_input": { "command": command },
    })
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn specific(output: &Child) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    if output.stdout.trim().is_empty() {
        return Value::Null;
    }

    json_line(&output.stdout)["hookSpecificOutput"].clone()
}

fn context(output: &Child) -> String {
    specific(output)["additionalContext"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn denial(output: &Child) -> Option<String> {
    let specific = specific(output);

    (specific["permissionDecision"] == json!("deny")).then(|| {
        specific["permissionDecisionReason"]
            .as_str()
            .expect("the reason is a string")
            .to_owned()
    })
}

fn assert_silent(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "", "stdout stays empty");
}

fn assert_warns_without_decision(output: &Child, code: &str) {
    let specific = specific(output);

    assert!(specific.get("permissionDecision").is_none(), "{specific}");
    assert!(
        context(output).contains(code),
        "context: {}",
        context(output)
    );
}

fn assert_released_with_error_line(output: &Child) {
    assert_eq!(output.code, Some(0), "stdout: {}", output.stdout);
    assert_eq!(output.stdout, "", "stdout stays empty");
    assert_eq!(
        output.stderr.matches('\n').count(),
        1,
        "stderr: {}",
        output.stderr
    );
}

fn gate_missing(output: &Child) -> Vec<String> {
    json_line(&output.stderr)["error"]["details"][0]["context"]["missing"]
        .as_array()
        .expect("the refusal lists what is missing")
        .iter()
        .map(|atom| atom.as_str().expect("an atom").to_owned())
        .collect()
}

fn gate_code(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

fn gate_reason(output: &Child) -> Value {
    json_line(&output.stdout)["reason"].clone()
}

fn system_message(output: &Child) -> String {
    json_line(&output.stdout)["systemMessage"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn exceptions_workflow(gate: bool) -> String {
    let mut text = String::from("version = 1\n");
    if gate {
        text.push_str("\n[creation_gate]\nenabled = true\n");
    }
    text.push_str("\n[branch]\n");
    for (pattern, mode) in [
        ("develop", "dry-run-with-mark"),
        ("staging", "dry-run"),
        ("sandbox", "skip"),
    ] {
        text.push_str(&format!(
            "\n[[branch.exception]]\npattern = \"{pattern}\"\nmode = \"{mode}\"\n"
        ));
    }

    text
}

// CA-010: a marca de nascimento pelo `branch-birth`

#[test]
fn it_should_mark_the_spec_of_the_created_work_item_branch() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);

    let output = edge.birth(&format!("git switch -c {W1_BRANCH}"));

    assert_silent(&output);
    assert!(edge.frozen(W1_SPEC));
}

#[test]
fn it_should_mark_only_the_spec_of_the_created_branch() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);
    edge.seed_spec(W2_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);

    let output = edge.birth(&format!("git switch -c {W1_BRANCH}"));

    assert_silent(&output);
    assert!(edge.frozen(W1_SPEC));
    assert!(!edge.frozen(W2_SPEC));
}

#[test]
fn it_should_not_rewrite_the_mark_on_a_second_call() {
    let edge = Edge::new();
    let spec = edge.seed_spec(W1_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);
    edge.birth(&format!("git switch -c {W1_BRANCH}"));
    let first = fs::metadata(spec.join(".frozen"))
        .and_then(|meta| meta.modified())
        .expect("the mark exists");

    let output = edge.birth(&format!("git switch -c {W1_BRANCH}"));

    assert_silent(&output);
    let second = fs::metadata(spec.join(".frozen"))
        .and_then(|meta| meta.modified())
        .expect("the mark stays");
    assert_eq!(first, second);
}

#[test]
fn it_should_mark_from_the_branch_list_when_the_name_has_a_substitution() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);

    edge.birth("git switch -c \"$NAME\"");

    assert!(edge.frozen(W1_SPEC));
}

#[test]
fn it_should_do_nothing_for_a_command_that_is_not_a_creation() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);
    edge.git(&["branch", W1_BRANCH]);

    let output = edge.birth("git status");

    assert_silent(&output);
    assert!(!edge.frozen(W1_SPEC));
}

#[test]
fn it_should_exit_zero_in_silence_for_branch_birth_without_a_marker() {
    let edge = Edge::unmarked();
    edge.seed_spec(W1_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);

    let output = edge.birth(&format!("git switch -c {W1_BRANCH}"));

    assert_silent(&output);
    assert_eq!(output.stderr, "");
    assert!(!edge.frozen(W1_SPEC));
}

#[test]
fn it_should_exit_zero_with_one_error_line_when_branch_birth_cannot_mark() {
    let edge = Edge::new();
    let spec = edge.seed_spec(W1_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);
    fs::set_permissions(&spec, fs::Permissions::from_mode(0o500)).expect("the folder is locked");

    let output = edge.birth(&format!("git switch -c {W1_BRANCH}"));

    fs::set_permissions(&spec, fs::Permissions::from_mode(0o755)).expect("the folder is unlocked");
    assert_released_with_error_line(&output);
}

#[test]
fn it_should_not_mark_a_branch_that_does_not_exist_yet() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);

    let output = edge.birth(&format!("git switch -c {W1_BRANCH}"));

    assert_silent(&output);
    assert!(!edge.frozen(W1_SPEC));
}

// CA-011: a rede do `session-start` e a classificadora

#[test]
fn it_should_mark_at_session_start_a_branch_created_outside_the_plugin_without_declarations() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);
    edge.git(&["switch", "-q", "-c", W1_BRANCH]);

    let output = edge.session_start();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(edge.frozen(W1_SPEC));
}

#[test]
fn it_should_leave_unmarked_a_branch_created_and_deleted_without_an_event() {
    let edge = Edge::new();
    edge.seed_spec(W1_SPEC);
    edge.git(&["branch", W1_BRANCH]);
    edge.git(&["branch", "-D", W1_BRANCH]);

    edge.session_start();

    assert!(!edge.frozen(W1_SPEC));
}

#[test]
fn it_should_mark_a_branch_of_the_declared_format_through_the_classifier() {
    let edge = Edge::new();
    edge.workflow("version = 1\n\n[branch]\nformat = \"work/<id>/<type>/<slug>\"\n");
    edge.seed_spec("wi-w1__login");
    edge.git(&["switch", "-q", "-c", "work/w1/fix/a-b"]);

    edge.birth("git switch -c work/w1/fix/a-b");

    assert!(edge.frozen("wi-w1__login"));
}

#[test]
fn it_should_mark_with_the_stable_policy_when_the_main_workflow_is_unreadable() {
    let edge = Edge::new();
    edge.workflow("version = 1\n\n[branch]\nformat = \"work/<id>/<type>/<slug>\"\n");
    edge.seed_spec("wi-w1__login");
    edge.git(&["switch", "-q", "-c", "work/w1/fix/a-b"]);
    edge.break_workflow();

    edge.birth("git switch -c work/w1/fix/a-b");

    assert!(edge.frozen("wi-w1__login"));
}

// CA-004: a linha de ignorar da pasta das worktrees

#[test]
fn it_should_write_the_worktrees_line_in_the_common_exclude_file() {
    let edge = Edge::new();
    edge.linked_under_worktrees();

    edge.birth("git switch -c y");

    assert!(
        edge.exclude_text().ends_with(EXCLUDE_LINE),
        "{}",
        edge.exclude_text()
    );
}

#[test]
fn it_should_hide_the_worktrees_folder_from_git_status_after_the_pass() {
    let edge = Edge::new();
    edge.linked_under_worktrees();

    edge.birth("git switch -c y");

    assert_eq!(edge.git(&["status", "--porcelain"]), "");
}

#[test]
fn it_should_not_repeat_the_worktrees_line_on_a_second_pass() {
    let edge = Edge::new();
    edge.linked_under_worktrees();
    edge.birth("git switch -c y");

    edge.birth("git switch -c z");

    assert_eq!(edge.exclude_text().matches(EXCLUDE_LINE).count(), 1);
}

#[test]
fn it_should_not_touch_the_exclude_file_when_a_gitignore_already_ignores_the_folder() {
    let edge = Edge::new();
    fs::write(edge.main.join(".gitignore"), ".worktrees/\n").expect("gitignore is written");
    edge.git(&["add", "-A"]);
    edge.git(&["commit", "-q", "-m", "ignore"]);
    edge.linked_under_worktrees();
    let before = edge.exclude_text();

    let output = edge.birth("git switch -c y");

    assert_silent(&output);
    assert_eq!(edge.exclude_text(), before);
}

#[test]
fn it_should_not_touch_the_exclude_file_when_the_global_excludes_ignore_the_folder() {
    let edge = Edge::new();
    let global = edge.scratch.path().join("global-ignore");
    fs::write(&global, ".worktrees/\n").expect("global excludes are written");
    let config = edge.scratch.path().join("gitconfig");
    fs::write(
        &config,
        format!("[core]\n\texcludesFile = {}\n", global.display()),
    )
    .expect("global config is written");
    edge.linked_under_worktrees();
    let before = edge.exclude_text();

    let output = edge
        .hook_cmd(
            "branch-birth",
            &bash_event("PostToolUse", &edge.main, "git switch -c y"),
        )
        .env("GIT_CONFIG_GLOBAL", &config)
        .run();

    assert_silent(&output);
    assert_eq!(edge.exclude_text(), before);
}

#[test]
fn it_should_ask_check_ignore_with_a_trailing_slash_on_the_folder() {
    let edge = Edge::new();
    edge.linked_under_worktrees();
    let bin = edge.logging_git(None);

    edge.hook_cmd(
        "branch-birth",
        &bash_event("PostToolUse", &edge.main, "git switch -c y"),
    )
    .env("PATH", path_with(&bin))
    .run();

    assert!(
        edge.git_log()
            .lines()
            .any(|line| line.ends_with("check-ignore -q -- .worktrees/")),
        "git argv: {}",
        edge.git_log()
    );
}

#[test]
fn it_should_create_the_info_folder_when_it_is_missing() {
    let edge = Edge::new();
    edge.linked_under_worktrees();
    fs::remove_dir_all(edge.main.join(".git/info")).expect("info is removed");

    edge.birth("git switch -c y");

    assert_eq!(edge.exclude_text(), EXCLUDE_LINE);
}

#[test]
fn it_should_put_the_line_on_its_own_line_when_the_exclude_file_lacks_a_final_newline() {
    let edge = Edge::new();
    edge.linked_under_worktrees();
    fs::write(edge.exclude_path(), "/build").expect("exclude is written");

    edge.birth("git switch -c y");

    assert_eq!(edge.exclude_text(), format!("/build\n{EXCLUDE_LINE}"));
}

#[test]
fn it_should_write_nothing_and_report_to_stderr_when_check_ignore_fails() {
    let edge = Edge::new();
    edge.linked_under_worktrees();
    let before = edge.exclude_text();
    let bin = edge.logging_git(Some(128));

    let output = edge
        .hook_cmd(
            "branch-birth",
            &bash_event("PostToolUse", &edge.main, "git switch -c y"),
        )
        .env("PATH", path_with(&bin))
        .run();

    assert_eq!(output.code, Some(0));
    assert!(!output.stderr.is_empty(), "the failure goes to stderr");
    assert_eq!(edge.exclude_text(), before);
}

#[test]
fn it_should_write_nothing_for_a_worktree_dir_outside_the_main_directory() {
    let edge = Edge::new();
    let outside = edge.scratch.path().join("wt");
    edge.workflow(&format!(
        "version = 1\n\n[worktree]\ndir = \"{}\"\n",
        outside.display()
    ));
    git_ok(
        &edge.main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            LINKED,
            &outside.join("x").to_string_lossy(),
        ],
    );
    let before = edge.exclude_text();

    let output = edge.birth("git switch -c y");

    assert_silent(&output);
    assert_eq!(edge.exclude_text(), before);
}

#[test]
fn it_should_change_no_tracked_file_in_the_exclude_pass() {
    let edge = Edge::new();
    edge.linked_under_worktrees();

    edge.birth("git switch -c y");

    assert!(edge.exclude_text().contains(EXCLUDE_LINE), "the pass ran");
    assert_eq!(edge.git(&["diff", "--name-only", "HEAD"]), "");
}

// CA-012: o aviso dos hooks do git no `session-start`

fn assert_hooks_warning(output: &Child, code: &str) {
    let context = context(output);
    let message = system_message(output);

    assert!(message.contains(code), "systemMessage: {message}");
    assert!(message.contains(INSTALL), "systemMessage: {message}");
    assert!(
        context.lines().any(|line| line == message),
        "context: {context}"
    );
}

fn assert_no_hooks_warning(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    for code in [
        "git-hooks-missing",
        "git-hooks-outdated",
        "hooks-path-conflict",
        "hooks-existing",
    ] {
        assert!(!output.stdout.contains(code), "{code}: {}", output.stdout);
    }
}

#[test]
fn it_should_say_git_hooks_missing_with_the_install_command_on_a_clone_without_hooks() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);

    let output = edge.session_start();

    assert_hooks_warning(&output, "git-hooks-missing");
}

#[test]
fn it_should_say_that_installing_is_the_action_of_a_person() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);

    let output = edge.session_start();

    let message = system_message(&output).to_lowercase();
    assert!(message.contains("person"), "systemMessage: {message}");
}

#[test]
fn it_should_say_nothing_about_git_hooks_with_the_five_scripts_installed() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    edge.install_hooks();

    let output = edge.session_start();

    assert_no_hooks_warning(&output);
}

#[test]
fn it_should_say_git_hooks_outdated_when_the_post_commit_script_is_missing() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    edge.install_hooks();
    fs::remove_file(edge.hooks_dir().join("post-commit")).expect("post-commit is removed");

    let output = edge.session_start();

    assert_hooks_warning(&output, "git-hooks-outdated");
}

#[test]
fn it_should_say_git_hooks_outdated_with_only_the_four_scripts_of_today() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    edge.install_hooks();
    fs::remove_file(edge.hooks_dir().join("post-merge")).expect("post-merge is removed");

    let output = edge.session_start();

    assert_hooks_warning(&output, "git-hooks-outdated");
}

#[test]
fn it_should_say_git_hooks_outdated_when_a_script_has_other_content() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    edge.install_hooks();
    write_executable(&edge.hooks_dir().join("pre-push"), "#!/bin/sh\nexit 0\n");

    let output = edge.session_start();

    assert_hooks_warning(&output, "git-hooks-outdated");
}

#[test]
fn it_should_say_git_hooks_outdated_when_the_scripts_call_another_binary() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    edge.install_hooks();
    let own = fs::canonicalize(env!("CARGO_BIN_EXE_omhl")).expect("the binary exists");
    for name in [
        "pre-commit",
        "pre-merge-commit",
        "pre-push",
        "post-commit",
        "post-merge",
    ] {
        let path = edge.hooks_dir().join(name);
        let text = fs::read_to_string(&path).expect("the script is readable");
        write_executable(
            &path,
            &text.replace(&own.display().to_string(), "/opt/old/omhl"),
        );
    }

    let output = edge.session_start();

    assert_hooks_warning(&output, "git-hooks-outdated");
}

#[test]
fn it_should_say_hooks_path_conflict_with_the_force_hint_for_a_foreign_hooks_path() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    let other = edge.scratch.path().join("other-hooks");
    edge.git(&[
        "config",
        "--local",
        "core.hooksPath",
        &other.to_string_lossy(),
    ]);

    let output = edge.session_start();

    assert_hooks_warning(&output, "hooks-path-conflict");
    assert!(system_message(&output).contains("--force"));
}

#[test]
fn it_should_say_hooks_existing_with_the_force_hint_for_foreign_scripts_in_the_default_folder() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    write_executable(
        &edge.main.join(".git/hooks/pre-commit"),
        "#!/bin/sh\nexit 0\n",
    );

    let output = edge.session_start();

    assert_hooks_warning(&output, "hooks-existing");
    assert!(system_message(&output).contains("--force"));
}

#[test]
fn it_should_say_nothing_about_git_hooks_without_the_git_table() {
    let edge = Edge::new();

    let output = edge.session_start();

    assert_no_hooks_warning(&output);
}

#[test]
fn it_should_say_nothing_about_git_hooks_without_a_marker() {
    let edge = Edge::unmarked();
    edge.workflow(GIT_HOOKS_ON);

    let output = edge.session_start();

    assert_no_hooks_warning(&output);
}

#[test]
fn it_should_say_nothing_about_git_hooks_with_an_unreadable_workflow() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);
    edge.break_workflow();

    let output = edge.session_start();

    assert_no_hooks_warning(&output);
}

#[test]
fn it_should_write_no_config_and_no_script_in_the_git_hooks_comparison() {
    let edge = Edge::new();
    edge.workflow(GIT_HOOKS_ON);

    edge.session_start();

    assert_eq!(
        git_in(
            &edge.main,
            &["config", "--local", "--get", "core.hooksPath"]
        )
        .status
        .code(),
        Some(1)
    );
    assert!(!edge.hooks_dir().exists(), "no hooks folder is created");
}

// CA-002: as declarações vêm do diretório principal

#[test]
fn it_should_ignore_a_git_table_declared_only_in_the_linked_worktree() {
    let edge = Edge::new();
    edge.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        W1_BRANCH,
        &edge.scratch.path().join("l").to_string_lossy(),
    ]);
    let linked = fs::canonicalize(edge.scratch.path().join("l")).expect("the worktree exists");
    fs::create_dir_all(linked.join(".omhl")).expect("the folder is created");
    fs::write(linked.join(".omhl/workflow.toml"), GIT_HOOKS_ON).expect("linked workflow");

    let output = edge.session_start_from(&linked);

    assert_no_hooks_warning(&output);
}

#[test]
fn it_should_ignore_a_provider_declared_only_in_the_linked_worktree_in_the_push_warning() {
    let edge = Edge::new();
    edge.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        W1_BRANCH,
        &edge.scratch.path().join("l").to_string_lossy(),
    ]);
    let linked = fs::canonicalize(edge.scratch.path().join("l")).expect("the worktree exists");
    fs::create_dir_all(linked.join(".omhl")).expect("the folder is created");
    fs::write(linked.join(".omhl/workflow.toml"), PROVIDER_ON).expect("linked workflow");
    let gh = edge.fake_gh();

    let output = edge
        .hook_cmd(
            "command-guard",
            &bash_event("PreToolUse", &linked, "git push origin trunk"),
        )
        .env("PATH", path_with(&gh))
        .run();

    assert_silent(&output);
    assert_eq!(edge.gh_log(), "", "gh is never started");
}

#[test]
fn it_should_ignore_a_creation_gate_declared_only_in_the_linked_worktree() {
    let edge = Edge::new();
    edge.work_item(None);
    edge.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "chore/wi-w9__z",
        &edge.scratch.path().join("l").to_string_lossy(),
    ]);
    let linked = fs::canonicalize(edge.scratch.path().join("l")).expect("the worktree exists");
    fs::create_dir_all(linked.join(".omhl")).expect("the folder is created");
    fs::write(linked.join(".omhl/workflow.toml"), GATE_ON).expect("linked workflow");

    let output = edge.guard_from(&linked, &format!("git switch -c {W1_BRANCH}"));

    assert_eq!(denial(&output), None);
}

// CA-013: o portão de criação ligado

#[test]
fn it_should_deny_a_creation_without_the_premise_the_type_requires() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);

    let output = edge.guard(&format!("git switch -c {W1_BRANCH}"));

    let reason = denial(&output).expect("the creation is denied");
    assert!(
        reason.starts_with("worktree-gate-refused"),
        "reason: {reason}"
    );
}

#[test]
fn it_should_list_in_the_denial_the_same_missing_items_as_gate_worktree() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);
    let missing = gate_missing(&edge.gate_worktree(W1_BRANCH));

    let output = edge.guard(&format!("git switch -c {W1_BRANCH}"));

    let reason = denial(&output).expect("the creation is denied");
    assert!(!missing.is_empty());
    assert!(
        missing.iter().all(|atom| reason.contains(atom)),
        "{missing:?} in {reason}"
    );
}

#[test]
fn it_should_let_a_creation_pass_when_the_requirements_are_met() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None).technical_premise();

    let output = edge.guard("git switch -c refactor/wi-w1__x");

    assert_eq!(denial(&output), None);
}

#[test]
fn it_should_let_main_pass_as_an_exempt_branch() {
    let edge = Edge::new();
    edge.workflow(GATE_ON);

    let output = edge.guard("git branch main origin/main");

    assert_eq!(denial(&output), None);
}

#[test]
fn it_should_let_an_exception_of_each_mode_pass() {
    let edge = Edge::new();
    edge.workflow(&exceptions_workflow(true));

    let denied: Vec<&str> = ["develop", "staging", "sandbox"]
        .into_iter()
        .filter(|name| denial(&edge.guard(&format!("git switch -c {name}"))).is_some())
        .collect();

    assert_eq!(denied, Vec::<&str>::new());
}

#[test]
fn it_should_use_the_row_of_the_recorded_type_over_the_branch_prefix() {
    let edge = Edge::new();
    edge.workflow(GATE_ON)
        .work_item(Some("refactor"))
        .technical_premise();

    let output = edge.guard(&format!("git switch -c {W1_BRANCH}"));

    assert_eq!(denial(&output), None);
}

#[test]
fn it_should_use_the_prefix_of_the_created_branch_for_a_work_item_without_type() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None).technical_premise();

    let output = edge.guard(&format!("git switch -c {W1_BRANCH}"));

    assert!(denial(&output).is_some(), "feat needs a business premise");
}

#[test]
fn it_should_deny_a_branch_outside_any_format_with_prefix_not_allowed() {
    let edge = Edge::new();
    edge.workflow(GATE_ON);

    let output = edge.guard("git switch -c random-name");

    let reason = denial(&output).expect("the creation is denied");
    assert!(reason.contains("prefix-not-allowed"), "reason: {reason}");
}

#[test]
fn it_should_treat_worktree_add_b_like_switch_c() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);

    let output = edge.guard(&format!("git worktree add -b {W1_BRANCH} ../x"));

    let reason = denial(&output).expect("the creation is denied");
    assert!(
        reason.starts_with("worktree-gate-refused"),
        "reason: {reason}"
    );
}

#[test]
fn it_should_treat_checkout_b_like_switch_c() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);

    let output = edge.guard(&format!("git checkout -b {W1_BRANCH}"));

    let reason = denial(&output).expect("the creation is denied");
    assert!(
        reason.starts_with("worktree-gate-refused"),
        "reason: {reason}"
    );
}

#[test]
fn it_should_not_call_the_gate_for_gh_pr_checkout_or_a_remote_checkout() {
    let edge = Edge::new();
    edge.workflow(GATE_ON);

    let outputs = [
        edge.guard("gh pr checkout 3"),
        edge.guard("git checkout origin/x"),
    ];

    for output in &outputs {
        assert_silent(output);
    }
}

#[test]
fn it_should_go_through_the_type_row_with_a_format_without_type_in_both_callers() {
    let edge = Edge::new();
    edge.workflow("version = 1\n\n[creation_gate]\nenabled = true\n\n[branch]\nformat = \"work/<id>/<slug>\"\n")
        .work_item(Some("refactor"))
        .technical_premise();

    let guard = edge.guard("git switch -c work/w1/x");
    let gate = edge.gate_worktree("work/w1/x");

    assert_eq!(denial(&guard), None);
    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);
}

#[test]
fn it_should_refuse_a_work_item_without_type_under_a_format_without_type_in_both_callers() {
    let edge = Edge::new();
    edge.workflow("version = 1\n\n[creation_gate]\nenabled = true\n\n[branch]\nformat = \"work/<id>/<slug>\"\n")
        .work_item(None);

    let guard = edge.guard("git switch -c work/w1/x");
    let gate = edge.gate_worktree("work/w1/x");

    let reason = denial(&guard).expect("the creation is denied");
    assert!(
        reason.contains("work-item-type-unknown"),
        "reason: {reason}"
    );
    assert_eq!(gate_code(&gate), json!("work-item-type-unknown"));
}

// CA-014: o portão em falha e fora do campo

#[test]
fn it_should_let_the_creation_pass_with_a_warning_when_the_database_cannot_be_opened() {
    let edge = Edge::new();
    edge.workflow(GATE_ON);
    let blocked = edge.scratch.path().join("not-a-folder");
    fs::write(&blocked, "x").expect("a file stands where the data folder would be");

    let output = edge
        .hook_cmd(
            "command-guard",
            &bash_event(
                "PreToolUse",
                &edge.main,
                &format!("git switch -c {W1_BRANCH}"),
            ),
        )
        .xdg(&blocked)
        .run();

    assert_eq!(denial(&output), None);
    assert!(
        context(&output).contains(GATE_UNAVAILABLE),
        "context: {}",
        context(&output)
    );
}

#[test]
fn it_should_let_the_creation_pass_with_a_warning_when_the_workflow_is_unreadable() {
    let edge = Edge::new();
    edge.workflow(GATE_ON);
    edge.break_workflow();

    let output = edge.guard(&format!("git switch -c {W1_BRANCH}"));

    assert_eq!(denial(&output), None);
    assert!(
        context(&output).contains(GATE_UNAVAILABLE),
        "context: {}",
        context(&output)
    );
}

#[test]
fn it_should_let_the_creation_pass_with_a_warning_when_git_fails() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);
    let bin = edge.scratch.path().join("broken-git");
    fs::create_dir_all(&bin).expect("bin folder is created");
    write_executable(&bin.join("git"), "#!/bin/sh\nexit 1\n");

    let output = edge
        .hook_cmd(
            "command-guard",
            &bash_event(
                "PreToolUse",
                &edge.main,
                &format!("git switch -c {W1_BRANCH}"),
            ),
        )
        .env("PATH", path_with(&bin))
        .run();

    assert_eq!(denial(&output), None);
    assert!(
        context(&output).contains(GATE_UNAVAILABLE),
        "context: {}",
        context(&output)
    );
}

#[test]
fn it_should_not_open_the_database_for_a_creation_with_the_gate_off() {
    for workflow in [
        "version = 1\n",
        "version = 1\n\n[creation_gate]\nenabled = false\n",
    ] {
        let edge = Edge::new();
        edge.workflow(workflow);
        let blocked = edge.scratch.path().join("not-a-folder");
        fs::write(&blocked, "x").expect("a file stands where the data folder would be");

        let output = edge
            .hook_cmd(
                "command-guard",
                &bash_event(
                    "PreToolUse",
                    &edge.main,
                    &format!("git switch -c {W1_BRANCH}"),
                ),
            )
            .xdg(&blocked)
            .run();

        assert_silent(&output);
    }
}

#[test]
fn it_should_not_decide_a_creation_whose_name_has_a_substitution() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);

    let output = edge.guard("git switch -c \"$NAME\"");

    assert_eq!(denial(&output), None);
}

#[test]
fn it_should_deny_through_command_guard_while_agent_role_lets_the_same_call_pass() {
    let edge = Edge::new();
    edge.workflow(GATE_ON).work_item(None);
    let event = bash_event(
        "PreToolUse",
        &edge.main,
        &format!("git switch -c {W1_BRANCH}"),
    );

    let guard = edge.hook("command-guard", &event);
    let role = edge.hook("agent-role", &event);

    assert!(denial(&guard).is_some());
    assert_eq!(denial(&role), None);
}

// CA-015: o mesmo `exempt-branch` no `gate-worktree` e no `command-guard`

#[test]
fn it_should_give_exempt_branch_in_gate_worktree_for_main_and_each_declared_exception() {
    let edge = Edge::new();
    edge.workflow(&exceptions_workflow(true));

    let reasons: Vec<Value> = ["main", "develop", "staging", "sandbox"]
        .into_iter()
        .map(|name| gate_reason(&edge.gate_worktree(name)))
        .collect();

    assert_eq!(reasons, vec![json!("exempt-branch"); 4]);
}

#[test]
fn it_should_let_main_and_each_declared_exception_pass_the_command_guard() {
    let edge = Edge::new();
    edge.workflow(&exceptions_workflow(true));

    let denied: Vec<&str> = ["main", "develop", "staging", "sandbox"]
        .into_iter()
        .filter(|name| denial(&edge.guard(&format!("git branch {name}"))).is_some())
        .collect();

    assert_eq!(denied, Vec::<&str>::new());
}

#[test]
fn it_should_refuse_an_undeclared_develop_with_prefix_not_allowed_in_both_callers() {
    let edge = Edge::new();
    edge.workflow(GATE_ON);

    let guard = edge.guard("git switch -c develop");
    let gate = edge.gate_worktree("develop");

    let reason = denial(&guard).expect("the creation is denied");
    assert!(reason.contains("prefix-not-allowed"), "reason: {reason}");
    assert_eq!(gate_code(&gate), json!("prefix-not-allowed"));
}

// CA-016: o aviso de push para `main`

#[test]
fn it_should_warn_without_decision_for_each_push_that_hits_main_in_a_project_without_marker() {
    let edge = Edge::unmarked();

    for command in [
        "git push origin main",
        "git push origin HEAD:main",
        "git push origin feat:main",
        "git push origin feat:refs/heads/main",
        "git push -f origin +feat:main",
        "git push origin :main",
        "git push origin --delete main",
        "git push --all",
        "git push --mirror",
    ] {
        let output = edge.guard(command);

        assert_warns_without_decision(&output, PUSH_WARNING);
    }
}

#[test]
fn it_should_say_that_main_takes_the_delivery_by_pull_request() {
    let edge = Edge::unmarked();

    let output = edge.guard("git push origin main");

    assert!(
        context(&output).contains("pull request"),
        "context: {}",
        context(&output)
    );
}

#[test]
fn it_should_not_warn_for_a_push_of_another_branch() {
    let edge = Edge::unmarked();

    let output = edge.guard("git push origin feat/x");

    assert_silent(&output);
}

#[test]
fn it_should_not_warn_for_a_dry_run_push_of_main() {
    let edge = Edge::unmarked();

    let output = edge.guard("git push --dry-run origin main");

    assert_silent(&output);
}

#[test]
fn it_should_warn_for_a_push_without_refspec_on_main() {
    let edge = Edge::unmarked();

    let output = edge.guard("git push");

    assert_warns_without_decision(&output, PUSH_WARNING);
}

#[test]
fn it_should_not_warn_for_a_push_without_refspec_on_another_branch() {
    let edge = Edge::unmarked();
    edge.git(&["switch", "-q", "-c", "feat/x"]);

    let output = edge.guard("git push");

    assert_silent(&output);
}

#[test]
fn it_should_not_start_gh_with_the_integration_off() {
    let edge = Edge::new();
    let gh = edge.fake_gh();

    let output = edge
        .hook_cmd(
            "command-guard",
            &bash_event("PreToolUse", &edge.main, "git push origin trunk"),
        )
        .env("PATH", path_with(&gh))
        .run();

    assert_silent(&output);
    assert_eq!(edge.gh_log(), "", "gh is never started");
}

#[test]
fn it_should_read_the_default_branch_with_the_fake_gh_on_the_path() {
    let edge = Edge::new();
    edge.workflow(PROVIDER_ON);
    edge.git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/acme/widgets.git",
    ]);
    let gh = edge.fake_gh();

    let output = edge
        .hook_cmd(
            "command-guard",
            &bash_event("PreToolUse", &edge.main, "git push origin trunk"),
        )
        .env("PATH", path_with(&gh))
        .run();

    assert_eq!(
        edge.gh_log(),
        "repo view github.com/acme/widgets --json defaultBranchRef\n"
    );
    assert_warns_without_decision(&output, PUSH_WARNING);
}

// CA-018: o `mcp-write-warning`, uma chamada por ferramenta

fn mcp_write(cwd: &Path, tool: &str, branch: &str) -> Value {
    json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": cwd,
        "tool_name": format!("mcp__github-official__{tool}"),
        "tool_input": { "owner": "acme", "repo": "widgets", "branch": branch },
    })
}

#[test]
fn it_should_warn_without_decision_for_each_write_tool_aimed_at_main() {
    let edge = Edge::unmarked();

    for tool in ["push_files", "create_or_update_file", "delete_file"] {
        let output = edge.hook("mcp-write-warning", &mcp_write(&edge.main, tool, "main"));

        assert_warns_without_decision(&output, PUSH_WARNING);
    }
}

#[test]
fn it_should_not_warn_for_each_write_tool_aimed_at_another_branch() {
    let edge = Edge::unmarked();

    for tool in ["push_files", "create_or_update_file", "delete_file"] {
        let output = edge.hook("mcp-write-warning", &mcp_write(&edge.main, tool, "feat/x"));

        assert_silent(&output);
    }
}

// EMD-036: a ajuda lista o quinto script

#[test]
fn it_should_name_post_merge_in_the_help_of_the_hooks_group() {
    let output = omhl(&["hooks", "--help"]).run();

    assert!(
        output.stdout.contains("post-merge"),
        "help: {}",
        output.stdout
    );
}

#[test]
fn it_should_name_post_merge_in_the_help_of_hooks_install() {
    let output = omhl(&["hooks", "install", "--help"]).run();

    assert!(
        output.stdout.contains("post-merge"),
        "help: {}",
        output.stdout
    );
}

// CA-039 e CA-040: a negação do merge direto pelo binário real (CMP-031, CMP-032)

const MERGE_POINTER: &str = "omhl merge <work-item> [--squash|--rebase]";

fn mcp_merge_event(cwd: &Path, tool_name: &str) -> Value {
    json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": cwd,
        "tool_name": tool_name,
        "tool_input": { "owner": "acme", "repo": "widgets", "pullNumber": 3 },
    })
}

impl Edge {
    fn mcp_merge(&self, tool_name: &str) -> Child {
        self.hook("mcp-merge-guard", &mcp_merge_event(&self.main, tool_name))
    }
}

#[test]
fn it_should_deny_the_github_mcp_merge_with_the_integration_on() {
    let edge = Edge::new();
    edge.workflow(PROVIDER_ON);

    let output = edge.mcp_merge("mcp__github-official__merge_pull_request");

    let reason = denial(&output).expect("the merge is denied");
    assert!(reason.contains(MERGE_POINTER), "{reason}");
}

#[test]
fn it_should_deny_the_merge_of_an_mcp_server_with_another_name() {
    let edge = Edge::new();
    edge.workflow(PROVIDER_ON);

    let output = edge.mcp_merge("mcp__plugin_x_github__merge_pull_request");

    assert!(denial(&output).is_some_and(|reason| reason.contains(MERGE_POINTER)));
}

#[test]
fn it_should_let_the_mcp_merge_pass_with_the_integration_off() {
    let edge = Edge::new();

    let output = edge.mcp_merge("mcp__github-official__merge_pull_request");

    assert_silent(&output);
}

#[test]
fn it_should_let_the_mcp_merge_pass_without_the_marker() {
    let edge = Edge::unmarked();

    let output = edge.mcp_merge("mcp__github-official__merge_pull_request");

    assert_silent(&output);
}

#[test]
fn it_should_fail_the_mcp_merge_guard_showing_the_error_of_an_unreadable_workflow() {
    let edge = Edge::new();
    edge.workflow(BROKEN);

    let output = edge.mcp_merge("mcp__github-official__merge_pull_request");

    assert_ne!(output.code, Some(0), "stdout: {}", output.stdout);
    assert!(
        output.stderr.contains("invalid-workflow-config"),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_decide_the_mcp_merge_without_opening_the_database() {
    let edge = Edge::new();
    edge.workflow(PROVIDER_ON);
    let not_a_folder = edge.scratch.path().join("data-is-a-file");
    fs::write(&not_a_folder, "x").expect("the file is written");

    let output = edge
        .hook_cmd(
            "mcp-merge-guard",
            &mcp_merge_event(&edge.main, "mcp__github-official__merge_pull_request"),
        )
        .env("XDG_DATA_HOME", &not_a_folder)
        .run();

    assert!(denial(&output).is_some_and(|reason| reason.contains(MERGE_POINTER)));
}

#[test]
fn it_should_deny_gh_pr_merge_in_bash_with_the_integration_on() {
    let edge = Edge::new();
    edge.workflow(PROVIDER_ON);

    let output = edge.guard("gh pr merge 3");

    assert!(denial(&output).is_some_and(|reason| reason.contains(MERGE_POINTER)));
}

#[test]
fn it_should_keep_the_bash_merge_denial_when_agent_role_also_runs() {
    let edge = Edge::new();
    edge.workflow(PROVIDER_ON);
    let mut event = bash_event("PreToolUse", &edge.main, "gh pr merge 3");
    event["agent_type"] = json!("oh-my-hexflow:verifier");

    let role = edge.hook("agent-role", &event);
    let guarded = edge.hook("command-guard", &event);

    assert_ne!(
        specific(&role)["permissionDecision"],
        json!("allow"),
        "agent-role does not allow what the guard denies"
    );
    assert!(denial(&guarded).is_some_and(|reason| reason.contains(MERGE_POINTER)));
}
