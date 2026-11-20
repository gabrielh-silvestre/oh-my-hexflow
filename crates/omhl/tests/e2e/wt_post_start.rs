use crate::support;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::git::git_in;
use support::project::Project;
use tempfile::TempDir;

const BRANCH: &str = "feat/wi-w1__login";
const BILLING: &str = "wi-w1__billing";
const LOGIN: &str = "wi-w1__login";
const MARK: &str = ".frozen";
const PART: &str = "parte-01.md";
const SPECS: &str = ".omhl/specs";

// Comando `omhl hook wt-post-start` sobre repositório git real

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

    fn wt_post_start(&self, branch: &str) -> Output {
        omhl(&["hook", "wt-post-start", "--branch", branch])
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

    let output = repo.wt_post_start(BRANCH);

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
    repo.wt_post_start(BRANCH);

    let output = repo.wt_post_start(BRANCH);

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

    let output = repo.wt_post_start("feat/login");

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
    repo.seed(LOGIN);

    let outputs = ["develop", "main"].map(|branch| repo.wt_post_start(branch));

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

    let output = omhl(&["hook", "wt-post-start", "--branch", BRANCH])
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

    let output = omhl(&["hook", "wt-post-start", "--branch", BRANCH])
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

    let output = repo.wt_post_start(BRANCH);

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

    let output = repo.wt_post_start(BRANCH);

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

    let output = repo.wt_post_start(BRANCH);

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

    let output = repo.wt_post_start(BRANCH);

    assert_success(&output);
    assert_eq!(fs::read(&part).expect("spec part is read"), before);
}

#[test]
fn it_should_mark_a_work_item_without_a_database_record() {
    let repo = Repo::new();
    repo.seed(LOGIN);

    let output = repo.wt_post_start(BRANCH);

    assert_success(&output);
    assert!(repo.folder(LOGIN).join(MARK).is_file());
    assert!(!repo.project.db_path().exists());
}
