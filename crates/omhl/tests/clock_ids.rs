mod support;

use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::ports::clock::SystemClock;
use ::omhl::ports::ids::V7Ids;
use hexlog::Clock;
use serde_json::json;
use support::omhl::{acme, node_item, request};
use support::raw::RawHarness;
use support::{FakeProbe, sid};

#[test]
fn it_should_produce_an_instant_the_engine_accepts_from_the_system_clock() {
    let raw = RawHarness::new();
    let probe = FakeProbe::dirty();
    let deps = OmhlDeps {
        clock: &SystemClock,
        ids: &V7Ids,
        probe: &probe,
    };
    let mut omhl = Omhl::open(&raw.config(), deps).expect("omhl opens");

    let ensured = omhl.ensure_tenant(&acme());
    let instant = SystemClock.now();

    assert_eq!(
        (ensured.is_ok(), instant.len(), instant.ends_with('Z')),
        (true, 24, true)
    );
}

#[test]
fn it_should_generate_ids_the_engine_accepts_as_record_ids() {
    let raw = RawHarness::new();
    let probe = FakeProbe::dirty();
    let deps = OmhlDeps {
        clock: &SystemClock,
        ids: &V7Ids,
        probe: &probe,
    };
    let mut omhl = Omhl::open(&raw.config(), deps).expect("omhl opens");
    omhl.ensure_tenant(&acme()).expect("the tenant is created");
    let item = node_item(
        "guidelines.g1",
        "guideline",
        json!({
            "text": "be kind",
            "kind": "business",
            "confirmed_by": { "by": "person:tester", "question": "Adopt it?", "answer": "Yes." },
        }),
    );

    let outcome = omhl
        .append(&request(vec![item]))
        .expect("the append is accepted");
    let stored = omhl
        .get(&acme(), &sid("guidelines.g1"))
        .expect("the read succeeds")
        .expect("the node is found");

    assert_eq!(stored.record_id, outcome.records[0].id);
}
