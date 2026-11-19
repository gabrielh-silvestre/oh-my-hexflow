use crate::support;

use std::path::Path;

use serde_json::{Value, json};
use support::check_run_tree::{BRANCH, W1, copy_index, node_wire, tree_json};
use support::child::{Output as Child, omhl, omhl_json};
use support::git::{GitRepo, git_command_with_env, git_with_env, write_executable};
use support::omhl::work_item_data;
use tempfile::TempDir;

const R5_TREE: &str = "9584cf266afbbe518e9d17df4fe80daa27a993e8";
const R5_COMMIT_TWO_TREE: &str = "ae6c0f89eb17e5e7f4149486e1e2bbfdc92ec714";
const R5_COMMIT_TWO_ATTESTATION: &str =
    "attestations.ae6c0f89eb17e5e7f4149486e1e2bbfd.c92ec714.checks.fmt.work-items.w1";
const R5_HOOK_INDEX_TREE: &str = "2560276e6c83384e10e6f1a3224488091399cea9";
const R5_HOOK_INDEX_ATTESTATION: &str =
    "attestations.2560276e6c83384e10e6f1a322448809.1399cea9.checks.fmt.work-items.w1";
const FMT_TOML: &str = "blocking = true\nhow = \"Always passes.\"\nrun = [\"true\"]\ntimeout_secs = 60\nexpect = [{ exit = 0, outcome = \"passed\" }]\nwhen = { event = \"before-commit\" }\n";

// Binário

/// `R5` com a pasta de dados fora dele, o work item gravado por `omhl append` e o check por
/// `omhl load`.
struct BinaryRepo {
    repo: GitRepo,
    xdg: TempDir,
    home: TempDir,
}

impl BinaryRepo {
    fn new() -> BinaryRepo {
        let repo = GitRepo::init();
        repo.write(".omhl.toml", "version = 1\ntenant = \"acme\"\n");
        repo.write(".omhl/checks/fmt.toml", FMT_TOML);
        repo.write("a.txt", "a\n");
        repo.commit_all("init");
        repo.git(&["switch", "-c", BRANCH]);
        let binary = BinaryRepo {
            repo,
            xdg: TempDir::new().expect("the data directory is created"),
            home: TempDir::new().expect("the home directory is created"),
        };
        binary.seed();

        binary
    }

    fn seed(&self) {
        let appended = self
            .omhl(&["append"])
            .stdin(node_wire(W1, "work-item", &work_item_data("t")).to_string())
            .run();
        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
        let loaded = self.omhl(&["load"]).run();
        assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
        assert_eq!(json_line(&loaded)["created"], json!(["checks.fmt"]));
    }

    fn path(&self) -> &Path {
        self.repo.path()
    }

    fn omhl(&self, args: &[&str]) -> support::child::Cmd {
        omhl_json(args).cwd(self.path()).xdg(self.xdg.path())
    }

    /// `git add <file>` com `GIT_INDEX_FILE` apontando para `index`.
    fn add_to_index(&self, index: &Path, file: &str) {
        let output = git_command_with_env(
            self.path(),
            self.xdg.path(),
            self.home.path(),
            &["add", file],
        )
        .env("GIT_INDEX_FILE", index)
        .output()
        .expect("git runs");
        assert!(
            output.status.success(),
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn get(&self, id: &str) -> Value {
        let fetched = self.omhl(&["get", id]).run();
        assert_eq!(fetched.code, Some(0), "stderr: {}", fetched.stderr);

        json_line(&fetched)["node"].clone()
    }
}

fn json_line(output: &Child) -> Value {
    serde_json::from_str(output.stdout.trim_end()).expect("the stdout line is JSON")
}

// Testes

#[test]
fn it_should_attest_the_tree_of_the_index_file_named_in_the_environment() {
    let binary = BinaryRepo::new();
    let elsewhere = TempDir::new().expect("the hook index directory is created");
    let hook_index = elsewhere.path().join("hook-index");
    copy_index(&binary.repo, &hook_index);
    binary.repo.write("x.txt", "x\n");
    binary.add_to_index(&hook_index, "x.txt");

    let output = binary
        .omhl(&[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-commit",
        ])
        .env("GIT_INDEX_FILE", &hook_index)
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output)["snapshot"],
        tree_json(R5_HOOK_INDEX_TREE)
    );
    assert_eq!(binary.repo.git(&["write-tree"]).trim(), R5_TREE);
    assert_eq!(
        binary.get(R5_HOOK_INDEX_ATTESTATION)["data"]["outcome"],
        json!("passed")
    );
}

#[test]
fn it_should_attest_the_staged_tree_from_a_hand_written_pre_commit_hook() {
    let binary = BinaryRepo::new();
    let hook = format!(
        "#!/bin/sh\nexec \"{}\" check run --event before-commit\n",
        env!("CARGO_BIN_EXE_omhl")
    );
    write_executable(
        &binary.path().join(".git").join("hooks").join("pre-commit"),
        &hook,
    );
    binary.repo.write("a.txt", "a2\n");

    let committed = git_with_env(
        binary.path(),
        binary.xdg.path(),
        binary.home.path(),
        &["commit", "-a", "-m", "two"],
    );

    assert!(
        committed.status.success(),
        "the commit failed: {}",
        String::from_utf8_lossy(&committed.stderr)
    );
    assert_eq!(
        binary.repo.git(&["rev-list", "--count", "HEAD"]).trim(),
        "2"
    );
    assert_eq!(
        binary.repo.git(&["rev-parse", "HEAD^{tree}"]).trim(),
        R5_COMMIT_TWO_TREE
    );
    let attestation = binary.get(R5_COMMIT_TWO_ATTESTATION);
    assert_eq!(attestation["snapshot"], tree_json(R5_COMMIT_TWO_TREE));
    assert_eq!(attestation["data"]["outcome"], json!("passed"));
}

#[test]
fn it_should_resolve_a_relative_git_index_file_against_the_process_directory_and_not_the_project_directory()
 {
    let binary = BinaryRepo::new();
    let elsewhere = TempDir::new().expect("the process directory is created");
    let index = elsewhere.path().join("idx");
    copy_index(&binary.repo, &index);
    binary.repo.write("x.txt", "x\n");
    binary.add_to_index(&index, "x.txt");
    let project_dir = binary.path().to_str().expect("the path is UTF-8");

    let output = omhl(&[
        "--project-dir",
        project_dir,
        "check",
        "run",
        "--format",
        "json",
        "--event",
        "before-commit",
    ])
    .cwd(elsewhere.path())
    .xdg(binary.xdg.path())
    .env("GIT_INDEX_FILE", "idx")
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output)["snapshot"]["id"],
        json!(R5_HOOK_INDEX_TREE)
    );
}
