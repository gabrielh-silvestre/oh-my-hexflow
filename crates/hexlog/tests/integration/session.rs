use crate::support;

use hexlog::{ErrorCode, ScannedRecord};
use serde_json::{Value, json};
use support::tamper::Inspect;
use support::{
    DEFAULT_INSTANT, Harness, node, node_lineage, relation, relation_lineage, request, retire,
    revise, session, tenant,
};

const SESSION_A: &str = "0f6c1d2e-aaaa-4bbb-8ccc-000000000001";
const SESSION_B: &str = "0f6c1d2e-aaaa-4bbb-8ccc-000000000002";

fn stored(harness: &Harness, seq: u64) -> (String, String) {
    let mut inspect = Inspect::open(&harness.db_path());
    let binds = ["acme", &seq.to_string()];
    let body = inspect.read_text(
        "SELECT CAST(body AS TEXT) FROM records WHERE tenant = ? AND seq = ?",
        &binds,
    );
    let hash = inspect.read_text(
        "SELECT CAST(hash AS TEXT) FROM records WHERE tenant = ? AND seq = ?",
        &binds,
    );

    (body[0].clone(), hash[0].clone())
}

fn scanned(harness: &Harness) -> Vec<ScannedRecord> {
    let mut found = Vec::new();
    harness
        .open()
        .scan_records(&tenant("acme"), |record| found.push(record))
        .expect("the scan runs");

    found
}

#[test]
fn it_should_keep_old_records_unchanged_and_verify_a_database_mixing_old_and_new_records() {
    let harness = Harness::with_native_other();
    let old_id = {
        let mut hexlog = harness.open_acme();
        let outcome = hexlog
            .append(&request(
                "acme",
                None,
                vec![node("a.one", "note", json!({"text": "one"}))],
            ))
            .expect("the old record is appended");
        outcome.records[0].id.clone()
    };
    let old_before = stored(&harness, 1);
    let label = session(SESSION_A);

    {
        let mut hexlog = harness.open();
        hexlog
            .append_with_session(
                &request(
                    "acme",
                    None,
                    vec![revise(
                        node("a.one", "note", json!({"text": "two"})),
                        &old_id,
                    )],
                ),
                &label,
            )
            .expect("the revision with session is appended");
        hexlog
            .append_with_session(
                &request("acme", None, vec![relation("after", "a.one", "n.other")]),
                &label,
            )
            .expect("the relation with session is appended");
    }
    let report = harness
        .open()
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs");

    let (old_body, old_hash) = stored(&harness, 1);
    let new_body: Value = serde_json::from_str(&stored(&harness, 2).0).expect("JSON body");
    assert!(report.ok, "breaks: {:?}", report.breaks);
    assert_eq!(report.total_records, 3);
    assert_eq!((old_body.clone(), old_hash), old_before);
    assert!(!old_body.contains("session"));
    assert_eq!(new_body["session"], SESSION_A);
    assert_eq!(new_body["v"], 1);
}

#[test]
fn it_should_replay_a_batch_whatever_the_session_of_the_retry() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let req = request(
        "acme",
        Some("K1"),
        vec![node("a.one", "note", json!({"text": "one"}))],
    );
    let first = hexlog
        .append_with_session(&req, &session(SESSION_A))
        .expect("the batch is appended");

    let retry = hexlog
        .append_with_session(&req, &session(SESSION_B))
        .expect("the retry replays");

    assert!(!first.replayed);
    assert!(retry.replayed);
    assert_eq!(retry.records, first.records);
}

#[test]
fn it_should_scan_every_record_in_seq_order_with_lineage_retirement_data_and_session() {
    let harness = Harness::with_native_other();
    harness
        .open_acme()
        .append(&request(
            "acme",
            None,
            vec![node("a.one", "note", json!({"text": "one"}))],
        ))
        .expect("the old record is appended");
    let mut hexlog = harness.open();
    hexlog
        .append_with_session(
            &request(
                "acme",
                None,
                vec![
                    node("a.two", "note", json!({})),
                    relation("after", "a.one", "n.other"),
                ],
            ),
            &session(SESSION_A),
        )
        .expect("the batch is appended");
    let two_id = hexlog
        .lineage_records(&tenant("acme"), &node_lineage("a.two"))
        .expect("lineage is read")[0]
        .record_id
        .clone();
    hexlog
        .append_with_session(
            &request("acme", None, vec![retire("a.two", &two_id)]),
            &session(SESSION_B),
        )
        .expect("the retirement is appended");
    drop(hexlog);

    let records = scanned(&harness);

    let seqs: Vec<u64> = records.iter().map(|record| record.seq).collect();
    let sessions: Vec<Option<&str>> = records
        .iter()
        .map(|record| record.session.as_ref().map(|label| label.as_str()))
        .collect();
    let retired: Vec<bool> = records.iter().map(|record| record.retired).collect();
    let types: Vec<&str> = records
        .iter()
        .map(|record| record.type_name.as_str())
        .collect();
    assert_eq!(seqs, [1, 2, 3, 4]);
    assert_eq!(
        sessions,
        [None, Some(SESSION_A), Some(SESSION_A), Some(SESSION_B)]
    );
    assert_eq!(retired, [false, false, false, true]);
    assert_eq!(types, ["note", "note", "after", "note"]);
    assert_eq!(records[0].lineage, node_lineage("a.one"));
    assert_eq!(
        records[2].lineage,
        relation_lineage("after", "a.one", "n.other")
    );
    assert_eq!(records[3].lineage, node_lineage("a.two"));
    assert_eq!(records[0].data, Some(json!({"text": "one"})));
    assert_eq!(records[2].data, None);
    assert_eq!(records[3].data, None);
    assert!(records.iter().all(|record| record.at == DEFAULT_INSTANT));
}

#[test]
fn it_should_scan_across_page_boundaries_without_skipping_or_repeating_a_record() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    for batch in 0..11 {
        let items = (0..50)
            .map(|n| node(&format!("x.{}", batch * 50 + n), "note", json!({})))
            .collect();
        hexlog
            .append(&request("acme", None, items))
            .expect("the batch is appended");
    }
    drop(hexlog);

    let records = scanned(&harness);

    let seqs: Vec<u64> = records.iter().map(|record| record.seq).collect();
    assert_eq!(seqs, (1..=550).collect::<Vec<u64>>());
}

#[test]
fn it_should_refuse_to_scan_an_unknown_tenant() {
    let harness = Harness::new();
    let mut hexlog = harness.open();

    let error = hexlog
        .scan_records(&tenant("acme"), |_| unreachable!("no record is visited"))
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::TenantNotFound);
}
