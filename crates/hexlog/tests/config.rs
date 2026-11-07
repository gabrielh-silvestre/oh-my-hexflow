mod support;

use std::path::PathBuf;
use std::time::Duration;

use hexlog::{Config, ErrorCode, HexlogError, Limits};
use support::catalog::reference_catalog;
use support::{Harness, assert_refusal, detail_pairs};

const SQLITE_MAX_BUSY_MS: u64 = 2_147_483_647;

fn refusal(harness: &Harness, config: &Config) -> HexlogError {
    match harness.try_open_with(config, reference_catalog()) {
        Err(error) => error,
        Ok(_) => panic!("expected the open to be refused"),
    }
}

fn with_limits(harness: &Harness, edit: impl FnOnce(&mut Limits)) -> Config {
    let mut config = harness.config();
    edit(&mut config.limits);

    config
}

#[test]
fn it_should_refuse_limits_outside_the_supported_range_when_opening() {
    let harness = Harness::new();
    let cases = [
        ("batch_items", with_limits(&harness, |l| l.batch_items = 0)),
        ("batch_items", with_limits(&harness, |l| l.batch_items = 51)),
        ("details_max", with_limits(&harness, |l| l.details_max = 0)),
        ("details_max", with_limits(&harness, |l| l.details_max = 51)),
        ("page_max", with_limits(&harness, |l| l.page_max = 0)),
        ("page_max", with_limits(&harness, |l| l.page_max = 201)),
        (
            "page_default",
            with_limits(&harness, |l| l.page_default = 0),
        ),
        (
            "page_default",
            with_limits(&harness, |l| l.page_default = l.page_max + 1),
        ),
    ];

    for (field, config) in &cases {
        let error = refusal(&harness, config);

        assert_refusal(
            &error,
            ErrorCode::InvalidInput,
            "limits-out-of-range",
            &format!("/limits/{field}"),
        );
        assert_eq!(detail_pairs(&error).len(), 1, "one detail for {field}");
    }
    let at_the_edge = with_limits(&harness, |l| l.page_default = l.page_max);
    assert!(
        harness
            .try_open_with(&at_the_edge, reference_catalog())
            .is_ok()
    );
}

#[test]
fn it_should_refuse_a_busy_timeout_above_what_sqlite_accepts() {
    let harness = Harness::new();

    for timeout in [Duration::MAX, Duration::from_millis(SQLITE_MAX_BUSY_MS + 1)] {
        let mut config = harness.config();
        config.busy_timeout = timeout;

        let error = refusal(&harness, &config);

        assert_refusal(
            &error,
            ErrorCode::InvalidInput,
            "busy-timeout-out-of-range",
            "/busy_timeout",
        );
    }
    let mut at_the_edge = harness.config();
    at_the_edge.busy_timeout = Duration::from_millis(SQLITE_MAX_BUSY_MS);
    assert!(
        harness
            .try_open_with(&at_the_edge, reference_catalog())
            .is_ok()
    );
}

#[test]
fn it_should_refuse_a_database_path_read_as_a_uri_or_memory_database() {
    let harness = Harness::new();

    for path in ["file:hexlog.db", ":memory:"] {
        let config = Config::new(PathBuf::from(path));

        let error = refusal(&harness, &config);

        assert_refusal(
            &error,
            ErrorCode::InvalidInput,
            "db-path-invalid",
            "/db_path",
        );
    }
}
