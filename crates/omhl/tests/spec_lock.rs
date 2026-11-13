mod support;

use std::fs;
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

mod lock {
    use std::path::Path;

    use ::omhl::workflow::lock::{
        Facts, SpecPath, Tool, Verdict, decide, next_amendment, parse_spec_path,
    };

    use super::{CUSTOM_SPECS, DEFAULT_SPECS, FOLDER, FROZEN, IN_WORKTREE};

    const MAIN_FOLDER: &str = "/main/.omhl/specs/wi-w1__login";

    // Camada pura: `omhl::workflow::lock::{parse_spec_path, next_amendment, decide, Tool}`

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn spec_path(work_item: &str, folder: &str, file: &str) -> SpecPath {
        SpecPath {
            work_item: work_item.to_owned(),
            folder: folder.to_owned(),
            file: file.to_owned(),
        }
    }

    /// O work item é sempre `w1` e a pasta `wi-w1__login`; `existing` lista a pasta, e o alvo existe
    /// quando está nela.
    fn verdict(tool: Tool, file: &str, existing: &[&str], frozen: bool, linked: bool) -> Verdict {
        let files = strings(existing);
        let facts = Facts {
            frozen,
            linked_worktree: linked,
            file_exists: existing.contains(&file),
            folder_files: &files,
            main_folder: Path::new(MAIN_FOLDER),
        };

        decide(tool, &spec_path("w1", FOLDER, file), &facts)
    }

    fn denial(verdict: Verdict) -> (&'static str, String) {
        match verdict {
            Verdict::Deny { code, reason } => (code, reason),
            Verdict::Allow => panic!("expected a denial, got Allow"),
        }
    }

    #[test]
    fn it_should_parse_the_work_item_folder_and_file_of_a_path_under_the_default_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_parse_a_path_in_a_linked_worktree_the_same_way() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/scratch/wt/.omhl/specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_parse_a_path_under_a_custom_specs_dir() {
        let parsed = parse_spec_path(
            CUSTOM_SPECS,
            Path::new("/repo/docs/team-specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_not_parse_the_default_dir_when_the_specs_dir_is_customized() {
        let parsed = parse_spec_path(
            CUSTOM_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_path_outside_the_specs_dir() {
        let parsed = parse_spec_path(DEFAULT_SPECS, Path::new("/repo/src/main.rs"));

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_work_item_folder_outside_the_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/docs/wi-w1__login/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_file_directly_under_the_specs_dir() {
        let parsed = parse_spec_path(DEFAULT_SPECS, Path::new("/repo/.omhl/specs/notes.md"));

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_without_the_work_item_segment() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/drafts/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_without_the_slug_separator() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_with_an_empty_slug() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_a_folder_whose_id_is_not_a_valid_name() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-W1__login/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_not_parse_the_folder_path_itself() {
        let parsed = parse_spec_path(DEFAULT_SPECS, Path::new("/repo/.omhl/specs/wi-w1__login"));

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_parse_a_path_that_climbs_out_and_back_into_the_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/../specs/wi-w1__login/./parte-01.md"),
        );

        assert_eq!(parsed, Some(spec_path("w1", FOLDER, "parte-01.md")));
    }

    #[test]
    fn it_should_not_parse_a_path_that_climbs_out_of_the_specs_dir() {
        let parsed = parse_spec_path(
            DEFAULT_SPECS,
            Path::new("/repo/.omhl/specs/wi-w1__login/../../../src/parte-01.md"),
        );

        assert_eq!(parsed, None);
    }

    #[test]
    fn it_should_name_the_first_amendment_when_the_folder_has_none() {
        let next = next_amendment(&strings(&["parte-01.md", "parte-02.md"]));

        assert_eq!(next, "emenda-01.md");
    }

    #[test]
    fn it_should_name_the_first_amendment_for_an_empty_folder() {
        let next = next_amendment(&[]);

        assert_eq!(next, "emenda-01.md");
    }

    #[test]
    fn it_should_name_the_amendment_after_the_only_one() {
        let next = next_amendment(&strings(&["parte-01.md", "emenda-01.md"]));

        assert_eq!(next, "emenda-02.md");
    }

    #[test]
    fn it_should_name_the_amendment_after_the_highest_number_even_with_a_gap() {
        let next = next_amendment(&strings(&["emenda-01.md", "emenda-03.md"]));

        assert_eq!(next, "emenda-04.md");
    }

    #[test]
    fn it_should_name_the_next_amendment_whatever_the_listing_order() {
        let next = next_amendment(&strings(&["emenda-02.md", "emenda-01.md"]));

        assert_eq!(next, "emenda-03.md");
    }

    #[test]
    fn it_should_ignore_files_that_are_not_two_digit_amendments_when_numbering() {
        let files = strings(&[
            "emenda-1.md",
            "emenda-x1.md",
            "emenda-07.txt",
            "emenda-.md",
            "notes.md",
        ]);

        let next = next_amendment(&files);

        assert_eq!(next, "emenda-01.md");
    }

    #[test]
    fn it_should_parse_only_the_three_file_editing_tools() {
        let parsed: Vec<Option<Tool>> = ["Write", "Edit", "MultiEdit", "Read", "Bash", "write"]
            .into_iter()
            .map(Tool::parse)
            .collect();

        assert_eq!(
            parsed,
            vec![
                Some(Tool::Write),
                Some(Tool::Edit),
                Some(Tool::MultiEdit),
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn it_should_allow_any_tool_when_the_work_item_has_no_branch() {
        let verdicts: Vec<Verdict> = [Tool::Write, Tool::Edit, Tool::MultiEdit]
            .into_iter()
            .map(|tool| verdict(tool, "parte-01.md", &["parte-01.md"], false, false))
            .collect();

        assert_eq!(verdicts, vec![Verdict::Allow; 3]);
    }

    #[test]
    fn it_should_deny_edit_on_an_existing_file_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_multi_edit_on_an_existing_file_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::MultiEdit,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_write_over_an_existing_file_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_a_new_part_when_the_work_item_has_a_branch() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "parte-02.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_allow_the_first_amendment_when_the_work_item_has_a_branch() {
        let verdict = verdict(Tool::Write, "emenda-01.md", &["parte-01.md"], true, false);

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_allow_the_amendment_after_the_highest_existing_one() {
        let verdict = verdict(
            Tool::Write,
            "emenda-04.md",
            &["parte-01.md", "emenda-01.md", "emenda-03.md"],
            true,
            false,
        );

        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn it_should_deny_an_amendment_that_skips_a_number() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-03.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_an_amendment_that_reuses_a_lower_number() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-02.md",
            &["parte-01.md", "emenda-01.md", "emenda-03.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_write_over_an_existing_amendment() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-01.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_edit_on_an_existing_amendment() {
        let (code, _) = denial(verdict(
            Tool::Edit,
            "emenda-01.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_deny_a_new_amendment_whose_number_has_one_digit() {
        let (code, _) = denial(verdict(
            Tool::Write,
            "emenda-1.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert_eq!(code, FROZEN);
    }

    #[test]
    fn it_should_put_the_main_folder_and_the_next_amendment_path_in_the_frozen_reason() {
        let (_, reason) = denial(verdict(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md", "emenda-01.md"],
            true,
            false,
        ));

        assert!(reason.contains(MAIN_FOLDER), "reason: {reason}");
        assert!(
            reason.contains(&format!("{MAIN_FOLDER}/emenda-02.md")),
            "reason: {reason}"
        );
    }

    #[test]
    fn it_should_put_the_code_in_the_frozen_reason() {
        let (_, reason) = denial(verdict(
            Tool::Edit,
            "parte-01.md",
            &["parte-01.md"],
            true,
            false,
        ));

        assert!(reason.contains(FROZEN), "reason: {reason}");
    }

    #[test]
    fn it_should_deny_a_spec_in_a_linked_worktree_even_without_a_branch() {
        let (code, _) = denial(verdict(Tool::Write, "parte-01.md", &[], false, true));

        assert_eq!(code, IN_WORKTREE);
    }

    #[test]
    fn it_should_point_to_the_main_folder_when_denying_a_spec_in_a_linked_worktree() {
        let (_, reason) = denial(verdict(Tool::Write, "parte-01.md", &[], false, true));

        assert!(reason.contains(MAIN_FOLDER), "reason: {reason}");
        assert!(reason.contains(IN_WORKTREE), "reason: {reason}");
    }
}

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

// Fora de `specs.dir`: nada é aberto

/// Projeto sem git, `--db` sob um arquivo comum (não pode nascer) e `PATH` vazio (nenhum `git`
/// pode ser executado): o que o hook abrir aparece como erro no stderr.
fn run_with_nothing_openable(project: &Project, file_path: &Path) -> Child {
    let scratch = TempDir::new().expect("scratch directory is created");
    let blocker = scratch.path().join("blocker");
    fs::write(&blocker, "not a directory").expect("blocker file is written");
    let db = blocker.join("omhl.db");
    let empty_path = scratch.path().join("empty-path");
    fs::create_dir(&empty_path).expect("empty PATH directory is created");
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["--db", &db.to_string_lossy(), "hook", "spec-lock"])
        .cwd(elsewhere.path())
        .xdg(project.xdg())
        .env("PATH", &empty_path)
        .stdin(payload(project.dir(), "Write", file_path))
        .run()
}

#[test]
fn it_should_open_nothing_for_a_path_outside_the_specs_dir() {
    let project = Project::new("acme");

    let output = run_with_nothing_openable(&project, &project.dir().join("src/main.rs"));

    assert_allowed(&output);
}

#[test]
fn it_should_open_nothing_for_a_work_item_folder_outside_the_specs_dir() {
    let project = Project::new("acme");

    let output = run_with_nothing_openable(
        &project,
        &project.dir().join("docs").join(FOLDER).join("parte-01.md"),
    );

    assert_allowed(&output);
}

#[test]
fn it_should_open_nothing_for_a_path_under_the_specs_dir_without_a_work_item_segment() {
    let project = Project::new("acme");

    let output = run_with_nothing_openable(&project, &project.dir().join(".omhl/specs/notes.md"));

    assert_allowed(&output);
}

#[test]
fn it_should_release_with_an_error_line_when_a_spec_path_needs_git_that_cannot_run() {
    let project = Project::new("acme");
    let spec = project
        .dir()
        .join(DEFAULT_SPECS)
        .join(FOLDER)
        .join("parte-01.md");

    let output = run_with_nothing_openable(&project, &spec);

    assert_released_with_error(&output);
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
