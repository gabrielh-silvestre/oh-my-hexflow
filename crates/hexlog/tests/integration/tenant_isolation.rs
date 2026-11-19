use crate::support;

use hexlog::{Config, ErrorCode, Hexlog, VerifyPolicy};
use serde_json::json;
use support::catalog::reference_catalog;
use support::tamper::Tamper;
use support::{Harness, assert_refusal, node, request, sid, tenant};

/// Grava um node `note` no tenant `name`, sem `key`.
fn write_note(hexlog: &mut Hexlog<'_>, name: &str, id: &str, text: &str) {
    hexlog
        .append(&request(
            name,
            None,
            vec![node(id, "note", json!({ "text": text }))],
        ))
        .expect("the note is appended");
}

/// Banco com os tenants `acme` e `beta`, cada um com dois nodes.
fn two_tenants(harness: &Harness) {
    let mut hexlog = harness.open_acme();
    hexlog
        .ensure_tenant(&tenant("beta"))
        .expect("tenant beta is created");
    for name in ["acme", "beta"] {
        write_note(&mut hexlog, name, "a.one", "one");
        write_note(&mut hexlog, name, "a.two", "two");
    }
}

fn is_ok(harness: &Harness, name: &str) -> bool {
    harness
        .open()
        .verify_chain(&tenant(name), None)
        .expect("verification runs")
        .ok
}

#[test]
fn it_should_keep_tenant_a_valid_when_tenant_b_is_tampered() {
    let harness = Harness::new();
    two_tenants(&harness);
    let changed = Tamper::open(&harness.db_path()).execute(
        "UPDATE records SET hash = ? WHERE tenant = ? AND seq = 1",
        &[&"f".repeat(64), "beta"],
    );

    let (acme_ok, beta_ok) = (is_ok(&harness, "acme"), is_ok(&harness, "beta"));

    assert_eq!(changed, 1);
    assert!(acme_ok);
    assert!(!beta_ok);
}

#[test]
fn it_should_keep_tenant_a_valid_when_every_row_of_tenant_b_is_removed() {
    let harness = Harness::new();
    two_tenants(&harness);
    let removed =
        Tamper::open(&harness.db_path()).execute("DELETE FROM records WHERE tenant = ?", &["beta"]);

    let acme_ok = is_ok(&harness, "acme");

    assert_eq!(removed, 2);
    assert!(acme_ok);
}

#[test]
fn it_should_derive_different_anchors_for_different_tenants() {
    let harness = Harness::new();
    let mut hexlog = harness.open();

    let acme = hexlog
        .ensure_tenant(&tenant("acme"))
        .expect("acme is created");
    let beta = hexlog
        .ensure_tenant(&tenant("beta"))
        .expect("beta is created");

    assert_ne!(acme.anchor, beta.anchor);
}

#[test]
fn it_should_give_different_hashes_to_identical_content_in_different_tenants() {
    let harness = Harness::new();
    two_tenants(&harness);
    let mut hexlog = harness.open();

    let acme = hexlog.chain_head(&tenant("acme")).expect("acme chain head");
    let beta = hexlog.chain_head(&tenant("beta")).expect("beta chain head");

    assert_eq!(acme.seq, beta.seq);
    assert_ne!(acme.hash, beta.hash);
}

#[test]
fn it_should_scope_semantic_ids_per_tenant() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    hexlog
        .ensure_tenant(&tenant("beta"))
        .expect("tenant beta is created");
    write_note(&mut hexlog, "acme", "a.one", "from acme");

    write_note(&mut hexlog, "beta", "a.one", "from beta");

    let acme = hexlog.get(&tenant("acme"), &sid("a.one")).unwrap().unwrap();
    let beta = hexlog.get(&tenant("beta"), &sid("a.one")).unwrap().unwrap();
    assert_eq!(acme.data, json!({"text": "from acme"}));
    assert_eq!(beta.data, json!({"text": "from beta"}));
    assert_ne!(acme.record_id, beta.record_id);
}

#[test]
fn it_should_report_tenant_not_found_when_verifying_an_unknown_tenant() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();

    let result = hexlog.verify_chain(&tenant("ghost"), None);

    let Err(error) = result else {
        panic!("an unknown tenant is refused");
    };

    assert_refusal(
        &error,
        ErrorCode::TenantNotFound,
        "tenant-not-found",
        "/tenant",
    );
}

#[test]
fn it_should_refuse_a_write_to_an_unknown_tenant_the_same_way_under_both_policies() {
    for verify_policy in [VerifyPolicy::Tail, VerifyPolicy::Full] {
        let harness = Harness::new();
        let config = Config {
            verify_policy,
            ..harness.config()
        };
        let mut hexlog = harness.open_with(&config, reference_catalog());

        let malformed = hexlog
            .append(&request("ghost", None, Vec::new()))
            .unwrap_err();
        let well_formed = hexlog
            .append(&request(
                "ghost",
                None,
                vec![node("a.one", "note", json!({}))],
            ))
            .unwrap_err();
        let head = hexlog.chain_head(&tenant("ghost")).unwrap_err();

        assert_refusal(&malformed, ErrorCode::InvalidInput, "batch-size", "/items");
        assert_refusal(
            &well_formed,
            ErrorCode::TenantNotFound,
            "tenant-not-found",
            "/tenant",
        );
        assert_eq!(head.code, ErrorCode::TenantNotFound);
    }
}
