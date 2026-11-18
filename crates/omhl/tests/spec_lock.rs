mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
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

    json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": cwd,
        "tool_name": tool,
        "tool_input": tool_input,
    })
    .to_string()
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
fn it_should_release_with_an_error_line_when_a_spec_path_needs_git_that_fails() {
    let project = Project::new("acme");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let (output, git_called) = run_with_nothing_openable(&project, &spec);

    assert_released_with_error(&output);
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
    omhl(&["spec", "init", "work-items.w1", "login"])
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

// Erro interno libera

#[test]
fn it_should_release_with_an_error_line_when_the_repository_is_broken() {
    let project = Project::new("acme");
    fs::write(project.dir().join(".git"), "gitdir: /nonexistent/gitdir\n")
        .expect("broken .git file is written");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = run_spec_lock(project.xdg(), &payload(project.dir(), "Edit", &spec));

    assert_released_with_error(&output);
}

#[test]
fn it_should_release_with_an_error_line_when_the_project_is_not_a_git_repository() {
    let project = Project::new("acme");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = run_spec_lock(project.xdg(), &payload(project.dir(), "Edit", &spec));

    assert_released_with_error(&output);
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
