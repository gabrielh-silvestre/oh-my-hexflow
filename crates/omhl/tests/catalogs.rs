mod support;

use std::collections::BTreeSet;

use hexlog::{Catalog, DataViolation};
use omhl::graphs;
use serde_json::{Value, json};
use support::{rty, tname};

const NODE_TYPES: [&str; 7] = [
    "premise",
    "guideline",
    "decision",
    "check",
    "attestation",
    "work-item",
    "pull-request",
];
const RELATION_TYPES: [&str; 4] = ["after", "supports", "contradicts", "answers"];

fn catalog() -> Catalog {
    graphs::catalog().expect("the embedded catalogs build")
}

fn violation(pointer: &str, code: &str) -> DataViolation {
    DataViolation {
        pointer: pointer.to_owned(),
        code: code.to_owned(),
    }
}

/// Violações que o catalog aponta para o dado; falha o teste se o dado for aceito.
fn violations_of(type_name: &str, data: &Value) -> Vec<DataViolation> {
    catalog()
        .validate_data(&tname(type_name), data)
        .expect_err("the data is rejected")
}

fn is_accepted(type_name: &str, data: &Value) -> bool {
    catalog().validate_data(&tname(type_name), data).is_ok()
}

fn evidence() -> Value {
    json!([{ "source": "docs/spec.md", "summary": "the spec says so" }])
}

fn gate() -> Value {
    json!({
        "name": "pr-gate",
        "passed": true,
        "chain_head_seq": 3,
        "snapshot": { "kind": "git-commit", "id": "a".repeat(40) },
    })
}

fn attestation(outcome: &str, ok: bool) -> Value {
    json!({
        "check": "check-lint",
        "work_item": "wi-1",
        "outcome": outcome,
        "ok": ok,
    })
}

fn pull_request(number: u64) -> Value {
    json!({
        "title": "Add the catalogs",
        "status": "open",
        "provider": "github",
        "repo": "owner/repo",
        "number": number,
    })
}

fn property_names(catalog: &Catalog, type_name: &str, key: &str) -> BTreeSet<String> {
    let def = catalog.type_def(&tname(type_name)).expect("type exists");
    let listed = &def.schema[key];

    match listed {
        Value::Object(map) => map.keys().cloned().collect(),
        Value::Array(items) => items
            .iter()
            .map(|item| item.as_str().expect("required name is text").to_owned())
            .collect(),
        other => panic!("unexpected `{key}` shape: {other}"),
    }
}

fn endpoints(catalog: &Catalog, name: &str) -> (Vec<String>, Vec<String>, bool) {
    let def = catalog
        .relation_type_def(&rty(name))
        .expect("relation type exists");
    let sorted = |names: &Option<Vec<hexlog::TypeName>>| {
        let mut listed: Vec<String> = names
            .as_ref()
            .expect("endpoints are declared")
            .iter()
            .map(|type_name| type_name.as_str().to_owned())
            .collect();
        listed.sort();
        listed
    };

    (sorted(&def.from), sorted(&def.to), def.acyclic)
}

fn names(listed: &[&str]) -> Vec<String> {
    let mut sorted: Vec<String> = listed.iter().map(|name| (*name).to_owned()).collect();
    sorted.sort();
    sorted
}

#[test]
fn it_should_build_the_catalog_with_seven_node_types_and_four_relation_types() {
    let built = graphs::catalog();

    let catalog = built.expect("the embedded catalogs build");
    let node_types: Vec<bool> = NODE_TYPES
        .iter()
        .map(|name| catalog.type_def(&tname(name)).is_some())
        .collect();
    let relation_types: Vec<bool> = RELATION_TYPES
        .iter()
        .map(|name| catalog.relation_type_def(&rty(name)).is_some())
        .collect();
    assert_eq!((node_types, relation_types), (vec![true; 7], vec![true; 4]));
}

#[test]
fn it_should_require_a_snapshot_only_for_the_attestation() {
    let catalog = catalog();

    let requiring: Vec<&str> = NODE_TYPES
        .into_iter()
        .filter(|name| {
            catalog
                .type_def(&tname(name))
                .expect("type exists")
                .requires_snapshot
        })
        .collect();

    assert_eq!(requiring, ["attestation"]);
}

#[test]
fn it_should_declare_the_endpoints_of_each_relation_type() {
    let catalog = catalog();
    let sources = ["premise", "guideline", "decision"];

    let declared: Vec<(Vec<String>, Vec<String>, bool)> = RELATION_TYPES
        .iter()
        .map(|name| endpoints(&catalog, name))
        .collect();

    assert_eq!(
        declared,
        [
            (names(&["check"]), names(&["check"]), true),
            (names(&sources), names(&["decision"]), false),
            (names(&sources), names(&sources), false),
            (names(&["decision"]), names(&["premise"]), false),
        ]
    );
}

#[test]
fn it_should_accept_a_premise_with_assumed_and_confirmed_status() {
    let assumed = json!({ "text": "users have a git repo", "status": "assumed" });
    let confirmed = json!({ "text": "users have a git repo", "status": "confirmed" });

    let accepted = (
        is_accepted("premise", &assumed),
        is_accepted("premise", &confirmed),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_reject_a_premise_status_outside_assumed_and_confirmed() {
    let premise = json!({ "text": "users have a git repo", "status": "doubtful" });

    let found = violations_of("premise", &premise);

    assert!(found.contains(&violation("/status", "enum")), "{found:?}");
}

#[test]
fn it_should_accept_a_decision_with_embedded_evidence() {
    let decision = json!({ "text": "use sqlite", "evidence": evidence() });

    let accepted = is_accepted("decision", &decision);

    assert!(accepted);
}

#[test]
fn it_should_reject_a_decision_without_evidence() {
    let decision = json!({ "text": "use sqlite" });

    let found = violations_of("decision", &decision);

    assert!(
        found.contains(&violation("/evidence", "required")),
        "{found:?}"
    );
}

#[test]
fn it_should_reject_a_decision_with_an_empty_evidence_list() {
    let decision = json!({ "text": "use sqlite", "evidence": [] });

    let found = violations_of("decision", &decision);

    assert!(
        found.contains(&violation("/evidence", "min-items")),
        "{found:?}"
    );
}

#[test]
fn it_should_accept_a_check_with_and_without_matcher() {
    let with_matcher = json!({
        "when": { "vocabulary": "free", "event": "pre-merge", "matcher": "src/**" },
        "how": "run the linter",
        "blocking": true,
    });
    let without_matcher = json!({
        "when": { "vocabulary": "free", "event": "pre-merge" },
        "how": "run the linter",
        "blocking": false,
    });

    let accepted = (
        is_accepted("check", &with_matcher),
        is_accepted("check", &without_matcher),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_reject_a_check_whose_vocabulary_is_not_free() {
    let check = json!({
        "when": { "vocabulary": "git", "event": "pre-merge" },
        "how": "run the linter",
        "blocking": true,
    });

    let found = violations_of("check", &check);

    assert!(
        found.contains(&violation("/when/vocabulary", "const")),
        "{found:?}"
    );
}

#[test]
fn it_should_accept_an_attestation_for_each_outcome_with_its_ok_flag() {
    let passed = attestation("passed", true);
    let warned = attestation("passed_with_warning", true);
    let failed = attestation("failed", false);

    let accepted = (
        is_accepted("attestation", &passed),
        is_accepted("attestation", &warned),
        is_accepted("attestation", &failed),
    );

    assert_eq!(accepted, (true, true, true));
}

#[test]
fn it_should_reject_an_attestation_whose_ok_flag_contradicts_the_outcome() {
    let failed_but_ok = attestation("failed", true);
    let passed_but_not_ok = attestation("passed", false);

    let rejections = (
        violations_of("attestation", &failed_but_ok).contains(&violation("", "one-of")),
        violations_of("attestation", &passed_but_not_ok).contains(&violation("", "one-of")),
    );

    assert_eq!(rejections, (true, true));
}

#[test]
fn it_should_accept_an_open_work_item_without_a_gate_object() {
    let work_item = json!({ "title": "Ship the catalogs", "status": "open" });

    let accepted = is_accepted("work-item", &work_item);

    assert!(accepted);
}

#[test]
fn it_should_accept_a_closed_work_item_with_a_gate_object() {
    let work_item = json!({ "title": "Ship the catalogs", "status": "closed", "gate": gate() });

    let accepted = is_accepted("work-item", &work_item);

    assert!(accepted);
}

#[test]
fn it_should_reject_a_closed_work_item_without_a_gate_object() {
    let work_item = json!({ "title": "Ship the catalogs", "status": "closed" });

    let found = violations_of("work-item", &work_item);

    assert!(found.contains(&violation("", "one-of")), "{found:?}");
}

#[test]
fn it_should_reject_an_open_work_item_with_a_gate_object() {
    let work_item = json!({ "title": "Ship the catalogs", "status": "open", "gate": gate() });

    let found = violations_of("work-item", &work_item);

    assert!(found.contains(&violation("", "one-of")), "{found:?}");
}

#[test]
fn it_should_reject_a_work_item_status_outside_open_and_closed() {
    let work_item = json!({ "title": "Ship the catalogs", "status": "merged" });

    let found = violations_of("work-item", &work_item);

    assert!(found.contains(&violation("/status", "enum")), "{found:?}");
}

#[test]
fn it_should_accept_a_pull_request_with_base_and_own_fields() {
    let pull_request = pull_request(7);

    let accepted = is_accepted("pull-request", &pull_request);

    assert!(accepted);
}

#[test]
fn it_should_reject_a_pull_request_without_its_own_required_fields() {
    let pull_request = json!({ "title": "Add the catalogs", "status": "open" });

    let found = violations_of("pull-request", &pull_request);

    let missing: BTreeSet<&str> = found
        .iter()
        .filter(|found| found.code == "required")
        .map(|found| found.pointer.as_str())
        .collect();
    assert_eq!(missing, BTreeSet::from(["/number", "/provider", "/repo"]));
}

#[test]
fn it_should_reject_a_pull_request_number_below_one() {
    let pull_request = pull_request(0);

    let found = violations_of("pull-request", &pull_request);

    assert!(
        found.contains(&violation("/number", "minimum")),
        "{found:?}"
    );
}

#[test]
fn it_should_declare_every_work_item_property_and_required_name_in_the_pull_request_schema() {
    let catalog = catalog();

    let base = (
        property_names(&catalog, "work-item", "properties"),
        property_names(&catalog, "work-item", "required"),
    );
    let child = (
        property_names(&catalog, "pull-request", "properties"),
        property_names(&catalog, "pull-request", "required"),
    );

    assert!(
        base.0.is_subset(&child.0) && base.1.is_subset(&child.1),
        "work-item {base:?} is not within pull-request {child:?}"
    );
}

#[test]
fn it_should_list_exactly_the_work_item_types() {
    let catalog = catalog();

    let listed = graphs::WORK_ITEM_TYPES;

    assert_eq!(listed, ["work-item", "pull-request"]);
    assert!(
        listed
            .iter()
            .all(|name| catalog.type_def(&tname(name)).is_some())
    );
}
