mod support;

use std::collections::BTreeSet;

use hexlog::{Catalog, DataViolation, NativeNodes, RelationTypeDef, TypeDef};
use omhl::graphs;
use omhl::native::NativeBase;
use serde_json::{Value, json};
use support::omhl::{acme, check_data, node_item, request};
use support::raw::RawHarness;
use support::{EmptyNative, rty, sid, tname};

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const OLD_CHECKS_CATALOG: &str = include_str!("fixtures/checks_catalog_free_events.json");
const OLD_VOCABULARY: &str = "free";

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

fn without(mut data: Value, key: &str) -> Value {
    data.as_object_mut()
        .expect("the data is an object")
        .remove(key);

    data
}

fn with(mut data: Value, key: &str, value: Value) -> Value {
    data[key] = value;

    data
}

fn mechanical_check() -> Value {
    let mut data = check_data("before-commit", "Run the linter", true);
    data["concern"] = json!("style");
    data["variant"] = json!("lint");
    data["run"] = json!(["cargo", "clippy"]);
    data["timeout_secs"] = json!(120);
    data["expect"] = json!([{ "exit": 0, "outcome": "passed" }]);

    data
}

fn mechanical_attestation() -> Value {
    json!({
        "check": "checks.lint",
        "work_item": "work-items.w1",
        "outcome": "passed",
        "ok": true,
        "exit_code": 0,
        "duration_ms": 1234,
        "timed_out": false,
        "truncated": false,
        "command": ["cargo", "clippy"],
        "byproduct": EMPTY_SHA256,
    })
}

fn old_catalog() -> Catalog {
    let file: Value = serde_json::from_str(OLD_CHECKS_CATALOG).expect("the old catalog is JSON");
    let types: Vec<TypeDef> =
        serde_json::from_value(file["types"].clone()).expect("the old types decode");
    let relations: Vec<RelationTypeDef> =
        serde_json::from_value(file["relations"].clone()).expect("the old relations decode");

    Catalog::new(types, relations).expect("the old catalog builds")
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
        "when": { "vocabulary": "closed", "event": "before-merge", "matcher": "src/**" },
        "how": "run the linter",
        "blocking": true,
    });
    let without_matcher = json!({
        "when": { "vocabulary": "closed", "event": "before-merge" },
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
fn it_should_reject_a_check_whose_vocabulary_is_not_closed() {
    let check = json!({
        "when": { "vocabulary": "git", "event": "before-merge" },
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

#[test]
fn it_should_accept_a_mechanical_check_with_run_timeout_and_expect() {
    let report_lint = with(
        with(
            with(
                with(
                    with(
                        check_data("before-commit", "Run clippy on the workspace.", true),
                        "concern",
                        json!("style"),
                    ),
                    "variant",
                    json!("lint"),
                ),
                "run",
                json!([
                    "cargo",
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked"
                ]),
            ),
            "timeout_secs",
            json!(600),
        ),
        "expect",
        json!([
            { "exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning" },
            { "exit": 0, "outcome": "passed" },
        ]),
    );

    let accepted = (
        is_accepted("check", &mechanical_check()),
        is_accepted("check", &report_lint),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_accept_a_check_that_is_only_text() {
    let by_hand = check_data("before-commit", "Review the diff by hand.", false);
    let mut with_matcher = check_data("before-merge", "run the linter", true);
    with_matcher["when"]["matcher"] = json!("src/**");

    let accepted = (
        is_accepted("check", &by_hand),
        is_accepted("check", &with_matcher),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_reject_a_style_check_with_the_suite_variant() {
    let check = with(mechanical_check(), "variant", json!("suite"));

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("", "one-of")]);
}

#[test]
fn it_should_reject_a_concern_without_a_variant() {
    let check = without(mechanical_check(), "variant");

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("", "one-of")]);
}

#[test]
fn it_should_reject_run_without_timeout_secs() {
    let check = without(mechanical_check(), "timeout_secs");

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("", "any-of")]);
}

#[test]
fn it_should_reject_run_without_expect() {
    let check = without(mechanical_check(), "expect");

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("", "any-of")]);
}

#[test]
fn it_should_reject_an_empty_expect_list() {
    let check = with(mechanical_check(), "expect", json!([]));

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("/expect", "min-items")]);
}

#[test]
fn it_should_reject_an_unknown_outcome_in_an_expect_rule() {
    let check = with(
        mechanical_check(),
        "expect",
        json!([{ "exit": 0, "outcome": "skipped" }]),
    );

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("/expect/0/outcome", "enum")]);
}

#[test]
fn it_should_reject_an_unknown_field_in_an_expect_rule() {
    let check = with(
        mechanical_check(),
        "expect",
        json!([{ "exit": 0, "outcome": "passed", "code": 1 }]),
    );

    let found = violations_of("check", &check);

    assert_eq!(
        found,
        vec![violation("/expect/0/code", "additional-properties")]
    );
}

#[test]
fn it_should_reject_a_snapshot_kind_field_in_a_check() {
    let check = with(mechanical_check(), "snapshot_kind", json!("git-tree"));

    let found = violations_of("check", &check);

    assert_eq!(
        found,
        vec![violation("/snapshot_kind", "additional-properties")]
    );
}

#[test]
fn it_should_reject_a_run_longer_than_64_items() {
    let too_long = with(mechanical_check(), "run", json!(vec!["x"; 65]));
    let at_limit = with(mechanical_check(), "run", json!(vec!["x"; 64]));

    let found = violations_of("check", &too_long);

    assert_eq!(found, vec![violation("/run", "max-items")]);
    assert!(is_accepted("check", &at_limit));
}

#[test]
fn it_should_accept_the_new_attestation_fields() {
    let attestation = mechanical_attestation();

    let accepted = is_accepted("attestation", &attestation);

    assert!(accepted);
}

#[test]
fn it_should_accept_an_attestation_without_exit_code() {
    let timed_out = with(
        with(
            with(
                without(mechanical_attestation(), "exit_code"),
                "timed_out",
                json!(true),
            ),
            "outcome",
            json!("failed"),
        ),
        "ok",
        json!(false),
    );

    let accepted = is_accepted("attestation", &timed_out);

    assert!(accepted);
}

#[test]
fn it_should_reject_a_byproduct_that_is_not_a_hash() {
    let upper = EMPTY_SHA256.to_uppercase();
    let candidates = [upper.as_str(), &EMPTY_SHA256[..63], "not-a-hash"];

    let found: Vec<Vec<DataViolation>> = candidates
        .iter()
        .map(|byproduct| {
            let attestation = with(mechanical_attestation(), "byproduct", json!(byproduct));
            violations_of("attestation", &attestation)
        })
        .collect();

    assert_eq!(found, vec![vec![violation("/byproduct", "format")]; 3]);
}

#[test]
fn it_should_reject_an_event_outside_the_closed_list() {
    let check = check_data("before-deploy", "Run the linter", true);

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("/when/event", "enum")]);
}

#[test]
fn it_should_reject_the_native_review_event_in_a_check_record() {
    let check = check_data("before-publishing-review-comments", "Run the linter", true);

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("/when/event", "enum")]);
}

#[test]
fn it_should_reject_an_exit_code_that_is_not_an_integer() {
    let as_text = with(mechanical_attestation(), "exit_code", json!("0"));
    let as_bool = with(mechanical_attestation(), "exit_code", json!(true));

    let found = (
        violations_of("attestation", &as_text),
        violations_of("attestation", &as_bool),
    );

    assert_eq!(
        found,
        (
            vec![violation("/exit_code", "type")],
            vec![violation("/exit_code", "type")]
        )
    );
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

#[test]
fn it_should_keep_the_native_catalog_version_unchanged() {
    let native = NativeBase;

    let versions = (
        omhl::native::NATIVE_CATALOG_VERSION,
        native.catalog_version(),
    );

    assert_eq!(versions, ("1.0.0", "1.0.0"));
}

#[test]
fn it_should_reject_an_expect_rule_with_both_stdout_and_stderr_in_the_catalog() {
    let check = with(
        mechanical_check(),
        "expect",
        json!([{ "exit": 0, "stdout": "ok", "stderr": "warn", "outcome": "passed" }]),
    );

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("/expect/0", "not")]);
}
