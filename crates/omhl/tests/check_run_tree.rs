mod support;

use std::fs;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use hexlog::{Snapshot, SnapshotKind};
use omhl::app::App;
use omhl::app::action::{Action, Caller, CheckTarget};
use omhl::app::error::AppError;
use omhl::app::retry::RetryPolicy;
use omhl::facade::OmhlDeps;
use omhl::ports::blob::BlobStore;
use omhl::ports::git::GitProbe;
use omhl::ports::process::ProcessRunner;
use omhl::probe::{ProbeError, ProbeOutcome, SnapshotProbe, TreeSource};
use omhl::runner::{CommandRunner, RunError, RunReport, RunSpec};
use serde_json::{Value, json};
use support::busy::with_write_lock;
use support::child::{Output as Child, omhl};
use support::ext::write_check_file;
use support::git::{GitRepo, git_command_with_env, git_with_env, write_executable};
use support::omhl::{acme, check_data, work_item_data};
use support::pr_gate::CI_GREEN;
use support::raw::RawHarness;
use support::runner::FakeRunner;
use support::{FakeProbe, snap, tenant, wi_head_facts};
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const BRANCH: &str = "feat/wi-w1__x";
const TREE_ID: &str = "cccccccccccccccccccccccccccccccccccccccc";
const TREE_ATTESTATION: &str =
    "attestations.cccccccccccccccccccccccccccccccc.cccccccc.checks.fmt.work-items.w1";
// oráculos O-1, O-2, O-5, O-6, O-7 e O-8 (git 2.43.0)
const R1_TREE: &str = "08585692ce06452da6f82ae66b90d98b55536fca";
const R1_STAGED_N_TREE: &str = "59f7652701c5d8000449777e6ba3bbdae8a87bea";
const R3_TREE: &str = "c0eb12773e533e0388d6f1b55b9e3540fcf60add";
const R3_ATTESTATION: &str =
    "attestations.c0eb12773e533e0388d6f1b55b9e3540.fcf60add.checks.fmt.work-items.w1";
const R5_TREE: &str = "9584cf266afbbe518e9d17df4fe80daa27a993e8";
const R5_COMMIT_TWO_TREE: &str = "ae6c0f89eb17e5e7f4149486e1e2bbfdc92ec714";
const R5_COMMIT_TWO_ATTESTATION: &str =
    "attestations.ae6c0f89eb17e5e7f4149486e1e2bbfd.c92ec714.checks.fmt.work-items.w1";
const R5_HOOK_INDEX_TREE: &str = "2560276e6c83384e10e6f1a3224488091399cea9";
const R5_HOOK_INDEX_ATTESTATION: &str =
    "attestations.2560276e6c83384e10e6f1a322448809.1399cea9.checks.fmt.work-items.w1";
const TREE_CHANGED_HINT: &str = "the command changed the working tree; keep checks read-only, add generated files to .gitignore, or restore tracked files with git checkout -- <path>";
const GUARD_REFUSED_HINT: &str =
    "the index changed while the command ran; do not stage during a hook, then run again";
const PROBE_FAILED_HINT: &str =
    "git could not read the tree; run git status and fix the repository state";
const BUSY_MESSAGE: &str =
    "database is busy: another omhl process is writing; run the command again";
const FMT_TOML: &str = "blocking = true\nhow = \"Always passes.\"\nrun = [\"true\"]\ntimeout_secs = 60\nexpect = [{ exit = 0, outcome = \"passed\" }]\nwhen = { event = \"before-commit\" }\n";

type Response = (Vec<String>, Result<RunReport, RunError>);

// Banco e loja de blobs de um teste; a raiz de projeto e a sonda vêm de cada teste.
struct Fixture {
    raw: RawHarness,
    blobs: BlobStore,
}

impl Fixture {
    fn new() -> Fixture {
        let raw = RawHarness::new();
        let blobs = BlobStore::new(raw.dir.path());

        Fixture { raw, blobs }
    }

    fn app<'a>(
        &'a self,
        probe: &'a dyn SnapshotProbe,
        root: &Path,
        runner: &'a dyn CommandRunner,
    ) -> App<'a> {
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe,
        };
        let app = App::open(&self.raw.db_path(), &acme(), deps, &RetryPolicy::default())
            .unwrap_or_else(|error| panic!("the app opens: {error}"));

        app.with_project_root(root)
            .with_exec(runner, &self.blobs)
            .with_retry_after_run(RetryPolicy {
                busy_timeout: Duration::from_millis(10),
                pauses: vec![Duration::from_millis(10), Duration::from_millis(10)],
            })
    }
}

// Snapshots, checks e respostas do runner

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn tree_json(id: &str) -> Value {
    json!({ "kind": "git-tree", "id": id })
}

fn mechanical_check(event: &str, run: &[&str], blocking: bool) -> Value {
    let mut data = check_data(event, "run it", blocking);
    data["run"] = json!(run);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);

    data
}

fn default_probe() -> FakeProbe {
    FakeProbe::clean(snap('a'))
        .with_head_facts(wi_head_facts())
        .with_tree(TreeSource::Index, Ok(ProbeOutcome::Reproducible(tree('c'))))
        .with_tree_dirty(false)
}

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn printing(command: &[&str], stdout: &[u8]) -> Response {
    let report = RunReport {
        exit_code: Some(0),
        stdout: stdout.to_vec(),
        stderr: vec![],
        timed_out: false,
        truncated: false,
        duration: Duration::from_millis(5),
    };

    (argv(command), Ok(report))
}

fn ok(command: &[&str]) -> Response {
    printing(command, b"ok\n")
}

fn received(specs: &Receiver<RunSpec>) -> Vec<Vec<String>> {
    specs.try_iter().map(|spec| spec.argv).collect()
}

// Ações e leitura pelo App

fn node_wire(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "items": [{ "body": { "kind": "node", "id": id, "type": type_name, "data": data } }] })
}

/// Grava `work-items.w1` aberto pela CLI e cada check pelo loader.
fn seed(app: &mut App<'_>, root: &Path, checks: &[(&str, Value)]) {
    let work_item = Action::Append {
        request: node_wire(W1, "work-item", &work_item_data("t")),
    };
    app.dispatch(&Caller::cli(), &work_item)
        .unwrap_or_else(|error| panic!("the work item is accepted: {error}"));
    for (id, data) in checks {
        write_check_file(root, id, data);
        let check = Action::Append {
            request: node_wire(id, "check", data),
        };
        app.dispatch(&Caller::loader(), &check)
            .unwrap_or_else(|error| panic!("the check {id} is accepted: {error}"));
    }
}

fn on_event(event: &str) -> Action {
    Action::CheckRun {
        target: CheckTarget::Event(event.to_owned()),
        pre_push: None,
    }
}

fn run_event(app: &mut App<'_>, event: &str) -> Result<Value, AppError> {
    app.dispatch(&Caller::cli(), &on_event(event))
}

fn get(app: &mut App<'_>, id: &str) -> Value {
    let action = Action::Get { id: id.to_owned() };
    let mut response = app
        .dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("get is accepted: {error}"));

    response["node"].take()
}

fn list_page(app: &mut App<'_>, type_name: Option<&str>) -> Value {
    let action = Action::List {
        type_name: type_name.map(str::to_owned),
        id_prefix: None,
        limit: None,
        after_seq: None,
        where_eq: Vec::new(),
    };

    app.dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("list is accepted: {error}"))
}

fn attestations(app: &mut App<'_>) -> Value {
    list_page(app, Some("attestation"))
}

fn no_attestations() -> Value {
    json!({ "items": [] })
}

fn details(error: &AppError) -> Value {
    error.to_json()["error"]["details"].clone()
}

fn detail_field(error: &AppError, field: &str) -> Vec<Value> {
    details(error)
        .as_array()
        .expect("the error has details")
        .iter()
        .map(|detail| detail[field].clone())
        .collect()
}

// Repositórios reais

/// `R1`: `a.txt` commitado na `main`, depois a branch do work item.
fn r1() -> GitRepo {
    let repo = GitRepo::init();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    repo.git(&["switch", "-c", BRANCH]);

    repo
}

/// `R3`: `R1` com `.gitignore` = `target/` commitado.
fn r3() -> GitRepo {
    let repo = GitRepo::init();
    repo.write("a.txt", "a\n");
    repo.write(".gitignore", "target/\n");
    repo.commit_all("init");
    repo.git(&["switch", "-c", BRANCH]);

    repo
}

fn porcelain(repo: &GitRepo) -> String {
    repo.git(&["status", "--porcelain"])
}

fn copy_index(repo: &GitRepo, to: &Path) {
    fs::copy(repo.path().join(".git").join("index"), to).expect("the index is copied");
}

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
        omhl(args).cwd(self.path()).xdg(self.xdg.path())
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
fn it_should_attest_a_before_commit_check_with_the_staged_tree() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe();
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let output = run_event(&mut app, "before-commit").expect("the check run succeeds");

    assert_eq!(
        output,
        json!({
            "event": "before-commit",
            "work_item": "work-items.w1",
            "snapshot": { "kind": "git-tree", "id": TREE_ID },
            "recorded": true,
            "results": [{
                "check": "checks.fmt", "outcome": "passed", "ok": true, "blocking": true,
                "exit_code": 0, "duration_ms": 5, "timed_out": false, "truncated": false,
            }],
            "blocked": [],
        })
    );
    assert_eq!(received(&specs), vec![argv(&["true"])]);
    let attestation = get(&mut app, TREE_ATTESTATION);
    assert_eq!(attestation["snapshot"], tree_json(TREE_ID));
    assert_eq!(attestation["data"]["outcome"], json!("passed"));
    assert_eq!(attestation["data"]["exit_code"], json!(0));
    assert_eq!(attestation["data"]["command"], json!(["true"]));
    assert_eq!(attestation["author"]["client"], json!("executor"));
    assert_eq!(attestation["author"]["agent"], json!("executor"));
}

#[test]
fn it_should_refuse_to_run_before_commit_when_the_working_tree_differs_from_the_index() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe().with_tree(TreeSource::Index, Ok(ProbeOutcome::Dirty));
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let error = run_event(&mut app, "before-commit").expect_err("a dirty working tree is refused");

    assert_eq!(error.code(), "worktree-differs-from-index");
    assert!(specs.try_recv().is_err());
    assert_eq!(attestations(&mut app), no_attestations());
}

#[test]
fn it_should_name_the_refusal_worktree_differs_from_index() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe().with_tree(TreeSource::Index, Ok(ProbeOutcome::Dirty));
    let (runner, _specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let error = run_event(&mut app, "before-commit").expect_err("a dirty working tree is refused");

    let rendered = error.to_json();
    assert_eq!(
        rendered["error"]["code"],
        json!("worktree-differs-from-index")
    );
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["code"], json!("worktree-differs-from-index"));
    assert_eq!(details[0]["path"], json!(""));
    let context = &details[0]["context"];
    assert_eq!(context["kind"], json!("worktree-differs-from-index"));
    assert_eq!(context["event"], json!("before-commit"));
    assert_eq!(context["work_item"], json!("work-items.w1"));
    assert_eq!(context["branch"], json!(BRANCH));
}

#[test]
fn it_should_refuse_to_run_before_commit_when_an_untracked_file_exists() {
    let fx = Fixture::new();
    let repo = r1();
    let probe = GitProbe::new(repo.path());
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );
    repo.write("u.txt", "u\n");
    assert_eq!(porcelain(&repo), "?? u.txt\n");

    let error = run_event(&mut app, "before-commit").expect_err("an untracked file is refused");

    assert_eq!(error.code(), "worktree-differs-from-index");
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(attestations(&mut app), no_attestations());
}

#[test]
fn it_should_refuse_before_pr_when_an_untracked_file_exists() {
    let fx = Fixture::new();
    let repo = r1();
    let probe = GitProbe::new(repo.path());
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[(
            "checks.lint",
            mechanical_check("before-pr", &["true"], true),
        )],
    );
    repo.write("u.txt", "u\n");

    let error = run_event(&mut app, "before-pr").expect_err("an untracked file is refused");

    assert_eq!(error.code(), "worktree-dirty");
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(attestations(&mut app), no_attestations());
}

#[test]
fn it_should_stop_and_report_when_the_guard_refuses_the_snapshot_after_the_run() {
    let fx = Fixture::new();
    let repo = r1();
    let probe = GitProbe::new(repo.path());
    let runner = ProcessRunner::new();
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[
            (
                "checks.a-stage",
                mechanical_check(
                    "before-commit",
                    &["sh", "-c", "echo n > n && git add n"],
                    true,
                ),
            ),
            (
                "checks.b-true",
                mechanical_check("before-commit", &["true"], true),
            ),
        ],
    );

    let error = run_event(&mut app, "before-commit").expect_err("the guard refuses the snapshot");

    assert_eq!(error.code(), "attestation-refused");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.a-stage"));
    assert_eq!(details[0]["code"], json!("snapshot-tree-mismatch"));
    assert_eq!(details[0]["context"]["kind"], json!("guard-refused"));
    assert_eq!(details[0]["context"]["expected"], json!("passed"));
    assert_eq!(details[0]["context"]["snapshot"], tree_json(R1_TREE));
    assert_eq!(details[0]["context"]["hint"], json!(GUARD_REFUSED_HINT));
    assert_eq!(details[1]["path"], json!("checks.b-true"));
    assert_eq!(details[1]["code"], json!("not-run"));
    assert_eq!(attestations(&mut app), no_attestations());
    assert_eq!(repo.git(&["write-tree"]).trim(), R1_STAGED_N_TREE);
}

#[test]
fn it_should_stop_and_report_when_the_command_changes_a_tracked_file_recording_nothing() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe().with_tree_dirty(true);
    let (runner, _specs) = FakeRunner::new(vec![printing(&["true"], b"touched\n")]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let error = run_event(&mut app, "before-commit").expect_err("the changed tree stops the run");

    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["path"], json!("checks.fmt"));
    assert_eq!(details[0]["code"], json!("command-changed-worktree"));
    assert_eq!(details[0]["context"]["kind"], json!("tree-changed"));
    assert_eq!(details[0]["context"]["hint"], json!(TREE_CHANGED_HINT));
    assert_eq!(
        details[0]["context"]["output_tail"],
        json!({ "stdout": ["touched"], "stderr": [] })
    );
    assert_eq!(attestations(&mut app), no_attestations());
    let ids: Vec<Value> = list_page(&mut app, None)["items"]
        .as_array()
        .expect("the page has items")
        .iter()
        .map(|node| node["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!(W1), json!("checks.fmt")]);
}

#[test]
fn it_should_not_count_an_ignored_file_as_a_change() {
    let fx = Fixture::new();
    let repo = r3();
    let probe = GitProbe::new(repo.path());
    let runner = ProcessRunner::new();
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[(
            "checks.fmt",
            mechanical_check(
                "before-commit",
                &["sh", "-c", "mkdir -p target && echo x > target/out"],
                true,
            ),
        )],
    );

    let output = run_event(&mut app, "before-commit").expect("an ignored file is not a change");

    assert_eq!(output["results"][0]["outcome"], json!("passed"));
    assert_eq!(output["snapshot"], tree_json(R3_TREE));
    assert!(!get(&mut app, R3_ATTESTATION).is_null());
}

#[test]
fn it_should_leave_the_files_as_the_command_left_them() {
    let fx = Fixture::new();
    let repo = r1();
    let probe = GitProbe::new(repo.path());
    let runner = ProcessRunner::new();
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[(
            "checks.fmt",
            mechanical_check(
                "before-commit",
                &["sh", "-c", "printf 'changed\\n' > a.txt"],
                true,
            ),
        )],
    );

    let error = run_event(&mut app, "before-commit").expect_err("the changed file stops the run");

    assert_eq!(error.code(), "check-not-run");
    assert_eq!(
        details(&error)[0]["code"],
        json!("command-changed-worktree")
    );
    let content = fs::read_to_string(repo.path().join("a.txt")).expect("a.txt is read");
    assert_eq!(content, "changed\n");
    assert_eq!(porcelain(&repo), " M a.txt\n");
}

#[test]
fn it_should_attest_the_head_tree_when_run_outside_a_hook_on_a_clean_tree() {
    let fx = Fixture::new();
    let repo = r1();
    let probe = GitProbe::new(repo.path());
    let (runner, _specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let output = run_event(&mut app, "before-commit").expect("the check run succeeds");

    assert_eq!(output["snapshot"], tree_json(R1_TREE));
    assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]).trim(), R1_TREE);
}

#[test]
fn it_should_not_leak_the_hook_git_index_into_the_command() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe();
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let _ = run_event(&mut app, "before-commit");

    let mut removed = specs
        .try_recv()
        .expect("the runner received a spec")
        .env_remove;
    removed.sort();
    assert_eq!(
        removed,
        argv(&[
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "GIT_COMMON_DIR",
            "GIT_DIR",
            "GIT_INDEX_FILE",
            "GIT_PREFIX",
            "GIT_WORK_TREE",
            "RUSTC_WORKSPACE_WRAPPER",
            "RUSTC_WRAPPER",
            "RUSTFLAGS"
        ])
    );
}

#[test]
fn it_should_keep_inheriting_the_rest_of_the_environment() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe();
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );

    let _ = run_event(&mut app, "before-commit");

    let spec = specs.try_recv().expect("the runner received a spec");
    assert_eq!(spec.env_remove.len(), 10);
    for kept in [
        "PATH",
        "HOME",
        "GIT_AUTHOR_NAME",
        "GIT_CONFIG_GLOBAL",
        "OMHL_RUN_ID",
    ] {
        assert!(
            !spec.env_remove.iter().any(|name| name == kept),
            "{kept} is removed"
        );
    }
    assert!(
        !spec
            .env_set
            .iter()
            .any(|(key, _)| key == "PATH" || key == "HOME"),
        "env_set overrides PATH or HOME: {:?}",
        spec.env_set
    );
}

#[test]
fn it_should_stop_and_report_when_the_probe_fails() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let unreadable = ProbeError {
        message: "index unreadable".to_owned(),
    };
    let probe = default_probe().with_tree(TreeSource::Index, Err(unreadable));
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[
            (
                "checks.a-fmt",
                mechanical_check("before-commit", &["true"], true),
            ),
            (
                "checks.b-lint",
                mechanical_check("before-commit", &["true"], true),
            ),
        ],
    );

    let error = run_event(&mut app, "before-commit").expect_err("the probe failure stops the run");

    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.a-fmt"));
    assert_eq!(details[0]["code"], json!("not-run"));
    assert_eq!(details[0]["context"]["kind"], json!("probe-failed"));
    let os_error = details[0]["context"]["os_error"]
        .as_str()
        .expect("the context carries os_error");
    assert!(
        os_error.contains("index unreadable"),
        "os_error: {os_error}"
    );
    assert_eq!(details[0]["context"]["hint"], json!(PROBE_FAILED_HINT));
    assert_eq!(details[1]["path"], json!("checks.b-lint"));
    assert_eq!(details[1]["code"], json!("not-run"));
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(attestations(&mut app), no_attestations());
}

#[test]
fn it_should_fail_closed_when_the_database_is_busy_before_running() {
    let raw = RawHarness::new();
    let probe = default_probe();
    let deps = || OmhlDeps {
        clock: &raw.clock,
        ids: &raw.ids,
        probe: &probe,
    };
    drop(
        App::open(&raw.db_path(), &acme(), deps(), &RetryPolicy::default())
            .unwrap_or_else(|error| panic!("the app opens: {error}")),
    );
    let short = RetryPolicy {
        busy_timeout: Duration::from_millis(10),
        pauses: vec![Duration::from_millis(10)],
    };

    let opened = with_write_lock(&raw.config(), &acme(), |_release| {
        App::open(&raw.db_path(), &tenant("beta"), deps(), &short).map(drop)
    });

    let error = opened.expect_err("the new tenant cannot be written under the lock");
    assert_eq!(error.code(), "BUSY");
    assert_eq!(error.message(), BUSY_MESSAGE);
    let mut beta = App::open(
        &raw.db_path(),
        &tenant("beta"),
        deps(),
        &RetryPolicy::default(),
    )
    .unwrap_or_else(|error| panic!("beta opens after the lock: {error}"));
    assert_eq!(attestations(&mut beta), no_attestations());
}

#[test]
fn it_should_stop_and_report_when_a_before_pr_command_creates_an_untracked_file() {
    let fx = Fixture::new();
    let repo = r1();
    let probe = GitProbe::new(repo.path());
    let runner = ProcessRunner::new();
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[
            (
                "checks.a-touch",
                mechanical_check("before-pr", &["sh", "-c", "echo u > u.txt"], true),
            ),
            (
                "checks.b-true",
                mechanical_check("before-pr", &["true"], true),
            ),
        ],
    );

    let error = run_event(&mut app, "before-pr").expect_err("the new file stops the run");

    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.a-touch"));
    assert_eq!(details[0]["code"], json!("command-changed-tree"));
    assert_eq!(details[0]["context"]["kind"], json!("tree-changed"));
    assert_eq!(details[1]["path"], json!("checks.b-true"));
    assert_eq!(details[1]["code"], json!("not-run"));
    assert_eq!(attestations(&mut app), no_attestations());
    assert_eq!(porcelain(&repo), "?? u.txt\n");
}

#[test]
fn it_should_not_run_the_remaining_checks_after_the_tree_changed() {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe = default_probe().with_tree_dirty(true);
    let (runner, specs) = FakeRunner::new(vec![ok(&["echo", "a"]), ok(&["echo", "b"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[
            (
                "checks.a-fmt",
                mechanical_check("before-commit", &["echo", "a"], true),
            ),
            (
                "checks.b-lint",
                mechanical_check("before-commit", &["echo", "b"], true),
            ),
        ],
    );

    let error = run_event(&mut app, "before-commit").expect_err("the changed tree stops the run");

    assert_eq!(received(&specs), vec![argv(&["echo", "a"])]);
    assert_eq!(error.code(), "check-not-run");
    assert_eq!(
        detail_field(&error, "code"),
        vec![json!("command-changed-worktree"), json!("not-run")]
    );
    assert_eq!(
        detail_field(&error, "path"),
        vec![json!("checks.a-fmt"), json!("checks.b-lint")]
    );
}

#[test]
fn it_should_pass_the_pull_request_gate_when_the_executor_attested_the_before_commit_check_at_the_head_tree()
 {
    let fx = Fixture::new();
    let root = TempDir::new().expect("the project root is created");
    let probe =
        default_probe().with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree('c'))));
    let (runner, _specs) = FakeRunner::new(vec![ok(&["true"])]);
    let mut app = fx.app(&probe, root.path(), &runner);
    seed(
        &mut app,
        root.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["true"], true),
        )],
    );
    run_event(&mut app, "before-commit").expect("the executor attests checks.fmt");
    let attest = Action::Attest {
        check: CI_GREEN.to_owned(),
        work_item: W1.to_owned(),
        outcome: "passed".to_owned(),
        note: None,
    };
    app.dispatch(&Caller::cli(), &attest)
        .unwrap_or_else(|error| panic!("the native check is attested: {error}"));

    let gate = app
        .dispatch(
            &Caller::cli(),
            &Action::GatePr {
                work_item: W1.to_owned(),
            },
        )
        .unwrap_or_else(|error| panic!("the gate is evaluated: {error}"));

    assert_eq!(gate["passed"], json!(true));
    let questions = gate["questions"]
        .as_array()
        .expect("the gate has questions");
    let labels: Vec<Value> = questions
        .iter()
        .map(|question| question["label"].clone())
        .collect();
    assert_eq!(
        labels,
        vec![
            json!("checks.ci-green-before-merge"),
            json!("checks.fmt"),
            json!("spec-digest-match:checks.fmt"),
            json!("no-open-contradiction"),
            json!("decisions-anchored"),
            json!("anchors-confirmed")
        ]
    );
    assert!(
        questions
            .iter()
            .all(|question| question["passed"] == json!(true)),
        "questions: {questions:?}"
    );
}

#[test]
fn it_should_attest_the_tree_of_the_index_file_named_in_the_environment() {
    let binary = BinaryRepo::new();
    let elsewhere = TempDir::new().expect("the hook index directory is created");
    let hook_index = elsewhere.path().join("hook-index");
    copy_index(&binary.repo, &hook_index);
    binary.repo.write("x.txt", "x\n");
    binary.add_to_index(&hook_index, "x.txt");

    let output = binary
        .omhl(&["check", "run", "--event", "before-commit"])
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

#[test]
fn it_should_stop_and_report_when_a_before_commit_command_creates_an_untracked_file() {
    let fx = Fixture::new();
    let repo = r1();
    let elsewhere = TempDir::new().expect("the hook index directory is created");
    let hook_index = elsewhere.path().join("hook-index");
    copy_index(&repo, &hook_index);
    let probe = GitProbe::new(repo.path()).with_index_file(Some(&hook_index), elsewhere.path());
    let runner = ProcessRunner::new();
    let mut app = fx.app(&probe, repo.path(), &runner);
    seed(
        &mut app,
        repo.path(),
        &[(
            "checks.fmt",
            mechanical_check("before-commit", &["sh", "-c", "echo u > u.txt"], true),
        )],
    );

    let error = run_event(&mut app, "before-commit").expect_err("the new file stops the run");

    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["path"], json!("checks.fmt"));
    assert_eq!(details[0]["code"], json!("command-changed-worktree"));
    assert_eq!(details[0]["context"]["kind"], json!("tree-changed"));
    assert_eq!(attestations(&mut app), no_attestations());
    assert_eq!(porcelain(&repo), "?? u.txt\n");
}
