use crate::support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use hexlog::{AppendItem, AppendOutcome, AppendRequest, Author, Snapshot};
use omhl::app::App;
use omhl::app::action::{Action, Caller, CheckTarget};
use omhl::app::error::AppError;
use omhl::app::retry::RetryPolicy;
use omhl::byproduct::ByproductStore;
use omhl::facade::{Omhl, OmhlDeps};
use omhl::ports::blob::BlobStore;
use omhl::probe::HeadFacts;
use serde_json::{Value, json};
use support::omhl::{
    acme, attestation_id_for, check_data, node_item, with_snapshot, work_item_data,
};
use support::raw::RawHarness;
use support::runner::FakeRunner;
use support::{
    FakeProbe, author, executor_attestation_data, executor_author, loader_author, snap,
    wi_head_facts,
};

const W1: &str = "work-items.w1";
const FMT: &str = "checks.fmt";
const LINT: &str = "checks.lint";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const B1: &str = "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1";
const B2: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
const B3: &str = "b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3";
const REAL_BLOB: &[u8] = b"omhl-byproduct v1 stdout=3 stderr=0\nok\n";
// Semeadura da medida de desempenho: 20 checks x 10 work items = 200 pares, 10 árvores cada.
const BULK_CHECKS: usize = 20;
const BULK_WORK_ITEMS: usize = 10;
const BULK_TREES: usize = 10;
const SEED_BATCH: usize = 50;
const PRUNE_CEILING: Duration = Duration::from_secs(10);

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
            &snap('a'),
        );
        for check in [FMT, LINT] {
            fixture.seed(
                loader_author(),
                vec![node_item(
                    check,
                    "check",
                    check_data("before-commit", "x", true),
                )],
                &snap('a'),
            );
        }

        fixture
    }

    fn seed(&self, author: Author, items: Vec<AppendItem>, head: &Snapshot) -> AppendOutcome {
        let probe = FakeProbe::clean(head.clone());
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

    /// Attestation de `check` para `work_item` na árvore `fill` apontando para `byproduct`.
    fn attest(&self, fill: char, check: &str, work_item: &str, byproduct: &str) {
        let data = executor_attestation_data(check, work_item, "passed", byproduct);
        let snapshot = snap(fill);
        let id = attestation_id_for(&snapshot, check, work_item);
        let item = with_snapshot(node_item(&id, "attestation", data), &snapshot);
        self.seed(executor_author(), vec![item], &snapshot);
    }

    fn app<'a>(&'a self, probe: &'a FakeProbe, runner: &'a FakeRunner) -> App<'a> {
        self.app_without_retention(probe, runner)
            .with_retention(&self.blobs)
    }

    fn app_without_retention<'a>(
        &'a self,
        probe: &'a FakeProbe,
        runner: &'a FakeRunner,
    ) -> App<'a> {
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe,
        };

        App::open(&self.raw.db_path(), &acme(), deps, &RetryPolicy::default())
            .unwrap_or_else(|error| panic!("the app opens: {error}"))
            .with_project_root(self.raw.dir.path())
            .with_exec(runner, &self.blobs)
            .with_develop_mark(probe)
    }

    /// Pasta do (banco, tenant) onde o `BlobStore` grava.
    fn root(&self) -> PathBuf {
        self.raw.dir.path().join("blobs/by-db/hexlog.db/acme")
    }

    fn blob_path(&self, name: &str) -> PathBuf {
        self.root().join(&name[..2]).join(name)
    }

    /// Cria o arquivo `name` (hash ou temporário) com a data de `age` atrás.
    fn file_aged(&self, name: &str, age: Duration) -> PathBuf {
        let path = self.blob_path(name);
        write_aged(&path, b"x", age);

        path
    }

    fn stamp(&self) -> PathBuf {
        self.root().join(".pruned-at")
    }
}

fn write_aged(path: &Path, bytes: &[u8], age: Duration) {
    fs::create_dir_all(path.parent().expect("the file has a folder")).expect("folder is created");
    fs::write(path, bytes).expect("the file is written");
    set_age(path, age);
}

fn set_age(path: &Path, age: Duration) {
    let file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("the file opens");
    file.set_modified(SystemTime::now() - age)
        .expect("the date is set");
}

fn days(count: u64) -> Duration {
    DAY * u32::try_from(count).expect("a small number of days")
}

fn probe() -> FakeProbe {
    FakeProbe::clean(snap('a')).with_head_facts(wi_head_facts())
}

fn no_runs() -> FakeRunner {
    FakeRunner::new(vec![]).0
}

fn run_before_pr(app: &mut App<'_>) -> Result<Value, AppError> {
    let action = Action::CheckRun {
        target: CheckTarget::Event("before-pr".to_owned()),
        pre_push: None,
    };

    app.dispatch(&Caller::cli(), &action)
}

fn prune(app: &mut App<'_>) -> Value {
    app.dispatch(&Caller::cli(), &Action::ByproductPrune)
        .unwrap_or_else(|error| panic!("the prune is accepted: {error}"))
}

fn tail(app: &mut App<'_>, check: &str, work_item: &str) -> Result<Value, AppError> {
    let action = Action::Byproduct {
        check: check.to_owned(),
        work_item: work_item.to_owned(),
        tail: 50,
    };

    app.dispatch(&Caller::cli(), &action)
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn modified_ago(path: &Path) -> Duration {
    SystemTime::now()
        .duration_since(
            fs::metadata(path)
                .expect("the file exists")
                .modified()
                .expect("a date"),
        )
        .unwrap_or_default()
}

// CA-018: a poda no `check run`

#[test]
fn it_should_delete_a_stale_blob_that_is_not_live_and_a_stale_temporary_file_on_check_run() {
    let fx = Fixture::new();
    let stale = fx.file_aged(B1, days(8));
    let temporary = fx.file_aged(&format!("{B2}.tmp-4242-0"), days(8));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!((exists(&stale), exists(&temporary)), (false, false));
}

#[test]
fn it_should_keep_a_young_blob_a_live_blob_and_a_file_with_an_odd_name_on_check_run() {
    let fx = Fixture::new();
    fx.attest('a', FMT, W1, B3);
    let live = fx.file_aged(B3, days(30));
    let young = fx.file_aged(B1, days(6));
    let odd = fx.file_aged("b1-notes.txt", days(8));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(
        (exists(&live), exists(&young), exists(&odd)),
        (true, true, true)
    );
}

#[test]
fn it_should_do_nothing_when_the_stamp_is_less_than_a_day_old() {
    let fx = Fixture::new();
    let stale = fx.file_aged(B1, days(8));
    write_aged(&fx.stamp(), b"", Duration::from_secs(60 * 60));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");

    assert!(exists(&stale));
}

#[test]
fn it_should_renew_a_stamp_older_than_a_day_and_prune() {
    let fx = Fixture::new();
    let stale = fx.file_aged(B1, days(8));
    write_aged(&fx.stamp(), b"", days(2));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!(
        (
            exists(&stale),
            modified_ago(&fx.stamp()) < Duration::from_secs(60)
        ),
        (false, true)
    );
}

#[test]
fn it_should_not_change_the_output_when_a_removal_fails() {
    let fx = Fixture::new();
    let locked = fx.file_aged(B1, days(8));
    let folder = locked
        .parent()
        .expect("the blob has a folder")
        .to_path_buf();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).expect("the folder is locked");
    // Como `root` ignora o modo, o teste só vale para um usuário comum.
    assert!(
        fs::write(folder.join("probe"), b"").is_err(),
        "run this test as a non-root user"
    );
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);
    let mut without = fx.app_without_retention(&probe, &runner);

    let pruned = run_before_pr(&mut app);
    let plain = run_before_pr(&mut without);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
        .expect("the folder is unlocked");

    assert_eq!(
        (pruned.is_ok(), pruned.ok(), exists(&locked)),
        (true, plain.ok(), true)
    );
}

#[test]
fn it_should_prune_on_a_pre_push_run_that_has_nothing_to_attest() {
    let fx = Fixture::new();
    let stale = fx.file_aged(B1, days(8));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);
    let action = Action::CheckRun {
        target: CheckTarget::Event("before-pr".to_owned()),
        pre_push: Some(String::new()),
    };

    let output = app
        .dispatch(&Caller::cli(), &action)
        .expect("the run succeeds");

    assert_eq!(
        (output["push"].as_str(), exists(&stale)),
        (Some("nothing-to-attest"), false)
    );
}

#[test]
fn it_should_prune_on_a_develop_run() {
    let fx = Fixture::new();
    let stale = fx.file_aged(B1, days(8));
    let facts = HeadFacts {
        current: Some("develop".to_owned()),
        rebase_head: None,
        containing: vec!["develop".to_owned()],
        head: Some("a".repeat(40)),
    };
    let probe = FakeProbe::clean(snap('a'))
        .with_head_facts(facts)
        .with_develop_mark(true);
    let runner = no_runs();
    let mut app = fx.app(&probe, &runner);
    let action = Action::CheckRun {
        target: CheckTarget::Event("before-commit".to_owned()),
        pre_push: None,
    };

    let output = app
        .dispatch(&Caller::cli(), &action)
        .expect("the run succeeds");

    assert_eq!(
        (output["recorded"].as_bool(), exists(&stale)),
        (Some(false), false)
    );
}

#[test]
fn it_should_neither_prune_nor_create_the_stamp_without_the_retention_wired() {
    let fx = Fixture::new();
    let stale = fx.file_aged(B1, days(8));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app_without_retention(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");

    assert_eq!((exists(&stale), exists(&fx.stamp())), (true, false));
}

#[test]
fn it_should_not_touch_the_blobs_of_another_tenant_in_the_same_database() {
    let fx = Fixture::new();
    let other_root = fx.raw.dir.path().join("blobs/by-db/hexlog.db/other");
    let foreign = other_root.join(&B1[..2]).join(B1);
    write_aged(&foreign, b"x", days(30));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);
    fx.file_aged(B2, days(8));

    run_before_pr(&mut app).expect("the run succeeds");
    let pruned = prune(&mut app);

    assert_eq!(
        (exists(&foreign), pruned["removed"].as_u64()),
        (true, Some(0))
    );
}

#[test]
fn it_should_not_touch_the_blobs_of_another_database_in_the_same_folder() {
    let fx = Fixture::new();
    let foreign = fx
        .raw
        .dir
        .path()
        .join("blobs/by-db/scratch.db/acme")
        .join(&B1[..2])
        .join(B1);
    write_aged(&foreign, b"x", days(30));
    fx.file_aged(B2, days(8));
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");
    let pruned = prune(&mut app);

    assert_eq!(
        (exists(&foreign), pruned["removed"].as_u64()),
        (true, Some(0))
    );
}

// CA-019: o que a poda nunca apaga

#[test]
fn it_should_keep_the_output_of_a_live_attestation_readable_after_the_next_check_run() {
    let fx = Fixture::new();
    let hash = fx.blobs.put(REAL_BLOB).expect("the blob is stored");
    let path = fx.blob_path(hash.as_str());
    set_age(&path, days(30));
    fx.attest('a', FMT, W1, hash.as_str());
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    run_before_pr(&mut app).expect("the run succeeds");
    let read = tail(&mut app, FMT, W1).expect("the output is still there");

    assert_eq!(read["lines"], json!(["ok"]));
}

#[test]
fn it_should_delete_a_blob_shared_by_two_pairs_only_when_no_live_pair_uses_it() {
    let fx = Fixture::new();
    let shared = fx.file_aged(B1, days(30));
    fx.attest('a', FMT, W1, B1);
    fx.attest('a', LINT, W1, B1);
    fx.attest('b', FMT, W1, B2);
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    let while_lint_uses_it = prune(&mut app);
    let kept = exists(&shared);
    fx.attest('b', LINT, W1, B3);
    let after_lint_moved_on = prune(&mut app);

    assert_eq!(
        (
            kept,
            while_lint_uses_it["kept_live"].as_u64(),
            exists(&shared),
            after_lint_moved_on["removed"].as_u64(),
        ),
        (true, Some(1), false, Some(1))
    );
}

#[test]
fn it_should_delete_the_blob_of_a_previous_tree_of_the_same_pair() {
    let fx = Fixture::new();
    let previous = fx.file_aged(B1, days(30));
    let latest = fx.file_aged(B2, days(30));
    fx.attest('a', FMT, W1, B1);
    fx.attest('b', FMT, W1, B2);
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    let pruned = prune(&mut app);

    assert_eq!(
        (
            exists(&previous),
            exists(&latest),
            pruned["freed_bytes"].as_u64()
        ),
        (false, true, Some(1))
    );
}

// CA-018: 2 000 attestations em 200 pares

fn seed_bulk_attestations(fx: &Fixture) {
    for check in 0..BULK_CHECKS {
        fx.seed(
            loader_author(),
            vec![node_item(
                &format!("checks.c{check}"),
                "check",
                check_data("before-commit", "x", true),
            )],
            &snap('a'),
        );
    }
    for work_item in 0..BULK_WORK_ITEMS {
        fx.seed(
            author(),
            vec![node_item(
                &format!("work-items.bw{work_item}"),
                "work-item",
                work_item_data("t"),
            )],
            &snap('a'),
        );
    }
    for tree in 0..BULK_TREES {
        let fill = char::from_digit(u32::try_from(tree).expect("a small number"), 10)
            .expect("a decimal digit");
        let items: Vec<AppendItem> = (0..BULK_CHECKS * BULK_WORK_ITEMS)
            .map(|pair| {
                let (check, work_item) = (pair / BULK_WORK_ITEMS, pair % BULK_WORK_ITEMS);
                let (check, work_item) = (
                    format!("checks.c{check}"),
                    format!("work-items.bw{work_item}"),
                );
                let data =
                    executor_attestation_data(&check, &work_item, "passed", &bulk_hash(tree, pair));
                let id = attestation_id_for(&snap(fill), &check, &work_item);
                with_snapshot(node_item(&id, "attestation", data), &snap(fill))
            })
            .collect();
        for batch in items.chunks(SEED_BATCH) {
            fx.seed(executor_author(), batch.to_vec(), &snap(fill));
        }
    }
}

/// Um nome de arquivo de 64 hexadecimais por (árvore, par); o `<xx>` varia com o par.
fn bulk_hash(tree: usize, pair: usize) -> String {
    let number = tree * BULK_CHECKS * BULK_WORK_ITEMS + pair;

    format!("{:02x}{number:062x}", pair % 256)
}

#[test]
fn it_should_prune_2000_attestations_in_200_pairs_within_the_ceiling() {
    let fx = Fixture::new();
    seed_bulk_attestations(&fx);
    for tree in 0..BULK_TREES {
        for pair in 0..BULK_CHECKS * BULK_WORK_ITEMS {
            fx.file_aged(&bulk_hash(tree, pair), days(30));
        }
    }
    let (probe, runner) = (probe(), no_runs());
    let mut app = fx.app(&probe, &runner);

    let started = Instant::now();
    let pruned = prune(&mut app);
    let elapsed = started.elapsed();

    // Teto de 10 s: vale porque a maior de 5 medidas em debug foi de 558 ms, abaixo de 3 s (329, 411, 415, 491 e 558 ms).
    assert_eq!(
        (
            pruned["removed"].as_u64(),
            pruned["kept_live"].as_u64(),
            elapsed < PRUNE_CEILING
        ),
        (Some(1800), Some(200), true)
    );
}
