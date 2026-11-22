use crate::support;

use std::collections::BTreeSet;

use hexlog::{Catalog, RelationTypeDef, TypeDef};
use omhl::app::list_filter::is_text_field;
use omhl::graphs;
use serde_json::{Value, json};
use support::omhl::{acme, check_data, node_item, request};
use support::raw::RawHarness;
use support::{EmptyNative, sid, tname};

const OLD_CHECKS_CATALOG: &str = include_str!("../fixtures/checks_catalog_free_events.json");
const OLD_VOCABULARY: &str = "free";
const OLD_DECISIONS_CATALOG: &str = include_str!("../fixtures/decisions_catalog_v1.json");
const PREVIOUS_DECISIONS_CATALOG: &str = include_str!("../fixtures/decisions_catalog_v2.json");
const PREVIOUS_DECISIONS_CATALOG_V3: &str = include_str!("../fixtures/decisions_catalog_v3.json");
const PREVIOUS_DECISIONS_CATALOG_V4: &str = include_str!("../fixtures/decisions_catalog_v4.json");

fn catalog() -> Catalog {
    graphs::catalog().expect("the embedded catalogs build")
}

fn is_accepted(type_name: &str, data: &Value) -> bool {
    catalog().validate_data(&tname(type_name), data).is_ok()
}

fn with(mut data: Value, key: &str, value: Value) -> Value {
    data[key] = value;

    data
}

fn old_catalog() -> Catalog {
    catalog_from(OLD_CHECKS_CATALOG)
}

fn old_decisions_catalog() -> Catalog {
    catalog_from(OLD_DECISIONS_CATALOG)
}

fn previous_decisions_catalog() -> Catalog {
    catalog_from(PREVIOUS_DECISIONS_CATALOG)
}

fn catalog_from(source: &str) -> Catalog {
    let file: Value = serde_json::from_str(source).expect("the old catalog is JSON");
    let types: Vec<TypeDef> =
        serde_json::from_value(file["types"].clone()).expect("the old types decode");
    let relations: Vec<RelationTypeDef> =
        serde_json::from_value(file["relations"].clone()).expect("the old relations decode");

    Catalog::new(types, relations).expect("the old catalog builds")
}

#[test]
fn it_should_still_verify_a_chain_written_with_the_previous_catalog() {
    let raw = RawHarness::new();
    let acme = acme();
    let mut old_data = check_data("before-deploy", "Run lint", true);
    old_data["when"]["vocabulary"] = json!(OLD_VOCABULARY);
    {
        let mut old = raw.hexlog(old_catalog(), &EmptyNative);
        old.ensure_tenant(&acme).expect("the tenant is created");
        for id in ["checks.lint", "checks.fmt"] {
            old.append(&request(vec![node_item(id, "check", old_data.clone())]))
                .expect("the old catalog accepts the check");
        }
    }

    let mut reopened = raw.hexlog(catalog(), &EmptyNative);
    let report = reopened
        .verify_chain(&acme, None)
        .expect("the chain verifies");
    let stored = reopened
        .get(&acme, &sid("checks.lint"))
        .expect("the node is read")
        .expect("the node exists");

    assert_eq!(
        (report.ok, report.total_records, report.total_breaks),
        (true, 2, 0)
    );
    assert_eq!(stored.data, old_data);
    assert!(!is_accepted("check", &old_data));
}

fn confirmation() -> Value {
    json!({
        "by": "person:gabriel",
        "question": "Is the repository shared?",
        "answer": "Yes, always.",
    })
}

/// `premise` ou `guideline` mínimo e válido, sem os campos novos.
fn plain(type_name: &str) -> Value {
    match type_name {
        "premise" => json!({ "text": "users have a git repo", "status": "assumed" }),
        _ => json!({ "text": "prefer small modules" }),
    }
}

fn confirmed(type_name: &str, confirmation: Value) -> Value {
    with(plain(type_name), "confirmed_by", confirmation)
}

fn gap() -> Value {
    json!({ "question": "Who deploys?", "context": "the spec does not say" })
}

#[test]
fn it_should_still_verify_a_decisions_chain_written_with_the_previous_catalog() {
    let raw = RawHarness::new();
    let acme = acme();
    {
        let mut old = raw.hexlog(old_decisions_catalog(), &EmptyNative);
        old.ensure_tenant(&acme).expect("the tenant is created");
        for item in old_decisions_records() {
            old.append(&request(vec![item]))
                .expect("the old catalog accepts the record");
        }
    }

    let mut reopened = raw.hexlog(catalog(), &EmptyNative);
    let report = reopened
        .verify_chain(&acme, None)
        .expect("the chain verifies");

    assert_eq!(
        (report.ok, report.total_records, report.total_breaks),
        (true, 2, 0)
    );
}

#[test]
fn it_should_append_a_confirmed_premise_to_a_chain_written_with_the_previous_catalog() {
    let raw = RawHarness::new();
    let acme = acme();
    {
        let mut old = raw.hexlog(old_decisions_catalog(), &EmptyNative);
        old.ensure_tenant(&acme).expect("the tenant is created");
        old.append(&request(old_decisions_records()))
            .expect("the old catalog accepts the records");
    }
    let mut reopened = raw.hexlog(catalog(), &EmptyNative);
    let premise = with(
        confirmed("premise", confirmation()),
        "kind",
        json!("business"),
    );

    let appended = reopened.append(&request(vec![node_item(
        "premises.p-2",
        "premise",
        premise,
    )]));

    assert!(appended.is_ok(), "{appended:?}");
}

#[test]
fn it_should_still_verify_a_decisions_chain_written_with_the_previous_catalog_v2() {
    let raw = RawHarness::new();
    let acme = acme();
    {
        let mut previous = raw.hexlog(previous_decisions_catalog(), &EmptyNative);
        previous
            .ensure_tenant(&acme)
            .expect("the tenant is created");
        previous
            .append(&request(vec![
                node_item(
                    "premises.p-1",
                    "premise",
                    confirmed("premise", confirmation()),
                ),
                node_item(
                    "guidelines.g-1",
                    "guideline",
                    confirmed("guideline", confirmation()),
                ),
            ]))
            .expect("the previous catalog accepts the records");
    }

    let mut reopened = raw.hexlog(catalog(), &EmptyNative);
    let report = reopened
        .verify_chain(&acme, None)
        .expect("the chain verifies");

    assert_eq!(
        (report.ok, report.total_records, report.total_breaks),
        (true, 2, 0)
    );
}

#[test]
fn it_should_still_verify_a_decisions_chain_written_with_the_previous_catalog_v3() {
    let raw = RawHarness::new();
    let acme = acme();
    {
        let mut previous = raw.hexlog(catalog_from(PREVIOUS_DECISIONS_CATALOG_V3), &EmptyNative);
        previous
            .ensure_tenant(&acme)
            .expect("the tenant is created");
        previous
            .append(&request(vec![
                node_item("gaps.q-1", "gap", gap()),
                node_item(
                    "premises.p-1",
                    "premise",
                    confirmed("premise", confirmation()),
                ),
            ]))
            .expect("the previous catalog accepts the records");
    }

    let mut reopened = raw.hexlog(catalog(), &EmptyNative);
    let report = reopened
        .verify_chain(&acme, None)
        .expect("the chain verifies");

    assert_eq!(
        (report.ok, report.total_records, report.total_breaks),
        (true, 2, 0)
    );
}

#[test]
fn it_should_still_verify_a_decisions_chain_written_with_the_previous_catalog_v4() {
    let raw = RawHarness::new();
    let acme = acme();
    {
        let mut previous = raw.hexlog(catalog_from(PREVIOUS_DECISIONS_CATALOG_V4), &EmptyNative);
        previous
            .ensure_tenant(&acme)
            .expect("the tenant is created");
        previous
            .append(&request(vec![
                node_item(
                    "premises.p-1",
                    "premise",
                    confirmed("premise", confirmation()),
                ),
                node_item(
                    "premises.p-2",
                    "premise",
                    with(plain("premise"), "kind", json!("business")),
                ),
            ]))
            .expect("the previous catalog accepts the records");
    }

    let mut reopened = raw.hexlog(catalog(), &EmptyNative);
    let report = reopened
        .verify_chain(&acme, None)
        .expect("the chain verifies");

    assert_eq!(
        (report.ok, report.total_records, report.total_breaks),
        (true, 2, 0)
    );
}

fn old_decisions_premise() -> Value {
    json!({ "text": "users have a git repo", "status": "confirmed", "work_item": "work-items.w1" })
}

fn old_decisions_guideline() -> Value {
    json!({ "text": "prefer small modules" })
}

fn old_decisions_records() -> Vec<hexlog::AppendItem> {
    vec![
        node_item("premises.p-1", "premise", old_decisions_premise()),
        node_item("guidelines.g-1", "guideline", old_decisions_guideline()),
    ]
}

// Campos de texto para o filtro `where` do `list`

fn where_fields(catalog: &Catalog, type_name: &str, text: bool) -> BTreeSet<String> {
    let def = catalog.type_def(&tname(type_name)).expect("type exists");

    def.schema["properties"]
        .as_object()
        .expect("the type declares properties")
        .iter()
        .filter(|(_, schema)| is_text_field(schema) == text)
        .map(|(field, _)| field.clone())
        .collect()
}

fn field_set(fields: &[&str]) -> BTreeSet<String> {
    fields.iter().map(|field| (*field).to_owned()).collect()
}

#[test]
fn it_should_classify_every_catalog_field_for_where() {
    let catalog = catalog();
    let expected: [(&str, &[&str], &[&str]); 9] = [
        (
            "check",
            &["how", "concern", "variant"],
            &["when", "blocking", "run", "timeout_secs", "expect"],
        ),
        (
            "attestation",
            &[
                "check",
                "work_item",
                "outcome",
                "note",
                "spec_digest",
                "byproduct",
            ],
            &[
                "ok",
                "exit_code",
                "duration_ms",
                "timed_out",
                "truncated",
                "command",
            ],
        ),
        (
            "premise",
            &["text", "status", "work_item", "kind", "origin"],
            &["confirmed_by"],
        ),
        ("guideline", &["text", "kind"], &["confirmed_by"]),
        ("decision", &["text", "work_item"], &["evidence"]),
        ("gap", &["question", "context", "work_item", "kind"], &[]),
        (
            "divergence",
            &[
                "source_ref",
                "kind",
                "text",
                "resolution",
                "work_item",
                "amendment",
            ],
            &["evidence", "confirmed_by"],
        ),
        ("work-item", &["title", "status"], &["gate"]),
        (
            "pull-request",
            &["title", "status", "provider", "repo", "branch"],
            &["gate", "number"],
        ),
    ];

    for (type_name, text, other) in expected {
        assert_eq!(
            where_fields(&catalog, type_name, true),
            field_set(text),
            "text fields of {type_name}"
        );
        assert_eq!(
            where_fields(&catalog, type_name, false),
            field_set(other),
            "non-text fields of {type_name}"
        );
    }
}

#[test]
fn it_should_treat_an_enum_field_without_a_type_as_text_for_where() {
    let catalog = catalog();
    let def = catalog.type_def(&tname("premise")).expect("type exists");

    let status = &def.schema["properties"]["status"];

    assert!(status.get("type").is_none(), "the enum carries no `type`");
    assert!(is_text_field(status));
}
