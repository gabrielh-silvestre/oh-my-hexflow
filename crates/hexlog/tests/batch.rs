mod support;

use hexlog::{AppendItem, AppendRequest, Author, Catalog, ErrorCode, HexlogError};
use serde_json::json;
use support::catalog::{catalog_of, reference_catalog};
use support::{Harness, assert_refusal, author, detail_pairs, node, request, tenant};

const FIELDS_PER_ITEM: usize = 20;

fn wide_catalog() -> Catalog {
    let names: Vec<String> = (1..=FIELDS_PER_ITEM).map(|n| format!("f{n:02}")).collect();

    catalog_of(vec![("wide", json!({"type": "object", "required": names}))])
}

fn wide_items(count: usize) -> Vec<AppendItem> {
    (0..count)
        .map(|index| node(&format!("a.n{index}"), "wide", json!({})))
        .collect()
}

fn missing_field_pairs(items: usize) -> Vec<(String, String)> {
    (0..items)
        .flat_map(|item| {
            (1..=FIELDS_PER_ITEM).map(move |n| {
                (
                    format!("/items/{item}/body/data/f{n:02}"),
                    "required".to_owned(),
                )
            })
        })
        .collect()
}

#[test]
fn it_should_persist_nothing_when_any_item_of_the_batch_is_invalid() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let head_before = hexlog.chain_head(&tenant("acme")).unwrap();
    let items = vec![
        node("a.one", "note", json!({})),
        node("a.two", "note", json!({})),
        node("a.three", "note", json!({"count": -1})),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidRecord);
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_reject_empty_and_oversized_batches() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let oversized = (0..51)
        .map(|index| node(&format!("a.n{index}"), "note", json!({})))
        .collect();

    for items in [Vec::new(), oversized] {
        let error = hexlog.append(&request("acme", None, items)).unwrap_err();

        assert_refusal(&error, ErrorCode::InvalidInput, "batch-size", "/items");
    }
}

#[test]
fn it_should_reject_a_second_item_for_the_same_lineage_in_the_same_batch() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let head_before = hexlog.chain_head(&tenant("acme")).unwrap();
    let items = vec![
        node("a.one", "note", json!({"text": "root"})),
        node("a.one", "note", json!({"text": "revision"})),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::ForkRejected,
        "lineage-in-batch",
        "/items/1/body",
    );
    assert_eq!(error.details[0].current, None);
    assert_eq!(error.details[0].retired, None);
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_cap_the_violations_of_a_batch_at_50_with_a_single_marker() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme_with(&harness.config(), wide_catalog());

    let capped = hexlog
        .append(&request("acme", None, wide_items(3)))
        .unwrap_err();
    let uncapped = hexlog
        .append(&request("acme", None, wide_items(2)))
        .unwrap_err();

    let mut expected = missing_field_pairs(3);
    expected.truncate(50);
    expected.push((String::new(), "too-many-errors".to_owned()));
    let owned = |error: &HexlogError| -> Vec<(String, String)> {
        detail_pairs(error)
            .into_iter()
            .map(|(path, code)| (path.to_owned(), code.to_owned()))
            .collect()
    };
    assert_eq!(owned(&capped), expected);
    assert_eq!(owned(&uncapped), missing_field_pairs(2));
}

#[test]
fn it_should_reject_an_author_field_that_is_empty_or_longer_than_100_characters() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let long = "a".repeat(101);
    let cases = [
        (
            "/author/agent",
            Author {
                agent: String::new(),
                ..author()
            },
        ),
        (
            "/author/agent",
            Author {
                agent: long.clone(),
                ..author()
            },
        ),
        (
            "/author/client",
            Author {
                client: String::new(),
                ..author()
            },
        ),
        (
            "/author/client",
            Author {
                client: long.clone(),
                ..author()
            },
        ),
        (
            "/author/model",
            Author {
                model: Some(String::new()),
                ..author()
            },
        ),
        (
            "/author/model",
            Author {
                model: Some(long),
                ..author()
            },
        ),
    ];

    for (path, author) in cases {
        let req = AppendRequest {
            author,
            ..request("acme", None, vec![node("a.one", "note", json!({}))])
        };

        let error = hexlog.append(&req).unwrap_err();

        assert_refusal(&error, ErrorCode::InvalidInput, "author-format", path);
    }
}

#[test]
fn it_should_reject_data_over_the_canonical_character_limit_counted_in_utf16_units() {
    let harness = Harness::new();
    let mut config = harness.config();
    config.limits.data_max_chars = 20;
    let mut hexlog = harness.open_acme_with(&config, reference_catalog());
    // O JCS é `{"text":"<texto>"}`: 11 unidades fixas mais as do texto, e o emoji conta 2.
    let over = json!({"text": "aaaaaaaa\u{1F4A9}"});
    let at_limit = json!({"text": "aaaaaaa\u{1F4A9}"});

    let refused = hexlog
        .append(&request("acme", None, vec![node("a.one", "note", over)]))
        .unwrap_err();
    let accepted = hexlog.append(&request(
        "acme",
        None,
        vec![node("a.two", "note", at_limit)],
    ));

    assert_refusal(
        &refused,
        ErrorCode::InvalidRecord,
        "data-too-large",
        "/items/0/body/data",
    );
    assert!(accepted.is_ok());
}

#[test]
fn it_should_report_type_not_found_before_aggregating_data_violations() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let items = vec![
        node("a.one", "note", json!({"count": -1})),
        node("a.two", "ghost", json!({})),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::TypeNotFound,
        "type-not-found",
        "/items/1/body/type",
    );
}
