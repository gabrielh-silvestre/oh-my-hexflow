use crate::support;

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use hexlog::{
    AppendItem, AppendOutcome, AppendRequest, Author, Clock, Config, Snapshot, SnapshotKind,
};
use omhl::app::App;
use omhl::app::action::{Action, Caller, CheckTarget};
use omhl::app::error::AppError;
use omhl::app::retry::RetryPolicy;
use omhl::facade::{Omhl, OmhlDeps};
use omhl::mechanical::spec_digest;
use omhl::ports::blob::BlobStore;
use omhl::probe::{HeadFacts, ProbeOutcome, TreeSource};
use omhl::runner::{CommandRunner, RunError, RunErrorKind, RunReport, RunSpec};
use serde_json::{Value, json};
use support::busy::{
    AppendingRunner, AttemptClock, GatedRunner, lock_at_gate, with_write_lock,
    with_write_lock_until_attempt,
};
use support::ext::write_check_file;
use support::gate::relation_item;
use support::omhl::{acme, check_data, node_item, with_snapshot, work_item_data};
use support::raw::RawHarness;
use support::runner::FakeRunner;
use support::{
    FakeProbe, author, executor_attestation_data, executor_author, loader_author, snap,
    wi_head_facts,
};
use tempfile::TempDir;

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const W1: &str = "work-items.w1";
const BYPRODUCT: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const LINT: &[&str] = &["cargo", "clippy"];
const BUILD: &[&str] = &["cargo", "build"];
const DOCS: &[&str] = &["cargo", "doc"];
const BUSY_HINT: &str = "the database stayed locked for about 74 seconds; the verdict above was computed but not recorded; run the same command again when the other process ends";

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

type Response = (Vec<String>, Result<RunReport, RunError>);

// Banco, raiz de projeto e loja de blobs de um teste; as sementes passam por um `Omhl` largado
// antes do `App`, sempre com a sonda limpa no HEAD (a guarda sonda o snapshot das attestations).
struct Fixture {
    raw: RawHarness,
    root: TempDir,
    blobs: BlobStore,
}

impl Fixture {
    fn new() -> Fixture {
        let raw = RawHarness::new();
        let blobs = BlobStore::new(&raw.db_path(), &acme());
        let fixture = Fixture {
            raw,
            root: TempDir::new().expect("the project root is created"),
            blobs,
        };
        fixture.seed_work_item(W1);

        fixture
    }

    fn omhl<'a>(&'a self, probe: &'a FakeProbe) -> Omhl<'a> {
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe,
        };
        let mut omhl = Omhl::open(&self.raw.config(), deps).expect("omhl opens");
        omhl.ensure_tenant(&acme()).expect("the tenant is created");

        omhl
    }

    fn seed(&self, author: Author, items: Vec<AppendItem>) -> AppendOutcome {
        let probe = FakeProbe::clean(snap('a'));
        let request = AppendRequest {
            tenant: acme(),
            author,
            key: None,
            items,
        };

        self.omhl(&probe)
            .append(&request)
            .expect("the seed is accepted")
    }

    fn seed_work_item(&self, id: &str) {
        self.seed(
            author(),
            vec![node_item(id, "work-item", work_item_data("t"))],
        );
    }

    fn seed_check(&self, id: &str, data: Value) {
        write_check_file(self.root.path(), id, &data);
        self.seed(loader_author(), vec![node_item(id, "check", data)]);
    }

    fn seed_after(&self, from: &str, to: &str) {
        self.seed(loader_author(), vec![relation_item("after", from, to)]);
    }

    /// Attestation de `checks.<slug>` para `work-items.w1` no HEAD; devolve o `record_id`.
    fn seed_attestation(&self, slug: &str, outcome: &str) -> Value {
        let data = executor_attestation_data(&format!("checks.{slug}"), W1, outcome, BYPRODUCT);
        let item = with_snapshot(node_item(&att(slug), "attestation", data), &snap('a'));

        json!(self.seed(executor_author(), vec![item]).records[0].id)
    }

    fn close_w1(&self) {
        let probe = FakeProbe::clean(snap('a'))
            .with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree('c'))));
        let snapshots = support::pr_gate::snapshots(&snap('a'), &tree('c'));
        support::pr_gate::close(&mut self.omhl(&probe), W1, &snapshots);
    }

    fn app<'a>(&'a self, probe: &'a FakeProbe, runner: &'a dyn CommandRunner) -> App<'a> {
        self.app_with(probe, runner, &RetryPolicy::default())
    }

    fn app_with<'a>(
        &'a self,
        probe: &'a FakeProbe,
        runner: &'a dyn CommandRunner,
        retry: &RetryPolicy,
    ) -> App<'a> {
        self.app_on_clock(probe, runner, retry, &self.raw.clock)
    }

    fn app_on_clock<'a>(
        &'a self,
        probe: &'a FakeProbe,
        runner: &'a dyn CommandRunner,
        retry: &RetryPolicy,
        clock: &'a dyn Clock,
    ) -> App<'a> {
        let deps = OmhlDeps {
            clock,
            ids: &self.raw.ids,
            probe,
        };
        let app = App::open(&self.raw.db_path(), &acme(), deps, retry)
            .unwrap_or_else(|error| panic!("the app opens: {error}"));

        app.with_project_root(self.root.path())
            .with_exec(runner, &self.blobs)
            .with_develop_mark(probe)
            .with_retry_after_run(after_run_policy())
    }

    fn root(&self) -> &str {
        self.root.path().to_str().expect("the root is UTF-8")
    }

    fn config(&self) -> Config {
        self.raw.config()
    }

    fn blob_files(&self) -> Vec<Vec<u8>> {
        files_under(&self.raw.dir.path().join("blobs"))
    }
}

fn files_under(dir: &Path) -> Vec<Vec<u8>> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flat_map(|entry| {
            let path = entry.expect("the blob entry is read").path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![fs::read(&path).expect("the blob file is read")]
            }
        })
        .collect()
}

fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

// Políticas de tentativa

fn after_run_policy() -> RetryPolicy {
    RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: vec![Duration::from_millis(10); 6],
    }
}

fn busy_policy() -> RetryPolicy {
    RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: Vec::new(),
    }
}

// Sonda e fatos do HEAD

fn facts(current: Option<&str>, containing: &[&str]) -> HeadFacts {
    HeadFacts {
        current: current.map(str::to_owned),
        rebase_head: None,
        containing: containing
            .iter()
            .map(|branch| (*branch).to_owned())
            .collect(),
        head: Some(SHA.to_owned()),
    }
}

fn develop() -> HeadFacts {
    facts(Some("develop"), &["develop"])
}

/// Sonda limpa no HEAD com os fatos e a marca dados; o `Receiver` conta as chamadas de `consume`.
fn probe_on(head_facts: HeadFacts, mark: bool) -> (FakeProbe, Receiver<()>) {
    let (tx, consumed) = mpsc::channel();
    let probe = FakeProbe::clean(snap('a'))
        .with_head_facts(head_facts)
        .with_develop_mark(mark)
        .with_consume_log(tx);

    (probe, consumed)
}

fn wi_probe() -> FakeProbe {
    probe_on(wi_head_facts(), false).0
}

fn dirty_probe() -> FakeProbe {
    FakeProbe::dirty()
        .with_head_facts(wi_head_facts())
        .with_develop_mark(false)
}

// Checks e respostas do runner

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn run_data(event: &str, blocking: bool, run: &[&str]) -> Value {
    let mut data = check_data(event, "x", blocking);
    data["run"] = json!(run);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);

    data
}

fn report(exit_code: Option<i32>) -> RunReport {
    RunReport {
        exit_code,
        stdout: vec![],
        stderr: vec![],
        timed_out: false,
        truncated: false,
        duration: Duration::from_millis(1500),
    }
}

fn ok(command: &[&str]) -> Response {
    (argv(command), Ok(report(Some(0))))
}

fn exit(command: &[&str], code: i32) -> Response {
    (argv(command), Ok(report(Some(code))))
}

fn answer(command: &[&str], result: Result<RunReport, RunError>) -> Response {
    (argv(command), result)
}

fn run_error(kind: RunErrorKind, message: &str) -> Result<RunReport, RunError> {
    Err(RunError {
        kind,
        message: message.to_owned(),
    })
}

fn received(specs: &Receiver<RunSpec>) -> Vec<Vec<String>> {
    specs.try_iter().map(|spec| spec.argv).collect()
}

// Ações e leitura pelo App

fn on_event(event: &str) -> Action {
    Action::CheckRun {
        target: CheckTarget::Event(event.to_owned()),
        pre_push: None,
    }
}

fn on_check(id: &str) -> Action {
    Action::CheckRun {
        target: CheckTarget::Check(id.to_owned()),
        pre_push: None,
    }
}

fn on_push(stdin: &str) -> Action {
    Action::CheckRun {
        target: CheckTarget::Event("before-pr".to_owned()),
        pre_push: Some(stdin.to_owned()),
    }
}

fn run_pr(app: &mut App<'_>) -> Result<Value, AppError> {
    app.dispatch(&Caller::cli(), &on_event("before-pr"))
}

fn att(slug: &str) -> String {
    format!("attestations.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaa.checks.{slug}.work-items.w1")
}

/// O objeto `node` da resposta de `get` (`null` quando o node não existe).
fn get(app: &mut App<'_>, id: &str) -> Value {
    let action = Action::Get { id: id.to_owned() };
    let mut response = app
        .dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("get is accepted: {error}"));

    response["node"].take()
}

/// Todos os nodes vigentes do tipo, todas as páginas.
fn list(app: &mut App<'_>, type_name: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut after_seq = None;

    loop {
        let action = Action::List {
            type_name: Some(type_name.to_owned()),
            id_prefix: None,
            limit: None,
            after_seq,
            where_eq: Vec::new(),
        };
        let page = app
            .dispatch(&Caller::cli(), &action)
            .unwrap_or_else(|error| panic!("list is accepted: {error}"));
        items.extend(
            page["items"]
                .as_array()
                .expect("the page has items")
                .clone(),
        );
        after_seq = page["next_after_seq"].as_u64();
        if after_seq.is_none() {
            return items;
        }
    }
}

fn details(error: &AppError) -> Value {
    error.to_json()["error"]["details"].clone()
}

/// Os detalhes como eram antes dos campos curtos do check reprovado no `context`.
fn without_context(error: &AppError) -> Value {
    let mut details = details(error);
    for detail in details.as_array_mut().expect("the error has details") {
        detail
            .as_object_mut()
            .expect("a detail is an object")
            .remove("context");
    }

    details
}

fn detail_paths(error: &AppError) -> Vec<Value> {
    details(error)
        .as_array()
        .expect("the error has details")
        .iter()
        .map(|detail| detail["path"].clone())
        .collect()
}

fn all_codes(error: &AppError) -> Vec<Value> {
    let mut codes = vec![json!(error.code())];
    codes.extend(
        details(error)
            .as_array()
            .expect("the error has details")
            .iter()
            .map(|detail| detail["code"].clone()),
    );

    codes
}

fn no_attestations() -> Vec<Value> {
    Vec::new()
}

// Testes

#[test]
fn it_should_attest_a_passing_check_with_the_head_commit() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, &["true"]));
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the check run succeeds");

    assert_eq!(
        output,
        json!({
            "event": "before-pr",
            "work_item": "work-items.w1",
            "snapshot": { "kind": "git-commit", "id": SHA },
            "recorded": true,
            "results": [{
                "check": "checks.lint", "outcome": "passed", "ok": true, "blocking": true,
                "exit_code": 0, "duration_ms": 1500, "timed_out": false, "truncated": false,
            }],
            "blocked": [],
        })
    );
    let attestation = get(&mut app, &att("lint"));
    assert_eq!(attestation["data"]["outcome"], json!("passed"));
    assert_eq!(attestation["data"]["exit_code"], json!(0));
    assert_eq!(attestation["data"]["command"], json!(["true"]));
    assert_eq!(
        attestation["snapshot"],
        json!({ "kind": "git-commit", "id": SHA })
    );
    assert_eq!(
        attestation["author"],
        json!({ "agent": "executor", "client": "executor" })
    );
    assert_eq!(
        get(&mut app, "checks.lint")["author"],
        json!({ "agent": "loader", "client": "loader" })
    );
    let spec = specs.try_recv().expect("the runner received a spec");
    assert_eq!(spec.argv, argv(&["true"]));
    assert_eq!(spec.timeout, Duration::from_secs(60));
    assert_eq!(spec.max_stream_bytes, 4_194_304);
    assert_eq!(spec.cwd, fx.root.path());
    assert_eq!(spec.project_root, fx.root.path());
}

#[test]
fn it_should_record_command_exit_code_duration_ms_timed_out_and_truncated_in_the_attestation() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let truncated = RunReport {
        truncated: true,
        ..report(Some(0))
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(LINT, Ok(truncated))]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    run_pr(&mut app).expect("the check run succeeds");

    assert_eq!(
        get(&mut app, &att("lint"))["data"],
        json!({
            "check": "checks.lint", "work_item": "work-items.w1", "outcome": "passed", "ok": true,
            "command": ["cargo", "clippy"], "exit_code": 0, "duration_ms": 1500,
            "timed_out": false, "truncated": true,
            "spec_digest": spec_digest(&run_data("before-pr", true, LINT)),
            "byproduct": "a939fea7c32eeb59326e08723c45c1bba9bebea969012c3b7b02710e5e773c48",
        })
    );
}

#[test]
fn it_should_run_every_check_of_the_event_in_after_order() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.docs", run_data("before-pr", true, DOCS));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    fx.seed_after("checks.build", "checks.lint");
    let (runner, specs) = FakeRunner::new(vec![ok(BUILD), ok(DOCS), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the check run succeeds");

    assert_eq!(received(&specs), vec![argv(DOCS), argv(LINT), argv(BUILD)]);
    let checks: Vec<Value> = output["results"]
        .as_array()
        .expect("the output has results")
        .iter()
        .map(|result| result["check"].clone())
        .collect();
    assert_eq!(
        checks,
        vec![
            json!("checks.docs"),
            json!("checks.lint"),
            json!("checks.build")
        ]
    );
}

#[test]
fn it_should_run_only_the_check_named_by_id() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(BUILD), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &on_check("checks.lint"))
        .expect("the check run succeeds");

    assert_eq!(received(&specs), vec![argv(LINT)]);
    assert_eq!(output["event"], json!("before-pr"));
    assert_eq!(output["results"][0]["check"], json!("checks.lint"));
    assert_eq!(output["results"].as_array().map(Vec::len), Some(1));
    let ids: Vec<Value> = list(&mut app, "attestation")
        .iter()
        .map(|node| node["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!(att("lint"))]);
}

#[test]
fn it_should_answer_check_not_found_for_an_unknown_id() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = app
        .dispatch(&Caller::cli(), &on_check("checks.nope"))
        .expect_err("an unknown check is refused");

    assert_eq!(error.code(), "check-not-found");
    assert_eq!(
        details(&error),
        json!([{ "path": "/check", "code": "check-not-found", "message": "check-not-found" }])
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_answer_check_not_mechanical_for_a_text_only_check() {
    let fx = Fixture::new();
    fx.seed_check("checks.text", check_data("before-pr", "x", true));
    let (runner, specs) = FakeRunner::new(vec![]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = app
        .dispatch(&Caller::cli(), &on_check("checks.text"))
        .expect_err("a text-only check is refused");

    assert_eq!(error.code(), "check-not-mechanical");
    assert_eq!(details(&error)[0]["path"], json!("/check"));
    assert_eq!(details(&error)[0]["code"], json!("check-not-mechanical"));
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_report_an_empty_result_when_the_event_has_no_checks() {
    let fx = Fixture::new();
    let (runner, specs) = FakeRunner::new(vec![]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the check run succeeds");

    assert_eq!(output["event"], json!("before-pr"));
    assert_eq!(output["work_item"], json!("work-items.w1"));
    assert_eq!(output["recorded"], json!(true));
    assert_eq!(output["results"], json!([]));
    assert_eq!(output["blocked"], json!([]));
    assert_eq!(output["checks_planned"], json!(0));
    assert_eq!(
        output["warnings"],
        json!([{ "code": "no-checks-for-event" }])
    );
    assert_eq!(output.get("snapshot"), Some(&Value::Null));
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_not_execute_native_text_only_checks() {
    let fx = Fixture::new();
    let (runner, specs) = FakeRunner::new(vec![]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let merge = app
        .dispatch(&Caller::cli(), &on_event("before-merge"))
        .expect("the before-merge run succeeds");
    let commit = app
        .dispatch(&Caller::cli(), &on_event("before-commit"))
        .expect("the before-commit run succeeds");

    for output in [&merge, &commit] {
        assert_eq!(output["results"], json!([]));
        assert_eq!(output.get("snapshot"), Some(&Value::Null));
    }
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_refuse_a_branch_whose_name_is_outside_the_pattern() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let (probe, _consumed) = probe_on(facts(Some("random-branch"), &["random-branch"]), false);
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the branch is refused");

    assert_eq!(error.code(), "branch-name-invalid");
    assert_eq!(details(&error)[0]["code"], json!("branch-name-invalid"));
    assert_eq!(
        details(&error)[0]["context"],
        json!({
            "kind": "branch-name-invalid",
            "event": "before-pr",
            "branch": "random-branch",
            "hint": "rename the branch to type/wi-<id>__<slug> with git branch -m",
        })
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_run_every_check_and_report_all_even_when_the_first_fails() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.docs", run_data("before-pr", false, DOCS));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![exit(BUILD, 1), exit(DOCS, 1), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the run fails");

    assert_eq!(received(&specs), vec![argv(BUILD), argv(DOCS), argv(LINT)]);
    assert_eq!(error.code(), "checks-failed");
    assert_eq!(
        without_context(&error),
        json!([
            {
                "path": "checks.build",
                "code": "failed",
                "message": "omhl byproduct checks.build work-items.w1 --tail 40",
            },
            {
                "path": "checks.docs",
                "code": "failed",
                "message": "non-blocking: omhl byproduct checks.docs work-items.w1 --tail 40",
            },
        ])
    );
    assert_eq!(
        get(&mut app, &att("build"))["data"]["outcome"],
        json!("failed")
    );
    assert_eq!(
        get(&mut app, &att("docs"))["data"]["outcome"],
        json!("failed")
    );
    assert_eq!(
        get(&mut app, &att("lint"))["data"]["outcome"],
        json!("passed")
    );
}

#[test]
fn it_should_fail_with_checks_failed_when_a_blocking_check_fails() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, _specs) = FakeRunner::new(vec![exit(LINT, 1)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the run fails");

    assert_eq!(error.code(), "checks-failed");
    assert_eq!(
        without_context(&error),
        json!([{
            "path": "checks.lint",
            "code": "failed",
            "message": "omhl byproduct checks.lint work-items.w1 --tail 40",
        }])
    );
    let data = get(&mut app, &att("lint"))["data"].clone();
    assert_eq!(data["outcome"], json!("failed"));
    assert_eq!(data["ok"], json!(false));
    assert_eq!(data["exit_code"], json!(1));
}

#[test]
fn it_should_succeed_when_only_a_non_blocking_check_fails() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", false, LINT));
    let (runner, _specs) = FakeRunner::new(vec![exit(LINT, 1)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the run succeeds");

    assert_eq!(
        output["results"],
        json!([{
            "check": "checks.lint", "outcome": "failed", "ok": false, "blocking": false,
            "exit_code": 1, "duration_ms": 1500, "timed_out": false, "truncated": false,
        }])
    );
    assert_eq!(output["blocked"], json!([]));
    assert_eq!(output["recorded"], json!(true));
}

#[test]
fn it_should_not_echo_the_command_output_in_the_error_details_when_the_attestation_was_recorded() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let noisy = RunReport {
        stdout: b"SECRET-OUT-7f2\n".to_vec(),
        stderr: b"SECRET-ERR-7f2\n".to_vec(),
        ..report(Some(1))
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(LINT, Ok(noisy))]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the run fails");

    assert_eq!(error.code(), "checks-failed");
    let text = error.to_json().to_string();
    assert!(!text.contains("SECRET-OUT-7f2"), "stdout leaked: {text}");
    assert!(!text.contains("SECRET-ERR-7f2"), "stderr leaked: {text}");
    assert!(!text.contains("output_tail"), "output_tail leaked: {text}");
    let context = details(&error)[0]["context"].clone();
    let keys: Vec<&str> = context
        .as_object()
        .expect("the detail carries the short failure fields")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "blocking",
            "duration_ms",
            "exit_code",
            "run",
            "timed_out",
            "timeout_secs",
            "truncated"
        ]
    );
    assert!(!get(&mut app, &att("lint")).is_null());
}

#[test]
fn it_should_not_rerun_commands_when_the_append_is_busy() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let (clock, attempts) = AttemptClock::new();
    let mut app = fx.app_on_clock(&probe, &runner, &busy_policy(), &clock);

    let result =
        with_write_lock_until_attempt(&fx.config(), &acme(), attempts, 2, || run_pr(&mut app));

    let output = result.expect("the run succeeds once the lock is released");
    assert_eq!(output["recorded"], json!(true));
    assert_eq!(received(&specs).len(), 1);
    assert!(!get(&mut app, &att("lint")).is_null());
}

#[test]
fn it_should_retry_the_append_with_its_own_budget_after_the_run() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let (clock, attempts) = AttemptClock::new();
    let mut app = fx.app_on_clock(&probe, &runner, &busy_policy(), &clock);

    // a 7ª tentativa é a última das 6 pausas de `after_run_policy`; o `busy_policy` só daria a 1ª
    let result =
        with_write_lock_until_attempt(&fx.config(), &acme(), attempts, 7, || run_pr(&mut app));

    result.expect("the append retries past the lock");
    assert!(!get(&mut app, &att("lint")).is_null());
    assert_eq!(received(&specs).len(), 1);
}

#[test]
fn it_should_report_outcome_when_the_append_stays_busy_after_the_run() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app_with(&probe, &runner, &busy_policy());

    let result = with_write_lock(&fx.config(), &acme(), |_| run_pr(&mut app));

    let error = result.expect_err("the append stays busy");
    assert_eq!(error.code(), "attestation-not-recorded");
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["path"], json!("checks.lint"));
    assert_eq!(details[0]["code"], json!("attestation-not-recorded"));
    let context = &details[0]["context"];
    assert_eq!(context["kind"], json!("busy-exhausted"));
    assert_eq!(context["expected"], json!("passed"));
    assert_eq!(context["check"], json!("checks.lint"));
    assert_eq!(context["work_item"], json!("work-items.w1"));
    assert_eq!(context["hint"], json!(BUSY_HINT));
    assert_eq!(context.get("stored"), None);
    assert!(!all_codes(&error).contains(&json!("BUSY")));
    assert_eq!(list(&mut app, "attestation"), no_attestations());
    assert_eq!(received(&specs).len(), 1);
}

#[test]
fn it_should_report_the_guard_refusal_when_the_append_is_refused_after_the_run() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let seeded = fx.seed_attestation("lint", "failed");
    let (runner, _specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the guard refuses the new outcome");

    assert_eq!(error.code(), "attestation-refused");
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["path"], json!("checks.lint"));
    assert_eq!(details[0]["code"], json!("outcome-immutable"));
    let stored = get(&mut app, &att("lint"));
    assert_eq!(stored["data"]["outcome"], json!("failed"));
    assert_eq!(stored["record_id"], seeded);
    assert!(!all_codes(&error).contains(&json!("BUSY")));
}

#[test]
fn it_should_record_timed_out_and_fail_on_timeout() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let timed_out = RunReport {
        timed_out: true,
        duration: Duration::from_millis(60_000),
        ..report(None)
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(LINT, Ok(timed_out))]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the timeout fails the run");

    assert_eq!(error.code(), "checks-failed");
    assert_eq!(
        get(&mut app, &att("lint"))["data"],
        json!({
            "check": "checks.lint", "work_item": "work-items.w1", "outcome": "failed", "ok": false,
            "command": ["cargo", "clippy"], "duration_ms": 60000, "timed_out": true,
            "truncated": false, "spec_digest": spec_digest(&run_data("before-pr", true, LINT)),
            "byproduct": "a939fea7c32eeb59326e08723c45c1bba9bebea969012c3b7b02710e5e773c48",
        })
    );
}

#[test]
fn it_should_fail_when_no_rule_matches_the_exit_code() {
    let fx = Fixture::new();
    let mut data = run_data("before-pr", true, LINT);
    data["expect"] = json!([
        { "exit": 0, "outcome": "passed" },
        { "exit": 1, "outcome": "passed_with_warning" },
    ]);
    fx.seed_check("checks.lint", data);
    let (runner, _specs) = FakeRunner::new(vec![exit(LINT, 3)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("an unmatched exit fails the run");

    assert_eq!(error.code(), "checks-failed");
    let data = get(&mut app, &att("lint"))["data"].clone();
    assert_eq!(data["outcome"], json!("failed"));
    assert_eq!(data["exit_code"], json!(3));
    assert_eq!(data["ok"], json!(false));
}

#[test]
fn it_should_refuse_to_run_when_the_head_is_dirty() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = dirty_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("a dirty tree is refused");

    assert_eq!(error.code(), "worktree-dirty");
    assert_eq!(details(&error)[0]["code"], json!("worktree-dirty"));
    assert_eq!(
        details(&error)[0]["context"]["kind"],
        json!("worktree-dirty")
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_reuse_the_attestation_id_and_base_on_a_repeat_with_the_same_outcome() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, _specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    run_pr(&mut app).expect("the first run succeeds");
    let first = get(&mut app, &att("lint"))["record_id"].clone();
    run_pr(&mut app).expect("the repeat succeeds");

    let attestations = list(&mut app, "attestation");
    assert_eq!(attestations.len(), 1);
    assert_eq!(attestations[0]["id"], json!(att("lint")));
    assert_eq!(attestations[0]["data"]["outcome"], json!("passed"));
    assert_ne!(get(&mut app, &att("lint"))["record_id"], first);
}

#[test]
fn it_should_keep_running_the_other_checks_after_an_immutable_outcome_conflict() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.docs", run_data("before-pr", true, DOCS));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    fx.seed_attestation("build", "failed");
    let (runner, specs) = FakeRunner::new(vec![ok(BUILD), exit(DOCS, 1), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the run fails");

    assert_eq!(received(&specs), vec![argv(BUILD), argv(DOCS), argv(LINT)]);
    assert_eq!(error.code(), "attestation-refused");
    assert_eq!(
        detail_paths(&error),
        vec![json!("checks.build"), json!("checks.docs")]
    );
    assert_eq!(details(&error)[0]["code"], json!("outcome-immutable"));
    assert_eq!(
        without_context(&error)[1],
        json!({
            "path": "checks.docs",
            "code": "failed",
            "message": "omhl byproduct checks.docs work-items.w1 --tail 40",
        })
    );
    assert_eq!(
        get(&mut app, &att("lint"))["data"]["outcome"],
        json!("passed")
    );
    assert_eq!(
        get(&mut app, &att("docs"))["data"]["outcome"],
        json!("failed")
    );
    assert_eq!(
        get(&mut app, &att("build"))["data"]["outcome"],
        json!("failed")
    );
}

#[test]
fn it_should_keep_running_the_other_checks_after_a_spawn_error() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let not_found = run_error(
        RunErrorKind::NotFound,
        "No such file or directory (os error 2)",
    );
    let (runner, specs) = FakeRunner::new(vec![answer(BUILD, not_found), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the spawn error fails the run");

    assert_eq!(received(&specs).len(), 2);
    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["path"], json!("checks.build"));
    assert_eq!(details[0]["code"], json!("spawn-failed"));
    assert_eq!(details[0]["context"]["kind"], json!("spawn-not-found"));
    assert_eq!(
        get(&mut app, &att("lint"))["data"]["outcome"],
        json!("passed")
    );
    assert!(get(&mut app, &att("build")).is_null());
}

#[test]
fn it_should_fit_the_attestation_of_the_worst_valid_check_in_the_record_limit() {
    let fx = Fixture::new();
    let work_item = format!("work-items.{}", "w".repeat(63));
    let check = format!("checks.{}", "c".repeat(63));
    let branch = format!("feat/wi-{}__x", "w".repeat(63));
    let run = vec!["a".repeat(854); 7];
    let mut data = check_data("before-pr", &"h".repeat(8000), true);
    data["run"] = json!(run);
    data["timeout_secs"] = json!(7200);
    data["expect"] = json!([{ "exit": -2_147_483_648_i64, "outcome": "passed_with_warning" }]);
    fx.seed_work_item(&work_item);
    let digest = spec_digest(&data);
    fx.seed_check(&check, data);
    let worst = RunReport {
        truncated: true,
        duration: Duration::from_millis(7_202_000),
        ..report(Some(i32::MIN))
    };
    let (runner, _specs) = FakeRunner::new(vec![(run.clone(), Ok(worst))]);
    let (probe, _consumed) = probe_on(facts(Some(&branch), &[&branch]), false);
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the worst valid check is attested");

    assert_eq!(
        output["results"][0]["outcome"],
        json!("passed_with_warning")
    );
    let id = format!("attestations.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaa.{check}.{work_item}");
    assert_eq!(id.len(), 200);
    assert_eq!(
        get(&mut app, &id)["data"],
        json!({
            "check": check, "work_item": work_item, "outcome": "passed_with_warning", "ok": true,
            "command": run, "exit_code": -2_147_483_648_i64, "duration_ms": 7_202_000,
            "timed_out": false, "truncated": true, "spec_digest": digest,
            "byproduct": "a939fea7c32eeb59326e08723c45c1bba9bebea969012c3b7b02710e5e773c48",
        })
    );
}

#[test]
fn it_should_record_nothing_and_carry_the_spawn_context_for_the_check_that_could_not_be_spawned() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    let not_found = run_error(
        RunErrorKind::NotFound,
        "No such file or directory (os error 2)",
    );
    let (runner, _specs) = FakeRunner::new(vec![answer(BUILD, not_found)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the spawn error fails the run");

    assert_eq!(list(&mut app, "attestation"), no_attestations());
    assert_eq!(error.code(), "check-not-run");
    let mut detail = details(&error)[0].clone();
    // o `message` do detalhe não é conferido; só a presença
    let message = detail
        .as_object_mut()
        .and_then(|object| object.remove("message"));
    assert!(message.is_some_and(|message| message.is_string()));
    assert_eq!(
        detail,
        json!({
            "path": "checks.build",
            "code": "spawn-failed",
            "context": {
                "kind": "spawn-not-found",
                "event": "before-pr",
                "check": "checks.build",
                "work_item": "work-items.w1",
                "branch": "feat/wi-w1__x",
                "argv": ["cargo", "build"],
                "cwd": fx.root(),
                "snapshot": { "kind": "git-commit", "id": SHA },
                "os_error": "No such file or directory (os error 2)",
                "hint": "`cargo` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again",
            },
        })
    );
}

#[test]
fn it_should_author_the_attestation_as_the_executor() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, _specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);
    let executor = json!({ "agent": "executor", "client": "executor" });

    app.dispatch(&Caller::cli(), &on_event("before-pr"))
        .expect("the cli run succeeds");
    let after_cli = get(&mut app, &att("lint"))["author"].clone();
    app.dispatch(&Caller::hook(), &on_event("before-pr"))
        .expect("the hook run succeeds");
    let after_hook = get(&mut app, &att("lint"))["author"].clone();

    assert_eq!(after_cli, executor);
    assert_eq!(after_hook, executor);
    assert_eq!(
        get(&mut app, "checks.lint")["author"],
        json!({ "agent": "loader", "client": "loader" })
    );
}

#[test]
fn it_should_not_rerun_the_first_check_when_the_second_append_stays_busy() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (inner, specs) = FakeRunner::new(vec![ok(BUILD), ok(LINT)]);
    let reached = Barrier::new(2);
    let locked = Barrier::new(2);
    let passed = AtomicBool::new(false);
    let runner = GatedRunner {
        inner,
        gate: argv(LINT),
        reached: &reached,
        locked: &locked,
        passed: &passed,
    };
    let probe = wi_probe();
    let retry = RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: vec![Duration::from_millis(10); 2],
    };
    let mut app = fx.app_with(&probe, &runner, &retry);
    let config = fx.config();
    let tenant = acme();
    let (done_tx, done) = mpsc::channel();

    let outcome = thread::scope(|scope| {
        scope.spawn(|| lock_at_gate(&config, &tenant, &reached, &locked, done));
        // um pânico do dispatch deixaria a thread da trava presa no `reached`: passa pelas barreiras antes de propagá-lo
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| run_pr(&mut app)));
        if !passed.load(Ordering::SeqCst) {
            reached.wait();
            locked.wait();
        }
        let _ = done_tx.send(());
        outcome
    });

    let error = outcome
        .unwrap_or_else(|payload| panic::resume_unwind(payload))
        .expect_err("the second append stays busy");
    assert_eq!(error.code(), "attestation-not-recorded");
    assert_eq!(detail_paths(&error), vec![json!("checks.lint")]);
    assert_eq!(
        get(&mut app, &att("build"))["data"]["outcome"],
        json!("passed")
    );
    assert!(get(&mut app, &att("lint")).is_null());
    assert_eq!(received(&specs), vec![argv(BUILD), argv(LINT)]);
}

#[test]
fn it_should_let_another_connection_append_while_a_command_runs() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (inner, _specs) = FakeRunner::new(vec![ok(LINT)]);
    let tenant = acme();
    let (appended_tx, appended) = mpsc::channel();
    let runner = AppendingRunner {
        inner,
        config: Config {
            busy_timeout: Duration::from_millis(50),
            ..fx.config()
        },
        tenant: &tenant,
        ids: &fx.raw.ids,
        appended: appended_tx,
    };
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the run succeeds");

    assert_eq!(appended.try_recv(), Ok(Ok(())));
    assert_eq!(output["recorded"], json!(true));
    assert!(!get(&mut app, "premises.concurrent").is_null());
}

#[test]
fn it_should_keep_running_the_other_checks_after_the_append_stays_busy() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(BUILD), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app_with(&probe, &runner, &busy_policy());

    let result = with_write_lock(&fx.config(), &acme(), |_| run_pr(&mut app));

    let error = result.expect_err("both appends stay busy");
    assert_eq!(received(&specs), vec![argv(BUILD), argv(LINT)]);
    assert_eq!(error.code(), "attestation-not-recorded");
    assert_eq!(
        detail_paths(&error),
        vec![json!("checks.build"), json!("checks.lint")]
    );
    for detail in details(&error).as_array().expect("the error has details") {
        assert_eq!(detail["context"]["kind"], json!("busy-exhausted"));
    }
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_report_spec_invalid_and_keep_running_the_other_checks_when_a_stored_spec_cannot_be_decoded()
 {
    let fx = Fixture::new();
    let mut bad = run_data("before-pr", true, &["cargo", "fmt"]);
    bad["expect"] = json!([{ "exit": 0, "stdout": "(", "outcome": "passed" }]);
    fx.seed_check("checks.a-bad", bad);
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the invalid spec fails the run");

    assert_eq!(received(&specs), vec![argv(LINT)]);
    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details.as_array().map(Vec::len), Some(1));
    assert_eq!(details[0]["path"], json!("checks.a-bad"));
    assert_eq!(details[0]["code"], json!("spec-invalid"));
    assert_eq!(
        details[0]["message"],
        json!("regex-invalid at /expect/0/stdout")
    );
    assert!(!error.to_json().to_string().contains("\"(\""));
    assert!(!get(&mut app, &att("lint")).is_null());
    assert!(get(&mut app, &att("a-bad")).is_null());
}

#[test]
fn it_should_report_spec_invalid_for_a_check_run_by_id_when_its_stored_spec_cannot_be_decoded() {
    let fx = Fixture::new();
    let mut bad = run_data("before-pr", true, &["cargo", "fmt"]);
    bad["expect"] = json!([{ "exit": 0, "stdout": "(", "outcome": "passed" }]);
    fx.seed_check("checks.a-bad", bad);
    let (runner, specs) = FakeRunner::new(vec![]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = app
        .dispatch(&Caller::cli(), &on_check("checks.a-bad"))
        .expect_err("the invalid spec fails the run");

    assert_eq!(error.code(), "check-not-run");
    assert_eq!(details(&error)[0]["code"], json!("spec-invalid"));
    assert_eq!(details(&error)[0]["path"], json!("checks.a-bad"));
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_stop_and_report_unsupported_platform_without_running_the_remaining_checks() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let unsupported = run_error(RunErrorKind::UnsupportedPlatform, "unsupported platform");
    let (runner, specs) = FakeRunner::new(vec![answer(BUILD, unsupported), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the platform is unsupported");

    assert_eq!(error.code(), "unsupported-platform");
    assert_eq!(received(&specs), vec![argv(BUILD)]);
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.build"));
    assert_eq!(details[0]["code"], json!("unsupported-platform"));
    assert_eq!(details[0]["context"]["kind"], json!("unsupported-platform"));
    assert_eq!(
        details[0]["context"]["os_error"],
        json!("unsupported platform")
    );
    assert_eq!(details[1]["path"], json!("checks.lint"));
    assert_eq!(details[1]["code"], json!("not-run"));
    assert_eq!(
        details[1]["context"],
        json!({
            "kind": "not-run", "event": "before-pr", "check": "checks.lint",
            "work_item": "work-items.w1", "branch": "feat/wi-w1__x",
            "argv": ["cargo", "clippy"], "cwd": fx.root(),
        })
    );
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_resolve_the_work_item_from_the_branch_name() {
    let fx = Fixture::new();
    fx.seed_work_item("work-items.w2");
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, _specs) = FakeRunner::new(vec![ok(LINT)]);
    let (probe, _consumed) = probe_on(
        facts(Some("fix/wi-w2__parser/edge"), &["fix/wi-w2__parser/edge"]),
        false,
    );
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the run succeeds");

    assert_eq!(output["work_item"], json!("work-items.w2"));
    let w2 = get(
        &mut app,
        "attestations.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaa.checks.lint.work-items.w2",
    );
    assert_eq!(w2["data"]["work_item"], json!("work-items.w2"));
    assert!(get(&mut app, &att("lint")).is_null());
}

#[test]
fn it_should_refuse_the_main_branch_without_running_anything() {
    let fx = Fixture::new();
    fx.seed_check("checks.tree", run_data("before-commit", true, &["true"]));
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let (probe, _consumed) = probe_on(facts(Some("main"), &["main"]), false);
    let mut app = fx.app(&probe, &runner);

    let error = app
        .dispatch(&Caller::cli(), &on_event("before-commit"))
        .expect_err("main is protected");

    assert_eq!(error.code(), "branch-protected");
    assert_eq!(
        details(&error)[0]["context"],
        json!({
            "kind": "branch-protected",
            "event": "before-commit",
            "branch": "main",
            "hint": "main receives no direct changes; create a branch type/wi-<id>__<slug>",
        })
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_refuse_a_detached_head_contained_in_several_branches() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let (probe, _consumed) = probe_on(facts(None, &["feat/wi-w1__x", "feat/wi-w2__y"]), false);
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("an ambiguous HEAD is refused");

    assert_eq!(error.code(), "head-ambiguous");
    assert_eq!(
        details(&error)[0]["context"],
        json!({
            "kind": "head-ambiguous",
            "event": "before-pr",
            "hint": "HEAD is detached and zero or several local branches contain it; switch to your branch with git switch",
        })
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_answer_work_item_not_found_for_a_branch_whose_work_item_does_not_exist() {
    let fx = Fixture::new();
    let (runner, specs) = FakeRunner::new(vec![]);
    let (probe, _consumed) = probe_on(facts(Some("feat/wi-w9__x"), &["feat/wi-w9__x"]), false);
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the work item does not exist");

    assert_eq!(error.code(), "work-item-not-found");
    assert_eq!(details(&error)[0]["path"], json!("/work_item"));
    assert_eq!(
        details(&error)[0]["context"],
        json!({
            "kind": "work-item-not-found",
            "event": "before-pr",
            "branch": "feat/wi-w9__x",
            "work_item": "work-items.w9",
        })
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_answer_work_item_closed_for_a_branch_whose_work_item_is_closed() {
    let fx = Fixture::new();
    fx.close_w1();
    let (runner, specs) = FakeRunner::new(vec![]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the work item is closed");

    assert_eq!(error.code(), "work-item-closed");
    assert_eq!(
        details(&error)[0]["context"]["work_item"],
        json!("work-items.w1")
    );
    assert_eq!(
        details(&error)[0]["context"]["branch"],
        json!("feat/wi-w1__x")
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_check_the_work_item_before_running_any_command() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    fx.close_w1();
    let (runner, specs) = FakeRunner::new(vec![ok(BUILD), ok(LINT)]);
    let probe = dirty_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the work item is closed");

    assert_eq!(error.code(), "work-item-closed");
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_run_without_recording_on_develop() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let (probe, _consumed) = probe_on(develop(), false);
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the dry run succeeds");

    assert_eq!(
        output,
        json!({
            "event": "before-pr",
            "work_item": null,
            "snapshot": { "kind": "git-commit", "id": SHA },
            "recorded": false,
            "results": [{
                "check": "checks.lint", "outcome": "passed", "ok": true, "blocking": true,
                "exit_code": 0, "duration_ms": 1500, "timed_out": false, "truncated": false,
            }],
            "blocked": [],
        })
    );
    assert_eq!(received(&specs).len(), 1);
    assert_eq!(list(&mut app, "attestation"), no_attestations());
    assert!(fx.blob_files().is_empty());
}

#[test]
fn it_should_fail_on_develop_with_the_output_tail_when_a_blocking_check_fails() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let failing = RunReport {
        stdout: b"line1\nline2\n".to_vec(),
        stderr: b"boom\n".to_vec(),
        ..report(Some(1))
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(LINT, Ok(failing))]);
    let (probe, _consumed) = probe_on(develop(), false);
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the dry run fails");

    assert_eq!(error.code(), "checks-failed");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.lint"));
    assert_eq!(details[0]["code"], json!("failed"));
    assert_eq!(
        details[0]["message"],
        json!("output in context.output_tail")
    );
    assert_eq!(
        details[0]["context"]["output_tail"],
        json!({ "stdout": ["line1", "line2"], "stderr": ["boom"] })
    );
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_answer_develop_mark_missing_on_develop_before_running_any_command() {
    let fx = Fixture::new();
    fx.seed_check("checks.tree", run_data("before-commit", true, &["true"]));
    let (runner, specs) = FakeRunner::new(vec![ok(&["true"])]);
    let (probe, _consumed) = probe_on(develop(), false);
    let mut app = fx.app(&probe, &runner);

    let error = app
        .dispatch(&Caller::cli(), &on_event("before-commit"))
        .expect_err("the develop mark is missing");

    assert_eq!(error.code(), "develop-mark-missing");
    assert_eq!(
        details(&error)[0]["context"],
        json!({
            "kind": "develop-mark-missing",
            "event": "before-commit",
            "branch": "develop",
            "hint": "create the mark with the /oh-my-hexflow:omhl-develop-mark command",
        })
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_not_consume_the_mark_when_develop_passes_with_it() {
    let fx = Fixture::new();
    fx.seed_check("checks.text", check_data("before-commit", "x", true));
    let (runner, _specs) = FakeRunner::new(vec![]);
    let (probe, consumed) = probe_on(develop(), true);
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &on_event("before-commit"))
        .expect("the dry run succeeds");

    assert_eq!(output["results"], json!([]));
    assert_eq!(output["recorded"], json!(false));
    assert_eq!(output["work_item"], Value::Null);
    assert_eq!(output.get("snapshot"), Some(&Value::Null));
    assert_eq!(consumed.try_iter().count(), 0);
}

#[test]
fn it_should_not_consume_the_mark_when_develop_fails() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, _specs) = FakeRunner::new(vec![exit(LINT, 1)]);
    let (probe, consumed) = probe_on(develop(), true);
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the dry run fails");

    assert_eq!(error.code(), "checks-failed");
    assert_eq!(consumed.try_iter().count(), 0);
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_carry_the_output_tail_when_the_append_stays_busy_after_the_run() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let stdout: String = (1..=25).map(|n| format!("out-{n}\n")).collect();
    let loud = RunReport {
        stdout: stdout.into_bytes(),
        stderr: format!("{}\n", "e".repeat(600)).into_bytes(),
        ..report(Some(0))
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(LINT, Ok(loud))]);
    let probe = wi_probe();
    let mut app = fx.app_with(&probe, &runner, &busy_policy());

    let result = with_write_lock(&fx.config(), &acme(), |_| run_pr(&mut app));

    let error = result.expect_err("the append stays busy");
    assert_eq!(error.code(), "attestation-not-recorded");
    let last_twenty: Vec<String> = (6..=25).map(|n| format!("out-{n}")).collect();
    assert_eq!(
        details(&error)[0]["context"]["output_tail"],
        json!({ "stdout": last_twenty, "stderr": ["e".repeat(500)] })
    );
}

#[test]
fn it_should_carry_the_stored_outcome_when_the_outcome_is_immutable() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    fx.seed_attestation("lint", "failed");
    let (runner, _specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the guard refuses the new outcome");

    assert_eq!(
        details(&error)[0]["context"],
        json!({
            "kind": "guard-refused",
            "event": "before-pr",
            "check": "checks.lint",
            "work_item": "work-items.w1",
            "branch": "feat/wi-w1__x",
            "argv": ["cargo", "clippy"],
            "cwd": fx.root(),
            "snapshot": { "kind": "git-commit", "id": SHA },
            "expected": "passed",
            "stored": "failed",
            "byproduct": "a939fea7c32eeb59326e08723c45c1bba9bebea969012c3b7b02710e5e773c48",
            "hint": "this snapshot already has the stored outcome for this check; change the tree to get a new verdict",
        })
    );
}

#[test]
fn it_should_not_require_the_develop_mark_for_a_before_pr_event() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let (probe, _consumed) = probe_on(develop(), false);
    let mut app = fx.app(&probe, &runner);

    let output = run_pr(&mut app).expect("the dry run succeeds without the mark");

    assert_eq!(output["recorded"], json!(false));
    assert_eq!(output["results"][0]["outcome"], json!("passed"));
    assert_eq!(received(&specs).len(), 1);
}

#[test]
fn it_should_not_require_the_develop_mark_for_a_before_merge_event() {
    let fx = Fixture::new();
    let gh = &["gh", "pr", "checks"];
    fx.seed_check("checks.ci", run_data("before-merge", true, gh));
    let (runner, specs) = FakeRunner::new(vec![ok(gh)]);
    let (probe, _consumed) = probe_on(develop(), false);
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &on_event("before-merge"))
        .expect("the dry run succeeds without the mark");

    assert_eq!(output["event"], json!("before-merge"));
    assert_eq!(output["recorded"], json!(false));
    assert_eq!(received(&specs), vec![argv(gh)]);
}

#[test]
fn it_should_consume_the_develop_mark_after_a_commit_on_develop() {
    let fx = Fixture::new();
    let (runner, _specs) = FakeRunner::new(vec![]);
    let (probe, consumed) = probe_on(develop(), true);
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &Action::ConsumeDevelopMark)
        .expect("the mark is consumed");

    assert_eq!(output, json!({ "consumed": true }));
    assert_eq!(consumed.try_iter().count(), 1);
}

#[test]
fn it_should_not_consume_the_develop_mark_after_a_commit_on_another_branch() {
    let fx = Fixture::new();
    let (runner, _specs) = FakeRunner::new(vec![]);
    let (probe, consumed) = probe_on(wi_head_facts(), true);
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &Action::ConsumeDevelopMark)
        .expect("the hook answers");

    assert_eq!(output, json!({ "consumed": false }));
    assert_eq!(consumed.try_iter().count(), 0);
}

const MARKER: &str = "TAIL-MARK-51";

fn marked_report(exit_code: i32) -> RunReport {
    RunReport {
        stdout: format!("{MARKER}\n").into_bytes(),
        ..report(Some(exit_code))
    }
}

const MARKED_BYPRODUCT: &[u8] = b"omhl-byproduct v1 stdout=13 stderr=0\nTAIL-MARK-51\n";

/// O marcador não aparece em nenhum tipo listado nem nos bytes do banco (com o `App` aberto); o único blob é o byproduct da saída, sem nada do context.
fn assert_marker_not_persisted(fx: &Fixture, app: &mut App<'_>) {
    for type_name in ["attestation", "check", "work-item", "premise"] {
        let listed = Value::Array(list(app, type_name)).to_string();
        assert!(!listed.contains(MARKER), "{type_name} has the marker");
        assert!(
            !listed.contains("output_tail"),
            "{type_name} has output_tail"
        );
        assert!(!listed.contains("\"context\""), "{type_name} has context");
    }
    let db = fs::read(fx.raw.db_path()).expect("the database file is read");
    assert!(!contains_bytes(&db, MARKER), "hexlog.db has the marker");
    if let Ok(wal) = fs::read(fx.raw.dir.path().join("hexlog.db-wal")) {
        assert!(
            !contains_bytes(&wal, MARKER),
            "hexlog.db-wal has the marker"
        );
    }
    assert_eq!(
        fx.blob_files(),
        vec![MARKED_BYPRODUCT.to_vec()],
        "the only blob is the byproduct of the marked output, with no error context"
    );
}

#[test]
fn it_should_never_persist_the_error_context_in_the_database() {
    let busy = Fixture::new();
    busy.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (busy_runner, _busy_specs) = FakeRunner::new(vec![answer(LINT, Ok(marked_report(0)))]);
    let busy_probe = wi_probe();
    let mut busy_app = busy.app_with(&busy_probe, &busy_runner, &busy_policy());
    let failed = Fixture::new();
    failed.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (failed_runner, _failed_specs) = FakeRunner::new(vec![answer(LINT, Ok(marked_report(1)))]);
    let failed_probe = wi_probe();
    let mut failed_app = failed.app(&failed_probe, &failed_runner);

    let busy_error = with_write_lock(&busy.config(), &acme(), |_| run_pr(&mut busy_app))
        .expect_err("the append stays busy");
    let failed_error = run_pr(&mut failed_app).expect_err("the blocking check fails");

    assert_eq!(
        details(&busy_error)[0]["context"]["output_tail"]["stdout"],
        json!([MARKER])
    );
    assert_marker_not_persisted(&busy, &mut busy_app);
    assert_eq!(failed_error.code(), "checks-failed");
    assert_marker_not_persisted(&failed, &mut failed_app);
}

#[test]
fn it_should_refuse_a_pushed_ref_that_is_not_the_checked_out_head_before_running_anything() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);
    let stdin = "refs/heads/feat/wi-w2__y bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feat/wi-w2__y 0000000000000000000000000000000000000000\n";

    let error = app
        .dispatch(&Caller::cli(), &on_push(stdin))
        .expect_err("the pushed ref is not the checked-out HEAD");

    assert_eq!(error.code(), "push-ref-not-checked-out");
    let context = details(&error)[0]["context"].clone();
    assert_eq!(context["kind"], json!("push-ref-not-checked-out"));
    assert_eq!(context["lref"], json!("refs/heads/feat/wi-w2__y"));
    assert_eq!(context["rref"], json!("refs/heads/feat/wi-w2__y"));
    assert_eq!(
        context["hint"],
        json!(
            "push refs/heads/feat/wi-w2__y from its own worktree; the pre-push hook attests only HEAD"
        )
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_report_nothing_to_attest_when_the_push_only_deletes_refs_or_pushes_tags() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);
    let stdin = "(delete) 0000000000000000000000000000000000000000 refs/heads/old bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nrefs/tags/v1 cccccccccccccccccccccccccccccccccccccccc refs/tags/v1 0000000000000000000000000000000000000000\n";

    let output = app
        .dispatch(&Caller::cli(), &on_push(stdin))
        .expect("nothing to attest is a success");

    assert_eq!(
        output,
        json!({ "event": "before-pr", "push": "nothing-to-attest", "results": [] })
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_bar_a_push_to_main_before_running_anything() {
    let fx = Fixture::new();
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let (runner, specs) = FakeRunner::new(vec![ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);
    let stdin = "refs/heads/feat/wi-w1__x aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa refs/heads/main 0000000000000000000000000000000000000000\n";

    let error = app
        .dispatch(&Caller::cli(), &on_push(stdin))
        .expect_err("a push to main is barred");

    assert_eq!(error.code(), "branch-protected");
    let context = details(&error)[0]["context"].clone();
    assert_eq!(context["kind"], json!("push-to-main"));
    assert_eq!(context["rref"], json!("refs/heads/main"));
    assert_eq!(
        context["hint"],
        json!("main receives no direct pushes; open a pull request")
    );
    assert_eq!(received(&specs), Vec::<Vec<String>>::new());
}

#[test]
fn it_should_stop_and_report_a_lost_parent_without_running_the_remaining_checks() {
    let fx = Fixture::new();
    fx.seed_check("checks.build", run_data("before-pr", true, BUILD));
    fx.seed_check("checks.lint", run_data("before-pr", true, LINT));
    let gone = run_error(RunErrorKind::ParentGone, "parent gone");
    let (runner, specs) = FakeRunner::new(vec![answer(BUILD, gone), ok(LINT)]);
    let probe = wi_probe();
    let mut app = fx.app(&probe, &runner);

    let error = run_pr(&mut app).expect_err("the lost parent stops the run");

    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.build"));
    assert_eq!(details[0]["code"], json!("not-run"));
    assert_eq!(details[0]["context"]["kind"], json!("parent-gone"));
    assert_eq!(
        details[0]["context"]["hint"],
        json!(
            "the process that started omhl exited before the checks ran; nothing ran; run the same command again"
        )
    );
    assert_eq!(details[1]["path"], json!("checks.lint"));
    assert_eq!(details[1]["code"], json!("not-run"));
    assert_eq!(
        details[1]["context"],
        json!({
            "kind": "not-run", "event": "before-pr", "check": "checks.lint",
            "work_item": "work-items.w1", "branch": "feat/wi-w1__x",
            "argv": ["cargo", "clippy"], "cwd": fx.root(),
        })
    );
    assert_eq!(received(&specs), vec![argv(BUILD)]);
    assert_eq!(list(&mut app, "attestation"), no_attestations());
}

#[test]
fn it_should_not_read_the_branch_when_the_develop_mark_is_absent_after_a_commit() {
    let fx = Fixture::new();
    let (runner, _specs) = FakeRunner::new(vec![]);
    let probe = FakeProbe::clean(snap('a')).with_develop_mark(false);
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &Action::ConsumeDevelopMark)
        .expect("the hook answers without reading the branch");

    assert_eq!(output, json!({ "consumed": false }));
}

#[test]
fn it_should_keep_the_develop_mark_silently_when_head_is_ambiguous_after_a_commit() {
    let fx = Fixture::new();
    let (runner, _specs) = FakeRunner::new(vec![]);
    let (probe, consumed) = probe_on(facts(None, &[]), true);
    let mut app = fx.app(&probe, &runner);

    let output = app
        .dispatch(&Caller::cli(), &Action::ConsumeDevelopMark)
        .expect("the hook answers");

    assert_eq!(output, json!({ "consumed": false }));
    assert_eq!(consumed.try_iter().count(), 0);
}
