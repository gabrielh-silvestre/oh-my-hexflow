use crate::support;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Output as Child, omhl, omhl_json};
use support::git::git_in;
use support::project::Project;
use tempfile::TempDir;

const BRANCH: &str = "feat/wi-w1__login";
const FOLDER: &str = "wi-w1__login";
const DEFAULT_SPECS: &str = ".omhl/specs";
const CUSTOM_SPECS: &str = "docs/team-specs";
const CUSTOM_WORKFLOW: &str = "version = 1\n\n[specs]\ndir = \"docs/team-specs\"\n";
const INVALID_WORKFLOW: &str = "version = 1\n\n[specs]\ndirr = \"x\"\n";
/// Sem pasta própria (raiz do repositório), `.git` e as raízes de `.omhl/` que o scan de extensions lê.
const UNUSABLE_SPECS_DIRS: [&str; 9] = [
    "",
    ".",
    "./",
    ".git",
    ".omhl",
    ".omhl/checks",
    ".omhl/guidelines",
    ".omhl/projection",
    ".omhl/workflow.toml",
];
const FROZEN: &str = "spec-frozen";
const IN_WORKTREE: &str = "spec-in-linked-worktree";

// Hook `omhl hook spec-lock` sobre repositório git real

/// Repositório git real (branch `main`, marcador e `files` commitados) e a pasta de dados.
struct Repo {
    project: Project,
    scratch: TempDir,
    main: PathBuf,
}

impl Repo {
    fn new() -> Repo {
        Repo::with_files(&[])
    }

    fn with_files(files: &[(&str, &str)]) -> Repo {
        let project = Project::new("acme");
        for (rel, content) in files {
            let target = project.dir().join(rel);
            fs::create_dir_all(target.parent().expect("the file has a parent")).expect("parent");
            fs::write(target, content).expect("file is written");
        }
        for args in [
            &["init", "-b", "main"][..],
            &["config", "user.name", "Test Author"],
            &["config", "user.email", "author@example.com"],
            &["add", "-A"],
            &["commit", "-m", "add marker"],
        ] {
            let output = git_in(project.dir(), args);
            assert!(output.status.success(), "git {args:?} failed");
        }
        let main = fs::canonicalize(project.dir()).expect("the main directory exists");

        Repo {
            project,
            scratch: TempDir::new().expect("scratch directory is created"),
            main,
        }
    }

    /// Branch local sem worktree.
    fn branch(&self, name: &str) {
        let created = git_in(self.project.dir(), &["branch", name]);

        assert!(created.status.success(), "git branch {name} failed");
    }

    /// Worktree ligada na branch nova `branch`; devolve o caminho canônico dela.
    fn linked(&self, branch: &str) -> PathBuf {
        let linked = self.scratch.path().join("wt");
        let added = git_in(
            self.project.dir(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                branch,
                &linked.to_string_lossy(),
            ],
        );
        assert!(added.status.success(), "git worktree add failed");

        fs::canonicalize(linked).expect("the worktree exists")
    }

    fn folder(&self, specs_dir: &str) -> PathBuf {
        self.main.join(specs_dir).join(FOLDER)
    }

    /// Grava `name` na pasta de spec do diretório principal.
    fn seed(&self, specs_dir: &str, name: &str) {
        let folder = self.folder(specs_dir);
        fs::create_dir_all(&folder).expect("spec folder is created");
        fs::write(folder.join(name), "seed\n").expect("spec file is written");
    }

    fn spec_file(&self, name: &str) -> PathBuf {
        self.folder(DEFAULT_SPECS).join(name)
    }

    /// Arquivo numa pasta do mesmo work item que não é a da spec (`DECOY_FOLDER`).
    fn decoy_file(&self, name: &str) -> PathBuf {
        self.main.join(DEFAULT_SPECS).join(DECOY_FOLDER).join(name)
    }

    /// Roda o hook de uma pasta que não é a do projeto: o `cwd` só pode vir do stdin.
    fn hook(&self, cwd: &Path, tool: &str, file_path: &Path) -> Child {
        run_spec_lock(self.project.xdg(), &payload(cwd, tool, file_path))
    }

    fn hook_in_main(&self, tool: &str, file_path: &Path) -> Child {
        self.hook(&self.main, tool, file_path)
    }

    fn database_files(&self) -> usize {
        fs::read_dir(self.project.xdg().join("omhl")).map_or(0, Iterator::count)
    }
}

fn payload(cwd: &Path, tool: &str, file_path: &Path) -> String {
    let tool_input = match tool {
        "Edit" => json!({ "file_path": file_path, "old_string": "a", "new_string": "b" }),
        "MultiEdit" => json!({
            "file_path": file_path,
            "edits": [{ "old_string": "a", "new_string": "b" }],
        }),
        _ => json!({ "file_path": file_path, "content": "x" }),
    };

    payload_of(cwd, tool, tool_input)
}

fn payload_of(cwd: &Path, tool: &str, tool_input: Value) -> String {
    let mut event = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": cwd,
        "tool_name": tool,
    });
    event["tool_input"] = tool_input;

    event.to_string()
}

fn run_spec_lock(xdg: &Path, stdin: &str) -> Child {
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["hook", "spec-lock"])
        .cwd(elsewhere.path())
        .xdg(xdg)
        .stdin(stdin)
        .run()
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn assert_allowed(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
    assert!(output.stderr.is_empty(), "stderr: {}", output.stderr);
}

/// Confere a forma da negação e devolve o `permissionDecisionReason`.
fn denied_reason(output: &Child) -> String {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stderr.is_empty(), "stderr: {}", output.stderr);
    assert_eq!(output.stdout.matches('\n').count(), 1, "one line");
    let value = json_line(&output.stdout);
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], json!("PreToolUse"));
    assert_eq!(specific["permissionDecision"], json!("deny"));

    specific["permissionDecisionReason"]
        .as_str()
        .expect("the reason is a string")
        .to_owned()
}

fn assert_released_with_error(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
    assert!(
        json_line(&output.stderr)["error"]["code"].is_string(),
        "stderr: {}",
        output.stderr
    );
}

// Sem branch do work item: nada trava

#[test]
fn it_should_allow_write_over_an_existing_part_when_the_work_item_has_no_branch() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_edit_on_an_existing_part_when_the_work_item_has_no_branch() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_a_new_part_when_the_work_item_has_no_branch() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-02.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_the_spec_of_a_work_item_whose_id_only_shares_a_prefix_with_a_branch() {
    let repo = Repo::new();
    repo.branch("feat/wi-w10__other");
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_the_spec_when_only_the_slug_of_another_branch_matches() {
    let repo = Repo::new();
    repo.branch("feat/wi-w2__login");
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert_allowed(&output);
}

// Com a branch do work item: a spec congela

#[test]
fn it_should_deny_edit_on_an_existing_part_when_the_work_item_has_a_branch() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_edit_through_a_path_that_climbs_out_and_back_into_the_specs_dir() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let escaping = repo
        .main
        .join(DEFAULT_SPECS)
        .join("../specs")
        .join(FOLDER)
        .join("parte-01.md");

    let output = repo.hook_in_main("Edit", &escaping);

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_multi_edit_on_an_existing_part_when_the_work_item_has_a_branch() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("MultiEdit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_write_over_an_existing_part_when_the_work_item_has_a_branch() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_new_part_when_the_work_item_has_a_branch() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-02.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_freeze_the_spec_for_a_branch_of_another_type_and_slug_with_the_same_id() {
    let repo = Repo::new();
    repo.branch("fix/wi-w1__other-slug");
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_name_the_main_folder_and_the_next_amendment_in_the_denial() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    let folder = repo.folder(DEFAULT_SPECS);
    assert!(reason.contains(&folder.to_string_lossy().into_owned()));
    assert!(
        reason.contains(&folder.join("emenda-01.md").to_string_lossy().into_owned()),
        "reason: {reason}"
    );
}

#[test]
fn it_should_deny_from_a_linked_worktree_session_a_spec_in_the_main_directory() {
    let repo = Repo::new();
    let linked = repo.linked(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook(&linked, "Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_not_create_the_database_when_denying() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    denied_reason(&output);
    assert_eq!(
        repo.database_files(),
        0,
        "the hook never opens the database"
    );
}

// Emendas

#[test]
fn it_should_allow_the_first_new_amendment() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-01.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_the_first_new_amendment_from_a_linked_worktree_session() {
    let repo = Repo::new();
    let linked = repo.linked(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook(&linked, "Write", &repo.spec_file("emenda-01.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_the_amendment_after_the_only_existing_one() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-02.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_the_amendment_after_the_highest_existing_one_even_with_a_gap() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-03.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-04.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_deny_an_amendment_that_skips_a_number() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-03.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_name_the_expected_amendment_when_denying_one_that_skips_a_number() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-03.md"));

    let reason = denied_reason(&output);
    let expected = repo.spec_file("emenda-02.md");
    assert!(
        reason.contains(&expected.to_string_lossy().into_owned()),
        "reason: {reason}"
    );
}

#[test]
fn it_should_deny_write_over_an_existing_amendment() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_edit_on_an_existing_amendment() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("emenda-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_new_amendment_whose_number_has_one_digit() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-1.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

// Pasta nova de work item congelado: nenhum arquivo de spec nasce nela

const FOLDER_UNKNOWN: &str = "spec-folder-unknown";
const DECOY_FOLDER: &str = "wi-w1__other";

#[test]
fn it_should_deny_the_first_amendment_in_a_new_folder_of_a_frozen_work_item() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.decoy_file("emenda-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FOLDER_UNKNOWN), "reason: {reason}");
}

#[test]
fn it_should_deny_the_first_amendment_when_a_frozen_work_item_has_no_spec_folder_at_all() {
    let repo = Repo::new();
    repo.branch(BRANCH);

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FOLDER_UNKNOWN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_new_part_in_a_new_folder_of_a_frozen_work_item() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.decoy_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FOLDER_UNKNOWN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_file_outside_the_spec_family_in_a_new_folder_of_a_frozen_work_item() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.decoy_file("notas.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FOLDER_UNKNOWN), "reason: {reason}");
}

#[test]
fn it_should_allow_a_new_file_outside_the_spec_family_in_an_existing_folder_of_a_frozen_work_item()
{
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file("notas.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_a_file_outside_the_spec_family_in_a_new_folder_when_the_work_item_has_no_branch()
{
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.decoy_file("notas.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_a_new_folder_when_the_work_item_has_no_branch() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.decoy_file("emenda-01.md"));

    assert_allowed(&output);
}

// Fora de `specs.dir`: nada é aberto

/// Projeto sem git, `--db` sob um arquivo comum (não pode nascer) e um `git` falso no `PATH` que
/// grava um sentinela e falha: o banco aberto aparece como erro no stderr, e o git consultado,
/// como sentinela. Devolve a saída e se o `git` falso foi executado.
fn run_with_nothing_openable(project: &Project, file_path: &Path) -> (Child, bool) {
    let scratch = TempDir::new().expect("scratch directory is created");
    let blocker = scratch.path().join("blocker");
    fs::write(&blocker, "not a directory").expect("blocker file is written");
    let db = blocker.join("omhl.db");
    let fake_bin = scratch.path().join("fake-bin");
    fs::create_dir(&fake_bin).expect("fake git directory is created");
    let sentinel = scratch.path().join("git-was-called");
    let fake_git = fake_bin.join("git");
    fs::write(
        &fake_git,
        format!("#!/bin/sh\n: > '{}'\nexit 1\n", sentinel.display()),
    )
    .expect("fake git is written");
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755))
        .expect("fake git is made executable");
    let elsewhere = TempDir::new().expect("other directory is created");

    let output = omhl(&["--db", &db.to_string_lossy(), "hook", "spec-lock"])
        .cwd(elsewhere.path())
        .xdg(project.xdg())
        .env("PATH", &fake_bin)
        .stdin(payload(project.dir(), "Write", file_path))
        .run();

    (output, sentinel.exists())
}

#[test]
fn it_should_open_nothing_for_a_path_outside_the_specs_dir() {
    let project = Project::new("acme");

    let (output, git_called) =
        run_with_nothing_openable(&project, &project.dir().join("src/main.rs"));

    assert_allowed(&output);
    assert!(
        !git_called,
        "git was consulted for a path outside any spec folder"
    );
}

#[test]
fn it_should_open_no_database_for_a_work_item_folder_outside_the_specs_dir() {
    let project = Project::new("acme");

    // Sem o git não há como saber o `specs.dir`: a pasta de work item faz o hook consultar o git, mas nada além.
    let (output, _git_called) = run_with_nothing_openable(
        &project,
        &project.dir().join("docs").join(FOLDER).join("parte-01.md"),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_open_nothing_for_a_path_under_the_specs_dir_without_a_work_item_segment() {
    let project = Project::new("acme");

    let (output, git_called) =
        run_with_nothing_openable(&project, &project.dir().join(".omhl/specs/notes.md"));

    assert_allowed(&output);
    assert!(
        !git_called,
        "git was consulted for a path without a work item folder"
    );
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_a_spec_path_needs_git_that_fails() {
    let project = Project::new("acme");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let (output, git_called) = run_with_nothing_openable(&project, &spec);

    unavailable_reason(&output);
    assert!(git_called, "git was not consulted for a spec path");
}

// `specs.dir` customizado

#[test]
fn it_should_freeze_the_spec_under_a_custom_specs_dir() {
    let repo = Repo::with_files(&[(".omhl/workflow.toml", CUSTOM_WORKFLOW)]);
    repo.branch(BRANCH);
    repo.seed(CUSTOM_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.folder(CUSTOM_SPECS).join("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(&repo.folder(CUSTOM_SPECS).to_string_lossy().into_owned()));
}

#[test]
fn it_should_allow_the_next_amendment_under_a_custom_specs_dir() {
    let repo = Repo::with_files(&[(".omhl/workflow.toml", CUSTOM_WORKFLOW)]);
    repo.branch(BRANCH);
    repo.seed(CUSTOM_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.folder(CUSTOM_SPECS).join("emenda-01.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_stop_freezing_the_default_dir_once_the_specs_dir_is_customized() {
    let repo = Repo::with_files(&[(".omhl/workflow.toml", CUSTOM_WORKFLOW)]);
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert_allowed(&output);
}

// Política do `workflow.toml` vem sempre do diretório principal

/// Sobrescreve o `workflow.toml` da cópia de trabalho `dir` (a versão da branch, não commitada).
fn write_workflow_in(dir: &Path, toml: &str) {
    let path = dir.join(".omhl/workflow.toml");
    fs::create_dir_all(path.parent().expect("the file has a parent")).expect("parent");
    fs::write(path, toml).expect("workflow.toml is written");
}

#[test]
fn it_should_deny_a_frozen_spec_from_a_linked_worktree_whose_workflow_file_customizes_specs_dir() {
    let repo = Repo::new();
    let linked = repo.linked(BRANCH);
    write_workflow_in(&linked, CUSTOM_WORKFLOW);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook(&linked, "Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_frozen_spec_under_the_custom_specs_dir_of_the_main_directory_from_a_linked_worktree_with_the_default()
 {
    let repo = Repo::with_files(&[(".omhl/workflow.toml", CUSTOM_WORKFLOW)]);
    let linked = repo.linked(BRANCH);
    write_workflow_in(&linked, "version = 1\n");
    repo.seed(CUSTOM_SPECS, "parte-01.md");

    let output = repo.hook(
        &linked,
        "Edit",
        &repo.folder(CUSTOM_SPECS).join("parte-01.md"),
    );

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

// `workflow.toml` ou marcador inexistente/inválido

#[test]
fn it_should_deny_a_frozen_spec_when_the_workflow_file_is_invalid_and_specs_dir_is_the_default() {
    let repo = Repo::with_files(&[(".omhl/workflow.toml", INVALID_WORKFLOW)]);
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_frozen_spec_when_the_session_cwd_has_no_marker_but_the_file_is_inside_a_project()
 {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let outside = TempDir::new().expect("directory without a marker is created");

    let output = repo.hook(outside.path(), "Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_with_spec_frozen_a_frozen_spec_of_another_omhl_repository_when_cwd_is_in_a_different_one()
 {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let other = Repo::new();

    let output = repo.hook(&other.main, "Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_allow_a_spec_like_path_in_a_git_repository_without_omhl_marker() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let unmarked = TempDir::new().expect("unmarked repository directory is created");
    let init = git_in(unmarked.path(), &["init", "-b", "main"]);
    assert!(init.status.success(), "git init failed");
    let spec_like = unmarked
        .path()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = repo.hook(&repo.main, "Edit", &spec_like);

    assert_allowed(&output);
}

#[test]
fn it_should_allow_a_spec_like_path_outside_any_repository_without_git_error() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let outside = TempDir::new().expect("directory outside any repository is created");
    let spec_like = outside
        .path()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = repo.hook(&repo.main, "Edit", &spec_like);

    assert_allowed(&output);
}

#[test]
fn it_should_decide_an_edit_event_without_cwd() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let stdin = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": { "file_path": repo.spec_file("parte-01.md"), "old_string": "a", "new_string": "b" },
    })
    .to_string();

    let output = run_spec_lock(repo.project.xdg(), &stdin);

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

// Caminho físico: o link simbólico não tira a spec do congelamento

fn link(target: &Path, at: &Path) {
    symlink(target, at).expect("symlink is created");
}

/// `.omhl/specs` vira um link para `target`, e a spec congelada é gravada através dele.
fn link_specs_dir_to(repo: &Repo, target: &Path) {
    fs::create_dir_all(target).expect("link target is created");
    fs::create_dir_all(repo.main.join(".omhl")).expect(".omhl is created");
    link(target, &repo.main.join(DEFAULT_SPECS));
    repo.seed(DEFAULT_SPECS, "parte-01.md");
}

#[test]
fn it_should_deny_an_edit_through_a_symlink_to_the_specs_dir() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let alias = repo.main.join("alias");
    link(&repo.main.join(DEFAULT_SPECS), &alias);

    let output = repo.hook_in_main("Edit", &alias.join(FOLDER).join("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_an_edit_through_a_symlink_to_the_work_item_folder() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let alias = repo.main.join("alias");
    link(&repo.folder(DEFAULT_SPECS), &alias);

    let output = repo.hook_in_main("Edit", &alias.join("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_write_through_a_dangling_symlink_into_a_frozen_spec() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let alias = repo.main.join("alias.md");
    link(&repo.spec_file("parte-02.md"), &alias);

    let output = repo.hook_in_main("Write", &alias);

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_a_path_that_climbs_through_a_symlink_and_back_into_the_specs_dir() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let sibling = repo.main.join(".omhl/sibling");
    fs::create_dir_all(&sibling).expect("sibling directory is created");
    link(&sibling, &repo.main.join("link"));
    let climbing = repo
        .main
        .join("link/../specs")
        .join(FOLDER)
        .join("parte-01.md");

    let output = repo.hook_in_main("Edit", &climbing);

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_by_the_lexical_path_when_specs_dir_is_a_symlink_to_outside_the_repository() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    link_specs_dir_to(&repo, &repo.scratch.path().join("outside-specs"));

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_when_specs_dir_is_a_symlink_to_a_folder_inside_the_repository() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    link_specs_dir_to(&repo, &repo.main.join("docs/real-specs"));

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_keep_freezing_when_the_repository_root_is_reached_through_a_symlink() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let root_link = repo.scratch.path().join("root-link");
    link(&repo.main, &root_link);

    let output = repo.hook_in_main(
        "Edit",
        &root_link
            .join(DEFAULT_SPECS)
            .join(FOLDER)
            .join("parte-01.md"),
    );

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_decide_by_the_lexical_path_and_print_path_unresolvable_when_the_physical_path_cannot_be_resolved()
 {
    let repo = Repo::new();
    repo.branch(BRANCH);
    fs::create_dir_all(repo.folder(DEFAULT_SPECS)).expect("spec folder is created");
    let looping = repo.spec_file("parte-01.md");
    link(Path::new("parte-01.md"), &looping);

    let output = repo.hook_in_main("Edit", &looping);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout)["hookSpecificOutput"]["permissionDecision"],
        json!("deny")
    );
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("path-unresolvable")
    );
}

#[test]
fn it_should_allow_a_symlink_that_points_outside_the_specs_dir() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let docs = repo.main.join("docs");
    fs::create_dir_all(&docs).expect("docs directory is created");
    let alias = repo.main.join("alias");
    link(&docs, &alias);

    let output = repo.hook_in_main("Write", &alias.join("notes.md"));

    assert_allowed(&output);
}

// `specs.dir` sem pasta própria: a política inválida cai nos padrões, o congelamento não some

fn workflow_with_specs_dir(dir: &str) -> String {
    format!("version = 1\n\n[specs]\ndir = \"{dir}\"\n")
}

/// Work item `w1` aberto no banco e, só depois, o `workflow.toml` da cópia de trabalho com o `dir`.
fn repo_with_open_work_item_and_specs_dir(dir: &str) -> Repo {
    let repo = Repo::new();
    let appended = omhl(&["append"])
        .cwd(&repo.main)
        .xdg(repo.project.xdg())
        .stdin(
            json!({ "items": [{ "body": {
                "kind": "node",
                "id": "work-items.w1",
                "type": "work-item",
                "data": { "title": "t", "status": "open" },
            } }] })
            .to_string(),
        )
        .run();
    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    write_workflow_in(&repo.main, &workflow_with_specs_dir(dir));

    repo
}

fn spec_init(repo: &Repo) -> Child {
    omhl_json(&["spec", "init", "work-items.w1", "login"])
        .cwd(&repo.main)
        .xdg(repo.project.xdg())
        .run()
}

#[test]
fn it_should_keep_freezing_the_default_dir_when_specs_dir_has_no_folder_of_its_own() {
    for dir in UNUSABLE_SPECS_DIRS {
        let repo = Repo::with_files(&[(".omhl/workflow.toml", &workflow_with_specs_dir(dir))]);
        repo.branch(BRANCH);
        repo.seed(DEFAULT_SPECS, "parte-01.md");

        let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

        let reason = denied_reason(&output);
        assert!(
            reason.contains(FROZEN),
            "specs.dir {dir:?}, reason: {reason}"
        );
    }
}

#[test]
fn it_should_refuse_spec_init_when_specs_dir_has_no_folder_of_its_own() {
    for dir in UNUSABLE_SPECS_DIRS {
        let repo = repo_with_open_work_item_and_specs_dir(dir);

        let output = spec_init(&repo);

        assert_eq!(
            output.code,
            Some(1),
            "specs.dir {dir:?}, stderr: {}",
            output.stderr
        );
        assert_eq!(
            json_line(&output.stderr)["error"]["code"],
            json!("invalid-workflow-config"),
            "specs.dir {dir:?}"
        );
    }
}

#[test]
fn it_should_not_create_the_work_item_folder_when_specs_dir_has_no_folder_of_its_own() {
    for dir in UNUSABLE_SPECS_DIRS {
        let repo = repo_with_open_work_item_and_specs_dir(dir);

        spec_init(&repo);

        assert!(
            !repo.main.join(dir).join(FOLDER).exists(),
            "specs.dir {dir:?} got a work item folder"
        );
    }
}

#[test]
fn it_should_not_write_a_gitignore_when_specs_dir_has_no_folder_of_its_own() {
    for dir in UNUSABLE_SPECS_DIRS {
        let repo = repo_with_open_work_item_and_specs_dir(dir);

        spec_init(&repo);

        assert!(
            !repo.main.join(dir).join(".gitignore").exists(),
            "specs.dir {dir:?} got a .gitignore"
        );
    }
}

// Spec dentro de uma worktree ligada

#[test]
fn it_should_deny_a_spec_written_inside_a_linked_worktree() {
    let repo = Repo::new();
    let linked = repo.linked(BRANCH);
    let target = linked.join(DEFAULT_SPECS).join(FOLDER).join("parte-01.md");

    let output = repo.hook(&linked, "Write", &target);

    let reason = denied_reason(&output);
    assert!(reason.contains(IN_WORKTREE), "reason: {reason}");
}

#[test]
fn it_should_point_to_the_main_directory_when_denying_a_spec_inside_a_linked_worktree() {
    let repo = Repo::new();
    let linked = repo.linked(BRANCH);
    let target = linked.join(DEFAULT_SPECS).join(FOLDER).join("parte-01.md");

    let output = repo.hook(&linked, "Write", &target);

    let reason = denied_reason(&output);
    assert!(reason.contains(&repo.folder(DEFAULT_SPECS).to_string_lossy().into_owned()));
}

#[test]
fn it_should_deny_a_spec_inside_a_linked_worktree_of_an_unrelated_branch() {
    let repo = Repo::new();
    let linked = repo.linked("chore/unrelated");
    let target = linked.join(DEFAULT_SPECS).join(FOLDER).join("parte-01.md");

    let output = repo.hook(&linked, "Write", &target);

    let reason = denied_reason(&output);
    assert!(reason.contains(IN_WORKTREE), "reason: {reason}");
}

// Ferramenta que não edita

#[test]
fn it_should_allow_a_non_edit_tool_event_without_cwd() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let stdin = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "tool_name": "Read",
        "tool_input": { "file_path": repo.spec_file("parte-01.md") },
    })
    .to_string();

    let output = run_spec_lock(repo.project.xdg(), &stdin);

    assert_allowed(&output);
}

// Erro interno antes do reconhecimento libera; depois dele, nega

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_the_repository_is_broken() {
    let project = Project::new("acme");
    fs::write(project.dir().join(".git"), "gitdir: /nonexistent/gitdir\n")
        .expect("broken .git file is written");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = run_spec_lock(project.xdg(), &payload(project.dir(), "Edit", &spec));

    unavailable_reason(&output);
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_the_project_is_not_a_git_repository() {
    let project = Project::new("acme");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = run_spec_lock(project.xdg(), &payload(project.dir(), "Edit", &spec));

    unavailable_reason(&output);
}

// Saída

#[test]
fn it_should_print_the_denial_as_the_only_root_key() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    denied_reason(&output);
    assert_eq!(
        json_line(&output.stdout)
            .as_object()
            .expect("the root is an object")
            .len(),
        1,
        "hookSpecificOutput is the only root key"
    );
}

#[test]
fn it_should_exit_zero_on_every_denial_never_two() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.seed(DEFAULT_SPECS, "emenda-01.md");
    let targets = [
        ("Edit", "parte-01.md"),
        ("MultiEdit", "parte-01.md"),
        ("Write", "parte-01.md"),
        ("Write", "parte-02.md"),
        ("Write", "emenda-01.md"),
        ("Write", "emenda-09.md"),
    ];

    let codes: Vec<Option<i32>> = targets
        .iter()
        .map(|(tool, name)| repo.hook_in_main(tool, &repo.spec_file(name)).code)
        .collect();

    assert_eq!(codes, vec![Some(0); targets.len()]);
}

// Marca `.frozen`: a spec congelada continua congelada sem a branch

const MARK: &str = ".frozen";
const OTHER_BRANCH: &str = "feat/wi-w2__billing";
const OTHER_FOLDER: &str = "wi-w2__billing";

fn delete_branch(repo: &Repo, name: &str) {
    let deleted = git_in(repo.project.dir(), &["branch", "-D", name]);

    assert!(deleted.status.success(), "git branch -D {name} failed");
}

#[test]
fn it_should_deny_the_spec_of_a_work_item_whose_branch_was_deleted_after_a_spec_event() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    denied_reason(&repo.hook_in_main("Edit", &repo.spec_file("parte-01.md")));
    delete_branch(&repo, BRANCH);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_write_the_mark_for_every_work_item_with_a_local_branch() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.branch(OTHER_BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    fs::create_dir_all(repo.main.join(DEFAULT_SPECS).join(OTHER_FOLDER)).expect("other folder");

    repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert!(repo.folder(DEFAULT_SPECS).join(MARK).is_file());
    assert!(
        repo.main
            .join(DEFAULT_SPECS)
            .join(OTHER_FOLDER)
            .join(MARK)
            .is_file()
    );
}

#[test]
fn it_should_keep_the_decision_when_the_mark_cannot_be_written() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    let real = repo.main.join("real-folder");
    fs::create_dir_all(&real).expect("real folder is created");
    fs::write(real.join("parte-01.md"), "seed\n").expect("spec file is written");
    fs::create_dir_all(repo.main.join(DEFAULT_SPECS)).expect("specs dir is created");
    link(&real, &repo.folder(DEFAULT_SPECS));

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout)["hookSpecificOutput"]["permissionDecision"],
        json!("deny")
    );
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("freeze-mark-unwritable")
    );
    assert!(!real.join(MARK).exists());
}

#[test]
fn it_should_report_the_unwritable_mark_once_when_both_paths_run_the_cure() {
    let repo = Repo::new();
    repo.branch(OTHER_BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let real = repo.main.join("real-folder");
    fs::create_dir_all(&real).expect("real folder is created");
    link(&real, &repo.main.join(DEFAULT_SPECS).join(OTHER_FOLDER));
    // O texto difere do caminho físico, então os dois passam pela cura.
    let detour = repo
        .main
        .join(DEFAULT_SPECS)
        .join("../specs")
        .join(FOLDER)
        .join("parte-01.md");

    let output = repo.hook_in_main("Edit", &detour);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {}", output.stdout);
    assert_eq!(
        output.stderr.matches("freeze-mark-unwritable").count(),
        1,
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_say_the_mark_could_not_be_read_when_the_specs_dir_listing_fails() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let specs = repo.main.join(DEFAULT_SPECS);
    for index in 0..10_001 {
        fs::write(specs.join(format!("filler-{index}")), "").expect("filler is written");
    }

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert!(
        reason.contains("mark could not be read"),
        "reason: {reason}"
    );
}

#[test]
fn it_should_deny_when_a_second_folder_of_the_work_item_has_the_mark() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let second = repo.main.join(DEFAULT_SPECS).join("wi-w1__other");
    fs::create_dir_all(&second).expect("second folder is created");
    fs::write(second.join(MARK), "").expect("mark is written");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

/// Arquivo comum que divide o prefixo `wi-w1__` com a pasta real e vem antes dela na ordem de bytes.
fn plant_regular_file(repo: &Repo) {
    fs::write(repo.main.join(DEFAULT_SPECS).join("wi-w1__0"), "").expect("file is planted");
}

#[test]
fn it_should_deny_when_a_regular_file_shares_the_work_item_folder_prefix() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    plant_regular_file(&repo);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_with_the_mark_only_in_the_real_folder_when_a_regular_file_shares_the_prefix() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    fs::write(repo.spec_file(MARK), "").expect("mark is written");
    plant_regular_file(&repo);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_write_the_mark_in_the_real_folder_when_a_regular_file_shares_the_prefix() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    plant_regular_file(&repo);

    repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert!(repo.spec_file(MARK).is_file());
}

#[test]
fn it_should_not_create_a_mark_for_a_work_item_without_a_branch() {
    let repo = Repo::new();
    repo.branch(OTHER_BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    assert_allowed(&output);
    assert!(!repo.folder(DEFAULT_SPECS).join(MARK).exists());
}

#[test]
fn it_should_not_change_the_next_amendment_number_after_the_cure() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let output = repo.hook_in_main("Write", &repo.spec_file("emenda-01.md"));

    assert_allowed(&output);
}

// Regra do `Write` novo fora da família

#[test]
fn it_should_allow_a_family_named_file_in_a_subfolder_of_a_frozen_spec() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let outputs = [
        repo.hook_in_main("Write", &repo.spec_file("sub/parte-01.md")),
        repo.hook_in_main("Write", &repo.spec_file("sub/emenda-07.md")),
    ];

    for output in &outputs {
        assert_allowed(output);
    }
}

// Falha depois do reconhecimento da spec: nega com `spec-lock-unavailable`

const FIX_CLOSING: &str = "the edit stays denied until this is fixed";
const RETRY_CLOSING: &str = "the edit stays denied; retry it";
const SPEC_BODY: &str = "SPEC-BODY-MUST-NOT-LEAK";

/// A razão de uma negação `spec-lock-unavailable`: uma linha só, com o prefixo e o work item.
fn unavailable_reason(output: &Child) -> String {
    let reason = denied_reason(output);
    assert!(
        reason.starts_with("spec-lock-unavailable: "),
        "reason: {reason}"
    );
    assert!(reason.contains("wi-w1"), "reason: {reason}");
    assert!(!reason.contains('\n'), "reason: {reason}");

    reason
}

/// Repositório com a spec do work item e o `.git/config` quebrado direto no disco, depois de criar a branch.
fn repo_with_broken_git_config(with_branch: bool, config: &str) -> Repo {
    let repo = Repo::new();
    if with_branch {
        repo.branch(BRANCH);
    }
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    fs::write(repo.main.join(".git/config"), config).expect("git config is broken");

    repo
}

fn executable_in(dir: &Path, name: &str, script: &str) {
    let path = dir.join(name);
    fs::write(&path, script).expect("script is written");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("script is executable");
}

fn run_with_path(repo: &Repo, path: &std::ffi::OsStr, file_path: &Path) -> Child {
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["hook", "spec-lock"])
        .cwd(elsewhere.path())
        .xdg(repo.project.xdg())
        .env("PATH", path)
        .stdin(payload(&repo.main, "Write", file_path))
        .run()
}

#[test]
fn it_should_deny_each_editing_tool_with_spec_lock_unavailable_when_the_git_config_is_broken() {
    let repo = repo_with_broken_git_config(true, "[core\n");

    for tool in ["Write", "Edit", "MultiEdit"] {
        let output = repo.hook_in_main(tool, &repo.spec_file("parte-01.md"));

        let reason = unavailable_reason(&output);
        assert!(reason.contains("bad config line"), "{tool}: {reason}");
    }
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_whether_or_not_the_work_item_has_a_branch() {
    for with_branch in [true, false] {
        let repo = repo_with_broken_git_config(with_branch, "[core\n");

        let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

        let reason = unavailable_reason(&output);
        assert!(
            reason.contains("bad config line"),
            "branch {with_branch}: {reason}"
        );
    }
}

#[test]
fn it_should_close_a_git_failure_denial_with_until_this_is_fixed() {
    let repo = repo_with_broken_git_config(true, "[core\n");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    assert!(unavailable_reason(&output).ends_with(FIX_CLOSING));
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_git_is_missing_from_the_path() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let empty = TempDir::new().expect("empty path directory is created");

    let output = run_with_path(
        &repo,
        empty.path().as_os_str(),
        &repo.spec_file("parte-01.md"),
    );

    unavailable_reason(&output);
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_the_spec_folder_has_too_many_entries() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    for number in 0..10_001 {
        fs::write(repo.folder(DEFAULT_SPECS).join(format!("f{number}.md")), "")
            .expect("entry is written");
    }

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    let reason = unavailable_reason(&output);
    assert!(reason.contains("too many entries"), "reason: {reason}");
    assert!(reason.ends_with(FIX_CLOSING), "reason: {reason}");
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_the_spec_folder_is_a_regular_file() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    fs::create_dir_all(repo.main.join(DEFAULT_SPECS)).expect("specs dir is created");
    fs::write(repo.folder(DEFAULT_SPECS), "not a folder").expect("file replaces the folder");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    // O caminho físico não se resolve (a pasta é arquivo): o aviso sai ao lado da negação.
    let value = json_line(&output.stdout);
    let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("the reason is a string");
    assert!(
        reason.starts_with("spec-lock-unavailable: "),
        "reason: {reason}"
    );
    assert!(reason.contains("cannot be read"), "reason: {reason}");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("path-unresolvable")
    );
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_and_retry_it_when_git_hangs_past_the_budget() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    let fake_bin = TempDir::new().expect("fake git directory is created");
    executable_in(fake_bin.path(), "git", "#!/bin/sh\nexec sleep 30\n");
    let path = format!(
        "{}:{}",
        fake_bin.path().display(),
        std::env::var("PATH").expect("PATH is set")
    );
    let started = std::time::Instant::now();

    let output = run_with_path(&repo, path.as_ref(), &repo.spec_file("parte-01.md"));

    assert!(
        started.elapsed() < std::time::Duration::from_secs(8),
        "the hook took {:?}",
        started.elapsed()
    );
    let reason = unavailable_reason(&output);
    assert!(reason.contains("git timed out"), "reason: {reason}");
    assert!(reason.ends_with(RETRY_CLOSING), "reason: {reason}");
}

#[test]
fn it_should_keep_a_git_message_with_a_line_break_on_one_line() {
    let repo = repo_with_broken_git_config(true, "[core]\n\trepositoryformatversion = \"x\\ny\"\n");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    let reason = unavailable_reason(&output);
    assert!(
        reason.contains("bad numeric config value"),
        "reason: {reason}"
    );
}

#[test]
fn it_should_not_put_the_text_of_a_spec_file_in_the_unavailable_reason() {
    let repo = repo_with_broken_git_config(true, "[core\n");
    fs::write(repo.spec_file("parte-01.md"), SPEC_BODY).expect("spec body is written");

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    assert!(!unavailable_reason(&output).contains(SPEC_BODY));
}

#[test]
fn it_should_deny_when_only_the_physical_path_fails_and_print_no_path_unresolvable() {
    let repo = Repo::new();
    let other = Project::new("other");
    let other_specs = other.dir().join(DEFAULT_SPECS);
    fs::create_dir_all(other_specs.join(FOLDER)).expect("other specs folder is created");
    fs::write(other_specs.join(FOLDER).join("parte-01.md"), "seed\n").expect("spec is written");
    assert!(
        git_in(other.dir(), &["init", "-b", "main"])
            .status
            .success()
    );
    fs::write(other.dir().join(".git/config"), "[core\n").expect("other git config is broken");
    fs::create_dir_all(repo.main.join(".omhl")).expect(".omhl is created");
    link(&other_specs, &repo.main.join(DEFAULT_SPECS));

    let output = repo.hook_in_main("Write", &repo.spec_file("parte-01.md"));

    unavailable_reason(&output);
}

// Arquivos de marcador na pasta congelada e marcador plantado no caminho de subida

const GARBAGE_MARKER: &str = "tenant = acme\n";
const VALID_MARKER: &str = "version = 1\ntenant = \"acme\"\n";

/// Spec congelada pela branch, com a marca `.frozen` já gravada: o hook só cura marcas que faltam.
fn frozen_repo() -> Repo {
    let repo = Repo::new();
    repo.branch(BRANCH);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    fs::write(repo.spec_file(MARK), "").expect("freeze mark is written");

    repo
}

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("the file has a parent")).expect("parent is created");
    fs::write(path, content).expect("file is written");
}

/// Caminho e conteúdo de cada arquivo regular fora do `.git`; o que não é arquivo regular fica sem conteúdo.
fn tree_snapshot(dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut entries = Vec::new();
    collect_tree(dir, &mut entries);
    entries.sort();

    entries
}

fn collect_tree(dir: &Path, entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
    for entry in fs::read_dir(dir).expect("directory is listed") {
        let path = entry.expect("entry is read").path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).expect("metadata is read");
        if metadata.is_dir() {
            collect_tree(&path, entries);
        }
        let content = metadata
            .is_file()
            .then(|| fs::read(&path).expect("file is read"));
        entries.push((path, content));
    }
}

#[test]
fn it_should_deny_marker_files_written_in_a_frozen_spec_folder_with_their_own_reason() {
    let repo = frozen_repo();

    for name in [".omhl.toml", ".omhl/workflow.toml", "sub/.omhl.toml"] {
        let output = repo.hook_in_main("Write", &repo.spec_file(name));

        let reason = denied_reason(&output);
        assert!(reason.contains(FROZEN), "{name}: {reason}");
        assert!(reason.contains("project marker files"), "{name}: {reason}");
    }
}

#[test]
fn it_should_deny_the_edit_of_a_valid_marker_that_already_sits_in_a_frozen_spec_folder() {
    let repo = frozen_repo();
    write_file(&repo.spec_file(".omhl.toml"), VALID_MARKER);

    let output = repo.hook_in_main("Edit", &repo.spec_file(".omhl.toml"));

    let reason = denied_reason(&output);
    assert!(reason.contains("project marker files"), "reason: {reason}");
}

#[test]
fn it_should_keep_denying_the_part_after_denying_a_marker_file() {
    let repo = frozen_repo();
    denied_reason(&repo.hook_in_main("Write", &repo.spec_file(".omhl.toml")));

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert!(!reason.contains("project marker files"), "reason: {reason}");
}

#[test]
fn it_should_allow_a_marker_file_in_the_folder_of_a_work_item_without_branch_or_mark() {
    let repo = Repo::new();
    repo.seed(DEFAULT_SPECS, "parte-01.md");

    let output = repo.hook_in_main("Write", &repo.spec_file(".omhl.toml"));

    assert_allowed(&output);
}

#[test]
fn it_should_deny_a_frozen_spec_when_garbage_sits_in_a_marker_planted_in_the_specs_dir() {
    let repo = frozen_repo();
    write_file(
        &repo.main.join(DEFAULT_SPECS).join(".omhl.toml"),
        GARBAGE_MARKER,
    );
    let before = tree_snapshot(&repo.main);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert_eq!(tree_snapshot(&repo.main), before);
}

#[test]
fn it_should_deny_a_frozen_spec_when_garbage_sits_in_a_marker_planted_in_the_work_item_folder() {
    let repo = frozen_repo();
    write_file(&repo.spec_file(".omhl.toml"), GARBAGE_MARKER);
    let before = tree_snapshot(&repo.main);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert_eq!(tree_snapshot(&repo.main), before);
}

#[test]
fn it_should_deny_a_frozen_spec_when_the_planted_marker_is_a_symlink() {
    let repo = frozen_repo();
    link(
        &repo.main.join(".omhl.toml"),
        &repo.main.join(DEFAULT_SPECS).join(".omhl.toml"),
    );
    let before = tree_snapshot(&repo.main);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert_eq!(tree_snapshot(&repo.main), before);
}

#[test]
fn it_should_deny_a_frozen_spec_when_the_planted_marker_is_a_fifo() {
    let repo = frozen_repo();
    let fifo = repo.spec_file(".omhl.toml");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo failed");
    let before = tree_snapshot(&repo.main);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert_eq!(tree_snapshot(&repo.main), before);
}

#[test]
fn it_should_judge_a_nested_project_with_a_broken_marker_by_the_project_above() {
    let repo = Repo::new();
    repo.branch(BRANCH);
    write_file(&repo.main.join("sub/.omhl.toml"), GARBAGE_MARKER);
    let nested_spec = repo
        .main
        .join("sub")
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = repo.hook_in_main("Edit", &nested_spec);

    let reason = denied_reason(&output);
    assert!(reason.contains("spec-folder-unknown"), "reason: {reason}");
}

#[test]
fn it_should_release_with_the_marker_error_when_no_valid_marker_exists_above_the_spec() {
    let project = Project::new("acme");
    write_file(&project.dir().join(".omhl.toml"), GARBAGE_MARKER);
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = run_spec_lock(project.xdg(), &payload(project.dir(), "Edit", &spec));

    assert_released_with_error(&output);
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("invalid-marker")
    );
}

// `workflow.toml` principal: mudar o `specs.dir` com spec congelada

const SPEC_DIR_FROZEN: &str = "spec-dir-frozen";
const NEW_CONTENT_STEP: &str = "reading the new content of workflow.toml";
const MOVED_WORKFLOW: &str = "version = 1\n\n[specs]\ndir = \"docs/specs\"\n";
const EXPLICIT_DEFAULT_WORKFLOW: &str =
    "version = 1\n\n[specs]\ndir = \".omhl/specs\"\nmax_lines = 300\n";

/// Work item congelado pela branch; `workflow` é o `.omhl/workflow.toml` commitado, se houver.
fn frozen_workflow_repo(workflow: Option<&str>) -> Repo {
    let files: Vec<(&str, &str)> = workflow
        .map(|content| (".omhl/workflow.toml", content))
        .into_iter()
        .collect();
    let repo = Repo::with_files(&files);
    repo.branch(BRANCH);

    repo
}

fn main_workflow(repo: &Repo) -> PathBuf {
    repo.main.join(".omhl/workflow.toml")
}

fn write_input(path: &Path, content: &str) -> Value {
    json!({ "file_path": path, "content": content })
}

fn edit_input(path: &Path, old: &str, new: &str, replace_all: Option<bool>) -> Value {
    let mut input = json!({ "file_path": path, "old_string": old, "new_string": new });
    if let Some(flag) = replace_all {
        input["replace_all"] = json!(flag);
    }

    input
}

fn hook_input(repo: &Repo, cwd: &Path, tool: &str, tool_input: Value) -> Child {
    run_spec_lock(repo.project.xdg(), &payload_of(cwd, tool, tool_input))
}

/// A negação `spec-dir-frozen`: uma linha, com o caminho, as duas pastas e o work item congelado.
fn spec_dir_frozen_reason(output: &Child) -> String {
    let reason = denied_reason(output);
    assert!(
        reason.starts_with(&format!("{SPEC_DIR_FROZEN}: ")),
        "reason: {reason}"
    );
    assert!(!reason.contains('\n'), "reason: {reason}");

    reason
}

/// A negação `spec-lock-unavailable` de um caminho que não é de work item: o sujeito é o caminho.
fn path_unavailable_reason(output: &Child, path: &Path, step: &str) -> String {
    let reason = denied_reason(output);
    assert!(
        reason.starts_with("spec-lock-unavailable: "),
        "reason: {reason}"
    );
    assert!(!reason.contains('\n'), "reason: {reason}");
    assert!(reason.contains(step), "reason: {reason}");
    assert!(
        reason.contains(&path.display().to_string()),
        "reason: {reason}"
    );

    reason
}

fn break_git_config(repo: &Repo) {
    fs::write(repo.main.join(".git/config"), "[core").expect("git config is broken");
}

#[test]
fn it_should_deny_a_write_of_the_main_workflow_file_that_moves_the_specs_dir_while_a_spec_is_frozen()
 {
    let repo = frozen_workflow_repo(None);
    let path = main_workflow(&repo);

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&path, MOVED_WORKFLOW),
    );

    let reason = spec_dir_frozen_reason(&output);
    let path_text = path.display().to_string();
    for fact in [
        path_text.as_str(),
        ".omhl/specs",
        "docs/specs",
        "wi-w1",
        "outside Claude Code",
    ] {
        assert!(reason.contains(fact), "{fact}: {reason}");
    }
}

#[test]
fn it_should_deny_an_edit_of_the_main_workflow_file_that_changes_only_the_dir_line() {
    let repo = frozen_workflow_repo(Some(EXPLICIT_DEFAULT_WORKFLOW));

    let output = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(
            &main_workflow(&repo),
            "dir = \".omhl/specs\"",
            "dir = \"docs/specs\"",
            None,
        ),
    );

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_deny_a_multi_edit_of_the_main_workflow_file_in_two_replacements() {
    let repo = frozen_workflow_repo(Some(EXPLICIT_DEFAULT_WORKFLOW));
    let input = json!({
        "file_path": main_workflow(&repo),
        "edits": [
            { "old_string": "max_lines = 300", "new_string": "max_lines = 200" },
            { "old_string": "dir = \".omhl/specs\"", "new_string": "dir = \"docs/specs\"" },
        ],
    });

    let output = hook_input(&repo, &repo.main, "MultiEdit", input);

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_allow_a_write_of_the_main_workflow_file_that_keeps_the_specs_dir() {
    let repo = frozen_workflow_repo(Some(EXPLICIT_DEFAULT_WORKFLOW));
    let path = main_workflow(&repo);

    for content in [
        "version = 1\n\n[specs]\ndir = \".omhl/specs\"\nmax_lines = 10\n",
        "version = 1\n\n[specs]\ndir = \"./.omhl/specs\"\n",
    ] {
        let output = hook_input(&repo, &repo.main, "Write", write_input(&path, content));

        assert_allowed(&output);
    }
}

#[test]
fn it_should_deny_a_write_of_invalid_content_over_a_main_workflow_file_with_a_custom_dir() {
    let repo = frozen_workflow_repo(Some(CUSTOM_WORKFLOW));

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&main_workflow(&repo), "[specs"),
    );

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_allow_a_write_of_invalid_content_when_the_main_workflow_file_is_absent_or_default() {
    for current in [None, Some(EXPLICIT_DEFAULT_WORKFLOW)] {
        let repo = frozen_workflow_repo(current);

        let output = hook_input(
            &repo,
            &repo.main,
            "Write",
            write_input(&main_workflow(&repo), "[specs"),
        );

        assert_allowed(&output);
    }
}

#[test]
fn it_should_allow_moving_the_specs_dir_when_no_work_item_has_a_branch_or_a_mark() {
    let repo = Repo::new();

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&main_workflow(&repo), MOVED_WORKFLOW),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_deny_moving_the_specs_dir_when_only_the_freeze_mark_is_left_after_the_branch_was_deleted()
 {
    let repo = frozen_repo();
    delete_branch(&repo, BRANCH);

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&main_workflow(&repo), MOVED_WORKFLOW),
    );

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_allow_moving_the_specs_dir_in_the_workflow_file_of_a_linked_worktree() {
    let repo = Repo::new();
    let linked = repo.linked(BRANCH);

    let output = hook_input(
        &repo,
        &linked,
        "Write",
        write_input(&linked.join(".omhl/workflow.toml"), MOVED_WORKFLOW),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_the_new_content_of_the_workflow_file_cannot_be_computed()
 {
    let repo = frozen_workflow_repo(Some(EXPLICIT_DEFAULT_WORKFLOW));
    let path = main_workflow(&repo);
    let cases = [
        ("Edit", edit_input(&path, "not in the file", "x", None)),
        ("Edit", json!({ "file_path": path, "new_string": "x" })),
        ("Write", json!({ "file_path": path })),
        ("MultiEdit", json!({ "file_path": path, "edits": "x" })),
    ];

    for (tool, input) in cases {
        let output = hook_input(&repo, &repo.main, tool, input);

        path_unavailable_reason(&output, &path, NEW_CONTENT_STEP);
    }
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_an_edit_of_a_main_workflow_file_that_is_missing() {
    let repo = frozen_workflow_repo(None);
    let path = main_workflow(&repo);

    let output = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(&path, "dir", "dir", None),
    );

    path_unavailable_reason(&output, &path, NEW_CONTENT_STEP);
}

#[test]
fn it_should_deny_the_workflow_file_with_spec_lock_unavailable_when_git_is_broken_and_allow_other_files()
 {
    let repo = frozen_workflow_repo(None);
    break_git_config(&repo);
    let path = main_workflow(&repo);

    let denied = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&path, MOVED_WORKFLOW),
    );
    let other = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&repo.main.join("notas.md"), "x"),
    );

    path_unavailable_reason(&denied, &path, "checking the main directory");
    assert_allowed(&other);
}

#[test]
fn it_should_deny_both_the_move_of_the_specs_dir_and_the_edit_of_the_spec_and_leave_the_tree_untouched()
 {
    let repo = frozen_workflow_repo(None);
    repo.seed(DEFAULT_SPECS, "parte-01.md");
    fs::write(repo.spec_file(MARK), "").expect("freeze mark is written");
    let before = tree_snapshot(&repo.main);

    let moved = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&main_workflow(&repo), MOVED_WORKFLOW),
    );
    let edited = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    spec_dir_frozen_reason(&moved);
    let reason = denied_reason(&edited);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert_eq!(tree_snapshot(&repo.main), before);
}

#[test]
fn it_should_deny_the_workflow_file_with_spec_lock_unavailable_when_a_planted_marker_sits_under_omhl_and_git_is_broken()
 {
    let repo = frozen_workflow_repo(None);
    write_file(&repo.main.join(".omhl/.omhl.toml"), VALID_MARKER);
    break_git_config(&repo);
    let path = main_workflow(&repo);

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&path, MOVED_WORKFLOW),
    );

    path_unavailable_reason(&output, &path, "checking the main directory");
}

#[test]
fn it_should_deny_with_spec_dir_frozen_when_a_planted_marker_and_a_fake_git_folder_sit_under_omhl()
{
    let repo = frozen_workflow_repo(None);
    write_file(&repo.main.join(".omhl/.omhl.toml"), VALID_MARKER);
    write_file(&repo.main.join(".omhl/.git/HEAD"), "ref: refs/heads/main\n");
    write_file(&repo.main.join(".omhl/.git/objects/x"), "");
    write_file(&repo.main.join(".omhl/.git/refs/x"), "");

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&main_workflow(&repo), MOVED_WORKFLOW),
    );

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_deny_by_the_normalized_path_a_write_that_climbs_out_of_the_specs_dir_to_the_workflow_file()
 {
    let repo = frozen_workflow_repo(None);
    let climbing = repo.main.join(".omhl/specs/../workflow.toml");

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&climbing, MOVED_WORKFLOW),
    );

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_deny_a_valid_write_without_the_dir_key_over_a_workflow_file_with_a_custom_dir() {
    let repo = frozen_workflow_repo(Some(CUSTOM_WORKFLOW));

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(
            &main_workflow(&repo),
            "version = 1\n\n[specs]\nmax_lines = 10\n",
        ),
    );

    spec_dir_frozen_reason(&output);
}

const TWO_DIR_LINES: &str =
    "version = 1\n\n# dir = \".omhl/specs\"\n[specs]\ndir = \".omhl/specs\"\n";

#[test]
fn it_should_deny_an_edit_with_replace_all_that_reaches_the_dir_line_in_a_later_occurrence() {
    let repo = frozen_workflow_repo(Some(TWO_DIR_LINES));

    let output = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(
            &main_workflow(&repo),
            "dir = \".omhl/specs\"",
            "dir = \"docs/specs\"",
            Some(true),
        ),
    );

    spec_dir_frozen_reason(&output);
}

#[test]
fn it_should_allow_an_edit_without_replace_all_that_only_reaches_the_commented_occurrence() {
    let repo = frozen_workflow_repo(Some(TWO_DIR_LINES));

    let output = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(
            &main_workflow(&repo),
            "dir = \".omhl/specs\"",
            "dir = \"docs/specs\"",
            Some(false),
        ),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_deny_an_edit_that_only_fits_the_workflow_file_after_the_crlf_is_normalized() {
    let repo = frozen_workflow_repo(Some(
        "version = 1\r\n\r\n[specs]\r\ndir = \".omhl/specs\"\r\n",
    ));

    let output = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(
            &main_workflow(&repo),
            "[specs]\ndir = \".omhl/specs\"",
            "[specs]\ndir = \"docs/specs\"",
            None,
        ),
    );

    spec_dir_frozen_reason(&output);
}

// Marcador e `workflow.toml` plantados sob `.omhl/`

fn plant_marker_and_workflow_under_omhl(repo: &Repo) {
    write_file(&repo.main.join(".omhl/specs/.omhl.toml"), VALID_MARKER);
    write_file(
        &repo.main.join(".omhl/specs/.omhl/workflow.toml"),
        "version = 1\n\n[specs]\ndir = \"x\"\n",
    );
}

fn planted_repo() -> Repo {
    let repo = frozen_repo();
    plant_marker_and_workflow_under_omhl(&repo);

    repo
}

#[test]
fn it_should_deny_a_frozen_spec_with_spec_lock_unavailable_when_planted_files_sit_under_omhl_and_git_is_broken()
 {
    let repo = planted_repo();
    break_git_config(&repo);

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    unavailable_reason(&output);
}

#[test]
fn it_should_deny_a_frozen_spec_with_spec_frozen_when_planted_files_sit_under_omhl_and_git_works() {
    let repo = planted_repo();

    let output = repo.hook_in_main("Edit", &repo.spec_file("parte-01.md"));

    let reason = denied_reason(&output);
    assert!(reason.contains(FROZEN), "reason: {reason}");
}

#[test]
fn it_should_deny_the_planted_workflow_file_when_git_is_broken_and_allow_it_when_git_works() {
    let repo = planted_repo();
    let planted = repo.main.join(".omhl/specs/.omhl/workflow.toml");
    let write = || {
        hook_input(
            &repo,
            &repo.main,
            "Write",
            write_input(&planted, MOVED_WORKFLOW),
        )
    };
    let released = write();
    break_git_config(&repo);

    let denied = write();

    assert_allowed(&released);
    path_unavailable_reason(&denied, &planted, "checking the main directory");
}

// Resíduo documentado: `specs.dir` customizado com marcador válido plantado na pasta de specs

fn run_with_home(repo: &Repo, home: &Path, stdin: &str) -> Child {
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["hook", "spec-lock"])
        .cwd(elsewhere.path())
        .xdg(repo.project.xdg())
        .env("HOME", home)
        .stdin(stdin)
        .run()
}

/// Spec congelada sob `specs.dir` customizado, ainda sem nada plantado.
fn custom_dir_repo() -> Repo {
    let repo = frozen_workflow_repo(Some(CUSTOM_WORKFLOW));
    repo.seed(CUSTOM_SPECS, "parte-01.md");

    repo
}

#[test]
fn it_should_residual_release_a_frozen_spec_when_a_planted_marker_and_a_fake_bare_repository_sit_in_the_custom_specs_dir()
 {
    let repo = custom_dir_repo();
    let spec = repo.folder(CUSTOM_SPECS).join("parte-01.md");
    let control = repo.hook_in_main("Edit", &spec);
    let specs_dir = repo.main.join(CUSTOM_SPECS);
    write_file(&specs_dir.join(".omhl.toml"), VALID_MARKER);
    write_file(&specs_dir.join("HEAD"), "ref: refs/heads/main\n");
    write_file(&specs_dir.join("objects/x"), "");
    write_file(&specs_dir.join("refs/x"), "");

    let output = repo.hook_in_main("Edit", &spec);

    let reason = denied_reason(&control);
    assert!(reason.contains(FROZEN), "reason: {reason}");
    assert_allowed(&output);
}

#[test]
fn it_should_residual_release_a_frozen_spec_when_a_planted_marker_sits_in_the_custom_specs_dir_and_the_global_git_config_is_broken()
 {
    let repo = custom_dir_repo();
    let spec = repo.folder(CUSTOM_SPECS).join("parte-01.md");
    let home = TempDir::new().expect("home directory is created");
    fs::write(home.path().join(".gitconfig"), "[core").expect("global git config is broken");
    let stdin = payload(&repo.main, "Edit", &spec);
    let control = run_with_home(&repo, home.path(), &stdin);
    write_file(
        &repo.main.join(CUSTOM_SPECS).join(".omhl.toml"),
        VALID_MARKER,
    );

    let output = run_with_home(&repo, home.path(), &stdin);

    unavailable_reason(&control);
    assert_allowed(&output);
}

// `.git` protegido: negado com `git-dir-protected`, sem chamar git

/// A negação `git-dir-protected`: uma linha, com o caminho, pedindo ao dono e sem sugerir o `Bash`.
fn git_dir_reason(output: &Child, path: &Path) -> String {
    let reason = denied_reason(output);
    assert!(
        reason.starts_with("git-dir-protected: "),
        "reason: {reason}"
    );
    assert!(reason.contains(&path.display().to_string()), "{reason}");
    assert!(reason.contains("ask the owner"), "reason: {reason}");
    assert!(!reason.contains("Bash"), "reason: {reason}");
    assert!(!reason.contains("terminal"), "reason: {reason}");
    assert!(!reason.contains('\n'), "reason: {reason}");

    reason
}

fn assert_denied_as_git_dir(repo: &Repo, tool: &str, path: &Path) {
    let output = repo.hook_in_main(tool, path);

    git_dir_reason(&output, path);
}

#[test]
fn it_should_deny_each_editing_tool_on_a_file_under_the_git_folder_with_git_dir_protected() {
    let repo = frozen_repo();

    for tool in ["Write", "Edit", "MultiEdit"] {
        assert_denied_as_git_dir(&repo, tool, &repo.main.join(".git/config"));
    }
    assert_denied_as_git_dir(&repo, "Write", &repo.main.join(".git/commondir"));
    assert_denied_as_git_dir(&repo, "Write", &repo.main.join(".omhl/.git/HEAD"));
}

#[test]
fn it_should_deny_a_write_of_the_git_folder_itself() {
    let repo = frozen_repo();

    assert_denied_as_git_dir(&repo, "Write", &repo.main.join(".git"));
}

#[test]
fn it_should_deny_a_write_that_reaches_the_git_folder_climbing_with_a_parent_dir() {
    let repo = frozen_repo();

    assert_denied_as_git_dir(&repo, "Write", &repo.main.join("docs/../.git/config"));
}

#[test]
fn it_should_deny_a_write_through_a_symlink_that_points_inside_the_git_folder() {
    let repo = frozen_repo();
    symlink(repo.main.join(".git"), repo.main.join("link")).expect("link is created");

    let output = repo.hook_in_main("Write", &repo.main.join("link/config"));

    git_dir_reason(&output, &repo.main.join(".git/config"));
}

#[test]
fn it_should_deny_the_git_folder_even_when_the_git_config_is_broken() {
    let repo = frozen_repo();
    fs::write(repo.main.join(".git/config"), "[core\n").expect("git config is broken");

    assert_denied_as_git_dir(&repo, "Write", &repo.main.join(".git/config"));
}

#[test]
fn it_should_deny_the_git_folder_without_calling_git_when_git_hangs() {
    let repo = frozen_repo();
    let fake_bin = TempDir::new().expect("fake git directory is created");
    executable_in(fake_bin.path(), "git", "#!/bin/sh\nexec sleep 30\n");
    let path = format!(
        "{}:{}",
        fake_bin.path().display(),
        std::env::var("PATH").expect("PATH is set")
    );
    let target = repo.main.join(".git/config");
    let started = std::time::Instant::now();

    let output = run_with_path(&repo, path.as_ref(), &target);

    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the hook took {:?}",
        started.elapsed()
    );
    git_dir_reason(&output, &target);
}

#[test]
fn it_should_deny_the_git_file_and_the_admin_folder_of_a_linked_worktree() {
    let repo = frozen_repo();
    let linked = repo.linked("feat/wi-w2__other");

    assert_denied_as_git_dir(&repo, "Write", &linked.join(".git"));
    assert_denied_as_git_dir(
        &repo,
        "Write",
        &repo.main.join(".git/worktrees/wt/commondir"),
    );
}

#[test]
fn it_should_not_let_a_planted_marker_under_omhl_hide_the_git_folder_below_it() {
    let repo = frozen_repo();
    write_file(&repo.main.join(".omhl/.omhl.toml"), VALID_MARKER);

    assert_denied_as_git_dir(&repo, "Write", &repo.main.join(".omhl/.git/HEAD"));
}

#[test]
fn it_should_allow_a_project_that_lives_under_a_folder_named_dot_git() {
    let home = TempDir::new().expect("directory is created");
    let project = home.path().join(".git/proj");
    write_file(&project.join(".omhl.toml"), VALID_MARKER);
    let output = run_spec_lock(
        home.path(),
        &payload(&project, "Write", &project.join("notes.md")),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_allow_names_that_only_resemble_dot_git_and_plain_files() {
    let repo = frozen_repo();

    for file in [".gitignore", ".github/workflows/ci.yml", "notes.md"] {
        let output = repo.hook_in_main("Write", &repo.main.join(file));

        assert_allowed(&output);
    }
}

#[test]
fn it_should_allow_a_path_with_dot_git_outside_any_omhl_project() {
    let outside = TempDir::new().expect("directory is created");
    let target = outside.path().join(".git/config");

    let output = run_spec_lock(outside.path(), &payload(outside.path(), "Write", &target));

    assert_allowed(&output);
}

#[test]
fn it_should_allow_a_path_with_dot_git_with_the_marker_error_when_the_only_marker_is_invalid() {
    let outside = TempDir::new().expect("directory is created");
    write_file(&outside.path().join(".omhl.toml"), "[core\n");
    let target = outside.path().join(".git/config");

    let output = run_spec_lock(outside.path(), &payload(outside.path(), "Write", &target));

    assert_released_with_error(&output);
}

// Marcador protegido: negado com `marker-protected` quando o resultado deixa de ser marcador válido

const BROKEN_MARKER: &str = "[core";
const MARKER_STEP: &str = "reading the new content of .omhl.toml";

fn main_marker(repo: &Repo) -> PathBuf {
    repo.main.join(".omhl.toml")
}

/// A negação `marker-protected`: uma linha, com o caminho, pedindo ao dono e sem sugerir o `Bash`.
fn marker_reason(output: &Child, path: &Path) -> String {
    let reason = denied_reason(output);
    assert!(reason.starts_with("marker-protected: "), "reason: {reason}");
    assert!(reason.contains(&path.display().to_string()), "{reason}");
    assert!(reason.contains("ask the owner"), "reason: {reason}");
    assert!(!reason.contains("Bash"), "reason: {reason}");
    assert!(!reason.contains("terminal"), "reason: {reason}");
    assert!(!reason.contains('\n'), "reason: {reason}");

    reason
}

fn write_garbage(repo: &Repo, path: &Path) -> Child {
    hook_input(repo, &repo.main, "Write", write_input(path, BROKEN_MARKER))
}

fn multi_edit_input(path: &Path, edits: &[(&str, &str)]) -> Value {
    let edits: Vec<Value> = edits
        .iter()
        .map(|(old, new)| json!({ "old_string": old, "new_string": new }))
        .collect();

    json!({ "file_path": path, "edits": edits })
}

#[test]
fn it_should_deny_garbage_written_over_the_valid_marker_and_leave_the_tree_untouched() {
    let repo = Repo::new();
    let before = tree_snapshot(&repo.main);

    let output = write_garbage(&repo, &main_marker(&repo));

    marker_reason(&output, &main_marker(&repo));
    assert_eq!(tree_snapshot(&repo.main), before);
}

#[test]
fn it_should_allow_an_edit_that_keeps_the_marker_valid_and_deny_one_that_breaks_it() {
    let repo = Repo::new();
    let path = main_marker(&repo);

    let keeps = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(&path, "acme", "other", None),
    );
    let breaks = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(&path, "version = 1", "version = 2", None),
    );

    assert_allowed(&keeps);
    marker_reason(&breaks, &path);
}

#[test]
fn it_should_judge_a_multi_edit_by_its_result() {
    let repo = Repo::new();
    let path = main_marker(&repo);

    let keeps = hook_input(
        &repo,
        &repo.main,
        "MultiEdit",
        multi_edit_input(&path, &[("acme", "beta"), ("beta", "gamma")]),
    );
    let breaks = hook_input(
        &repo,
        &repo.main,
        "MultiEdit",
        multi_edit_input(&path, &[("acme", "beta"), ("version = 1", "version = 3")]),
    );

    assert_allowed(&keeps);
    marker_reason(&breaks, &path);
}

#[test]
fn it_should_allow_a_write_of_a_new_valid_marker_over_the_valid_one() {
    let repo = Repo::new();

    let output = hook_input(
        &repo,
        &repo.main,
        "Write",
        write_input(&main_marker(&repo), "version = 1\ntenant = \"other\"\n"),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_deny_garbage_written_over_the_marker_of_a_subproject() {
    let repo = Repo::new();
    let path = repo.main.join("sub/.omhl.toml");
    write_file(&path, VALID_MARKER);

    let output = write_garbage(&repo, &path);

    marker_reason(&output, &path);
}

#[test]
fn it_should_allow_editing_an_invalid_marker_and_writing_a_new_marker_outside_a_frozen_folder() {
    let repo = Repo::new();
    let broken = repo.main.join("sub/.omhl.toml");
    write_file(&broken, BROKEN_MARKER);

    let edited = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(&broken, "[core", "[other", None),
    );
    let planted = write_garbage(&repo, &repo.main.join("fresh/.omhl.toml"));

    assert_allowed(&edited);
    assert_allowed(&planted);
}

#[test]
fn it_should_allow_a_symlink_named_as_the_marker_that_points_to_a_file_of_another_name() {
    let repo = Repo::new();
    write_file(&repo.main.join("alvo.txt"), VALID_MARKER);
    fs::create_dir_all(repo.main.join("l")).expect("folder is created");
    symlink(repo.main.join("alvo.txt"), repo.main.join("l/.omhl.toml")).expect("link is created");

    let output = write_garbage(&repo, &repo.main.join("l/.omhl.toml"));

    assert_allowed(&output);
}

#[test]
fn it_should_deny_by_the_physical_path_a_symlink_that_points_to_the_valid_marker() {
    let repo = Repo::new();
    fs::create_dir_all(repo.main.join("sub")).expect("folder is created");
    symlink(main_marker(&repo), repo.main.join("sub/.omhl.toml")).expect("link is created");

    let output = write_garbage(&repo, &repo.main.join("sub/.omhl.toml"));

    marker_reason(&output, &main_marker(&repo));
}

#[test]
fn it_should_allow_a_folder_named_as_the_marker() {
    let repo = Repo::new();
    fs::create_dir_all(repo.main.join("d/.omhl.toml")).expect("folder is created");

    let output = write_garbage(&repo, &repo.main.join("d/.omhl.toml"));

    assert_allowed(&output);
}

#[test]
fn it_should_deny_with_spec_lock_unavailable_when_the_new_content_of_the_marker_cannot_be_computed()
{
    let repo = Repo::new();
    let path = main_marker(&repo);

    let missing_text = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(&path, "absent text", "x", None),
    );
    let no_content = hook_input(&repo, &repo.main, "Write", json!({ "file_path": &path }));

    path_unavailable_reason(&missing_text, &path, MARKER_STEP);
    path_unavailable_reason(&no_content, &path, MARKER_STEP);
}

#[test]
fn it_should_deny_the_marker_without_calling_git_when_git_is_broken_and_hangs() {
    let repo = Repo::new();
    break_git_config(&repo);
    let fake_bin = TempDir::new().expect("fake git directory is created");
    executable_in(fake_bin.path(), "git", "#!/bin/sh\nexec sleep 30\n");
    let path = format!(
        "{}:{}",
        fake_bin.path().display(),
        std::env::var("PATH").expect("PATH is set")
    );
    let started = std::time::Instant::now();

    let output = run_with_path(&repo, path.as_ref(), &main_marker(&repo));

    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the hook took {:?}",
        started.elapsed()
    );
    marker_reason(&output, &main_marker(&repo));
}

#[test]
fn it_should_not_take_a_marker_planted_under_omhl_as_one_to_protect() {
    let repo = Repo::new();
    let planted = repo.main.join(".omhl/.omhl.toml");
    write_file(&planted, VALID_MARKER);

    let on_planted = write_garbage(&repo, &planted);
    let on_real = write_garbage(&repo, &main_marker(&repo));

    assert_allowed(&on_planted);
    marker_reason(&on_real, &main_marker(&repo));
}

#[test]
fn it_should_keep_the_frozen_spec_denial_for_a_marker_in_a_frozen_folder_of_a_custom_specs_dir() {
    let repo = frozen_workflow_repo(Some(MOVED_WORKFLOW));
    let planted = repo.main.join("docs/specs/wi-w1__a/.omhl.toml");
    write_file(&planted, VALID_MARKER);

    let output = write_garbage(&repo, &planted);

    let reason = denied_reason(&output);
    assert!(reason.starts_with("spec-frozen: "), "reason: {reason}");
}

#[test]
fn it_should_deny_an_edit_whose_old_text_only_fits_the_marker_after_the_crlf_is_normalized() {
    let repo = Repo::new();
    let path = main_marker(&repo);
    fs::write(&path, "version = 1\r\ntenant = \"acme\"\r\n").expect("marker is rewritten");

    let output = hook_input(
        &repo,
        &repo.main,
        "Edit",
        edit_input(&path, "version = 1\ntenant", "version = 9\ntenant", None),
    );

    marker_reason(&output, &path);
}
