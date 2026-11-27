use std::collections::BTreeSet;

use crate::graphs;
use crate::test_support::omhl::check_data;
use crate::test_support::{rty, tname};
use hexlog::{Catalog, DataViolation, RelationTypeDef, TypeDef};
use serde_json::{Value, json};

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const PREVIOUS_DECISIONS_CATALOG_V3: &str =
    include_str!("../../tests/fixtures/decisions_catalog_v3.json");
const PREVIOUS_DECISIONS_CATALOG_V4: &str =
    include_str!("../../tests/fixtures/decisions_catalog_v4.json");

const NODE_TYPES: [&str; 9] = [
    "premise",
    "guideline",
    "decision",
    "check",
    "attestation",
    "work-item",
    "pull-request",
    "gap",
    "divergence",
];
const RELATION_TYPES: [&str; 6] = [
    "after",
    "supports",
    "contradicts",
    "answers",
    "rests-on",
    "supersedes",
];

fn catalog() -> Catalog {
    graphs::catalog().expect("the embedded catalogs build")
}

fn violation(pointer: &str, code: &str) -> DataViolation {
    DataViolation {
        pointer: pointer.to_owned(),
        code: code.to_owned(),
    }
}

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

fn catalog_from(source: &str) -> Catalog {
    let file: Value = serde_json::from_str(source).expect("the old catalog is JSON");
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
fn it_should_build_the_catalog_with_nine_node_types_and_six_relation_types() {
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
    assert_eq!((node_types, relation_types), (vec![true; 9], vec![true; 6]));
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
            (names(&sources), names(&["decision", "premise"]), false),
            (names(&sources), names(&sources), false),
            (
                names(&["decision", "premise"]),
                names(&["premise", "gap"]),
                false
            ),
            (names(&sources), names(&["guideline", "premise"]), true),
            (names(&["guideline"]), names(&["guideline"]), true),
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
fn it_should_reject_an_expect_rule_with_both_stdout_and_stderr_in_the_catalog() {
    let check = with(
        mechanical_check(),
        "expect",
        json!([{ "exit": 0, "stdout": "ok", "stderr": "warn", "outcome": "passed" }]),
    );

    let found = violations_of("check", &check);

    assert_eq!(found, vec![violation("/expect/0", "not")]);
}

// Catálogo aditivo de diretivas e premissas: `kind`, `confirmed_by`, `gap` e `divergence`.

const CONFIRMED_TYPES: [&str; 2] = ["premise", "guideline"];

fn confirmation() -> Value {
    json!({
        "by": "person:gabriel",
        "question": "Is the repository shared?",
        "answer": "Yes, always.",
    })
}

fn text_of(length: usize) -> String {
    "x".repeat(length)
}

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

fn divergence() -> Value {
    json!({
        "source_ref": "spec#ac-3",
        "kind": "missing",
        "text": "the spec asks for a report the code does not have",
        "evidence": evidence(),
        "resolution": "open",
        "work_item": "work-items.dir0",
    })
}

fn evidence_of(count: usize) -> Value {
    Value::Array(vec![
        json!({ "source": "docs/spec.md", "summary": "the spec says so" });
        count
    ])
}

fn accepted_by_length(
    type_name: &str,
    lengths: &[usize],
    build: impl Fn(String) -> Value,
) -> Vec<bool> {
    lengths
        .iter()
        .map(|length| is_accepted(type_name, &build(text_of(*length))))
        .collect()
}

fn accepted_by_count(
    type_name: &str,
    counts: &[usize],
    build: impl Fn(Value) -> Value,
) -> Vec<bool> {
    counts
        .iter()
        .map(|count| is_accepted(type_name, &build(evidence_of(*count))))
        .collect()
}

#[test]
fn it_should_accept_a_premise_kind_of_technical_or_business_or_none() {
    let technical = with(plain("premise"), "kind", json!("technical"));
    let business = with(plain("premise"), "kind", json!("business"));

    let accepted = (
        is_accepted("premise", &technical),
        is_accepted("premise", &business),
        is_accepted("premise", &plain("premise")),
    );

    assert_eq!(accepted, (true, true, true));
}

#[test]
fn it_should_reject_a_premise_kind_outside_technical_and_business() {
    let premise = with(plain("premise"), "kind", json!("legal"));

    let found = violations_of("premise", &premise);

    assert_eq!(found, vec![violation("/kind", "enum")]);
}

#[test]
fn it_should_accept_a_premise_origin_of_late_or_none() {
    let late = with(plain("premise"), "origin", json!("late"));

    let accepted = (
        is_accepted("premise", &late),
        is_accepted("premise", &plain("premise")),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_reject_a_premise_origin_outside_late() {
    let premise = with(plain("premise"), "origin", json!("interview"));

    let found = violations_of("premise", &premise);

    assert_eq!(found, vec![violation("/origin", "enum")]);
}

// A mudança só alarga: toda premissa válida antes segue válida.
#[test]
fn it_should_widen_the_premise_definition_without_invalidating_the_previous_one() {
    let previous = catalog_from(PREVIOUS_DECISIONS_CATALOG_V4);
    let business = with(plain("premise"), "kind", json!("business"));
    let late = with(business.clone(), "origin", json!("late"));

    let compatible_change = (
        previous.type_hash(&tname("premise")) != catalog().type_hash(&tname("premise")),
        previous.validate_data(&tname("premise"), &business).is_ok()
            && is_accepted("premise", &business),
        previous.validate_data(&tname("premise"), &late).is_err() && is_accepted("premise", &late),
    );

    assert_eq!(compatible_change, (true, true, true));
}

#[test]
fn it_should_accept_a_guideline_with_each_valid_kind() {
    let business = with(plain("guideline"), "kind", json!("business"));
    let architecture = with(plain("guideline"), "kind", json!("architecture"));

    let accepted = (
        is_accepted("guideline", &business),
        is_accepted("guideline", &architecture),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_reject_a_guideline_kind_outside_business_and_architecture() {
    let guideline = with(plain("guideline"), "kind", json!("technical"));

    let found = violations_of("guideline", &guideline);

    assert_eq!(found, vec![violation("/kind", "enum")]);
}

#[test]
fn it_should_accept_a_confirmation_on_a_premise_and_on_a_guideline() {
    let accepted: Vec<bool> = CONFIRMED_TYPES
        .iter()
        .map(|name| is_accepted(name, &confirmed(name, confirmation())))
        .collect();

    assert_eq!(accepted, [true, true]);
}

#[test]
fn it_should_reject_a_confirmation_missing_by_question_or_answer() {
    let mut found = Vec::new();
    let mut expected = Vec::new();
    for name in CONFIRMED_TYPES {
        for field in ["by", "question", "answer"] {
            let incomplete = confirmed(name, without(confirmation(), field));
            found.push(violations_of(name, &incomplete));
            expected.push(vec![violation(
                &format!("/confirmed_by/{field}"),
                "required",
            )]);
        }
    }

    assert_eq!(found, expected);
}

#[test]
fn it_should_reject_a_confirmation_with_an_extra_property() {
    let found: Vec<Vec<DataViolation>> = CONFIRMED_TYPES
        .iter()
        .map(|name| {
            let extra = with(confirmation(), "via", json!("chat"));
            violations_of(name, &confirmed(name, extra))
        })
        .collect();

    assert_eq!(
        found,
        vec![vec![violation("/confirmed_by/via", "additional-properties")]; 2]
    );
}

#[test]
fn it_should_reject_a_confirmation_that_is_not_an_object() {
    let found: Vec<Vec<DataViolation>> = CONFIRMED_TYPES
        .iter()
        .map(|name| violations_of(name, &confirmed(name, json!("person:gabriel"))))
        .collect();

    assert_eq!(found, vec![vec![violation("/confirmed_by", "type")]; 2]);
}

#[test]
fn it_should_limit_the_confirmation_by_to_1_to_100_characters() {
    let accepted: Vec<Vec<bool>> = CONFIRMED_TYPES
        .iter()
        .map(|name| {
            accepted_by_length(name, &[0, 1, 100, 101], |text| {
                confirmed(name, with(confirmation(), "by", json!(text)))
            })
        })
        .collect();

    assert_eq!(accepted, vec![vec![false, true, true, false]; 2]);
}

#[test]
fn it_should_limit_the_confirmation_question_to_1_to_2000_characters() {
    let accepted: Vec<Vec<bool>> = CONFIRMED_TYPES
        .iter()
        .map(|name| {
            accepted_by_length(name, &[0, 1, 2000, 2001], |text| {
                confirmed(name, with(confirmation(), "question", json!(text)))
            })
        })
        .collect();

    assert_eq!(accepted, vec![vec![false, true, true, false]; 2]);
}

#[test]
fn it_should_limit_the_confirmation_answer_to_1_to_2000_characters() {
    let accepted: Vec<Vec<bool>> = CONFIRMED_TYPES
        .iter()
        .map(|name| {
            accepted_by_length(name, &[0, 1, 2000, 2001], |text| {
                confirmed(name, with(confirmation(), "answer", json!(text)))
            })
        })
        .collect();

    assert_eq!(accepted, vec![vec![false, true, true, false]; 2]);
}

#[test]
fn it_should_accept_a_gap_with_and_without_a_work_item() {
    let linked = with(gap(), "work_item", json!("work-items.dir0"));

    let accepted = (is_accepted("gap", &gap()), is_accepted("gap", &linked));

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_reject_a_gap_missing_question_or_context() {
    let found = (
        violations_of("gap", &without(gap(), "question")),
        violations_of("gap", &without(gap(), "context")),
    );

    assert_eq!(
        found,
        (
            vec![violation("/question", "required")],
            vec![violation("/context", "required")]
        )
    );
}

#[test]
fn it_should_reject_a_gap_with_an_extra_property() {
    let gap = with(gap(), "owner", json!("person:gabriel"));

    let found = violations_of("gap", &gap);

    assert_eq!(found, vec![violation("/owner", "additional-properties")]);
}

#[test]
fn it_should_accept_a_gap_with_kind_technical() {
    let gap = with(gap(), "kind", json!("technical"));

    let accepted = is_accepted("gap", &gap);

    assert!(accepted);
}

#[test]
fn it_should_accept_a_gap_with_kind_business() {
    let gap = with(gap(), "kind", json!("business"));

    let accepted = is_accepted("gap", &gap);

    assert!(accepted);
}

#[test]
fn it_should_reject_a_gap_with_a_kind_outside_technical_and_business() {
    let gap = with(gap(), "kind", json!("legal"));

    let found = violations_of("gap", &gap);

    assert_eq!(found, vec![violation("/kind", "enum")]);
}

#[test]
fn it_should_keep_a_gap_without_kind_valid() {
    let accepted = is_accepted("gap", &gap());

    assert!(accepted);
}

// O motor não expõe `major.minor` da definition; o que se observa é o hash da definition
// (mudou) e a regra da minor: a mudança só alarga (todo gap válido na definition anterior segue
// válido; o `kind` que a anterior rejeita passa a ser aceito), mesma major.
#[test]
fn it_should_raise_the_gap_definition_by_one_minor_and_keep_its_major() {
    let previous = catalog_from(PREVIOUS_DECISIONS_CATALOG_V3);
    let linked = with(gap(), "work_item", json!("work-items.dir0"));
    let technical = with(gap(), "kind", json!("technical"));

    let compatible_change = (
        previous.type_hash(&tname("gap")) != catalog().type_hash(&tname("gap")),
        previous.validate_data(&tname("gap"), &linked).is_ok() && is_accepted("gap", &linked),
        previous.validate_data(&tname("gap"), &gap()).is_ok() && is_accepted("gap", &gap()),
        previous.validate_data(&tname("gap"), &technical).is_err()
            && is_accepted("gap", &technical),
    );

    assert_eq!(compatible_change, (true, true, true, true));
}

#[test]
fn it_should_limit_the_gap_question_to_1_to_2000_characters() {
    let accepted = accepted_by_length("gap", &[0, 1, 2000, 2001], |text| {
        with(gap(), "question", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_limit_the_gap_context_to_1_to_2000_characters() {
    let accepted = accepted_by_length("gap", &[0, 1, 2000, 2001], |text| {
        with(gap(), "context", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_limit_the_gap_work_item_to_1_to_200_characters() {
    let accepted = accepted_by_length("gap", &[0, 1, 200, 201], |text| {
        with(gap(), "work_item", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_accept_a_complete_divergence() {
    let accepted = is_accepted("divergence", &divergence());

    assert!(accepted);
}

#[test]
fn it_should_accept_a_divergence_with_confirmation_and_amendment() {
    let amended = with(
        with(divergence(), "confirmed_by", confirmation()),
        "amendment",
        json!("emenda-01"),
    );

    let accepted = is_accepted("divergence", &amended);

    assert!(accepted);
}

#[test]
fn it_should_reject_a_divergence_missing_a_required_field() {
    let required = [
        "source_ref",
        "kind",
        "text",
        "evidence",
        "resolution",
        "work_item",
    ];

    let found: Vec<Vec<DataViolation>> = required
        .iter()
        .map(|field| violations_of("divergence", &without(divergence(), field)))
        .collect();

    let expected: Vec<Vec<DataViolation>> = required
        .iter()
        .map(|field| vec![violation(&format!("/{field}"), "required")])
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn it_should_reject_a_divergence_with_an_extra_property() {
    let divergence = with(divergence(), "severity", json!("high"));

    let found = violations_of("divergence", &divergence);

    assert_eq!(found, vec![violation("/severity", "additional-properties")]);
}

#[test]
fn it_should_accept_each_divergence_kind() {
    let accepted: Vec<bool> = ["missing", "partial", "contradicts", "unrequested"]
        .iter()
        .map(|kind| is_accepted("divergence", &with(divergence(), "kind", json!(kind))))
        .collect();

    assert_eq!(accepted, [true; 4]);
}

#[test]
fn it_should_reject_a_divergence_kind_outside_the_four_kinds() {
    let divergence = with(divergence(), "kind", json!("wrong"));

    let found = violations_of("divergence", &divergence);

    assert_eq!(found, vec![violation("/kind", "enum")]);
}

#[test]
fn it_should_accept_each_divergence_resolution() {
    let accepted: Vec<bool> = ["open", "spec-stale", "code-fixed", "accepted"]
        .iter()
        .map(|resolution| {
            is_accepted(
                "divergence",
                &with(divergence(), "resolution", json!(resolution)),
            )
        })
        .collect();

    assert_eq!(accepted, [true; 4]);
}

#[test]
fn it_should_reject_a_divergence_resolution_outside_the_four_resolutions() {
    let divergence = with(divergence(), "resolution", json!("ignored"));

    let found = violations_of("divergence", &divergence);

    assert_eq!(found, vec![violation("/resolution", "enum")]);
}

#[test]
fn it_should_limit_the_divergence_source_ref_to_1_to_200_characters() {
    let accepted = accepted_by_length("divergence", &[0, 1, 200, 201], |text| {
        with(divergence(), "source_ref", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_limit_the_divergence_text_to_1_to_2000_characters() {
    let accepted = accepted_by_length("divergence", &[0, 1, 2000, 2001], |text| {
        with(divergence(), "text", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_limit_the_divergence_work_item_to_1_to_200_characters() {
    let accepted = accepted_by_length("divergence", &[0, 1, 200, 201], |text| {
        with(divergence(), "work_item", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_limit_the_divergence_amendment_to_1_to_100_characters() {
    let accepted = accepted_by_length("divergence", &[0, 1, 100, 101], |text| {
        with(divergence(), "amendment", json!(text))
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_limit_the_divergence_evidence_to_1_to_20_items() {
    let accepted = accepted_by_count("divergence", &[0, 1, 20, 21], |items| {
        with(divergence(), "evidence", items)
    });

    assert_eq!(accepted, [false, true, true, false]);
}

#[test]
fn it_should_reject_a_divergence_evidence_item_without_summary() {
    let divergence = with(
        divergence(),
        "evidence",
        json!([{ "source": "docs/spec.md" }]),
    );

    let found = violations_of("divergence", &divergence);

    assert_eq!(found, vec![violation("/evidence/0/summary", "required")]);
}

#[test]
fn it_should_reject_a_divergence_evidence_item_with_an_extra_property() {
    let item = json!({ "source": "docs/spec.md", "summary": "says so", "line": 4 });
    let divergence = with(divergence(), "evidence", json!([item]));

    let found = violations_of("divergence", &divergence);

    assert_eq!(
        found,
        vec![violation("/evidence/0/line", "additional-properties")]
    );
}

#[test]
fn it_should_reject_a_divergence_confirmation_missing_a_field() {
    let incomplete = with(
        divergence(),
        "confirmed_by",
        without(confirmation(), "answer"),
    );

    let found = violations_of("divergence", &incomplete);

    assert_eq!(found, vec![violation("/confirmed_by/answer", "required")]);
}

#[test]
fn it_should_still_accept_a_premise_and_a_guideline_without_the_new_fields() {
    let premise = old_decisions_premise();
    let guideline = old_decisions_guideline();

    let accepted = (
        is_accepted("premise", &premise),
        is_accepted("guideline", &guideline),
    );

    assert_eq!(accepted, (true, true));
}

#[test]
fn it_should_still_accept_a_guideline_without_kind() {
    let guideline = confirmed("guideline", confirmation());

    let accepted = is_accepted("guideline", &guideline);

    assert!(accepted);
}

fn old_decisions_premise() -> Value {
    json!({ "text": "users have a git repo", "status": "confirmed", "work_item": "work-items.w1" })
}

fn old_decisions_guideline() -> Value {
    json!({ "text": "prefer small modules" })
}

fn work_item_with_type(type_value: &Value) -> Value {
    json!({ "title": "Add the catalogs", "status": "open", "type": type_value.clone() })
}

#[test]
fn it_should_accept_a_type_on_a_work_item_and_on_a_pull_request() {
    let typed_pull_request = {
        let mut data = pull_request(1);
        data["type"] = json!("feat");
        data
    };

    assert!(is_accepted(
        "work-item",
        &work_item_with_type(&json!("feat"))
    ));
    assert!(is_accepted("pull-request", &typed_pull_request));
}

#[test]
fn it_should_reject_an_empty_a_long_or_a_non_text_type_on_both_types() {
    let too_long = "x".repeat(65);
    let refused = [json!(""), json!(too_long), json!(7)];

    for type_value in refused {
        let work_item = work_item_with_type(&type_value);
        let mut pull_request = pull_request(1);
        pull_request["type"] = type_value;

        assert!(
            violations_of("work-item", &work_item)
                .iter()
                .any(|found| found.pointer == "/type"),
            "work-item accepted {work_item}"
        );
        assert!(
            violations_of("pull-request", &pull_request)
                .iter()
                .any(|found| found.pointer == "/type"),
            "pull-request accepted {pull_request}"
        );
    }
}

#[test]
fn it_should_accept_a_type_of_exactly_sixty_four_characters() {
    let exact = "t".repeat(64);

    assert!(is_accepted(
        "work-item",
        &work_item_with_type(&json!(exact))
    ));
}
