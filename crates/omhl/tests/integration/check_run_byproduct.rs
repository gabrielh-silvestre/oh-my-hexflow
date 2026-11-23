use crate::support;

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use hexlog::{
    AppendItem, AppendOutcome, AppendRequest, Author, Config, Deps, Hexlog, Lineage, ListFilter,
    PageRequest,
};
use omhl::app::App;
use omhl::app::action::{Action, Caller, CheckTarget};
use omhl::app::error::AppError;
use omhl::app::retry::RetryPolicy;
use omhl::byproduct::{ByproductError, ByproductErrorKind, ByproductHash, ByproductStore};
use omhl::facade::{Omhl, OmhlDeps};
use omhl::mechanical::redact::Redactor;
use omhl::ports::blob::BlobStore;
use omhl::runner::{CommandRunner, RunError, RunReport, RunSpec};
use serde_json::{Value, json};
use support::busy::with_write_lock;
use support::ext::write_check_file;
use support::omhl::{acme, check_data, node_item, with_snapshot, work_item_data};
use support::raw::RawHarness;
use support::runner::FakeRunner;
use support::{
    EmptyNative, FakeProbe, FixedClock, SeqIds, author, executor_attestation_data, executor_author,
    loader_author, sid, snap, tname, wi_head_facts,
};

const W1: &str = "work-items.w1";
const FMT_ATTESTATION: &str =
    "attestations.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaa.checks.fmt.work-items.w1";
const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
// blobs e hashes B1, B2 e B3 do contrato (oráculo `printf '<bytes>' | sha256sum`)
const B1_HASH: &str = "37f02437dd2ac574e241286d50ea54a38959ea7730c88dfb04629a26e5bbfb8a";
const B1_BLOB: &[u8] = b"omhl-byproduct v1 stdout=3 stderr=0\nok\n";
const B2_HASH: &str = "d3d0db0265dcfa8f15341ae83fc19fb2d4886a5292527ba9b176e99a130ab8bd";
const B2_BLOB: &[u8] = b"omhl-byproduct v1 stdout=4 stderr=4\nout\nerr\n";
const B3_HASH: &str = "e41e7f67536cb997cc36915fd5a48862b31d266fe4ec10649207789f971ac4ae";
const B3_BLOB: &[u8] = b"omhl-byproduct v1 stdout=7 stderr=0\npartial";
const REDACTED_HASH: &str = "01a5155783691836d9e324f0699393db05921d63d0739e3f492b605a282b1e1a";
const REDACTED_BLOB: &[u8] = b"omhl-byproduct v1 stdout=21 stderr=0\nAPI_TOKEN=[REDACTED]\n";
const SECRET_LINE: &[u8] = b"API_TOKEN=abcdef123456\n";
const STREAM_BYTES: usize = 4 * 1024 * 1024;
const REDACTION_CEILING: Duration = Duration::from_secs(3);
const BLOB_FAILED_HINT: &str =
    "the output store is not writable; fix its permissions or free disk space";

type Response = (Vec<String>, Result<RunReport, RunError>);

// Banco e loja de blobs de um teste; as sementes passam por um `Omhl` largado antes do `App`.
struct Fixture {
    raw: RawHarness,
    blobs: BlobStore,
}

impl Fixture {
    fn new() -> Fixture {
        let raw = RawHarness::new();
        let blobs = BlobStore::new(&raw.db_path(), &acme());
        let fixture = Fixture { raw, blobs };
        fixture.seed(
            author(),
            vec![node_item(W1, "work-item", work_item_data("t"))],
        );

        fixture
    }

    fn seed(&self, author: Author, items: Vec<AppendItem>) -> AppendOutcome {
        let probe = FakeProbe::clean(snap('a'));
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe: &probe,
        };
        let mut omhl = Omhl::open(&self.raw.config(), deps).expect("omhl opens");
        omhl.ensure_tenant(&acme()).expect("the tenant is created");
        let request = AppendRequest {
            tenant: acme(),
            author,
            key: None,
            items,
        };

        omhl.append(&request).expect("the seed is accepted")
    }

    fn seed_check(&self, id: &str, run: &[&str]) {
        self.seed_check_data(id, mechanical_check("before-pr", run));
    }

    fn seed_check_data(&self, id: &str, data: Value) {
        write_check_file(self.raw.dir.path(), id, &data);
        self.seed(loader_author(), vec![node_item(id, "check", data)]);
    }

    fn app<'a>(
        &'a self,
        probe: &'a FakeProbe,
        runner: &'a dyn CommandRunner,
        store: &'a dyn ByproductStore,
        open_retry: &RetryPolicy,
    ) -> App<'a> {
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe,
        };
        let app = App::open(&self.raw.db_path(), &acme(), deps, open_retry)
            .unwrap_or_else(|error| panic!("the app opens: {error}"));

        app.with_project_root(self.raw.dir.path())
            .with_exec(runner, store)
            .with_retry_after_run(RetryPolicy {
                busy_timeout: Duration::from_millis(10),
                pauses: vec![Duration::from_millis(10), Duration::from_millis(10)],
            })
    }

    fn stored_blob(&self, hash: &str) -> Option<Vec<u8>> {
        self.blobs
            .get(&byproduct_hash(hash))
            .expect("the blob store reads")
    }
}

/// Fixture com `checks.fmt` (`before-pr`, `["true"]`) gravado.
fn fixture_with_fmt() -> Fixture {
    let fx = Fixture::new();
    fx.seed_check("checks.fmt", &["true"]);

    fx
}

// Checks, respostas do runner e leitura pelo App

fn mechanical_check(event: &str, run: &[&str]) -> Value {
    let mut data = check_data(event, "run it", true);
    data["run"] = json!(run);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);

    data
}

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn report(stdout: &[u8], stderr: &[u8]) -> RunReport {
    RunReport {
        exit_code: Some(0),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
        timed_out: false,
        truncated: false,
        duration: Duration::from_millis(5),
    }
}

fn answer(command: &[&str], report: RunReport) -> Response {
    (argv(command), Ok(report))
}

fn received(specs: &Receiver<RunSpec>) -> Vec<Vec<String>> {
    specs.try_iter().map(|spec| spec.argv).collect()
}

fn probe() -> FakeProbe {
    FakeProbe::clean(snap('a')).with_head_facts(wi_head_facts())
}

fn byproduct_hash(text: &str) -> ByproductHash {
    ByproductHash::parse(text).expect("a valid byproduct hash")
}

fn run_before_pr(app: &mut App<'_>) -> Result<Value, AppError> {
    let action = Action::CheckRun {
        target: CheckTarget::Event("before-pr".to_owned()),
        pre_push: None,
    };

    app.dispatch(&Caller::cli(), &action)
}

fn get(app: &mut App<'_>, id: &str) -> Value {
    let action = Action::Get { id: id.to_owned() };
    let mut response = app
        .dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("get is accepted: {error}"));

    response["node"].take()
}

fn attestations(app: &mut App<'_>) -> Value {
    let action = Action::List {
        type_name: Some("attestation".to_owned()),
        id_prefix: None,
        limit: None,
        after_seq: None,
        where_eq: Vec::new(),
    };

    app.dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("list is accepted: {error}"))
}

fn no_attestations() -> Value {
    json!({ "items": [] })
}

fn details(error: &AppError) -> Value {
    error.to_json()["error"]["details"].clone()
}

// Segunda conexão ao mesmo banco

fn with_reader<R>(config: &Config, body: impl FnOnce(&mut Hexlog<'_>) -> R) -> R {
    let clock = FixedClock::default();
    let ids = SeqIds::new();
    let deps = Deps {
        clock: &clock,
        ids: &ids,
        catalog: omhl::graphs::catalog().expect("the embedded catalogs build"),
        native: &EmptyNative,
    };
    let mut hexlog = Hexlog::open(config, deps).expect("the reader opens the database");

    body(&mut hexlog)
}

fn attestation_count(config: &Config) -> usize {
    let filter = ListFilter {
        type_name: Some(tname("attestation")),
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    };
    let page = PageRequest {
        limit: 100,
        after_seq: None,
    };

    with_reader(config, |hexlog| {
        hexlog
            .list(&acme(), &filter, &page)
            .expect("the reader lists attestations")
            .items
            .len()
    })
}

// Lojas de blob falsas

/// Conta as attestations visíveis a outra conexão no instante do `put` e delega ao `BlobStore`.
struct CountingStore {
    config: Config,
    counts: mpsc::Sender<usize>,
    inner: BlobStore,
}

impl ByproductStore for CountingStore {
    fn put(&self, bytes: &[u8]) -> Result<ByproductHash, ByproductError> {
        // o teste pode já ter largado o Receiver; o envio sem ele não muda o `put`
        let _ = self.counts.send(attestation_count(&self.config));

        self.inner.put(bytes)
    }

    fn get(&self, hash: &ByproductHash) -> Result<Option<Vec<u8>>, ByproductError> {
        self.inner.get(hash)
    }
}

struct FullDiskStore;

impl ByproductStore for FullDiskStore {
    fn put(&self, _bytes: &[u8]) -> Result<ByproductHash, ByproductError> {
        Err(ByproductError {
            kind: ByproductErrorKind::Io,
            message: "disk full".to_owned(),
        })
    }

    fn get(&self, _hash: &ByproductHash) -> Result<Option<Vec<u8>>, ByproductError> {
        Ok(None)
    }
}

// Testes

#[test]
fn it_should_store_the_full_output_in_the_blob_store_and_record_its_hash() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    let output = run_before_pr(&mut app);

    assert!(output.is_ok(), "the run succeeds: {output:?}");
    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["byproduct"],
        json!(B1_HASH)
    );
    assert_eq!(fx.stored_blob(B1_HASH), Some(B1_BLOB.to_vec()));
}

#[test]
fn it_should_put_stderr_after_stdout_in_the_blob() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"out\n", b"err\n"))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["byproduct"],
        json!(B2_HASH)
    );
    assert_eq!(fx.stored_blob(B2_HASH), Some(B2_BLOB.to_vec()));
}

#[test]
fn it_should_record_the_truncated_flag_when_the_output_was_cut() {
    let fx = fixture_with_fmt();
    let cut = RunReport {
        truncated: true,
        ..report(b"partial", b"")
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], cut)]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    let output = run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(output["results"][0]["truncated"], json!(true));
    let data = get(&mut app, FMT_ATTESTATION)["data"].clone();
    assert_eq!(data["truncated"], json!(true));
    assert_eq!(data["byproduct"], json!(B3_HASH));
    assert_eq!(fx.stored_blob(B3_HASH), Some(B3_BLOB.to_vec()));
}

#[test]
fn it_should_keep_the_byproduct_when_the_attestation_append_is_refused() {
    let fx = fixture_with_fmt();
    let stored = with_snapshot(
        node_item(
            FMT_ATTESTATION,
            "attestation",
            executor_attestation_data("checks.fmt", W1, "failed", EMPTY_SHA),
        ),
        &snap('a'),
    );
    fx.seed(executor_author(), vec![stored]);
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    let error = run_before_pr(&mut app).expect_err("the guard refuses the new outcome");

    assert_eq!(error.code(), "attestation-refused");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.fmt"));
    assert_eq!(details[0]["code"], json!("outcome-immutable"));
    assert_eq!(details[0]["context"]["byproduct"], json!(B1_HASH));
    assert_eq!(details[0]["context"]["expected"], json!("passed"));
    assert_eq!(details[0]["context"]["stored"], json!("failed"));
    assert_eq!(fx.stored_blob(B1_HASH), Some(B1_BLOB.to_vec()));
    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["byproduct"],
        json!(EMPTY_SHA)
    );
}

#[test]
fn it_should_print_the_byproduct_hash_when_the_append_stays_busy() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let open_retry = RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: vec![],
    };
    let mut app = fx.app(&probe, &runner, &fx.blobs, &open_retry);

    let result = with_write_lock(&fx.raw.config(), &acme(), |_| run_before_pr(&mut app));

    let error = result.expect_err("the append stays busy");
    assert_eq!(error.code(), "attestation-not-recorded");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.fmt"));
    assert_eq!(details[0]["context"]["byproduct"], json!(B1_HASH));
    assert_eq!(details[0]["context"]["expected"], json!("passed"));
    assert_eq!(details[0]["context"]["kind"], json!("busy-exhausted"));
    assert!(!error.to_json().to_string().contains("BUSY"));
    assert_eq!(fx.stored_blob(B1_HASH), Some(B1_BLOB.to_vec()));
    assert_eq!(attestations(&mut app), no_attestations());
}

#[test]
fn it_should_keep_the_byproduct_hash_stable_for_identical_output() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    run_before_pr(&mut app).expect("the first run succeeds");
    let after_first = get(&mut app, FMT_ATTESTATION)["data"]["byproduct"].clone();
    run_before_pr(&mut app).expect("the second run succeeds");
    let after_second = get(&mut app, FMT_ATTESTATION)["data"]["byproduct"].clone();

    assert_eq!(after_first, json!(B1_HASH));
    assert_eq!(after_second, json!(B1_HASH));
    let revisions = with_reader(&fx.raw.config(), |hexlog| {
        hexlog
            .lineage_records(&acme(), &Lineage::Node(sid(FMT_ATTESTATION)))
            .expect("the lineage is read")
    });
    let hashes: Vec<Value> = revisions
        .iter()
        .map(|revision| revision.data.clone().expect("a live revision")["byproduct"].clone())
        .collect();
    assert_eq!(hashes, vec![json!(B1_HASH), json!(B1_HASH)]);
}

#[test]
fn it_should_serve_through_byproduct_the_blob_that_check_run_wrote() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"a\nb\n", b"warn\n"))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());
    run_before_pr(&mut app).expect("the run succeeds");
    let action = Action::Byproduct {
        check: "checks.fmt".to_owned(),
        work_item: W1.to_owned(),
        tail: 10,
    };

    let response = app
        .dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("byproduct is accepted: {error}"));

    assert_eq!(response["lines"], json!(["a", "b", "warn"]));
}

#[test]
fn it_should_rewrite_a_truncated_blob_when_the_check_runs_again() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());
    run_before_pr(&mut app).expect("the first run succeeds");
    let path = fx
        .raw
        .dir
        .path()
        .join("blobs/by-db/hexlog.db/acme")
        .join(&B1_HASH[..2])
        .join(B1_HASH);
    std::fs::write(&path, b"").expect("the blob is truncated");

    run_before_pr(&mut app).expect("the second run succeeds");

    assert_eq!(fx.stored_blob(B1_HASH), Some(B1_BLOB.to_vec()));
}

#[test]
fn it_should_not_store_the_environment_in_the_byproduct_or_in_the_attestation() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(fx.stored_blob(B1_HASH), Some(B1_BLOB.to_vec()));
    let data = get(&mut app, FMT_ATTESTATION)["data"].clone();
    let mut keys: Vec<&String> = data
        .as_object()
        .expect("data is an object")
        .keys()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "byproduct",
            "check",
            "command",
            "duration_ms",
            "exit_code",
            "ok",
            "outcome",
            "spec_digest",
            "timed_out",
            "truncated",
            "work_item"
        ]
    );
}

#[test]
fn it_should_store_the_byproduct_before_appending_the_attestation() {
    let fx = fixture_with_fmt();
    let (counts, counted) = mpsc::channel();
    let store = CountingStore {
        config: fx.raw.config(),
        counts,
        inner: BlobStore::new(&fx.raw.db_path(), &acme()),
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(b"ok\n", b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &store, &RetryPolicy::default());

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(counted.try_iter().collect::<Vec<_>>(), vec![0]);
    let listed = attestations(&mut app);
    assert_eq!(listed["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["byproduct"],
        json!(B1_HASH)
    );
}

#[test]
fn it_should_stop_and_report_when_the_blob_store_cannot_be_written_recording_nothing() {
    let fx = Fixture::new();
    fx.seed_check("checks.a-fmt", &["echo", "a"]);
    fx.seed_check("checks.b-lint", &["echo", "b"]);
    let (runner, specs) = FakeRunner::new(vec![
        answer(&["echo", "a"], report(b"a\n", b"")),
        answer(&["echo", "b"], report(b"b\n", b"")),
    ]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &FullDiskStore, &RetryPolicy::default());

    let error = run_before_pr(&mut app).expect_err("the blob store failure stops the run");

    assert_eq!(error.code(), "check-not-run");
    let details = details(&error);
    assert_eq!(details[0]["path"], json!("checks.a-fmt"));
    assert_eq!(details[0]["code"], json!("not-run"));
    assert_eq!(details[0]["context"]["kind"], json!("blob-failed"));
    assert_eq!(details[0]["context"]["expected"], json!("passed"));
    assert!(
        details[0]["context"]["os_error"]
            .as_str()
            .is_some_and(|text| text.contains("disk full"))
    );
    assert_eq!(details[0]["context"]["hint"], json!(BLOB_FAILED_HINT));
    assert_eq!(details[1]["path"], json!("checks.b-lint"));
    assert_eq!(details[1]["code"], json!("not-run"));
    assert_eq!(received(&specs), vec![argv(&["echo", "a"])]);
    assert_eq!(attestations(&mut app), no_attestations());
}

#[test]
fn it_should_fail_a_stream_rule_and_say_so_when_the_output_was_cut() {
    let fx = Fixture::new();
    let mut data = mechanical_check("before-pr", &["true"]);
    data["expect"] = json!([{ "exit": 0, "stdout": "done$", "outcome": "passed" }]);
    fx.seed_check_data("checks.fmt", data);
    let cut = RunReport {
        truncated: true,
        ..report(b"done", b"")
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], cut)]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    let error = run_before_pr(&mut app).expect_err("the stream rule cannot be checked");

    assert_eq!(error.code(), "checks-failed");
    assert_eq!(
        details(&error)[0]["message"],
        json!(
            "output was cut at 4 MiB; the rule could not be checked; omhl byproduct checks.fmt work-items.w1 --tail 40"
        )
    );
    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["outcome"],
        json!("failed")
    );
}

#[test]
fn it_should_not_blame_the_cut_when_the_failure_comes_from_the_exit_code() {
    let fx = Fixture::new();
    let mut data = mechanical_check("before-pr", &["true"]);
    data["expect"] = json!([{ "exit": 0, "stdout": "x$", "outcome": "passed" }]);
    fx.seed_check_data("checks.fmt", data);
    let cut = RunReport {
        exit_code: Some(2),
        truncated: true,
        ..report(b"done", b"")
    };
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], cut)]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    let error = run_before_pr(&mut app).expect_err("the exit code has no rule");

    assert_eq!(error.code(), "checks-failed");
    let message = details(&error)[0]["message"]
        .as_str()
        .expect("the message is text")
        .to_owned();
    assert!(!message.contains("output was cut"), "message: {message}");
}

#[test]
fn it_should_store_the_masked_output_and_record_the_hash_of_the_masked_blob() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(SECRET_LINE, b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["byproduct"],
        json!(REDACTED_HASH)
    );
    assert_eq!(fx.stored_blob(REDACTED_HASH), Some(REDACTED_BLOB.to_vec()));
}

#[test]
fn it_should_serve_the_masked_output_through_byproduct() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(SECRET_LINE, b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());
    run_before_pr(&mut app).expect("the run succeeds");
    let action = Action::Byproduct {
        check: "checks.fmt".to_owned(),
        work_item: W1.to_owned(),
        tail: 10,
    };

    let response = app
        .dispatch(&Caller::cli(), &action)
        .unwrap_or_else(|error| panic!("byproduct is accepted: {error}"));

    assert_eq!(response["lines"], json!(["API_TOKEN=[REDACTED]"]));
}

#[test]
fn it_should_evaluate_the_check_on_the_raw_output_not_on_the_masked_one() {
    let fx = Fixture::new();
    let mut data = mechanical_check("before-pr", &["true"]);
    data["expect"] = json!([{ "exit": 0, "stdout": "abcdef123456", "outcome": "passed" }]);
    fx.seed_check_data("checks.fmt", data);
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(SECRET_LINE, b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &fx.blobs, &RetryPolicy::default());

    run_before_pr(&mut app).expect("the pattern matches the raw output");

    assert_eq!(
        get(&mut app, FMT_ATTESTATION)["data"]["outcome"],
        json!("passed")
    );
}

#[test]
fn it_should_keep_the_output_tail_unmasked_when_the_blob_cannot_be_stored() {
    let fx = fixture_with_fmt();
    let (runner, _specs) = FakeRunner::new(vec![answer(&["true"], report(SECRET_LINE, b""))]);
    let probe = probe();
    let mut app = fx.app(&probe, &runner, &FullDiskStore, &RetryPolicy::default());

    let error = run_before_pr(&mut app).expect_err("the blob store failure stops the run");

    assert_eq!(
        details(&error)[0]["context"]["output_tail"]["stdout"],
        json!(["API_TOKEN=abcdef123456"])
    );
}

/// Linha de 100 bytes (com o `\n`) para o fluxo do teste de desempenho.
fn padded_line(text: &str) -> Vec<u8> {
    let mut line = format!("{text:<99}").into_bytes();
    line.truncate(99);
    line.push(b'\n');

    line
}

/// Linhas que disparam o pré-filtro (`token`, `secret` ou `://`); duas em cada três levam um segredo de verdade.
fn trigger_stream() -> Vec<u8> {
    let mut stream = Vec::with_capacity(STREAM_BYTES);
    for number in 0.. {
        let line = match number % 3 {
            0 => padded_line(&format!("db_token=fakesecret{number:09}")),
            1 => padded_line(&format!("postgres://app:fakepass{number:09}@db:5432/x")),
            _ => padded_line(&format!("checking secret store entry {number}")),
        };
        if stream.len() + line.len() > STREAM_BYTES {
            break;
        }
        stream.extend_from_slice(&line);
    }

    stream
}

/// Uma linha só, sem `\n`, com uma palavra-chave e um segredo no meio.
fn single_line_stream() -> Vec<u8> {
    let filler = b"lorem ipsum dolor sit amet ".repeat(STREAM_BYTES / 54);
    let mut stream = filler.clone();
    stream.extend_from_slice(b" token=abcdef123456 ");
    stream.extend_from_slice(&filler);

    stream
}

/// Tempo de `Redactor::new()` mais a redação dos dois fluxos (`stdout` e `stderr`) com a mesma entrada.
fn timed_redaction(stream: &[u8]) -> (Duration, Vec<u8>) {
    let started = Instant::now();
    let redactor = Redactor::new();
    let stdout = redactor.redact(stream);
    let _stderr = redactor.redact(stream);

    (started.elapsed(), stdout)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// Maior de 5 execuções em debug (regex com `perf` e `opt-level = 3`): 0,43 s com linhas curtas e 0,17 s com linha única.
#[test]
fn it_should_redact_4_mib_of_short_trigger_lines_per_stream_within_the_ceiling() {
    let stream = trigger_stream();

    let (elapsed, redacted) = timed_redaction(&stream);

    assert!(elapsed < REDACTION_CEILING, "took {elapsed:?}");
    assert!(contains(&redacted, b"db_token=[REDACTED]"));
    assert!(!contains(&redacted, b"fakesecret"));
    assert!(!contains(&redacted, b"fakepass"));
}

#[test]
fn it_should_redact_a_single_4_mib_line_per_stream_within_the_ceiling() {
    let stream = single_line_stream();

    let (elapsed, redacted) = timed_redaction(&stream);

    assert!(elapsed < REDACTION_CEILING, "took {elapsed:?}");
    assert!(contains(&redacted, b"token=[REDACTED] "));
    assert!(!contains(&redacted, b"abcdef123456"));
}
