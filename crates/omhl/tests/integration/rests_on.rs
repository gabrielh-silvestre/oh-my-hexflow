use crate::support;

use hexlog::{AppendItem, AppendRequest, ErrorCode, RecordId};
use omhl::app::App;
use omhl::app::action::{Action, Caller};
use omhl::app::retry::RetryPolicy;
use omhl::error::{OmhlError, RuleCode};
use omhl::facade::{Omhl, OmhlDeps};
use omhl::graphs;
use omhl::native::NativeBase;
use serde_json::{Value, json};
use support::omhl::{
    OmhlHarness, acme, decision_data, node_item, premise_data, request, revision_item,
};
use support::pr_gate::relation_item;
use support::raw::RawHarness;
use support::{FakeProbe, loader_author};

const SOURCE: &str = "guidelines.source";
const TARGET: &str = "guidelines.target";
const NATIVE_GUIDELINE: &str = "guidelines.commit-policy";
const DECISION: &str = "decisions.d-1";
const PREMISE: &str = "premises.p-1";
const OTHER_PREMISE: &str = "premises.p-2";

fn rule(code: RuleCode, pointer: &str) -> OmhlError {
    OmhlError::rule(code, pointer)
}

/// Ponteiro do destino da relação que ocupa o `index` do lote.
fn to_pointer(index: usize) -> String {
    format!("/items/{index}/body/to")
}

fn guideline(id: &str, kind: &str) -> AppendItem {
    node_item(
        id,
        "guideline",
        json!({ "text": "keep it simple", "kind": kind }),
    )
}

fn premise(id: &str) -> AppendItem {
    node_item(id, "premise", premise_data("users pay monthly", None))
}

fn decision(id: &str) -> AppendItem {
    node_item(id, "decision", decision_data("use sqlite", None))
}

fn rests_on(from: &str, to: &str) -> AppendItem {
    relation_item("rests-on", from, to)
}

/// O `loader` dispensa a confirmação humana da diretiva, que não é o assunto aqui.
fn appended(omhl: &mut Omhl<'_>, items: Vec<AppendItem>) -> Result<usize, OmhlError> {
    let request = AppendRequest {
        author: loader_author(),
        ..request(items)
    };

    omhl.append(&request).map(|outcome| outcome.records.len())
}

/// Grava o lote e devolve o id do record de cada item, na ordem.
fn seed(omhl: &mut Omhl<'_>, items: Vec<AppendItem>) -> Vec<RecordId> {
    let request = AppendRequest {
        author: loader_author(),
        ..request(items)
    };
    let outcome = omhl.append(&request).expect("the seed is accepted");

    outcome
        .records
        .into_iter()
        .map(|record| record.id)
        .collect()
}

/// Código e código do primeiro detalhe da recusa do motor; falha o teste se a recusa for outra.
fn engine_refusal(result: Result<usize, OmhlError>) -> (ErrorCode, String) {
    match result {
        Err(OmhlError::Hexlog(error)) => (error.code, error.details[0].code.clone()),
        other => panic!("the engine refuses the append, got {other:?}"),
    }
}

/// Grava uma diretiva sem `kind` direto no motor: a guarda do `omhl` não deixa gravá-la.
fn seed_legacy_guideline(raw: &RawHarness, id: &str) {
    let catalog = graphs::catalog().expect("the embedded catalogs build");
    let native = NativeBase;
    let mut hexlog = raw.hexlog(catalog, &native);
    hexlog
        .ensure_tenant(&acme())
        .expect("the tenant is created");
    let legacy = node_item(id, "guideline", json!({ "text": "keep it simple" }));

    hexlog
        .append(&request(vec![legacy]))
        .expect("the legacy guideline is accepted");
}

// Aceitos: origem que não é diretiva

#[test]
fn it_should_accept_a_decision_resting_on_a_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![decision(DECISION), guideline(TARGET, "business")],
    );

    let result = appended(&mut omhl, vec![rests_on(DECISION, TARGET)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_decision_resting_on_a_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![decision(DECISION), premise(PREMISE)]);

    let result = appended(&mut omhl, vec![rests_on(DECISION, PREMISE)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_premise_resting_on_a_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![premise(PREMISE), guideline(TARGET, "business")],
    );

    let result = appended(&mut omhl, vec![rests_on(PREMISE, TARGET)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_premise_resting_on_another_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![premise(PREMISE), premise(OTHER_PREMISE)]);

    let result = appended(&mut omhl, vec![rests_on(PREMISE, OTHER_PREMISE)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_decision_resting_on_an_architecture_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![decision(DECISION), guideline(TARGET, "architecture")],
    );

    let result = appended(&mut omhl, vec![rests_on(DECISION, TARGET)]);

    assert_eq!(result, Ok(1));
}

// Aceitos: origem diretiva

#[test]
fn it_should_accept_a_business_guideline_resting_on_a_business_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![guideline(SOURCE, "business"), guideline(TARGET, "business")],
    );

    let result = appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_an_architecture_guideline_resting_on_a_business_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![
            guideline(SOURCE, "architecture"),
            guideline(TARGET, "business"),
        ],
    );

    let result = appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_guideline_resting_on_a_native_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![guideline(SOURCE, "business")]);

    let result = appended(&mut omhl, vec![rests_on(SOURCE, NATIVE_GUIDELINE)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_native_guideline_resting_on_a_business_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![guideline(TARGET, "business")]);

    let result = appended(&mut omhl, vec![rests_on(NATIVE_GUIDELINE, TARGET)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_guideline_resting_on_a_legacy_guideline_without_kind() {
    let raw = RawHarness::new();
    seed_legacy_guideline(&raw, TARGET);
    let probe = FakeProbe::failing();
    let deps = OmhlDeps {
        clock: &raw.clock,
        ids: &raw.ids,
        probe: &probe,
    };
    let mut omhl = Omhl::open(&raw.config(), deps).expect("omhl opens");
    seed(&mut omhl, vec![guideline(SOURCE, "business")]);

    let result = appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_guideline_and_the_relation_that_rests_on_it_in_the_same_batch() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![guideline(SOURCE, "business")]);

    let result = appended(
        &mut omhl,
        vec![guideline(TARGET, "business"), rests_on(SOURCE, TARGET)],
    );

    assert_eq!(result, Ok(2));
}

// Recusados pela guarda

#[test]
fn it_should_refuse_an_architecture_guideline_resting_on_an_architecture_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![
            guideline(SOURCE, "architecture"),
            guideline(TARGET, "architecture"),
        ],
    );

    let result = appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]);

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnArchitecture, &to_pointer(0)))
    );
}

#[test]
fn it_should_refuse_a_business_guideline_resting_on_an_architecture_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![
            guideline(SOURCE, "business"),
            guideline(TARGET, "architecture"),
        ],
    );

    let result = appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]);

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnArchitecture, &to_pointer(0)))
    );
}

#[test]
fn it_should_refuse_a_guideline_resting_on_a_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![guideline(SOURCE, "business"), premise(PREMISE)],
    );

    let result = appended(&mut omhl, vec![rests_on(SOURCE, PREMISE)]);

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnTargetType, &to_pointer(0)))
    );
}

#[test]
fn it_should_refuse_a_native_guideline_resting_on_an_architecture_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![guideline(TARGET, "architecture")]);

    let result = appended(&mut omhl, vec![rests_on(NATIVE_GUIDELINE, TARGET)]);

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnArchitecture, &to_pointer(0)))
    );
}

#[test]
fn it_should_refuse_a_native_guideline_resting_on_a_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![premise(PREMISE)]);

    let result = appended(&mut omhl, vec![rests_on(NATIVE_GUIDELINE, PREMISE)]);

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnTargetType, &to_pointer(0)))
    );
}

#[test]
fn it_should_refuse_a_guideline_resting_on_an_architecture_guideline_created_in_the_same_batch() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![guideline(SOURCE, "business")]);

    let result = appended(
        &mut omhl,
        vec![guideline(TARGET, "architecture"), rests_on(SOURCE, TARGET)],
    );

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnArchitecture, &to_pointer(1)))
    );
}

// Recusados pelo motor

#[test]
fn it_should_leave_a_relation_placed_before_its_target_in_the_same_batch_to_the_engine() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(&mut omhl, vec![guideline(SOURCE, "business")]);

    let result = appended(
        &mut omhl,
        vec![rests_on(SOURCE, TARGET), guideline(TARGET, "business")],
    );

    assert_eq!(
        engine_refusal(result),
        (ErrorCode::InvalidRecord, "endpoint-not-found".to_owned())
    );
}

#[test]
fn it_should_leave_a_rests_on_cycle_to_the_engine() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        vec![guideline(SOURCE, "business"), guideline(TARGET, "business")],
    );
    appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]).expect("the first relation is accepted");

    let result = appended(&mut omhl, vec![rests_on(TARGET, SOURCE)]);

    assert_eq!(
        engine_refusal(result),
        (ErrorCode::CycleRejected, "cycle".to_owned())
    );
}

// Mapa do lote e revisão de kind

#[test]
fn it_should_accept_a_relation_when_the_target_kind_is_revised_to_business_earlier_in_the_batch() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let seeded = seed(
        &mut omhl,
        vec![
            guideline(SOURCE, "business"),
            guideline(TARGET, "architecture"),
        ],
    );
    let revision = revision_item(
        TARGET,
        "guideline",
        json!({ "text": "keep it simple", "kind": "business" }),
        &seeded[1],
    );

    let result = appended(&mut omhl, vec![revision, rests_on(SOURCE, TARGET)]);

    assert_eq!(result, Ok(2));
}

#[test]
fn it_should_refuse_a_relation_when_the_target_kind_is_revised_to_architecture_earlier_in_the_batch()
 {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let seeded = seed(
        &mut omhl,
        vec![guideline(SOURCE, "business"), guideline(TARGET, "business")],
    );
    let revision = revision_item(
        TARGET,
        "guideline",
        json!({ "text": "keep it simple", "kind": "architecture" }),
        &seeded[1],
    );

    let result = appended(&mut omhl, vec![revision, rests_on(SOURCE, TARGET)]);

    assert_eq!(
        result,
        Err(rule(RuleCode::RestsOnArchitecture, &to_pointer(1)))
    );
}

#[test]
fn it_should_accept_a_kind_revision_of_an_already_targeted_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let seeded = seed(
        &mut omhl,
        vec![guideline(SOURCE, "business"), guideline(TARGET, "business")],
    );
    appended(&mut omhl, vec![rests_on(SOURCE, TARGET)]).expect("the relation is accepted");
    let revision = revision_item(
        TARGET,
        "guideline",
        json!({ "text": "keep it simple", "kind": "architecture" }),
        &seeded[1],
    );

    let result = appended(&mut omhl, vec![revision]);

    assert_eq!(result, Ok(1));
}

// Causa e próximo passo pela camada `app`

fn open_app<'a>(raw: &'a RawHarness, probe: &'a FakeProbe) -> App<'a> {
    let deps = OmhlDeps {
        clock: &raw.clock,
        ids: &raw.ids,
        probe,
    };

    App::open(&raw.db_path(), &acme(), deps, &RetryPolicy::default())
        .unwrap_or_else(|error| panic!("the app opens: {error}"))
}

fn node_wire(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn relation_wire(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": { "kind": "relation", "relation_type": relation_type, "from": from, "to": to } })
}

fn guideline_wire(id: &str, kind: &str) -> Value {
    node_wire(
        id,
        "guideline",
        &json!({ "text": "keep it simple", "kind": kind }),
    )
}

fn append_wire(items: &[Value]) -> Action {
    Action::Append {
        request: json!({ "items": items }),
    }
}

fn dispatch_ok(app: &mut App<'_>, items: &[Value]) {
    app.dispatch(&Caller::loader(), &append_wire(items))
        .unwrap_or_else(|error| panic!("the append is accepted: {error}"));
}

fn dispatch_refused(app: &mut App<'_>, items: &[Value]) -> Value {
    app.dispatch(&Caller::loader(), &append_wire(items))
        .expect_err("the append is refused")
        .to_json()
}

/// Se a recusa traz uma mensagem e um próximo passo não vazios.
fn has_cause_and_next_step(error: &Value) -> (bool, bool) {
    let filled = |value: &Value| value.as_str().is_some_and(|text| !text.is_empty());

    (
        filled(&error["error"]["message"]),
        filled(&error["error"]["details"][0]["next_step"]),
    )
}

#[test]
fn it_should_refuse_a_rests_on_architecture_with_cause_and_next_step() {
    let raw = RawHarness::new();
    let probe = FakeProbe::failing();
    let mut app = open_app(&raw, &probe);
    dispatch_ok(
        &mut app,
        &[
            guideline_wire(SOURCE, "business"),
            guideline_wire(TARGET, "architecture"),
        ],
    );

    let error = dispatch_refused(&mut app, &[relation_wire("rests-on", SOURCE, TARGET)]);

    assert_eq!(error["error"]["code"], json!("rests-on-architecture"));
    assert_eq!(has_cause_and_next_step(&error), (true, true));
}

#[test]
fn it_should_refuse_a_rests_on_cycle_with_cause_and_next_step() {
    let raw = RawHarness::new();
    let probe = FakeProbe::failing();
    let mut app = open_app(&raw, &probe);
    dispatch_ok(
        &mut app,
        &[
            guideline_wire(SOURCE, "business"),
            guideline_wire(TARGET, "business"),
            relation_wire("rests-on", SOURCE, TARGET),
        ],
    );

    let error = dispatch_refused(&mut app, &[relation_wire("rests-on", TARGET, SOURCE)]);

    assert_eq!(error["error"]["code"], json!("CYCLE_REJECTED"));
    assert_eq!(has_cause_and_next_step(&error), (true, true));
}

#[test]
fn it_should_refuse_a_supersedes_cycle_with_cause_and_next_step() {
    let raw = RawHarness::new();
    let probe = FakeProbe::failing();
    let mut app = open_app(&raw, &probe);
    dispatch_ok(
        &mut app,
        &[
            guideline_wire(SOURCE, "business"),
            guideline_wire(TARGET, "business"),
            relation_wire("supersedes", SOURCE, TARGET),
        ],
    );

    let error = dispatch_refused(&mut app, &[relation_wire("supersedes", TARGET, SOURCE)]);

    assert_eq!(has_cause_and_next_step(&error), (true, true));
}

#[test]
fn it_should_refuse_a_rests_on_to_a_missing_node_with_cause_and_next_step() {
    let raw = RawHarness::new();
    let probe = FakeProbe::failing();
    let mut app = open_app(&raw, &probe);
    dispatch_ok(&mut app, &[guideline_wire(SOURCE, "business")]);

    let error = dispatch_refused(&mut app, &[relation_wire("rests-on", SOURCE, TARGET)]);

    assert_eq!(has_cause_and_next_step(&error), (true, true));
}

#[test]
fn it_should_keep_the_engine_refusal_of_an_older_relation_type_without_a_next_step() {
    let raw = RawHarness::new();
    let probe = FakeProbe::failing();
    let mut app = open_app(&raw, &probe);
    let check = json!({ "when": { "vocabulary": "closed", "event": "before-commit" }, "how": "lint it", "blocking": true });
    dispatch_ok(
        &mut app,
        &[
            node_wire("checks.first", "check", &check),
            node_wire("checks.second", "check", &check),
            relation_wire("after", "checks.first", "checks.second"),
        ],
    );

    let error = dispatch_refused(
        &mut app,
        &[relation_wire("after", "checks.second", "checks.first")],
    );

    assert_eq!(error["error"]["details"][0].get("next_step"), None);
}
