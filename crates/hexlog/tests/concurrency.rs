mod support;

use std::marker::PhantomData;
use std::sync::Barrier;
use std::thread;
use std::time::Duration;

use hexlog::{
    AppendOutcome, BreakReason, Config, Deps, ErrorCode, Hexlog, HexlogError, IdGenerator,
    VerifyPolicy,
};
use serde_json::json;
use support::catalog::reference_catalog;
use support::tamper::{Inspect, Tamper};
use support::{Harness, append_one, node, node_lineage, request, revise, sid, tenant};

const ROUNDS: usize = 50;
const OPENERS: usize = 8;
const APPENDERS: usize = 8;
const APPENDS_PER_THREAD: usize = 5;
const SEVEN_TRIGGERS: [&str; 7] = [
    "batches_no_delete",
    "batches_no_update",
    "records_chain_guard",
    "records_no_delete",
    "records_no_update",
    "tenants_no_delete",
    "tenants_no_update",
];

/// Espera de 120 s só nestes testes: sob IO pesado um `fsync` parado chega a dezenas de segundos
/// dentro do `COMMIT` de quem segura a trava (medido: até 19,5 s no WSL2). O padrão do produto não muda.
fn patient(harness: &Harness) -> Config {
    Config {
        busy_timeout: Duration::from_secs(120),
        ..harness.config()
    }
}

fn full_policy(harness: &Harness) -> Config {
    Config {
        verify_policy: VerifyPolicy::Full,
        ..harness.config()
    }
}

/// Move cada instância para a sua thread, solta todas juntas por uma `Barrier` e devolve o que
/// `work` retornou em cada uma, na ordem das instâncias.
fn race<'d, T: Send>(
    instances: Vec<Hexlog<'d>>,
    work: &(impl Fn(usize, &mut Hexlog<'d>) -> T + Sync),
) -> Vec<T> {
    let barrier = Barrier::new(instances.len());

    thread::scope(|scope| {
        let handles: Vec<_> = instances
            .into_iter()
            .enumerate()
            .map(|(index, mut hexlog)| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    work(index, &mut hexlog)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread does not panic"))
            .collect()
    })
}

/// Banco com o tenant `acme` e `nodes` nós `n.1`, `n.2`, ..., em que o `hash` do record `tampered_seq`
/// foi trocado por 64 `f`. O `Tamper` volta aberto, para conferir o banco depois.
fn tampered_chain(nodes: usize, tampered_seq: u64) -> (Harness, Tamper) {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    for number in 1..=nodes {
        append_one(&mut hexlog, node(&format!("n.{number}"), "note", json!({})));
    }
    drop(hexlog);
    let mut tamper = Tamper::open(&harness.db_path());

    let changed = tamper.execute(
        "UPDATE records SET hash = ? WHERE tenant = ? AND seq = ?",
        &["f".repeat(64).as_str(), "acme", &tampered_seq.to_string()],
    );

    assert_eq!(changed, 1);
    (harness, tamper)
}

fn stored_records(tamper: &mut Tamper) -> Vec<String> {
    tamper.read_text(
        "SELECT CAST(COUNT(*) AS TEXT) FROM records WHERE tenant = ?",
        &["acme"],
    )
}

#[test]
fn it_should_accept_only_one_of_two_concurrent_revisions_of_the_same_base() {
    let harness = Harness::new();
    let config = patient(&harness);
    let mut hexlog = harness.open_acme_with(&config, reference_catalog());
    append_one(&mut hexlog, node("a.one", "note", json!({})));
    let base = hexlog
        .get(&tenant("acme"), &sid("a.one"))
        .expect("the read answers")
        .expect("the node exists")
        .record_id;
    drop(hexlog);
    let instances = vec![
        harness.open_with(&config, reference_catalog()),
        harness.open_with(&config, reference_catalog()),
    ];

    let outcomes = race(instances, &|index, hexlog| {
        let item = revise(node("a.one", "note", json!({ "count": index })), &base);

        hexlog.append(&request("acme", None, vec![item]))
    });

    let successes = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    let refusals: Vec<&HexlogError> = outcomes
        .iter()
        .filter_map(|outcome| outcome.as_ref().err())
        .collect();
    let mut checker = harness.open_with(&config, reference_catalog());
    let revisions = checker
        .lineage_records(&tenant("acme"), &node_lineage("a.one"))
        .expect("the lineage is read");
    let report = checker
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs");
    assert_eq!(successes, 1);
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].code, ErrorCode::ForkRejected);
    assert_eq!(refusals[0].details[0].code, "not-current");
    assert_eq!(revisions.len(), 2);
    assert!(report.ok);
}

#[test]
fn it_should_write_once_when_the_same_key_races() {
    let harness = Harness::new();
    let config = patient(&harness);
    drop(harness.open_acme_with(&config, reference_catalog()));
    let instances = vec![
        harness.open_with(&config, reference_catalog()),
        harness.open_with(&config, reference_catalog()),
    ];
    let req = request("acme", Some("K1"), vec![node("a.one", "note", json!({}))]);
    let ids_before = harness.ids.next().as_u128();

    let outcomes: Vec<AppendOutcome> = race(instances, &|_, hexlog| hexlog.append(&req))
        .into_iter()
        .map(|outcome| outcome.expect("both appends succeed"))
        .collect();

    let ids_after = harness.ids.next().as_u128();
    let head = harness
        .open_with(&config, reference_catalog())
        .chain_head(&tenant("acme"))
        .expect("chain head");
    assert_eq!(outcomes.iter().filter(|o| o.replayed).count(), 1);
    assert_eq!(outcomes[0].records, outcomes[1].records);
    assert_eq!(head.seq, 1);
    // `ids_before` e `ids_after` consumiram um id cada; o que sobra é o que a corrida gastou.
    assert_eq!(ids_after - ids_before - 1, 1);
}

#[test]
fn it_should_keep_seq_contiguous_when_many_threads_append() {
    let harness = Harness::new();
    let config = patient(&harness);
    drop(harness.open_acme_with(&config, reference_catalog()));
    let instances = (0..APPENDERS)
        .map(|_| harness.open_with(&config, reference_catalog()))
        .collect();

    let outcomes = race(instances, &|thread, hexlog| {
        (0..APPENDS_PER_THREAD)
            .map(|number| {
                let item = node(&format!("t{thread}.n{number}"), "note", json!({}));

                hexlog.append(&request("acme", None, vec![item]))
            })
            .collect::<Vec<_>>()
    });

    let mut seqs: Vec<u64> = outcomes
        .into_iter()
        .flatten()
        .map(|outcome| outcome.expect("every append succeeds").records[0].seq)
        .collect();
    seqs.sort_unstable();
    let report = harness
        .open_with(&config, reference_catalog())
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs");
    let total = (APPENDERS * APPENDS_PER_THREAD) as u64;
    assert_eq!(seqs, (1..=total).collect::<Vec<u64>>());
    assert!(report.ok);
    assert_eq!(report.total_records, total);
}

fn require_send<T: Send>() {}

fn hexlog_is_send<'d>(_: PhantomData<Hexlog<'d>>) {
    require_send::<Hexlog<'d>>();
}

#[test]
fn it_should_be_send() {
    hexlog_is_send(PhantomData);
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();

    let mut hexlog = thread::scope(|scope| {
        scope
            .spawn(move || {
                append_one(&mut hexlog, node("a.one", "note", json!({})));
                hexlog
            })
            .join()
            .expect("thread does not panic")
    });

    let found = hexlog
        .get(&tenant("acme"), &sid("a.one"))
        .expect("the read answers");
    assert!(found.is_some());
}

#[test]
fn it_should_fail_with_chain_broken_on_write_when_the_tail_record_is_tampered() {
    let (harness, mut tamper) = tampered_chain(3, 3);
    let mut hexlog = harness.open();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("z.new", "note", json!({}))],
        ))
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::ChainBroken);
    assert_eq!(error.details[0].code, "tail-hash-mismatch");
    assert_eq!(stored_records(&mut tamper), ["3"]);
}

#[test]
fn it_should_not_notice_a_tampered_middle_record_on_write_under_the_tail_policy() {
    let (harness, _tamper) = tampered_chain(5, 2);
    let mut hexlog = harness.open();

    let outcome = hexlog.append(&request(
        "acme",
        None,
        vec![node("z.new", "note", json!({}))],
    ));

    let report = hexlog
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs");
    assert_eq!(outcome.expect("the write is accepted").records[0].seq, 6);
    assert!(!report.ok);
    assert_eq!(report.breaks[0].seq, Some(2));
    assert_eq!(report.breaks[0].reason, BreakReason::HashMismatch);
}

#[test]
fn it_should_fail_with_chain_broken_on_write_for_a_tampered_middle_record_under_the_full_policy() {
    let (harness, mut tamper) = tampered_chain(5, 2);
    let mut hexlog = harness.open_with(&full_policy(&harness), reference_catalog());

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("z.new", "note", json!({}))],
        ))
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::ChainBroken);
    assert_eq!(error.details[0].code, "hash-mismatch");
    assert_eq!(stored_records(&mut tamper), ["5"]);
}

#[test]
fn it_should_open_a_fresh_database_from_many_threads_without_error() {
    for round in 0..ROUNDS {
        let harness = Harness::new();
        let config = patient(&harness);
        let barrier = Barrier::new(OPENERS);

        let opened: Vec<Result<Hexlog<'_>, HexlogError>> = thread::scope(|scope| {
            let handles: Vec<_> = (0..OPENERS)
                .map(|_| {
                    let (barrier, harness, config) = (&barrier, &harness, &config);
                    scope.spawn(move || {
                        barrier.wait();
                        Hexlog::open(
                            config,
                            Deps {
                                clock: &harness.clock,
                                ids: &harness.ids,
                                catalog: reference_catalog(),
                                native: &harness.native,
                            },
                        )
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|handle| handle.join().expect("thread does not panic"))
                .collect()
        });

        let failures: Vec<&HexlogError> = opened
            .iter()
            .filter_map(|result| result.as_ref().err())
            .collect();
        let mut inspect = Inspect::open(&harness.db_path());
        let tables = inspect.read_text(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' AND name IN ('batches', 'records', 'tenants') ORDER BY name",
            &[],
        );
        let triggers = inspect.read_text(
            "SELECT name FROM sqlite_master WHERE type = 'trigger' ORDER BY name",
            &[],
        );
        let version = inspect.read_text(
            "SELECT CAST(user_version AS TEXT) FROM pragma_user_version",
            &[],
        );
        assert!(failures.is_empty(), "round {round}: {failures:?}");
        assert_eq!(tables, ["batches", "records", "tenants"], "round {round}");
        assert_eq!(triggers, SEVEN_TRIGGERS, "round {round}");
        assert_eq!(version, ["1"], "round {round}");
    }
}
