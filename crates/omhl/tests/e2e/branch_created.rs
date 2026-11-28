use crate::support;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::git::git_in;
use support::project::{DEVELOP_WORKFLOW, Project};
use support::repo_checks::repo_root;
use tempfile::TempDir;

const BRANCH: &str = "feat/wi-w1__login";
const BILLING: &str = "wi-w1__billing";
const LOGIN: &str = "wi-w1__login";
const MARK: &str = ".frozen";
const PART: &str = "parte-01.md";
const SPECS: &str = ".omhl/specs";

// Comando `omhl hook branch-created` sobre repositório git real

/// Repositório git real (branch `main`, marcador) com a pasta de dados fora dele.
struct Repo {
    project: Project,
}

impl Repo {
    fn new() -> Repo {
        let project = Project::new("acme");
        let init = git_in(project.dir(), &["init", "-q", "-b", "main"]);
        assert!(init.status.success(), "git init failed");

        Repo { project }
    }

    fn folder(&self, name: &str) -> PathBuf {
        self.project.dir().join(SPECS).join(name)
    }

    /// Cria a pasta de spec com a primeira parte.
    fn seed(&self, name: &str) {
        let folder = self.folder(name);
        fs::create_dir_all(&folder).expect("spec folder is created");
        fs::write(folder.join(PART), "seed\n").expect("spec part is written");
    }

    fn branch_created(&self, branch: &str) -> Output {
        omhl(&["hook", "branch-created", "--branch", branch])
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .run()
    }
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn assert_success(output: &Output) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_mark_every_folder_of_the_work_item_of_the_branch() {
    let repo = Repo::new();
    repo.seed(LOGIN);
    repo.seed(BILLING);

    let output = repo.branch_created(BRANCH);

    assert_success(&output);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [BILLING, LOGIN], "already": [] })
    );
    assert!(repo.folder(LOGIN).join(MARK).is_file());
    assert!(repo.folder(BILLING).join(MARK).is_file());
}

#[test]
fn it_should_report_already_on_the_second_run() {
    let repo = Repo::new();
    repo.seed(LOGIN);
    repo.branch_created(BRANCH);

    let output = repo.branch_created(BRANCH);

    assert_success(&output);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [], "already": [LOGIN] })
    );
}

#[test]
fn it_should_do_nothing_for_a_branch_that_is_not_a_work_item_branch() {
    let repo = Repo::new();
    repo.seed(LOGIN);

    let output = repo.branch_created("feat/login");

    assert_success(&output);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [], "already": [] })
    );
    assert!(!repo.folder(LOGIN).join(MARK).exists());
}

#[test]
fn it_should_do_nothing_for_develop_and_main() {
    let repo = Repo::new();
    repo.project.write_workflow(DEVELOP_WORKFLOW);
    repo.seed(LOGIN);

    let outputs = ["develop", "main"].map(|branch| repo.branch_created(branch));

    for output in &outputs {
        assert_success(output);
        assert_eq!(
            json_line(&output.stdout),
            json!({ "marked": [], "already": [] })
        );
    }
    assert!(!repo.folder(LOGIN).join(MARK).exists());
}

#[test]
fn it_should_be_silent_without_a_project_marker() {
    let bare = TempDir::new().expect("bare directory is created");

    let output = omhl(&["hook", "branch-created", "--branch", BRANCH])
        .cwd(bare.path())
        .run();

    assert_success(&output);
    assert_eq!(output.stdout, "");
    assert_eq!(output.stderr, "");
}

#[test]
fn it_should_not_read_stdin() {
    let repo = Repo::new();
    repo.seed(LOGIN);

    let output = omhl(&["hook", "branch-created", "--branch", BRANCH])
        .cwd(repo.project.dir())
        .xdg(repo.project.xdg())
        .stdin("{ not json")
        .run();

    assert_success(&output);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [LOGIN], "already": [] })
    );
}

#[test]
fn it_should_refuse_a_symlinked_spec_folder() {
    let repo = Repo::new();
    let real = repo.project.dir().join("real-folder");
    fs::create_dir_all(&real).expect("real folder is created");
    fs::create_dir_all(repo.project.dir().join(SPECS)).expect("specs dir is created");
    symlink(&real, repo.folder(LOGIN)).expect("symlink is created");

    let output = repo.branch_created(BRANCH);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("spec-store-unusable")
    );
    assert!(!real.join(MARK).exists());
}

#[test]
fn it_should_mark_the_real_folder_when_a_regular_file_shares_the_prefix() {
    let repo = Repo::new();
    repo.seed(LOGIN);
    fs::write(repo.folder("wi-w1__0"), "").expect("file is planted");

    let output = repo.branch_created(BRANCH);

    assert_success(&output);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "marked": [LOGIN], "already": [] })
    );
    assert!(repo.folder(LOGIN).join(MARK).is_file());
}

#[test]
fn it_should_mark_the_other_folders_before_refusing_a_symlinked_one() {
    let repo = Repo::new();
    repo.seed(LOGIN);
    let real = repo.project.dir().join("real-folder");
    fs::create_dir_all(&real).expect("real folder is created");
    symlink(&real, repo.folder("wi-w1__0")).expect("symlink is created");

    let output = repo.branch_created(BRANCH);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(repo.folder(LOGIN).join(MARK).is_file());
    assert!(!real.join(MARK).exists());
}

#[test]
fn it_should_leave_the_spec_files_byte_identical() {
    let repo = Repo::new();
    repo.seed(LOGIN);
    let part = repo.folder(LOGIN).join(PART);
    let before = fs::read(&part).expect("spec part is read");

    let output = repo.branch_created(BRANCH);

    assert_success(&output);
    assert_eq!(fs::read(&part).expect("spec part is read"), before);
}

#[test]
fn it_should_mark_a_work_item_without_a_database_record() {
    let repo = Repo::new();
    repo.seed(LOGIN);

    let output = repo.branch_created(BRANCH);

    assert_success(&output);
    assert!(repo.folder(LOGIN).join(MARK).is_file());
    assert!(!repo.project.db_path().exists());
}

// O nome antigo some sem apelido (CMP-039, CMP-040, EMD-017, EMD-018)

const OLD_NAME: &str = "wt-post-start";
const HOOKS: [&str; 9] = [
    "session-start",
    "spec-lock",
    "agent-role",
    "subagent-start",
    "branch-birth",
    "command-guard",
    "mcp-merge-guard",
    "mcp-write-warning",
    "branch-created",
];

#[test]
fn it_should_refuse_the_old_hook_name_on_the_command_line() {
    let bare = TempDir::new().expect("bare directory is created");

    let output = omhl(&["hook", OLD_NAME, "--branch", BRANCH])
        .cwd(bare.path())
        .run();

    assert_ne!(output.code, Some(0), "stdout: {}", output.stdout);
}

#[test]
fn it_should_leave_the_old_name_out_of_the_hook_group_help() {
    let output = omhl(&["hook", "--help"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!output.stdout.contains(OLD_NAME), "{}", output.stdout);
}

#[test]
fn it_should_list_every_hook_of_the_group_in_its_help() {
    let output = omhl(&["hook", "--help"]).run();

    let missing: Vec<&str> = HOOKS
        .into_iter()
        .filter(|hook| !output.stdout.contains(hook))
        .collect();
    assert!(
        missing.is_empty(),
        "missing {missing:?} in {}",
        output.stdout
    );
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("the folder is read") {
        let path = entry.expect("the entry is read").path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }

    found
}

#[test]
fn it_should_leave_the_old_name_only_in_this_test_under_crates_and_plugin() {
    let root = repo_root();
    let pattern = Regex::new("wt-post-start|WtPostStart|wt_post_start").expect("valid pattern");

    let found: Vec<String> = ["crates", "plugin"]
        .iter()
        .flat_map(|dir| files_under(&root.join(dir)))
        .filter(|path| {
            fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .any(|line| pattern.is_match(line))
        })
        .map(|path| {
            path.strip_prefix(&root)
                .expect("under the root")
                .display()
                .to_string()
        })
        .collect();

    assert_eq!(found, ["crates/omhl/tests/e2e/branch_created.rs"]);
}

/// O argv do `omhl` no `post-start` do `.config/wt.toml`, com `{{ branch }}` trocado por `branch`.
fn post_start_argv(branch: &str) -> Vec<String> {
    let text = fs::read_to_string(repo_root().join(".config/wt.toml")).expect("wt.toml is read");
    let config: toml::Value = toml::from_str(&text).expect("wt.toml is TOML");
    let template = config["post-start"]["omhl-freeze"]
        .as_str()
        .expect("the hook is a string");
    let command = template
        .rsplit("; ")
        .next()
        .expect("the template ends with the omhl command");

    command
        .replace("{{ branch }}", branch)
        .split_whitespace()
        .skip(1)
        .map(str::to_owned)
        .collect()
}

#[test]
fn it_should_accept_the_post_start_command_of_the_repository_wt_config() {
    let bare = TempDir::new().expect("bare directory is created");
    let argv = post_start_argv(BRANCH);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let output = omhl(&args).cwd(bare.path()).run();

    assert_eq!(args[..2], ["hook", "branch-created"]);
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}
