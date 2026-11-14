mod support;

use std::time::{Duration, UNIX_EPOCH};

use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::ports::clock::{SystemClock, format_system_time, format_utc};
use ::omhl::ports::ids::V7Ids;
use hexlog::{Clock, IdGenerator};
use serde_json::json;
use support::omhl::{acme, node_item, request};
use support::raw::RawHarness;
use support::{FakeProbe, sid};
use uuid::Variant;

const FIRST_INSTANT_AFTER_YEAR_9999_SECS: u64 = 253_402_300_800;

#[test]
fn it_should_format_epoch_zero_as_the_unix_origin() {
    let formatted = format_utc(0, 0);

    assert_eq!(formatted, "1970-01-01T00:00:00.000Z");
}

#[test]
fn it_should_zero_pad_the_milliseconds() {
    let formatted = format_utc(86_399, 5);

    assert_eq!(formatted, "1970-01-01T23:59:59.005Z");
}

#[test]
fn it_should_format_the_leap_day_of_2000() {
    let formatted = format_utc(951_782_400, 0);

    assert_eq!(formatted, "2000-02-29T00:00:00.000Z");
}

#[test]
fn it_should_not_treat_2100_as_a_leap_year() {
    let last_february_day = format_utc(4_107_456_000, 0);
    let day_after = format_utc(4_107_542_400, 0);

    assert_eq!(
        (last_february_day.as_str(), day_after.as_str()),
        ("2100-02-28T00:00:00.000Z", "2100-03-01T00:00:00.000Z")
    );
}

#[test]
fn it_should_clamp_an_instant_before_the_epoch_to_the_origin() {
    let before_epoch = UNIX_EPOCH - Duration::from_secs(1);

    let formatted = format_system_time(before_epoch);

    assert_eq!(formatted, "1970-01-01T00:00:00.000Z");
}

#[test]
fn it_should_clamp_an_instant_after_year_9999_to_its_last_millisecond() {
    let after_year_9999 = UNIX_EPOCH + Duration::from_secs(FIRST_INSTANT_AFTER_YEAR_9999_SECS);

    let formatted = format_system_time(after_year_9999);

    assert_eq!(formatted, "9999-12-31T23:59:59.999Z");
}

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
fn it_should_generate_uuid_v7_with_version_and_variant() {
    let id = V7Ids.next();

    assert_eq!(
        (id.get_version_num(), id.get_variant()),
        (7, Variant::RFC4122)
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
            "kind": "negocio",
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
