mod support;

use std::collections::HashSet;

use hexlog::{
    BreakReason, CHECKED_FIELDS, ChainReport, Config, ErrorCode, HexlogError, VerifyPolicy,
};
use serde_json::json;
use support::catalog::reference_catalog;
use support::mutations::{
    Reference, mutations, reference_chain, rewrite_body, rewrite_json, set_column,
};
use support::tamper::Tamper;
use support::{
    DEFAULT_INSTANT, Harness, all, assert_refusal, node, node_lineage, on_small_stack, page,
    relation_query, request, sid, tenant, unused_record_id,
};

const DELETE_RC: &str = "DELETE FROM records WHERE tenant = ? AND seq = ?";

/// Banco novo com a cadeia de referência (sem nós extras) e o nativo `n.other`.
fn reference_db() -> (Harness, Reference) {
    let harness = Harness::with_native_other();
    let reference = reference_chain(&harness, 0);

    (harness, reference)
}

fn verify(harness: &Harness) -> ChainReport {
    harness
        .open()
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs")
}

/// `(seq, motivo)` de cada quebra, na ordem em que saíram.
fn breaks_of(report: &ChainReport) -> Vec<(Option<u64>, BreakReason)> {
    report
        .breaks
        .iter()
        .map(|found| (found.seq, found.reason))
        .collect()
}

fn full_policy(harness: &Harness) -> Config {
    Config {
        verify_policy: VerifyPolicy::Full,
        ..harness.config()
    }
}

#[test]
fn it_should_verify_a_clean_chain() {
    let (harness, _reference) = reference_db();
    let mut hexlog = harness.open();

    let report = hexlog
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs");

    let head = hexlog.chain_head(&tenant("acme")).expect("chain head");
    assert!(report.ok);
    assert_eq!(report.total_records, 6);
    assert_eq!(report.total_breaks, 0);
    assert!(report.breaks.is_empty());
    assert_eq!(report.chain_head, head.hash);
}

#[test]
fn it_should_detect_a_change_in_each_checked_field() {
    for mutation in mutations() {
        let (harness, reference) = reference_db();

        let report = mutation.tamper_and_verify(&harness, &reference);

        assert!(!report.ok, "{} is detected", mutation.id);
        let first = report.breaks.first().expect("a break is reported");
        assert_eq!(first.reason, mutation.reason, "reason of {}", mutation.id);
        assert_eq!(
            first.seq,
            mutation.expected_seq(&reference),
            "seq of {}",
            mutation.id
        );
    }
}

#[test]
fn it_should_have_a_mutation_for_every_checked_field() {
    let ids: Vec<&str> = mutations().iter().map(|mutation| mutation.id).collect();

    let expected: Vec<&str> = CHECKED_FIELDS
        .iter()
        .copied()
        .chain(["expected_chain_head"])
        .collect();
    assert_eq!(ids, expected);
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    assert_eq!(CHECKED_FIELDS.len(), 33);
}

#[test]
fn it_should_detect_a_deleted_middle_record() {
    let (harness, reference) = reference_db();
    let deleted = Tamper::open(&reference.path).execute(
        DELETE_RC,
        &[&reference.tenant, &reference.rc(3).seq.to_string()],
    );

    let report = verify(&harness);

    assert_eq!(deleted, 1);
    assert!(!report.ok);
    assert_eq!(
        breaks_of(&report)[0],
        (Some(reference.rc(4).seq), BreakReason::DivergingSeq)
    );
}

#[test]
fn it_should_detect_an_inserted_forged_record() {
    let (harness, reference) = reference_db();
    let mut tamper = Tamper::open(&reference.path);
    let last = reference.rc(6).seq.to_string();
    let tail_hash = tamper
        .read_text(
            "SELECT CAST(hash AS TEXT) FROM records WHERE tenant = ? AND seq = ?",
            &[&reference.tenant, &last],
        )
        .remove(0);
    let forged_seq = reference.rc(6).seq + 1;
    let inserted = tamper.execute(
        "INSERT INTO records (tenant, seq, id, prev_hash, hash, body, at, type, lineage_kind, \
         lineage_key, retired, def_hash) \
         VALUES (?, ?, ?, ?, ?, '{}', ?, 'note', 'node', 'forged.node', 0, ?)",
        &[
            &reference.tenant,
            &forged_seq.to_string(),
            unused_record_id().as_str(),
            &tail_hash,
            &"e".repeat(64),
            DEFAULT_INSTANT,
            &"0".repeat(64),
        ],
    );
    drop(tamper);

    let report = verify(&harness);

    assert_eq!(inserted, 1);
    assert_eq!(report.total_records, 7);
    assert_eq!(
        breaks_of(&report)[0],
        (Some(forged_seq), BreakReason::HashMismatch)
    );
}

#[test]
fn it_should_not_detect_a_truncated_tail_without_expected_chain_head() {
    let (harness, reference) = reference_db();
    let deleted = Tamper::open(&reference.path).execute(
        DELETE_RC,
        &[&reference.tenant, &reference.rc(6).seq.to_string()],
    );

    let report = verify(&harness);

    assert_eq!(deleted, 1);
    assert!(report.ok);
    assert_eq!(report.total_records, 5);
}

#[test]
fn it_should_detect_a_truncated_tail_with_expected_chain_head() {
    let (harness, reference) = reference_db();
    let head = harness
        .open()
        .chain_head(&tenant("acme"))
        .expect("chain head");
    let deleted = Tamper::open(&reference.path).execute(
        DELETE_RC,
        &[&reference.tenant, &reference.rc(6).seq.to_string()],
    );

    let report = harness
        .open()
        .verify_chain(&tenant("acme"), Some(&head.hash))
        .expect("verification runs");

    assert_eq!(deleted, 1);
    assert!(!report.ok);
    assert_eq!(breaks_of(&report), [(None, BreakReason::ChainHeadNotFound)]);
}

#[test]
fn it_should_detect_a_rewritten_and_rechained_tail_with_expected_chain_head() {
    let (harness, reference) = reference_db();
    let head = harness
        .open()
        .chain_head(&tenant("acme"))
        .expect("chain head");
    // O autor não é espelhado em nenhuma coluna: o corpo novo é coerente e o hash recalculado fecha.
    let rewritten = rewrite_json(&mut Tamper::open(&reference.path), &reference, 6, |body| {
        body["author"]["agent"] = json!("forger");
    });

    let report = harness
        .open()
        .verify_chain(&tenant("acme"), Some(&head.hash))
        .expect("verification runs");

    assert_eq!(rewritten, 1);
    assert_eq!(breaks_of(&report), [(None, BreakReason::ChainHeadNotFound)]);
}

#[test]
fn it_should_report_invalid_body_for_a_duplicate_key_in_the_stored_body() {
    let (harness, reference) = reference_db();
    let mut tamper = Tamper::open(&reference.path);
    let stored = tamper.read_body(&reference.tenant, reference.rc(6).seq);
    assert!(
        stored.starts_with("{\"at\":"),
        "canonical bodies start with at"
    );
    let duplicated = stored.replacen(
        "{\"at\":",
        "{\"at\":\"1999-01-01T00:00:00.000Z\",\"at\":",
        1,
    );
    let rewritten = rewrite_body(&mut tamper, &reference, 6, &duplicated);
    drop(tamper);

    let report = verify(&harness);

    assert_eq!(rewritten, 1);
    assert_eq!(
        breaks_of(&report)[0],
        (Some(reference.rc(6).seq), BreakReason::InvalidBody)
    );
}

#[test]
fn it_should_report_invalid_body_for_a_stored_body_nested_beyond_the_parser_limit() {
    let (harness, reference) = reference_db();
    let nested = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let rewritten = rewrite_body(&mut Tamper::open(&reference.path), &reference, 6, &nested);

    let report = on_small_stack(|| verify(&harness));

    assert_eq!(rewritten, 1);
    assert_eq!(
        breaks_of(&report)[0],
        (Some(reference.rc(6).seq), BreakReason::InvalidBody)
    );
}

#[test]
fn it_should_report_a_break_instead_of_a_store_error_when_a_stored_value_is_not_valid_for_its_type()
{
    let cases = [
        ("retired", "2", BreakReason::ColumnMismatch),
        ("lineage_kind", "x", BreakReason::ColumnMismatch),
        ("hash", "ZZ", BreakReason::HashMismatch),
        ("type", "Bad Type", BreakReason::ColumnMismatch),
    ];

    for (column, value, reason) in cases {
        let (harness, reference) = reference_db();
        let changed = set_column(
            &mut Tamper::open(&reference.path),
            &reference,
            3,
            column,
            value,
        );

        let report = verify(&harness);

        assert_eq!(changed, 1, "{column} is altered");
        assert!(!report.ok, "{column} := {value} is detected");
        assert_eq!(
            breaks_of(&report)[0],
            (Some(reference.rc(3).seq), reason),
            "{column} := {value}"
        );
    }
}

#[test]
fn it_should_fail_with_chain_broken_on_write_when_the_tail_record_has_an_unreadable_column() {
    let (harness, reference) = reference_db();
    // Uma só conexão `Tamper` por banco: abrir outra tenta apagar de novo os triggers que já saíram.
    let mut tamper = Tamper::open(&reference.path);
    let changed = set_column(&mut tamper, &reference, 6, "retired", "2");
    let mut hexlog = harness.open();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![node("z.new", "note", json!({}))],
        ))
        .unwrap_err();

    drop(hexlog);
    let stored = tamper.read_text(
        "SELECT CAST(COUNT(*) AS TEXT) FROM records WHERE tenant = ?",
        &[&reference.tenant],
    );
    assert_eq!(changed, 1);
    assert_refusal(&error, ErrorCode::ChainBroken, "tail-unreadable", "");
    assert_eq!(stored, ["6"]);
}

#[test]
fn it_should_refuse_reads_with_chain_broken_when_the_policy_is_full_and_a_record_is_tampered() {
    let (harness, reference) = reference_db();
    let changed = set_column(
        &mut Tamper::open(&reference.path),
        &reference,
        3,
        "hash",
        &"f".repeat(64),
    );
    let mut hexlog = harness.open_with(&full_policy(&harness), reference_catalog());
    let acme = tenant("acme");

    let refusals: Vec<HexlogError> = vec![
        hexlog.get(&acme, &sid("a.one")).unwrap_err(),
        hexlog.list(&acme, &all(), &page(10)).unwrap_err(),
        hexlog
            .lineage_records(&acme, &node_lineage("a.one"))
            .unwrap_err(),
        hexlog.relations(&acme, &relation_query()).unwrap_err(),
        hexlog.chain_head(&acme).unwrap_err(),
    ];

    assert_eq!(changed, 1);
    for error in &refusals {
        assert_refusal(error, ErrorCode::ChainBroken, "hash-mismatch", "");
    }
}

#[test]
fn it_should_not_notice_a_tampered_middle_record_on_read_under_the_tail_policy() {
    let (harness, reference) = reference_db();
    let changed = set_column(
        &mut Tamper::open(&reference.path),
        &reference,
        3,
        "hash",
        &"f".repeat(64),
    );
    let mut hexlog = harness.open();

    let found = hexlog.get(&tenant("acme"), &sid("a.one"));
    let report = hexlog
        .verify_chain(&tenant("acme"), None)
        .expect("verification runs");

    assert_eq!(changed, 1);
    assert!(found.expect("the read answers").is_some());
    assert_eq!(
        breaks_of(&report)[0],
        (Some(reference.rc(3).seq), BreakReason::HashMismatch)
    );
}

#[test]
fn it_should_cap_the_reported_breaks_at_100_and_keep_counting() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    for batch in 0..3 {
        let items = (0..40)
            .map(|n| node(&format!("x.{}", batch * 40 + n), "note", json!({})))
            .collect();
        hexlog
            .append(&request("acme", None, items))
            .expect("the batch is appended");
    }
    drop(hexlog);
    let changed = Tamper::open(&harness.db_path()).execute(
        "UPDATE records SET at = '1999-01-01T00:00:00.000Z' WHERE tenant = ?",
        &["acme"],
    );

    let report = verify(&harness);

    assert_eq!(changed, 120);
    assert_eq!(report.breaks.len(), 100);
    assert_eq!(report.total_breaks, 120);
}
