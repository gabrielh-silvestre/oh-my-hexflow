mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output as GitOutput, Stdio};

use serde_json::{Value, json};
use support::child::{Cmd, Output, omhl};
use support::ext::ExtTree;
use support::git::{git_in, git_with_env, write_executable};
use support::omhl::work_item_data;
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "feat/wi-w1__x";
const W2: &str = "feat/wi-w2__y";
const WI_W1: &str = "work-items.w1";
const WI_W2: &str = "work-items.w2";
const SCRIPTS: [&str; 4] = ["pre-commit", "pre-merge-commit", "pre-push", "post-commit"];
const MARK_BYTES: &str = "omhl-develop-mark v1\n";
const BUILD_DIR_HINT: &str = "install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks";
const UNWRITABLE_HINT: &str =
    "the hooks folder of the user cannot be written; check XDG_DATA_HOME and permissions";

// Scripts esperados, escritos à mão a partir do contrato

fn script(p: &str, command: &str) -> String {
    format!(
        "#!/bin/sh\n[ -x \"{p}\" ] || {{ echo \"omhl: binary not found at {p}; run omhl hooks install again\" >&2; exit 1; }}; exec \"{p}\" {command}\n"
    )
}

fn script_command(name: &str) -> &'static str {
    match name {
        "pre-commit" | "pre-merge-commit" => "check run --event before-commit",
        "pre-push" => "check run --event before-pr --pre-push",
        "post-commit" => "hooks post-commit >/dev/null",
        other => panic!("unknown hook script {other}"),
    }
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("the test paths are UTF-8")
}

fn bin() -> PathBuf {
    fs::canonicalize(env!("CARGO_BIN_EXE_omhl")).expect("the omhl binary exists")
}

fn assert_scripts(dir: &Path, binary: &Path) {
    for name in SCRIPTS {
        let bytes = fs::read_to_string(dir.join(name))
            .unwrap_or_else(|error| panic!("cannot read the {name} hook: {error}"));
        assert_eq!(
            bytes,
            script(path_text(binary), script_command(name)),
            "bytes of {name}"
        );
    }
}

// Saídas

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Output) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

fn stdout(output: &GitOutput) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &GitOutput) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// O texto do erro do hook traz o `code` no cabeçalho (`✘ <code> · ...`) ou numa linha de detalhe.
fn has_code(output: &GitOutput, code: &str) -> bool {
    stderr(output).lines().map(str::trim_start).any(|line| {
        line.starts_with(&format!("✘ {code} · "))
            || line.starts_with(&format!("✘ BLOCKED · {code} · "))
            || line == format!("- {code}")
            || line.starts_with(&format!("- {code} at "))
    })
}

fn rev_parse_hooks(dir: &Path) -> String {
    stdout(&git_in(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
    ))
}

fn canonical_line(path: &Path) -> String {
    format!(
        "{}\n",
        fs::canonicalize(path).expect("the folder exists").display()
    )
}

/// Os campos de uma attestation que os testes conferem.
fn summary(item: &Value) -> Value {
    json!({
        "check": item["data"]["check"],
        "work_item": item["data"]["work_item"],
        "outcome": item["data"]["outcome"],
        "snapshot": item["snapshot"],
    })
}

/// Arquivos de `root` (recursivo), sem a pasta `.git` da raiz.
fn work_tree_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("the folder is listed") {
            let path = entry.expect("the entry is read").path();
            if path == root.join(".git") {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();

    found
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .expect("the folder is listed")
        .map(|entry| entry.expect("the entry is read").path())
        .collect();
    found.sort();

    found
}

// Fixture

struct HookRepo {
    project: Project,
    home: TempDir,
    scratch: TempDir,
}

impl HookRepo {
    fn new() -> HookRepo {
        let repo = HookRepo::without_git();
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Test Author"]);
        repo.git(&["config", "user.email", "author@example.com"]);

        repo
    }

    fn without_git() -> HookRepo {
        HookRepo {
            project: Project::new("acme"),
            home: TempDir::new().expect("home directory is created"),
            scratch: TempDir::new().expect("scratch directory is created"),
        }
    }

    fn d(&self) -> &Path {
        self.project.dir()
    }

    fn x(&self) -> &Path {
        self.project.xdg()
    }

    fn h(&self) -> PathBuf {
        self.x().join("omhl").join("hooks")
    }

    fn h_line(&self) -> String {
        format!("{}\n", self.h().display())
    }

    fn scratch(&self) -> &Path {
        self.scratch.path()
    }

    /// Grava o check `slug` da tabela do contrato e roda `omhl load`.
    fn check(&self, slug: &str) -> &HookRepo {
        let scratch = self.scratch().display();
        let (run, event) = match slug {
            "green" => (r#"["true"]"#.to_owned(), "before-commit"),
            "touch" => (format!(r#"["touch", "{scratch}/ran"]"#), "before-commit"),
            "green-pr" => (r#"["true"]"#.to_owned(), "before-pr"),
            "red-pr" => (r#"["false"]"#.to_owned(), "before-pr"),
            "count-pr" => {
                write_executable(
                    &self.scratch().join("count.sh"),
                    &format!("#!/bin/sh\necho run >> \"{scratch}/runs.log\"\n"),
                );
                (format!(r#"["{scratch}/count.sh"]"#), "before-pr")
            }
            other => panic!("unknown check {other}"),
        };
        let toml = format!(
            "blocking = true\nhow = \"Test check\"\nrun = {run}\ntimeout_secs = 10\nexpect = [{{ exit = 0, outcome = \"passed\" }}]\n\n[when]\nevent = \"{event}\"\n"
        );
        ExtTree::at(self.d()).check(slug, &toml);
        self.omhl_ok(&["load"]);

        self
    }

    fn work_items(&self, ids: &[&str]) -> &HookRepo {
        let items: Vec<Value> = ids
            .iter()
            .map(|id| {
                json!({
                    "body": { "kind": "node", "id": id, "type": "work-item", "data": work_item_data("t") }
                })
            })
            .collect();
        let output = self
            .omhl_in(&["append"])
            .stdin(json!({ "items": items }).to_string())
            .run();
        assert_eq!(output.code, Some(0), "append stderr: {}", output.stderr);

        self
    }

    fn seed(&self) -> &HookRepo {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", "init"]);

        self
    }

    fn omhl_in(&self, args: &[&str]) -> Cmd {
        omhl(args)
            .cwd(self.d())
            .xdg(self.x())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
    }

    fn omhl_ok(&self, args: &[&str]) -> Output {
        let output = self.omhl_in(args).run();
        assert_eq!(
            output.code,
            Some(0),
            "omhl {args:?} stderr: {}",
            output.stderr
        );

        output
    }

    fn install(&self) -> Output {
        self.omhl_in(&["hooks", "install"]).run()
    }

    fn install_force(&self) -> Output {
        self.omhl_in(&["hooks", "install", "--force"]).run()
    }

    /// `install()` como passo de preparo: o teste não segue se a instalação não saiu 0.
    fn installed(&self) -> Output {
        let output = self.install();
        assert_eq!(
            output.code,
            Some(0),
            "hooks install stderr: {}",
            output.stderr
        );

        output
    }

    /// `git` sem hook instalado ou que não dispara hook; exige exit 0 e devolve o stdout.
    fn git(&self, args: &[&str]) -> String {
        let output = git_in(self.d(), args);
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            stderr(&output)
        );

        stdout(&output)
    }

    fn git_hooked(&self, args: &[&str]) -> GitOutput {
        git_with_env(self.d(), self.x(), self.home.path(), args)
    }

    fn local_hooks_path(&self) -> GitOutput {
        git_in(self.d(), &["config", "--local", "--get", "core.hooksPath"])
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim_end().to_owned()
    }

    fn head_tree(&self) -> String {
        self.git(&["rev-parse", "HEAD^{tree}"])
            .trim_end()
            .to_owned()
    }

    fn head_parents_fields(&self) -> usize {
        self.git(&["rev-list", "--parents", "-n", "1", "HEAD"])
            .split_whitespace()
            .count()
    }

    fn count(&self, rev: &str) -> String {
        self.git(&["rev-list", "--count", rev])
    }

    /// Commit de preparo (antes do install, por `git_in`): grava `name` com `content` e commita.
    fn commit_before(&self, name: &str, content: &str) {
        fs::write(self.d().join(name), content).expect("the file is written");
        self.git(&["add", name]);
        self.git(&["commit", "-m", name]);
    }

    fn commit_f(&self) -> GitOutput {
        self.commit_f_with(&["-m", "f"])
    }

    fn commit_f_with(&self, args: &[&str]) -> GitOutput {
        fs::write(self.d().join("f.txt"), "f\n").expect("f.txt is written");
        self.git(&["add", "f.txt"]);
        let mut commit = vec!["commit"];
        commit.extend_from_slice(args);

        self.git_hooked(&commit)
    }

    fn copy_bin(&self, rel: &str) -> PathBuf {
        self.copy_bin_to(&self.scratch().join(rel))
    }

    fn copy_bin_to(&self, dst: &Path) -> PathBuf {
        fs::create_dir_all(dst.parent().expect("the copy has a parent folder"))
            .expect("the copy folder is created");
        // cópia por processo filho: `fs::copy` deixaria o arquivo aberto para escrita aqui (ETXTBSY)
        let status = Command::new("cp")
            .arg("-p")
            .arg(bin())
            .arg(dst)
            .status()
            .expect("cp runs");
        assert!(status.success(), "cp of the omhl binary failed");

        fs::canonicalize(dst).expect("the copy exists")
    }

    fn run_copy(&self, copy: &Path, args: &[&str]) -> Output {
        let mut command = Command::new(copy);
        command
            .args(args)
            .current_dir(self.d())
            .env_clear()
            .env("HOME", self.home.path())
            .env("XDG_DATA_HOME", self.x())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::null());
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        let output = command.output().expect("the omhl copy starts");

        Output {
            code: output.status.code(),
            stdout: stdout(&output),
            stderr: stderr(&output),
        }
    }

    fn installed_from(&self, copy: &Path) -> Output {
        let output = self.run_copy(copy, &["hooks", "install"]);
        assert_eq!(
            output.code,
            Some(0),
            "hooks install stderr: {}",
            output.stderr
        );

        output
    }

    fn attestations(&self) -> Vec<Value> {
        let listed = self.omhl_ok(&["list", "--type", "attestation"]);

        json_line(&listed.stdout)["items"]
            .as_array()
            .expect("items is an array")
            .clone()
    }

    fn attestation_summaries(&self) -> Vec<Value> {
        self.attestations().iter().map(summary).collect()
    }

    fn mark(&self) -> PathBuf {
        self.d().join(".git").join("omhl-develop-mark")
    }

    fn put_mark(&self) {
        fs::write(self.mark(), MARK_BYTES).expect("the mark is written");
    }

    fn remote_dir(&self) -> PathBuf {
        self.scratch().join("remote.git")
    }

    fn add_remote(&self) -> &HookRepo {
        let remote = self.remote_dir();
        let created = git_in(self.scratch(), &["init", "--bare", "remote.git"]);
        assert!(created.status.success(), "git init --bare failed");
        self.git(&[
            "remote",
            "add",
            "origin",
            &format!("file://{}", remote.display()),
        ]);

        self
    }

    fn remote_ref(&self, reference: &str) -> GitOutput {
        git_in(
            &self.remote_dir(),
            &["rev-parse", "--verify", "-q", reference],
        )
    }
}

fn install_from_build_copy(rel: &str) -> (HookRepo, PathBuf, Output) {
    let repo = HookRepo::new();
    repo.seed();
    let copy = repo.copy_bin(rel);
    let output = repo.run_copy(&copy, &["hooks", "install"]);

    (repo, copy, output)
}

// Testes

#[test]
fn it_should_install_executable_hooks_in_the_user_folder_and_set_an_absolute_hooks_path() {
    let repo = HookRepo::new();
    repo.seed();

    let output = repo.install();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    for name in SCRIPTS {
        let mode = fs::metadata(repo.h().join(name))
            .unwrap_or_else(|error| panic!("the {name} hook exists: {error}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "mode of {name}");
    }
    assert_scripts(&repo.h(), &bin());
    assert_eq!(stdout(&repo.local_hooks_path()), repo.h_line());
    assert!(repo.h().is_absolute());
    let value = json_line(&output.stdout);
    assert_eq!(value["hooks_path"], json!(path_text(&repo.h())));
    assert_eq!(value["scripts"], json!(SCRIPTS));
}

#[test]
fn it_should_resolve_the_omhl_binary_at_install_time() {
    let repo = HookRepo::new();
    repo.seed();
    let copy = repo.copy_bin("bin/omhl");

    let output = repo.run_copy(&copy, &["hooks", "install"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "hooks_path": path_text(&repo.h()), "omhl": path_text(&copy), "scripts": SCRIPTS })
    );
    assert_scripts(&repo.h(), &copy);
    for name in SCRIPTS {
        let bytes = fs::read_to_string(repo.h().join(name)).expect("the hook is read");
        assert!(
            !bytes.contains(path_text(&bin())),
            "{name} names the test binary"
        );
    }
}

#[test]
fn it_should_be_idempotent() {
    let repo = HookRepo::new();
    repo.seed();

    let first = repo.install();
    let first_bytes: Vec<String> = SCRIPTS
        .iter()
        .map(|name| fs::read_to_string(repo.h().join(name)).unwrap_or_default())
        .collect();
    let second = repo.install();

    let second_bytes: Vec<String> = SCRIPTS
        .iter()
        .map(|name| fs::read_to_string(repo.h().join(name)).unwrap_or_default())
        .collect();
    assert_eq!(
        [first.code, second.code],
        [Some(0); 2],
        "stderr: {}",
        second.stderr
    );
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(
        stdout(&git_in(
            repo.d(),
            &["config", "--local", "--get-all", "core.hooksPath"]
        )),
        repo.h_line()
    );
}

#[test]
fn it_should_refuse_a_different_hooks_path_without_force() {
    let repo = HookRepo::new();
    repo.seed();
    let other = repo.scratch().join("other-hooks");
    repo.git(&["config", "--local", "core.hooksPath", path_text(&other)]);

    let output = repo.install();

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert_eq!(error_code(&output), json!("hooks-path-conflict"));
    assert_eq!(
        stdout(&repo.local_hooks_path()),
        format!("{}\n", other.display())
    );
    assert!(!repo.h().join("pre-commit").exists());
}

#[test]
fn it_should_overwrite_the_hooks_path_with_force() {
    let repo = HookRepo::new();
    repo.seed();
    let other = repo.scratch().join("other-hooks");
    repo.git(&["config", "--local", "core.hooksPath", path_text(&other)]);

    let output = repo.install_force();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(stdout(&repo.local_hooks_path()), repo.h_line());
    for name in SCRIPTS {
        assert!(repo.h().join(name).is_file(), "{name} is installed");
    }
}

#[test]
fn it_should_refuse_to_overwrite_a_foreign_hook_file_without_force() {
    let repo = HookRepo::new();
    repo.seed();
    let foreign = repo.d().join(".git/hooks/pre-commit");
    write_executable(&foreign, "#!/bin/sh\nexit 0\n");

    let output = repo.install();

    assert_eq!(output.code, Some(1));
    assert_eq!(error_code(&output), json!("hooks-existing"));
    assert_eq!(
        fs::read_to_string(&foreign).expect("the foreign hook stays"),
        "#!/bin/sh\nexit 0\n"
    );
    assert_eq!(repo.local_hooks_path().status.code(), Some(1));
}

#[test]
fn it_should_resolve_the_installed_hook_from_a_linked_worktree() {
    let repo = HookRepo::new();
    repo.seed();
    repo.installed();
    let worktree = repo.scratch().join("wt");

    repo.git(&["worktree", "add", "-b", W2, path_text(&worktree)]);

    assert_eq!(rev_parse_hooks(&worktree), canonical_line(&repo.h()));
}

#[test]
fn it_should_report_where_the_hooks_live() {
    let repo = HookRepo::new();
    ExtTree::at(repo.d()).file("sub/.keep", "");
    repo.seed();

    let output = repo.installed();

    assert_eq!(
        json_line(&output.stdout)["hooks_path"],
        json!(path_text(&repo.h()))
    );
    assert_eq!(rev_parse_hooks(repo.d()), canonical_line(&repo.h()));
    assert_eq!(
        rev_parse_hooks(&repo.d().join("sub")),
        canonical_line(&repo.h())
    );
}

#[test]
fn it_should_fail_with_a_clear_error_outside_a_git_repository() {
    let repo = HookRepo::without_git();
    let ceiling = repo.d().parent().expect("the project has a parent folder");

    let output = repo
        .omhl_in(&["hooks", "install"])
        .env("GIT_CEILING_DIRECTORIES", ceiling)
        .run();

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert_eq!(error_code(&output), json!("probe-failed"));
    assert!(!repo.h().join("pre-commit").exists());
}

#[test]
fn it_should_not_touch_the_files_of_the_work_tree() {
    let repo = HookRepo::new();
    repo.seed();
    let hooks_dir = repo.d().join(".git/hooks");
    let files_before = work_tree_files(repo.d());
    let hooks_before = entries(&hooks_dir);

    repo.installed();

    assert_eq!(
        repo.git(&["status", "--porcelain=v1", "--untracked-files=all"]),
        ""
    );
    assert_eq!(work_tree_files(repo.d()), files_before);
    assert_eq!(entries(&hooks_dir), hooks_before);
}

#[test]
fn it_should_refuse_with_hooks_existing_when_the_git_hooks_folder_has_a_non_sample_hook() {
    let repo = HookRepo::new();
    repo.seed();
    write_executable(
        &repo.d().join(".git/hooks/pre-merge-commit"),
        "#!/bin/sh\nexit 0\n",
    );

    let output = repo.install();

    assert_eq!(output.code, Some(1));
    assert_eq!(error_code(&output), json!("hooks-existing"));
    assert_eq!(repo.local_hooks_path().status.code(), Some(1));
}

#[test]
fn it_should_refuse_with_hooks_path_conflict_when_a_global_hooks_path_is_set() {
    let repo = HookRepo::new();
    repo.seed();
    let global = repo.scratch().join("global.gitconfig");
    fs::write(
        &global,
        format!(
            "[core]\n\thooksPath = {}/global-hooks\n",
            repo.scratch().display()
        ),
    )
    .expect("the global config is written");

    let output = repo
        .omhl_in(&["hooks", "install"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .run();

    assert_eq!(output.code, Some(1));
    assert_eq!(error_code(&output), json!("hooks-path-conflict"));
    assert_eq!(repo.local_hooks_path().status.code(), Some(1));
}

#[test]
fn it_should_resolve_the_work_item_from_the_branch_name_through_the_installed_hook() {
    let repo = HookRepo::new();
    repo.check("green").work_items(&[WI_W1]).seed();
    repo.installed();
    repo.git(&["switch", "-c", W1]);

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(0), "stderr: {}", stderr(&commit));
    assert!(repo.attestation_summaries().contains(&json!({
        "check": "checks.green",
        "work_item": WI_W1,
        "outcome": "passed",
        "snapshot": { "kind": "git-tree", "id": repo.head_tree() },
    })));
}

#[test]
fn it_should_refuse_a_branch_outside_the_pattern_with_branch_name_invalid() {
    let repo = HookRepo::new();
    repo.check("green").work_items(&[WI_W1]).seed();
    repo.installed();
    repo.git(&["switch", "-c", "topic"]);

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(1));
    assert!(
        has_code(&commit, "branch-name-invalid"),
        "stderr: {}",
        stderr(&commit)
    );
    assert_eq!(repo.count("topic"), "1\n");
    assert_eq!(repo.attestations(), Vec::<Value>::new());
}

#[test]
fn it_should_refuse_a_commit_on_main_with_branch_protected() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.installed();

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(1));
    assert!(
        has_code(&commit, "branch-protected"),
        "stderr: {}",
        stderr(&commit)
    );
    assert_eq!(repo.count("main"), "1\n");
}

#[test]
fn it_should_refuse_a_detached_head_contained_in_several_branches_with_head_ambiguous() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.git(&["branch", W1]);
    repo.git(&["branch", W2]);
    repo.installed();
    repo.git(&["switch", "--detach"]);
    let head_before = repo.head();

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(1));
    assert!(
        has_code(&commit, "head-ambiguous"),
        "stderr: {}",
        stderr(&commit)
    );
    assert_eq!(repo.head(), head_before);
}

#[test]
fn it_should_run_without_recording_on_develop_and_pass_only_with_the_mark() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.installed();
    repo.git(&["switch", "-c", "develop"]);

    let without_mark = repo.commit_f();
    repo.put_mark();
    let with_mark = repo.git_hooked(&["commit", "-m", "f"]);

    assert_eq!(without_mark.status.code(), Some(1));
    assert!(
        has_code(&without_mark, "develop-mark-missing"),
        "stderr: {}",
        stderr(&without_mark)
    );
    assert_eq!(
        with_mark.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&with_mark)
    );
    assert_eq!(repo.count("develop"), "2\n");
    assert_eq!(repo.attestations(), Vec::<Value>::new());
}

#[test]
fn it_should_explain_in_the_help_that_hooks_install_writes_git_hooks_and_not_claude_code_hooks() {
    let command = omhl(&["hooks", "install", "--help"]);

    let output = command.run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.contains("git hooks"), "{}", output.stdout);
    assert!(output.stdout.contains("Claude Code"), "{}", output.stdout);
    assert!(
        output.stdout.contains("omhl hook session-start"),
        "{}",
        output.stdout
    );
}

#[test]
fn it_should_fail_with_a_clear_message_when_the_installed_binary_is_gone() {
    let repo = HookRepo::new();
    repo.check("green").work_items(&[WI_W1]).seed();
    let copy = repo.copy_bin("bin/omhl");
    repo.installed_from(&copy);
    repo.git(&["switch", "-c", W1]);
    fs::remove_file(&copy).expect("the copy is removed");

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(1));
    assert!(
        stderr(&commit).contains(&format!(
            "omhl: binary not found at {}; run omhl hooks install again",
            copy.display()
        )),
        "stderr: {}",
        stderr(&commit)
    );
    assert_eq!(repo.count(W1), "1\n");
}

#[test]
fn it_should_quote_an_omhl_path_that_contains_spaces() {
    let repo = HookRepo::new();
    repo.check("green").work_items(&[WI_W1]).seed();
    let copy = repo.copy_bin("bin dir/omhl");
    repo.installed_from(&copy);
    repo.git(&["switch", "-c", W1]);

    let commit = repo.commit_f();

    assert_eq!(
        fs::read_to_string(repo.h().join("pre-commit")).expect("the hook is read"),
        script(path_text(&copy), "check run --event before-commit")
    );
    assert_eq!(commit.status.code(), Some(0), "stderr: {}", stderr(&commit));
    assert!(
        repo.attestations()
            .iter()
            .any(|item| item["data"]["work_item"] == json!(WI_W1))
    );
}

#[test]
fn it_should_answer_work_item_closed_for_a_branch_whose_work_item_was_closed_after_many_others() {
    let repo = HookRepo::new();
    let others: Vec<String> = (1..=60).map(|n| format!("work-items.o{n:02}")).collect();
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    repo.work_items(&others[..30]).work_items(&others[30..]);
    repo.work_items(&[WI_W1]).check("green").seed();
    repo.installed();
    repo.git(&["switch", "-c", W1]);
    repo.omhl_in(&["close", WI_W1]).run();

    let commit = repo.commit_f();

    let fetched = repo.omhl_ok(&["get", WI_W1]);
    assert_eq!(
        json_line(&fetched.stdout)["node"]["data"]["status"],
        json!("closed")
    );
    assert_eq!(commit.status.code(), Some(1));
    assert!(
        has_code(&commit, "work-item-closed"),
        "stderr: {}",
        stderr(&commit)
    );
    assert_eq!(repo.attestations(), Vec::<Value>::new());
}

#[test]
fn it_should_bar_a_push_whose_remote_ref_is_main() {
    let repo = HookRepo::new();
    repo.check("green-pr").work_items(&[WI_W1]).seed();
    repo.add_remote();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.installed();

    let push = repo.git_hooked(&["push", "origin", "feat/wi-w1__x:main"]);

    assert_ne!(push.status.code(), Some(0));
    assert!(
        has_code(&push, "branch-protected"),
        "stderr: {}",
        stderr(&push)
    );
    assert!(stderr(&push).contains("push-to-main"));
    assert_eq!(repo.remote_ref("refs/heads/main").status.code(), Some(1));
}

#[test]
fn it_should_refuse_with_hooks_path_unreachable_when_git_resolves_another_hooks_folder_after_the_install()
 {
    let repo = HookRepo::new();
    repo.seed();
    let wt_hooks = repo.scratch().join("wt-hooks");
    repo.git(&["config", "extensions.worktreeConfig", "true"]);
    repo.git(&[
        "config",
        "--worktree",
        "core.hooksPath",
        path_text(&wt_hooks),
    ]);

    let plain = repo.install();
    let forced = repo.install_force();

    assert_eq!(plain.code, Some(1));
    assert_eq!(error_code(&plain), json!("hooks-path-conflict"));
    assert_eq!(forced.code, Some(1));
    assert_eq!(forced.stdout, "");
    assert_eq!(error_code(&forced), json!("hooks-path-unreachable"));
    assert_eq!(
        rev_parse_hooks(repo.d()),
        format!(
            "{}/wt-hooks\n",
            fs::canonicalize(repo.scratch())
                .expect("the scratch folder exists")
                .display()
        )
    );
}

#[test]
fn it_should_skip_the_checks_when_deleting_a_remote_branch() {
    let repo = HookRepo::new();
    repo.check("red-pr").work_items(&[WI_W1, WI_W2]).seed();
    repo.add_remote();
    repo.git(&["switch", "-c", W2]);
    repo.commit_before("g.txt", "g\n");
    repo.git(&["push", "origin", W2]);
    repo.git(&["switch", "-c", W1, "main"]);
    repo.commit_before("h.txt", "h\n");
    repo.installed();

    let push = repo.git_hooked(&["push", "origin", "--delete", "feat/wi-w2__y"]);

    assert_eq!(push.status.code(), Some(0), "stderr: {}", stderr(&push));
    assert_eq!(
        repo.remote_ref("refs/heads/feat/wi-w2__y").status.code(),
        Some(1)
    );
}

#[test]
fn it_should_skip_the_checks_when_pushing_a_tag() {
    let repo = HookRepo::new();
    repo.check("red-pr").work_items(&[WI_W1]).seed();
    repo.add_remote();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.installed();
    repo.git(&["tag", "v1"]);

    let push = repo.git_hooked(&["push", "origin", "v1"]);

    assert_eq!(push.status.code(), Some(0), "stderr: {}", stderr(&push));
    assert_eq!(
        stdout(&repo.remote_ref("refs/tags/v1")),
        repo.git(&["rev-parse", "v1"])
    );
}

#[test]
fn it_should_bar_deleting_main_on_the_remote() {
    let repo = HookRepo::new();
    repo.check("green-pr").work_items(&[WI_W1]).seed();
    let seed_sha = repo.head();
    repo.add_remote();
    repo.git(&["push", "origin", "main"]);
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.installed();

    let push = repo.git_hooked(&["push", "origin", "--delete", "main"]);

    assert_ne!(push.status.code(), Some(0));
    assert!(
        has_code(&push, "branch-protected"),
        "stderr: {}",
        stderr(&push)
    );
    assert!(stderr(&push).contains("push-to-main"));
    assert_eq!(
        stdout(&repo.remote_ref("refs/heads/main")),
        format!("{seed_sha}\n")
    );
}

#[test]
fn it_should_run_before_pr_once_when_pushing_several_refs() {
    let repo = HookRepo::new();
    repo.check("count-pr").work_items(&[WI_W1]).seed();
    repo.add_remote();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.installed();

    let push = repo.git_hooked(&[
        "push",
        "origin",
        "feat/wi-w1__x",
        "feat/wi-w1__x:refs/heads/feat/wi-w1__copy",
    ]);

    let head_line = format!("{}\n", repo.head());
    assert_eq!(push.status.code(), Some(0), "stderr: {}", stderr(&push));
    assert_eq!(
        fs::read_to_string(repo.scratch().join("runs.log")).expect("the check ran"),
        "run\n"
    );
    assert_eq!(
        stdout(&repo.remote_ref("refs/heads/feat/wi-w1__x")),
        head_line
    );
    assert_eq!(
        stdout(&repo.remote_ref("refs/heads/feat/wi-w1__copy")),
        head_line
    );
    assert!(repo.attestation_summaries().contains(&json!({
        "check": "checks.count-pr",
        "work_item": WI_W1,
        "outcome": "passed",
        "snapshot": { "kind": "git-commit", "id": repo.head() },
    })));
}

#[test]
fn it_should_install_the_hooks_in_the_xdg_data_folder() {
    let repo = HookRepo::new();
    repo.seed();

    let output = repo
        .omhl_in(&["hooks", "install"])
        .env("HOME", repo.home.path())
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    for name in SCRIPTS {
        assert!(
            repo.x().join("omhl/hooks").join(name).is_file(),
            "{name} is installed"
        );
    }
    assert!(!repo.home.path().join(".local/share/omhl/hooks").exists());
    assert_eq!(
        json_line(&output.stdout)["hooks_path"],
        json!(path_text(&repo.h()))
    );
}

#[test]
fn it_should_install_the_hooks_in_the_local_share_folder_of_home_when_xdg_is_unset() {
    let repo = HookRepo::new();
    repo.seed();
    let home_hooks = repo.home.path().join(".local/share/omhl/hooks");

    let output = omhl(&["hooks", "install"])
        .cwd(repo.d())
        .env("HOME", repo.home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    for name in SCRIPTS {
        assert!(home_hooks.join(name).is_file(), "{name} is installed");
    }
    assert_eq!(
        stdout(&repo.local_hooks_path()),
        format!("{}\n", home_hooks.display())
    );
    assert_eq!(
        json_line(&output.stdout)["hooks_path"],
        json!(path_text(&home_hooks))
    );
}

#[test]
fn it_should_refuse_with_hooks_dir_unwritable_when_the_user_folder_cannot_be_written() {
    let repo = HookRepo::new();
    repo.seed();
    fs::create_dir_all(repo.x().join("omhl")).expect("the omhl data folder is created");
    fs::write(repo.h(), "not a folder\n").expect("the blocking file is written");

    let output = repo.install();

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stdout, "");
    assert_eq!(error_code(&output), json!("hooks-dir-unwritable"));
    assert!(output.stderr.contains(UNWRITABLE_HINT), "{}", output.stderr);
    assert_eq!(repo.local_hooks_path().status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(repo.h()).expect("the blocking file stays"),
        "not a folder\n"
    );
}

#[test]
fn it_should_refuse_a_develop_commit_without_the_mark_through_the_installed_hook() {
    let repo = HookRepo::new();
    repo.check("touch").seed();
    repo.installed();
    repo.git(&["switch", "-c", "develop"]);

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(1));
    assert!(
        has_code(&commit, "develop-mark-missing"),
        "stderr: {}",
        stderr(&commit)
    );
    assert!(!repo.scratch().join("ran").exists());
    assert_eq!(repo.count("develop"), "1\n");
}

#[test]
fn it_should_not_require_the_develop_mark_on_a_push() {
    let repo = HookRepo::new();
    repo.check("green-pr").seed();
    repo.add_remote();
    repo.git(&["switch", "-c", "develop"]);
    repo.commit_before("g.txt", "g\n");
    repo.installed();

    let push = repo.git_hooked(&["push", "origin", "develop"]);

    assert_eq!(push.status.code(), Some(0), "stderr: {}", stderr(&push));
    assert_eq!(
        stdout(&repo.remote_ref("refs/heads/develop")),
        format!("{}\n", repo.head())
    );
    assert_eq!(repo.attestations(), Vec::<Value>::new());
}

#[test]
fn it_should_refuse_to_push_a_branch_that_is_not_checked_out() {
    let repo = HookRepo::new();
    repo.check("green-pr").work_items(&[WI_W2]).seed();
    repo.add_remote();
    repo.git(&["switch", "-c", W2]);
    repo.commit_before("g.txt", "g\n");
    repo.git(&["switch", "main"]);
    repo.installed();

    let push = repo.git_hooked(&["push", "origin", "feat/wi-w2__y"]);

    assert_ne!(push.status.code(), Some(0));
    assert!(
        has_code(&push, "push-ref-not-checked-out"),
        "stderr: {}",
        stderr(&push)
    );
    assert_eq!(
        repo.remote_ref("refs/heads/feat/wi-w2__y").status.code(),
        Some(1)
    );
}

#[test]
fn it_should_consume_the_develop_mark_in_the_post_commit_hook_after_a_commit_on_develop() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.installed();
    repo.git(&["switch", "-c", "develop"]);
    repo.put_mark();

    let commit = repo.commit_f();

    assert_eq!(commit.status.code(), Some(0), "stderr: {}", stderr(&commit));
    assert!(!repo.mark().exists());
}

#[test]
fn it_should_consume_the_develop_mark_after_a_commit_made_with_no_verify() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.installed();
    repo.git(&["switch", "-c", "develop"]);
    repo.put_mark();

    let commit = repo.commit_f_with(&["--no-verify", "-m", "f"]);

    assert_eq!(commit.status.code(), Some(0), "stderr: {}", stderr(&commit));
    assert!(!repo.mark().exists());
}

#[test]
fn it_should_keep_the_develop_mark_when_the_commit_is_aborted() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.installed();
    repo.git(&["switch", "-c", "develop"]);
    repo.put_mark();

    let commit = repo.commit_f_with(&["-m", ""]);

    assert_eq!(commit.status.code(), Some(1));
    assert!(
        stderr(&commit).contains("Aborting commit due to empty commit message."),
        "stderr: {}",
        stderr(&commit)
    );
    assert_eq!(
        fs::read_to_string(repo.mark()).expect("the mark stays"),
        MARK_BYTES
    );
    assert_eq!(repo.count("develop"), "1\n");
}

#[test]
fn it_should_warn_when_the_omhl_binary_lives_in_a_cargo_target_folder() {
    let cases = ["a/target/debug/omhl", "b/target/release/omhl"];

    let installs: Vec<(HookRepo, PathBuf, Output)> = cases
        .iter()
        .map(|rel| install_from_build_copy(rel))
        .collect();

    for (repo, copy, output) in &installs {
        assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
        assert_eq!(
            json_line(&output.stdout)["warnings"],
            json!([{ "code": "binary-in-build-dir", "path": path_text(copy), "hint": BUILD_DIR_HINT }])
        );
        assert_scripts(&repo.h(), copy);
    }
}

#[test]
fn it_should_warn_when_the_omhl_binary_lives_inside_a_linked_worktree() {
    let repo = HookRepo::new();
    repo.seed();
    let linked = repo.scratch().join("linked");
    repo.git(&["worktree", "add", "-b", "other", path_text(&linked)]);
    let copy = repo.copy_bin_to(&linked.join("plugin/bin/omhl"));

    let output = repo.run_copy(&copy, &["hooks", "install"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout)["warnings"],
        json!([{ "code": "binary-in-worktree", "path": path_text(&copy), "hint": BUILD_DIR_HINT }])
    );
    assert_scripts(&repo.h(), &copy);
}

#[test]
fn it_should_not_warn_when_the_omhl_binary_lives_in_the_main_worktree() {
    let repo = HookRepo::new();
    repo.seed();
    let copy = repo.copy_bin_to(&repo.d().join("plugin/bin/omhl"));

    let output = repo.run_copy(&copy, &["hooks", "install"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["warnings"], json!(null));
    assert_scripts(&repo.h(), &copy);
}

#[test]
fn it_should_install_a_pre_merge_commit_hook_that_bars_a_no_ff_merge_on_main() {
    let repo = HookRepo::new();
    repo.check("green").work_items(&[WI_W1]).seed();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.git(&["switch", "main"]);
    repo.installed();

    let merge = repo.git_hooked(&["merge", "--no-ff", "-m", "m", "feat/wi-w1__x"]);
    let abort = repo.git_hooked(&["merge", "--abort"]);

    assert_eq!(merge.status.code(), Some(1));
    assert!(
        has_code(&merge, "branch-protected"),
        "stderr: {}",
        stderr(&merge)
    );
    assert_eq!(repo.count("main"), "1\n");
    assert_eq!(abort.status.code(), Some(0), "stderr: {}", stderr(&abort));
}

#[test]
fn it_should_let_a_no_ff_merge_outside_main_pass_through_the_pre_merge_commit_hook() {
    let repo = HookRepo::new();
    repo.check("green").work_items(&[WI_W1, WI_W2]).seed();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.git(&["switch", "-c", W2, "main"]);
    repo.installed();

    let merge = repo.git_hooked(&["merge", "--no-ff", "-m", "m", "feat/wi-w1__x"]);

    assert_eq!(merge.status.code(), Some(0), "stderr: {}", stderr(&merge));
    assert_eq!(repo.head_parents_fields(), 3);
    assert!(repo.attestation_summaries().contains(&json!({
        "check": "checks.green",
        "work_item": WI_W2,
        "outcome": "passed",
        "snapshot": { "kind": "git-tree", "id": repo.head_tree() },
    })));
}

#[test]
fn it_should_require_the_develop_mark_for_a_clean_merge_on_develop_and_keep_it_afterwards() {
    let repo = HookRepo::new();
    repo.check("green").seed();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("g.txt", "g\n");
    repo.git(&["switch", "-c", "develop", "main"]);
    repo.installed();

    let without_mark = repo.git_hooked(&["merge", "--no-ff", "-m", "m", "feat/wi-w1__x"]);
    repo.git_hooked(&["merge", "--abort"]);
    repo.put_mark();
    let with_mark = repo.git_hooked(&["merge", "--no-ff", "-m", "m", "feat/wi-w1__x"]);

    assert_eq!(without_mark.status.code(), Some(1));
    assert!(
        has_code(&without_mark, "develop-mark-missing"),
        "stderr: {}",
        stderr(&without_mark)
    );
    assert_eq!(
        with_mark.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&with_mark)
    );
    assert_eq!(repo.head_parents_fields(), 3);
    assert_eq!(
        fs::read_to_string(repo.mark()).expect("the mark stays"),
        MARK_BYTES
    );
}

#[test]
fn it_should_spend_the_develop_mark_when_a_conflicted_merge_on_develop_is_closed_by_commit() {
    let repo = HookRepo::new();
    repo.check("green");
    fs::write(repo.d().join("a.txt"), "line\n").expect("a.txt is written");
    repo.seed();
    repo.git(&["switch", "-c", W1]);
    repo.commit_before("a.txt", "theirs\n");
    repo.git(&["switch", "-c", "develop", "main"]);
    repo.commit_before("a.txt", "ours\n");
    repo.installed();
    repo.put_mark();

    let merge = repo.git_hooked(&["merge", "--no-ff", "-m", "m", "feat/wi-w1__x"]);
    fs::write(repo.d().join("a.txt"), "resolved\n").expect("a.txt is resolved");
    repo.git(&["add", "a.txt"]);
    let commit = repo.git_hooked(&["commit", "--no-edit"]);

    assert_eq!(merge.status.code(), Some(1));
    assert!(
        stdout(&merge).contains("CONFLICT (content): Merge conflict in a.txt"),
        "stdout: {}",
        stdout(&merge)
    );
    assert_eq!(commit.status.code(), Some(0), "stderr: {}", stderr(&commit));
    assert_eq!(repo.head_parents_fields(), 3);
    assert!(!repo.mark().exists());
}
