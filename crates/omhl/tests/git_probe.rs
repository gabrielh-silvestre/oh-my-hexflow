mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use ::omhl::ports::git::GitProbe;
use ::omhl::probe::{DevelopMark, HeadFacts, ProbeError, ProbeOutcome, SnapshotProbe, TreeSource};
use hexlog::{Snapshot, SnapshotKind};
use support::git::{GitRepo, fake_program, git_command_with_env, git_in};
use tempfile::TempDir;

const BASE_TREE: &str = "f4b354863caa9cea99b95422c9dab70465757d87";
const A2_TREE: &str = "52f1df20d82674a56a52d430e0fb60413718a7af";
const IGNORED_TREE: &str = "c9f3fad2228f19ec0456dbf08e6494b22b1bc58f";
const ALT_INDEX_TREE: &str = "c1af2f9a5066cabd6fbdb33dc829668ebe416790";
const MARK_LINE: &str = "omhl-develop-mark v1";
const FEATURE: &str = "feat/wi-w1__x";

fn probe_dir(dir: &std::path::Path) -> Result<ProbeOutcome, ProbeError> {
    GitProbe::new(dir).probe()
}

fn reproducible(sha: &str) -> Result<ProbeOutcome, ProbeError> {
    Ok(ProbeOutcome::Reproducible(
        Snapshot::new(SnapshotKind::GitCommit, sha).expect("a valid commit id"),
    ))
}

fn error_message(result: Result<ProbeOutcome, ProbeError>) -> String {
    result.expect_err("the probe is expected to fail").message
}

fn clean_repo() -> (GitRepo, String) {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    let head = repo.commit_all("initial");
    (repo, head)
}

fn tree(hex: &str) -> Result<ProbeOutcome, ProbeError> {
    Ok(ProbeOutcome::Reproducible(
        Snapshot::new(SnapshotKind::GitTree, hex).expect("a valid tree id"),
    ))
}

fn index(probe: &GitProbe) -> Result<ProbeOutcome, ProbeError> {
    probe.probe_tree(TreeSource::Index)
}

fn head(probe: &GitProbe) -> Result<ProbeOutcome, ProbeError> {
    probe.probe_tree(TreeSource::Head)
}

/// Roda o `git` em `dir`, exige exit 0 e devolve o stdout sem o `\n` final.
fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = git_in(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_owned()
}

fn base() -> GitRepo {
    let repo = GitRepo::init();
    repo.write("a.txt", "a\n");
    repo.write("b.txt", "b\n");
    repo.commit_all("base");
    repo
}

fn base_on_feature() -> GitRepo {
    let repo = base();
    repo.git(&["switch", "-q", "-c", FEATURE]);
    repo
}

/// `base` mais `.gitignore` commitado e `x.log` ignorado na working tree.
fn ignored() -> GitRepo {
    let repo = base();
    repo.write(".gitignore", "*.log\n");
    repo.commit_all("ignore logs");
    repo.write("x.log", "l\n");
    repo
}

/// `ignored` mais um índice alternativo (com `x.log` em stage) que o índice real não tem.
fn ignored_with_alt_index() -> (GitRepo, TempDir) {
    let repo = ignored();
    repo.git(&["add", "-f", "x.log"]);
    let altdir = TempDir::new().expect("temp dir");
    fs::copy(
        repo.path().join(".git/index"),
        altdir.path().join("alt-index"),
    )
    .expect("copy the index");
    repo.git(&["rm", "-q", "--cached", "x.log"]);
    (repo, altdir)
}

fn add_worktree(repo: &GitRepo) -> (TempDir, PathBuf) {
    let parent = TempDir::new().expect("temp dir");
    let wt = parent.path().join("wt");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        FEATURE,
        &wt.to_string_lossy(),
    ]);
    (parent, wt)
}

fn commit_file(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).expect("write file");
    git_out(dir, &["add", "-A"]);
    git_out(dir, &["commit", "-q", "-m", name]);
}

/// Para no `edit` do primeiro commit do `rebase -i main`, com o rebase em andamento.
fn rebase_stopped_at_edit(dir: &Path) {
    git_out(
        dir,
        &[
            "-c",
            "sequence.editor=sed -i 's/^pick/edit/'",
            "rebase",
            "-i",
            "main",
        ],
    );
}

fn head_oid(dir: &Path) -> String {
    git_out(dir, &["rev-parse", "HEAD"])
}

/// O que o hook `pre-commit` do tracer viu.
struct HookView {
    index: String,
    pwd: String,
    tree: String,
}

fn wait_for(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

fn probe_in_hook(root: &Path, view: &HookView) -> GitProbe {
    GitProbe::new(root).with_index_file(Some(Path::new(&view.index)), Path::new(&view.pwd))
}

/// Técnica do tracer: instala um `pre-commit` que grava o ambiente e espera, lança o commit sem
/// esperar, chama `inspect` com o hook parado e só então deixa o commit terminar (exit 0).
fn during_commit<T>(
    repo: &Path,
    commit_dir: &Path,
    args: &[&str],
    inspect: impl FnOnce(&HookView) -> T,
) -> (T, HookView) {
    let hooks = TempDir::new().expect("temp dir");
    let reg = TempDir::new().expect("temp dir");
    let xdg = TempDir::new().expect("temp dir");
    let home = TempDir::new().expect("temp dir");
    let reg_path = reg.path().display();
    let script = format!(
        "#!/bin/sh\n\
         printf '%s' \"$GIT_INDEX_FILE\" > '{reg_path}/index'\n\
         pwd > '{reg_path}/pwd'\n\
         git write-tree > '{reg_path}/tree'\n\
         : > '{reg_path}/ready'\n\
         i=0\n\
         while [ ! -e '{reg_path}/go' ] && [ \"$i\" -lt 300 ]; do sleep 0.1; i=$((i+1)); done\n"
    );
    let hook = hooks.path().join("pre-commit");
    fs::write(&hook, script).expect("write the hook");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("make it executable");
    git_out(
        repo,
        &["config", "core.hooksPath", &hooks.path().to_string_lossy()],
    );
    let mut child = git_command_with_env(commit_dir, xdg.path(), home.path(), args)
        .spawn()
        .expect("spawn git commit");
    if !wait_for(&reg.path().join("ready")) {
        child.kill().expect("kill the stuck commit");
        panic!("the pre-commit hook never signalled ready");
    }
    let read = |name: &str| fs::read_to_string(reg.path().join(name)).expect("read the registry");
    let view = HookView {
        index: read("index"),
        pwd: read("pwd").trim_end_matches('\n').to_owned(),
        tree: read("tree").trim_end_matches('\n').to_owned(),
    };
    let seen = inspect(&view);
    fs::write(reg.path().join("go"), "").expect("release the hook");
    let output = child.wait_with_output().expect("wait for the commit");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (seen, view)
}

/// Um commit de `a.txt` = "a2\n" com o hook parado; `root` é onde o commit roda.
struct CommitRun {
    root: PathBuf,
    outcome: Result<ProbeOutcome, ProbeError>,
    view: HookView,
    _repo: GitRepo,
    _worktree_parent: Option<TempDir>,
}

fn run_commit(stage: bool, in_worktree: bool, args: &[&str]) -> CommitRun {
    let repo = base();
    let (root, worktree_parent) = if in_worktree {
        let (parent, wt) = add_worktree(&repo);
        (wt, Some(parent))
    } else {
        (repo.path().to_path_buf(), None)
    };
    fs::write(root.join("a.txt"), "a2\n").expect("write file");
    if stage {
        git_out(&root, &["add", "a.txt"]);
    }
    let (outcome, view) = during_commit(repo.path(), &root, args, |view| {
        index(&probe_in_hook(&root, view))
    });
    CommitRun {
        root,
        outcome,
        view,
        _repo: repo,
        _worktree_parent: worktree_parent,
    }
}

fn canonical(path: impl AsRef<Path>) -> PathBuf {
    fs::canonicalize(path).expect("canonicalize")
}

/// `a.txt` e `b.txt` modificados sem `add`, commit `-- a.txt` com o hook parado.
fn during_path_only_commit_with_b_modified<T>(inspect: impl FnOnce(&GitProbe) -> T) -> T {
    let repo = base();
    repo.write("a.txt", "a2\n");
    repo.write("b.txt", "b2\n");
    let (seen, _view) = during_commit(
        repo.path(),
        repo.path(),
        &["commit", "-q", "-m", "x", "--", "a.txt"],
        |view| inspect(&probe_in_hook(repo.path(), view)),
    );
    seen
}

/// `tree_dirty()` antes e depois de um comando de shell que roda na raiz do repositório.
fn tree_dirty_around(script: &str) -> (Result<bool, ProbeError>, Result<bool, ProbeError>) {
    let repo = base();
    let probe = GitProbe::new(repo.path());
    let before = probe.tree_dirty();
    let status = std::process::Command::new("sh")
        .args(["-c", script])
        .current_dir(repo.path())
        .status()
        .expect("run the shell command");
    assert!(status.success());

    (before, probe.tree_dirty())
}

fn mark_path(repo: &GitRepo) -> PathBuf {
    repo.path().join(".git/omhl-develop-mark")
}

// Matriz do repositório real

#[test]
fn it_should_report_the_head_commit_when_the_tree_is_clean() {
    let (repo, head) = clean_repo();

    let result = probe_dir(repo.path());

    assert_eq!(result, reproducible(&head));
}

#[test]
fn it_should_report_dirty_for_a_modified_tracked_file() {
    let (repo, _head) = clean_repo();
    repo.write("tracked.txt", "changed\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_a_staged_new_file() {
    let (repo, _head) = clean_repo();
    repo.write("staged.txt", "new\n");
    repo.git(&["add", "staged.txt"]);

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_an_untracked_file() {
    let (repo, _head) = clean_repo();
    repo.write("untracked.txt", "new\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_ignore_a_gitignored_file() {
    let repo = GitRepo::init();
    repo.write(".gitignore", "ignored.txt\n");
    let head = repo.commit_all("initial");
    repo.write("ignored.txt", "noise\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, reproducible(&head));
}

#[test]
fn it_should_judge_the_whole_enclosing_repository_from_a_subdirectory() {
    let repo = GitRepo::init();
    repo.write("sub/inner.txt", "inner\n");
    repo.write("top.txt", "top\n");
    let head = repo.commit_all("initial");
    let sub = repo.path().join("sub");
    let before = probe_dir(&sub);
    repo.write("top.txt", "changed outside sub\n");

    let after = probe_dir(&sub);

    assert_eq!(
        (before, after),
        (reproducible(&head), Ok(ProbeOutcome::Dirty))
    );
}

#[test]
fn it_should_refuse_a_branch_without_commits() {
    let repo = GitRepo::init();

    let result = probe_dir(repo.path());

    assert!(error_message(result).contains("no commits"));
}

#[test]
fn it_should_fail_outside_a_repository() {
    let outside = TempDir::new().expect("temp dir");
    let enclosing = git_in(outside.path(), &["rev-parse", "--git-dir"]);
    assert!(
        !enclosing.status.success(),
        "temp dir is inside a repository"
    );

    let result = probe_dir(outside.path());

    assert!(result.is_err());
}

#[test]
fn it_should_report_dirty_for_an_unresolved_merge_conflict() {
    let repo = GitRepo::init();
    repo.write("shared.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-b", "other"]);
    repo.write("shared.txt", "other side\n");
    repo.commit_all("other");
    repo.git(&["checkout", "main"]);
    repo.write("shared.txt", "main side\n");
    repo.commit_all("main");
    let merge = repo.git_unchecked(&["merge", "other"]);
    assert!(!merge.status.success(), "the merge is expected to conflict");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_a_submodule_with_a_modified_file() {
    let upstream = GitRepo::init();
    upstream.write("lib.txt", "lib\n");
    upstream.commit_all("lib");
    let repo = GitRepo::init();
    repo.write("top.txt", "top\n");
    repo.commit_all("initial");
    let upstream_path = upstream.path().to_string_lossy().into_owned();
    repo.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        &upstream_path,
        "vendor",
    ]);
    repo.commit_all("add submodule");
    repo.write("vendor/lib.txt", "edited inside the submodule\n");

    let result = probe_dir(repo.path());

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_not_run_the_fsmonitor_program_of_the_repository_config() {
    let (repo, head) = clean_repo();
    let tools = TempDir::new().expect("temp dir");
    let marker = tools.path().join("fsmonitor-ran");
    let program = fake_program(tools.path(), &format!("touch '{}'", marker.display()));
    repo.git(&["config", "core.fsmonitor", &program.to_string_lossy()]);

    let result = probe_dir(repo.path());

    assert_eq!((result, marker.exists()), (reproducible(&head), false));
}

// Programa falso

#[test]
fn it_should_relay_the_error_text_when_git_exits_nonzero() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(
        tools.path(),
        "echo 'fatal: detected dubious ownership' >&2\nexit 128",
    );

    let result = GitProbe::new(tools.path()).with_program(program).probe();

    assert!(error_message(result).contains("detected dubious ownership"));
}

#[test]
fn it_should_fail_when_the_program_exceeds_the_timeout() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "exec sleep 5");
    let probe = GitProbe::new(tools.path())
        .with_program(program)
        .with_timeout(Duration::from_millis(200));
    let started = Instant::now();

    let result = probe.probe();

    assert!(error_message(result).contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn it_should_time_out_even_when_a_grandchild_holds_the_pipe() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "sleep 5 &\nexit 0");
    let probe = GitProbe::new(tools.path())
        .with_program(program)
        .with_timeout(Duration::from_millis(200));
    let started = Instant::now();

    let result = probe.probe();

    assert!(error_message(result).contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn it_should_truncate_a_long_git_error_message() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(
        tools.path(),
        "head -c 1000 /dev/zero | tr '\\0' Z >&2\nexit 128",
    );

    let result = GitProbe::new(tools.path()).with_program(program).probe();

    assert_eq!(error_message(result).matches('Z').count(), 200);
}

#[test]
fn it_should_fail_on_malformed_output() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "echo garbage");

    let result = GitProbe::new(tools.path()).with_program(program).probe();

    assert!(error_message(result).contains("unexpected git output"));
}

#[test]
fn it_should_not_hang_on_output_larger_than_the_pipe_buffer() {
    let tools = TempDir::new().expect("temp dir");
    let program = fake_program(tools.path(), "head -c 2097152 /dev/zero | tr '\\0' a");
    let probe = GitProbe::new(tools.path()).with_program(program);
    let started = Instant::now();

    let result = probe.probe();

    assert!(error_message(result).contains("unexpected git output"));
    assert!(started.elapsed() < Duration::from_secs(5));
}

// S9: árvore sob os hooks de commit

#[test]
fn it_should_report_the_staged_tree_under_a_commit_dash_a_hook() {
    let run = run_commit(false, false, &["commit", "-q", "-a", "-m", "x"]);

    assert!(Path::new(&run.view.index).is_absolute());
    assert!(run.view.index.ends_with("/.git/index.lock"));
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(run.view.tree, A2_TREE);
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
}

#[test]
fn it_should_report_the_staged_tree_under_a_plain_commit_hook() {
    let run = run_commit(true, false, &["commit", "-q", "-m", "x"]);

    assert_eq!(run.view.index, ".git/index");
    assert_eq!(canonical(&run.view.pwd), canonical(&run.root));
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
}

#[test]
fn it_should_report_the_staged_tree_under_a_path_only_commit() {
    let run = run_commit(false, false, &["commit", "-q", "-m", "x", "--", "a.txt"]);

    let index = Path::new(&run.view.index);
    let name = index.file_name().expect("file name").to_string_lossy();
    assert!(index.is_absolute());
    assert!(name.starts_with("next-index-") && name.ends_with(".lock"));
    assert_eq!(
        canonical(index.parent().expect("parent")),
        canonical(run.root.join(".git"))
    );
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
}

#[test]
fn it_should_report_the_staged_tree_under_an_amend() {
    let run = run_commit(true, false, &["commit", "-q", "--amend", "-m", "x"]);

    assert_eq!(run.view.index, ".git/index");
    assert_eq!(canonical(&run.view.pwd), canonical(&run.root));
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
    assert_eq!(git_out(&run.root, &["rev-list", "--count", "HEAD"]), "1");
}

#[test]
fn it_should_report_the_staged_tree_inside_a_linked_worktree_for_a_plain_commit() {
    let run = run_commit(true, true, &["commit", "-q", "-m", "x"]);

    assert!(Path::new(&run.view.index).is_absolute());
    assert!(run.view.index.ends_with("/.git/worktrees/wt/index"));
    assert_eq!(canonical(&run.view.pwd), canonical(&run.root));
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
}

#[test]
fn it_should_report_the_staged_tree_inside_a_linked_worktree_for_commit_dash_a() {
    let run = run_commit(false, true, &["commit", "-q", "-a", "-m", "x"]);

    assert!(run.view.index.ends_with("/.git/worktrees/wt/index.lock"));
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
}

#[test]
fn it_should_report_the_staged_tree_inside_a_linked_worktree_for_a_path_only_commit() {
    let run = run_commit(false, true, &["commit", "-q", "-m", "x", "--", "a.txt"]);

    let index = Path::new(&run.view.index);
    let name = index.file_name().expect("file name").to_string_lossy();
    assert!(index.is_absolute());
    assert!(name.starts_with("next-index-") && name.ends_with(".lock"));
    assert!(
        index
            .parent()
            .expect("parent")
            .ends_with(".git/worktrees/wt")
    );
    assert_eq!(run.outcome, tree(A2_TREE));
    assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^{tree}"]), A2_TREE);
}

// S9: índice injetado

#[test]
fn it_should_use_the_injected_index_file_for_the_tree_calls_only() {
    let (repo, altdir) = ignored_with_alt_index();
    let alt_index = altdir.path().join("alt-index");
    let probe = GitProbe::new(repo.path()).with_index_file(Some(&alt_index), repo.path());

    let outcomes = (
        index(&probe),
        probe.tree_dirty(),
        probe.probe(),
        head(&probe),
    );

    assert_eq!(
        outcomes,
        (
            tree(ALT_INDEX_TREE),
            Ok(false),
            reproducible(&head_oid(repo.path())),
            tree(IGNORED_TREE),
        )
    );
}

#[test]
fn it_should_resolve_a_relative_git_index_file_against_the_given_base_directory() {
    let (repo, altdir) = ignored_with_alt_index();
    let probe =
        GitProbe::new(repo.path()).with_index_file(Some(Path::new("alt-index")), altdir.path());

    let result = index(&probe);

    assert_eq!(result, tree(ALT_INDEX_TREE));
}

#[test]
fn it_should_refuse_a_git_index_file_that_does_not_exist() {
    let repo = base();
    let elsewhere = TempDir::new().expect("temp dir");
    let missing = elsewhere.path().join("does-not-exist");
    let probe = GitProbe::new(repo.path()).with_index_file(Some(&missing), repo.path());

    let results = (index(&probe).is_err(), probe.tree_dirty().is_err());

    assert_eq!(results, (true, true));
}

// S9: árvore do índice e do HEAD

#[test]
fn it_should_report_dirty_when_the_working_tree_differs_from_the_index() {
    let repo = base();
    repo.write("a.txt", "a2\n");

    let result = index(&GitProbe::new(repo.path()));

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_a_path_only_commit_while_another_tracked_file_is_modified() {
    let outcome = during_path_only_commit_with_b_modified(index);

    assert_eq!(outcome, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_the_head_tree_for_a_clean_repository() {
    let repo = base();

    let result = head(&GitProbe::new(repo.path()));

    assert_eq!(result, tree(BASE_TREE));
}

#[test]
fn it_should_report_dirty_for_the_head_tree_when_a_tracked_file_is_modified() {
    let repo = base();
    repo.write("a.txt", "a2\n");

    let result = head(&GitProbe::new(repo.path()));

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_ignore_ignored_files_in_the_tree_check() {
    let repo = ignored();
    let probe = GitProbe::new(repo.path());

    let results = (index(&probe), probe.tree_dirty());

    assert_eq!(results, (tree(IGNORED_TREE), Ok(false)));
}

#[test]
fn it_should_count_untracked_files_as_dirty_in_the_tree_check() {
    let repo = base();
    repo.write("u.txt", "u\n");

    let result = GitProbe::new(repo.path()).tree_dirty();

    assert_eq!(result, Ok(true));
}

#[test]
fn it_should_detect_a_tracked_file_modified_by_a_command() {
    let results = tree_dirty_around("printf x >> a.txt");

    assert_eq!(results, (Ok(false), Ok(true)));
}

#[test]
fn it_should_detect_an_untracked_file_created_by_a_command() {
    let results = tree_dirty_around("printf n > new.txt");

    assert_eq!(results, (Ok(false), Ok(true)));
}

#[test]
fn it_should_detect_a_tracked_file_deleted_by_a_command() {
    let results = tree_dirty_around("rm b.txt");

    assert_eq!(results, (Ok(false), Ok(true)));
}

#[test]
fn it_should_report_an_error_for_the_tree_of_a_repository_without_commits() {
    let repo = GitRepo::init();
    repo.write("a.txt", "a\n");

    let result = head(&GitProbe::new(repo.path()));

    assert!(result.is_err());
}

#[test]
fn it_should_treat_a_merge_conflict_as_dirty() {
    let repo = base();
    repo.git(&["switch", "-q", "-c", "other"]);
    repo.write("a.txt", "x\n");
    repo.commit_all("other");
    repo.git(&["switch", "-q", "main"]);
    repo.write("a.txt", "y\n");
    repo.commit_all("main");
    let merge = repo.git_unchecked(&["merge", "other"]);
    assert!(!merge.status.success(), "the merge is expected to conflict");

    let result = index(&GitProbe::new(repo.path()));

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_a_submodule_change_as_dirty() {
    let upstream = base();
    let repo = base();
    let upstream_path = upstream.path().to_string_lossy().into_owned();
    repo.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        &upstream_path,
        "sm",
    ]);
    repo.commit_all("add submodule");
    repo.write("sm/a.txt", "changed\n");

    let result = index(&GitProbe::new(repo.path()));

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_no_tracked_change_inside_a_commit_dash_a_hook_by_keeping_the_index_file() {
    let repo = base();
    repo.write("a.txt", "a2\n");

    let (with_hook_index, _view) = during_commit(
        repo.path(),
        repo.path(),
        &["commit", "-q", "-a", "-m", "x"],
        |view| {
            (
                probe_in_hook(repo.path(), view).tree_dirty(),
                GitProbe::new(repo.path()).tree_dirty(),
            )
        },
    );

    assert_eq!(with_hook_index, (Ok(false), Ok(true)));
}

#[test]
fn it_should_report_a_tracked_change_inside_a_path_only_commit_hook_while_another_file_is_modified()
{
    let result = during_path_only_commit_with_b_modified(|probe| probe.tree_dirty());

    assert_eq!(result, Ok(true));
}

#[test]
fn it_should_report_dirty_for_the_index_tree_when_an_untracked_file_exists() {
    let repo = base();
    repo.write("u.txt", "u\n");

    let result = index(&GitProbe::new(repo.path()));

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_for_the_head_tree_when_an_untracked_file_exists() {
    let repo = base();
    repo.write("u.txt", "u\n");

    let result = head(&GitProbe::new(repo.path()));

    assert_eq!(result, Ok(ProbeOutcome::Dirty));
}

#[test]
fn it_should_report_dirty_inside_a_path_only_commit_hook_while_another_new_file_is_staged() {
    let repo = base();
    repo.write("a.txt", "a2\n");
    repo.write("c.txt", "c\n");
    repo.git(&["add", "c.txt"]);

    let (outcome, _view) = during_commit(
        repo.path(),
        repo.path(),
        &["commit", "-q", "-m", "x", "--", "a.txt"],
        |view| index(&probe_in_hook(repo.path(), view)),
    );

    assert_eq!(outcome, Ok(ProbeOutcome::Dirty));
}

// S9: fatos da branch

#[test]
fn it_should_report_the_current_branch() {
    let repo = base_on_feature();

    let facts = GitProbe::new(repo.path()).head_facts().expect("head facts");

    assert_eq!(
        (facts.current, facts.rebase_head),
        (Some(FEATURE.to_owned()), None)
    );
}

#[test]
fn it_should_report_no_current_branch_when_the_head_is_detached() {
    let repo = base();
    repo.git(&["switch", "-q", "--detach", "HEAD"]);

    let facts = GitProbe::new(repo.path()).head_facts().expect("head facts");

    assert_eq!(facts.current, None);
}

#[test]
fn it_should_report_the_rebase_head_during_an_interactive_rebase() {
    let repo = base_on_feature();
    commit_file(repo.path(), "f.txt", "f\n");
    rebase_stopped_at_edit(repo.path());

    let facts = GitProbe::new(repo.path()).head_facts().expect("head facts");

    assert_eq!(
        (facts.current, facts.rebase_head),
        (None, Some(format!("refs/heads/{FEATURE}\n")))
    );
    repo.git(&["rebase", "--abort"]);
}

#[test]
fn it_should_report_the_rebase_head_during_an_apply_rebase_conflict() {
    let repo = base_on_feature();
    repo.write("a.txt", "x\n");
    repo.commit_all("feature");
    repo.git(&["switch", "-q", "main"]);
    repo.write("a.txt", "y\n");
    repo.commit_all("main");
    repo.git(&["switch", "-q", FEATURE]);
    let rebase = repo.git_unchecked(&["rebase", "--apply", "main"]);
    assert!(
        !rebase.status.success(),
        "the rebase is expected to conflict"
    );

    let facts = GitProbe::new(repo.path()).head_facts().expect("head facts");

    assert_eq!(
        (facts.current, facts.rebase_head),
        (None, Some(format!("refs/heads/{FEATURE}\n")))
    );
}

#[test]
fn it_should_list_the_local_branches_that_contain_a_detached_head() {
    let repo = base_on_feature();
    commit_file(repo.path(), "f.txt", "f\n");
    repo.git(&["branch", "feat/wi-w2__y"]);
    repo.git(&["switch", "-q", "--detach", "HEAD"]);

    let facts = GitProbe::new(repo.path()).head_facts().expect("head facts");

    assert_eq!(
        (facts.current, facts.containing),
        (None, vec![FEATURE.to_owned(), "feat/wi-w2__y".to_owned()])
    );
}

#[test]
fn it_should_read_the_rebase_head_inside_a_linked_worktree() {
    let repo = base();
    let (_parent, wt) = add_worktree(&repo);
    commit_file(&wt, "f.txt", "f\n");
    rebase_stopped_at_edit(&wt);

    let in_worktree = GitProbe::new(&wt).head_facts().expect("head facts");
    let in_root = GitProbe::new(repo.path()).head_facts().expect("head facts");

    assert_eq!(
        (
            in_worktree.rebase_head,
            in_root.rebase_head,
            in_root.current
        ),
        (
            Some(format!("refs/heads/{FEATURE}\n")),
            None,
            Some("main".to_owned())
        )
    );
}

// S9: a marca da develop

#[test]
fn it_should_report_no_mark_when_the_file_does_not_exist() {
    let repo = base();

    let result = GitProbe::new(repo.path()).present();

    assert_eq!(result, Ok(false));
}

#[test]
fn it_should_report_the_mark_when_the_file_starts_with_the_marker_line() {
    let repo = base();
    let probe = GitProbe::new(repo.path());
    fs::write(
        mark_path(&repo),
        format!("{MARK_LINE}\ncreated for the test\n"),
    )
    .expect("write");
    let with_newline = probe.present();
    fs::write(mark_path(&repo), MARK_LINE).expect("write");

    let without_newline = probe.present();

    assert_eq!((with_newline, without_newline), (Ok(true), Ok(true)));
}

#[test]
fn it_should_report_no_mark_when_the_first_line_differs() {
    let repo = base();
    let probe = GitProbe::new(repo.path());
    let mut results = Vec::new();
    for content in ["omhl-develop-mark v2\n", "x\nomhl-develop-mark v1\n", ""] {
        fs::write(mark_path(&repo), content).expect("write");
        results.push(probe.present());
    }

    assert_eq!(results, vec![Ok(false), Ok(false), Ok(false)]);
}

#[test]
fn it_should_consume_the_mark_and_keep_the_other_files_of_the_git_dir() {
    let repo = base();
    let probe = GitProbe::new(repo.path());
    fs::write(
        mark_path(&repo),
        format!("{MARK_LINE}\ncreated for the test\n"),
    )
    .expect("write");
    let keep = repo.path().join(".git/omhl-develop-mark.keep");
    fs::write(&keep, "k").expect("write");

    let first = probe.consume();
    let second = probe.consume();

    assert_eq!(
        (
            first,
            mark_path(&repo).exists(),
            keep.exists(),
            repo.path().join(".git/HEAD").exists(),
            repo.path().join(".git/config").exists(),
            probe.present(),
            second,
        ),
        (Ok(()), false, true, true, true, Ok(false), Ok(()))
    );
}

#[test]
fn it_should_keep_the_mark_per_worktree() {
    let repo = base();
    let (_parent, wt) = add_worktree(&repo);
    let wt_mark = repo.path().join(".git/worktrees/wt/omhl-develop-mark");
    fs::write(&wt_mark, format!("{MARK_LINE}\n")).expect("write");
    let root_probe = GitProbe::new(repo.path());

    let results = (
        GitProbe::new(&wt).present(),
        root_probe.present(),
        root_probe.consume(),
        wt_mark.exists(),
    );

    assert_eq!(results, (Ok(true), Ok(false), Ok(()), true));
}

// S9: o commit em HeadFacts

#[test]
fn it_should_report_the_head_commit_in_the_head_facts() {
    let repo = base_on_feature();
    let unborn = GitRepo::init();

    let facts = GitProbe::new(repo.path()).head_facts().expect("head facts");
    let unborn_facts = GitProbe::new(unborn.path()).head_facts();

    assert_eq!(facts.head, Some(head_oid(repo.path())));
    assert_eq!(
        unborn_facts,
        Ok(HeadFacts {
            current: Some("main".to_owned()),
            rebase_head: None,
            containing: vec![],
            head: None,
        })
    );
}
