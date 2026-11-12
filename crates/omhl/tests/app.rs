mod support;

use std::fs;
use std::time::{Duration, Instant};

use hexlog::{RelationQuery, Snapshot, SnapshotKind};
use omhl::app::App;
use omhl::app::action::{Action, Caller};
use omhl::app::error::AppError;
use omhl::app::input::parse_json_text;
use omhl::app::retry::RetryPolicy;
use omhl::app::runtime::Runtime;
use omhl::facade::{Omhl, OmhlDeps};
use omhl::probe::{ProbeOutcome, TreeSource};
use omhl::project::Env;
use serde_json::{Value, json};
use support::busy::{AttemptClock, with_write_lock, with_write_lock_until_attempt};
use support::omhl::{acme, check_data, decision_data, premise_data, work_item_data};
use support::project::Project;
use support::raw::RawHarness;
use support::{DEFAULT_INSTANT, FakeProbe, rty, sid, snap};
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const USE_WAL: &str = "decisions.use-wal";
const TREE_A: &str = "08585692ce06452da6f82ae66b90d98b55536fca";
const TREE_ATTESTATION: &str =
    "attestations.08585692ce06452da6f82ae66b90d98b.55536fca.checks.run-tests.work-items.w1";
const BUSY_MESSAGE: &str =
    "database is busy: another omhl process is writing; run the command again";

// Montagem do ambiente e das ações

struct Fixture {
    raw: RawHarness,
    probe: FakeProbe,
}

impl Fixture {
    fn new(probe: FakeProbe) -> Fixture {
        Fixture {
            raw: RawHarness::new(),
            probe,
        }
    }

    fn clean() -> Fixture {
        Fixture::new(
            FakeProbe::clean(snap('a'))
                .with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree('c')))),
        )
    }

    fn deps(&self) -> OmhlDeps<'_> {
        OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe: &self.probe,
        }
    }

    fn try_open(&self, retry: &RetryPolicy) -> Result<App<'_>, AppError> {
        App::open(&self.raw.db_path(), &acme(), self.deps(), retry)
    }

    fn open_with(&self, retry: &RetryPolicy) -> App<'_> {
        match self.try_open(retry) {
            Ok(app) => app,
            Err(error) => panic!("the app opens: {error}"),
        }
    }

    fn open(&self) -> App<'_> {
        self.open_with(&no_retry())
    }

    fn open_error(&self) -> AppError {
        match self.try_open(&no_retry()) {
            Ok(_) => panic!("the app must refuse to open"),
            Err(error) => error,
        }
    }

    fn omhl(&self) -> Omhl<'_> {
        Omhl::open(&self.raw.config(), self.deps()).expect("omhl opens")
    }
}

fn no_retry() -> RetryPolicy {
    RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: Vec::new(),
    }
}

fn run_as(app: &mut App<'_>, caller: &Caller, action: &Action) -> Value {
    match app.dispatch(caller, action) {
        Ok(value) => value,
        Err(error) => panic!("the action is accepted: {error}"),
    }
}

fn run(app: &mut App<'_>, action: &Action) -> Value {
    run_as(app, &Caller::cli(), action)
}

fn refuse(app: &mut App<'_>, action: &Action) -> AppError {
    match app.dispatch(&Caller::cli(), action) {
        Ok(value) => panic!("the action must be refused, got {value}"),
        Err(error) => error,
    }
}

fn node_wire(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn relation_wire(relation_type: &str, from: &str, to: &str) -> Value {
    json!({
        "body": { "kind": "relation", "relation_type": relation_type, "from": from, "to": to }
    })
}

fn retirement_wire(id: &str, base: &str) -> Value {
    json!({
        "body": { "kind": "retirement", "lineage": { "kind": "node", "id": id } },
        "base": base,
    })
}

fn with_base(mut item: Value, base: &str) -> Value {
    item["base"] = json!(base);

    item
}

fn decision_wire(id: &str) -> Value {
    node_wire(id, "decision", &decision_data("Use WAL", None))
}

fn append_action(items: Vec<Value>) -> Action {
    let mut request = serde_json::Map::new();
    request.insert("items".to_owned(), Value::Array(items));

    Action::Append {
        request: Value::Object(request),
    }
}

fn raw_append_action(request: Value) -> Action {
    Action::Append { request }
}

fn get_action(id: &str) -> Action {
    Action::Get { id: id.to_owned() }
}

fn list_action(type_name: Option<&str>, limit: Option<u32>, after_seq: Option<u64>) -> Action {
    Action::List {
        type_name: type_name.map(str::to_owned),
        id_prefix: None,
        limit,
        after_seq,
    }
}

fn attest_action(check: &str, outcome: &str, note: Option<&str>) -> Action {
    Action::Attest {
        check: check.to_owned(),
        work_item: W1.to_owned(),
        outcome: outcome.to_owned(),
        note: note.map(str::to_owned),
    }
}

fn gate_pr_action() -> Action {
    Action::GatePr {
        work_item: W1.to_owned(),
    }
}

fn close_action() -> Action {
    Action::Close {
        work_item: W1.to_owned(),
    }
}

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn tree_a() -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, TREE_A).expect("valid snapshot")
}

/// Sonda limpa sobre a tree `TREE_A`; `with_head` também responde à fonte `Head`.
fn tree_probe(with_head: bool) -> FakeProbe {
    let probe = FakeProbe::clean(tree_a())
        .with_tree(TreeSource::Index, Ok(ProbeOutcome::Reproducible(tree_a())));
    if with_head {
        return probe.with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree_a())));
    }

    probe
}

/// Pedido cru que grava o atestado de `RUN_TESTS` em `W1` sobre `TREE_A` com o `kind` dado.
fn tree_attestation_action(kind: &str) -> Action {
    raw_append_action(json!({"items": [{
        "body": {
            "kind": "node",
            "id": TREE_ATTESTATION,
            "type": "attestation",
            "data": {
                "check": RUN_TESTS,
                "work_item": W1,
                "outcome": "passed",
                "ok": true,
            },
        },
        "snapshot": { "kind": kind, "id": TREE_A },
    }]}))
}

fn open_work_item(app: &mut App<'_>) {
    let item = node_wire(W1, "work-item", &work_item_data("Ship it"));

    run(app, &append_action(vec![item]));
}

// Leitura das respostas

fn node_of(app: &mut App<'_>, id: &str) -> Value {
    run(app, &get_action(id))["node"].clone()
}

fn record_id_of(response: &Value) -> String {
    response["records"][0]["id"]
        .as_str()
        .expect("the response carries a record id")
        .to_owned()
}

fn item_ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("the page carries items")
        .iter()
        .map(|item| item["id"].as_str().expect("an item has an id").to_owned())
        .collect()
}

fn detail_paths(error: &AppError) -> Vec<String> {
    error.to_json()["error"]["details"]
        .as_array()
        .expect("the error carries details")
        .iter()
        .map(|detail| {
            detail["path"]
                .as_str()
                .expect("a detail has a path")
                .to_owned()
        })
        .collect()
}

fn append_invalid(path: &str, message: &str) -> Value {
    json!({
        "error": {
            "code": "append-invalid",
            "message": message,
            "details": [{ "path": path, "code": "append-invalid", "message": message }],
        }
    })
}

// Entrada

#[test]
fn it_should_refuse_an_input_above_1_mib_before_parsing() {
    let bytes = vec![b' '; 1_048_576 + 1];

    let error = parse_json_text(&bytes).expect_err("the input is refused");

    assert_eq!(error.code(), "input-too-large");
}

#[test]
fn it_should_refuse_json_nested_beyond_the_parser_limit() {
    let bytes = "[".repeat(130);

    let error = parse_json_text(bytes.as_bytes()).expect_err("the input is refused");

    assert_eq!(error.code(), "input-too-deep");
}

#[test]
fn it_should_refuse_input_that_is_not_utf8() {
    let bytes = [0xff, 0xfe];

    let error = parse_json_text(&bytes).expect_err("the input is refused");

    assert_eq!(error.code(), "input-not-utf8");
}

#[test]
fn it_should_report_the_position_of_invalid_json_without_echoing_the_input() {
    let text = r#"{"a": SENTINEL_VALUE}"#;

    let error = parse_json_text(text.as_bytes()).expect_err("the input is refused");

    assert_eq!(error.code(), "input-invalid-json");
    assert!(
        error
            .message()
            .starts_with("input is not valid JSON at line 1 column "),
        "unexpected message: {}",
        error.message()
    );
    assert!(!error.to_json().to_string().contains("SENTINEL_VALUE"));
}

// Conversão do fio do append

#[test]
fn it_should_convert_a_node_item() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let data = decision_data("Use WAL", None);

    let appended = run(
        &mut app,
        &append_action(vec![node_wire(USE_WAL, "decision", &data)]),
    );
    let stored = run(&mut app, &get_action(USE_WAL));

    assert_eq!(appended["records"].as_array().map(Vec::len), Some(1));
    assert_eq!(stored["node"]["data"], data);
}

#[test]
fn it_should_convert_a_relation_item() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let items = vec![
        decision_wire("decisions.a"),
        decision_wire("decisions.b"),
        relation_wire("supports", "decisions.a", "decisions.b"),
    ];
    run(&mut app, &append_action(items));
    let mut omhl = fixture.omhl();
    let query = RelationQuery {
        relation_type: Some(rty("supports")),
        from: None,
        to: None,
        include_retired: false,
    };

    let relations = omhl.relations(&acme(), &query).expect("relations are read");

    let endpoints: Vec<_> = relations.into_iter().map(|r| (r.from, r.to)).collect();
    assert_eq!(endpoints, vec![(sid("decisions.a"), sid("decisions.b"))]);
}

#[test]
fn it_should_convert_a_retirement_item() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let appended = run(&mut app, &append_action(vec![decision_wire(USE_WAL)]));
    let base = record_id_of(&appended);

    run(
        &mut app,
        &append_action(vec![retirement_wire(USE_WAL, &base)]),
    );
    let stored = run(&mut app, &get_action(USE_WAL));

    assert_eq!(stored, json!({ "node": null }));
}

#[test]
fn it_should_convert_a_base_into_a_revision() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let appended = run(&mut app, &append_action(vec![decision_wire(USE_WAL)]));
    let base = record_id_of(&appended);
    let revised = decision_data("Use WAL with checkpoints", None);
    let revision = with_base(node_wire(USE_WAL, "decision", &revised), &base);

    run(&mut app, &append_action(vec![revision]));
    let stored = run(&mut app, &get_action(USE_WAL));

    assert_eq!(stored["node"]["data"], revised);
}

#[test]
fn it_should_refuse_an_unknown_item_key() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let mut item = decision_wire(USE_WAL);
    item["foo"] = json!(1);

    let error = refuse(&mut app, &append_action(vec![item]));

    assert_eq!(
        error.to_json(),
        append_invalid("/items/0/foo", "unknown key")
    );
}

#[test]
fn it_should_refuse_an_unknown_body_kind() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let mut item = decision_wire(USE_WAL);
    item["body"]["kind"] = json!("edge");

    let error = refuse(&mut app, &append_action(vec![item]));

    assert_eq!(
        error.to_json(),
        append_invalid("/items/0/body/kind", "unknown kind")
    );
}

#[test]
fn it_should_refuse_a_request_without_items() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();

    let error = refuse(&mut app, &raw_append_action(json!({})));

    assert_eq!(error.to_json(), append_invalid("/items", "missing key"));
}

#[test]
fn it_should_refuse_a_tenant_key_in_the_request() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let request = json!({ "tenant": "other", "items": [decision_wire(USE_WAL)] });

    let error = refuse(&mut app, &raw_append_action(request));

    assert_eq!(error.to_json(), append_invalid("/tenant", "unknown key"));
}

#[test]
fn it_should_set_the_author_from_the_caller() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    run_as(
        &mut app,
        &Caller::mcp(),
        &append_action(vec![decision_wire(USE_WAL)]),
    );

    let node = node_of(&mut app, USE_WAL);

    assert_eq!(
        node["author"],
        json!({ "agent": "claude", "client": "mcp" })
    );
}

#[test]
fn it_should_replay_the_same_idempotency_key() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let action = raw_append_action(json!({ "key": "k1", "items": [decision_wire(USE_WAL)] }));
    run(&mut app, &action);

    let second = run(&mut app, &action);
    let listed = run(&mut app, &list_action(Some("decision"), None, None));

    assert_eq!(second["replayed"], json!(true));
    assert_eq!(item_ids(&listed), vec![USE_WAL]);
}

// Leitura

#[test]
fn it_should_list_by_type() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let items = vec![
        decision_wire("decisions.a"),
        decision_wire("decisions.b"),
        node_wire(
            "premises.p",
            "premise",
            &premise_data("SQLite is fast", None),
        ),
    ];
    run(&mut app, &append_action(items));

    let listed = run(&mut app, &list_action(Some("decision"), None, None));

    assert_eq!(item_ids(&listed), vec!["decisions.a", "decisions.b"]);
}

#[test]
fn it_should_paginate_a_list() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let items = vec![
        decision_wire("decisions.d1"),
        decision_wire("decisions.d2"),
        decision_wire("decisions.d3"),
    ];
    run(&mut app, &append_action(items));
    let mut pages = Vec::new();
    let mut after = None;

    for _ in 0..5 {
        let page = run(&mut app, &list_action(Some("decision"), Some(1), after));
        pages.push(item_ids(&page));
        after = page["next_after_seq"].as_u64();
        if after.is_none() {
            break;
        }
    }

    assert_eq!(
        pages,
        vec![
            vec!["decisions.d1"],
            vec!["decisions.d2"],
            vec!["decisions.d3"]
        ]
    );
    assert_eq!(after, None);
}

#[test]
fn it_should_get_a_node_by_semantic_id() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let data = decision_data("Use WAL", None);
    let appended = run(
        &mut app,
        &append_action(vec![node_wire(USE_WAL, "decision", &data)]),
    );

    let stored = run(&mut app, &get_action(USE_WAL));

    let expected = json!({
        "id": USE_WAL,
        "type": "decision",
        "record_id": record_id_of(&appended),
        "seq": appended["records"][0]["seq"],
        "at": DEFAULT_INSTANT,
        "data": data,
        "author": { "agent": "user", "client": "cli" },
        "shadowed": false,
    });
    assert_eq!(stored["node"], expected);
}

#[test]
fn it_should_get_null_for_an_unknown_id() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();

    let stored = run(&mut app, &get_action("decisions.missing"));

    assert_eq!(stored, json!({ "node": null }));
}

#[test]
fn it_should_refuse_a_malformed_semantic_id() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();

    let error = refuse(&mut app, &get_action("Bad Id"));

    assert_eq!(error.code(), "INVALID_INPUT");
    assert_eq!(detail_paths(&error), vec!["/id"]);
}

#[test]
fn it_should_refuse_an_id_prefix_longer_than_two_hundred_characters() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let action = Action::List {
        type_name: None,
        id_prefix: Some("a".repeat(201)),
        limit: None,
        after_seq: None,
    };

    let error = refuse(&mut app, &action);

    assert_eq!(
        error.to_json(),
        json!({
            "error": {
                "code": "invalid-arguments",
                "message": "unknown value",
                "details": [{
                    "path": "/id_prefix",
                    "code": "invalid-arguments",
                    "message": "unknown value",
                }],
            }
        })
    );
}

#[test]
fn it_should_verify_a_fresh_chain() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();

    let report = run(&mut app, &Action::Verify);

    assert_eq!(report["ok"], json!(true));
    assert_eq!(report["breaks"], json!([]));
    assert_eq!(report["total_breaks"], json!(0));
}

// Gate, atestado e fechamento

#[test]
fn it_should_evaluate_the_pr_gate_with_the_probe_snapshot() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);

    let report = run(&mut app, &gate_pr_action());

    assert_eq!(report["gate"], json!("pull-request"));
    assert_eq!(report["passed"], json!(false));
    assert!(report["chain_head"]["seq"].is_u64());
    assert!(report["chain_head"]["hash"].is_string());
}

#[test]
fn it_should_refuse_the_pr_gate_when_the_probe_fails() {
    let fixture = Fixture::new(FakeProbe::failing());
    let mut app = fixture.open();
    open_work_item(&mut app);

    let error = refuse(&mut app, &gate_pr_action());

    assert_eq!(error.code(), "probe-failed");
    assert_eq!(
        error.message(),
        "probe-failed at /snapshot: probe unavailable"
    );
}

#[test]
fn it_should_pass_the_pr_gate_when_every_required_check_is_attested() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);
    run(&mut app, &attest_action(RUN_TESTS, "passed", None));
    run(
        &mut app,
        &attest_action(CI_GREEN, "passed_with_warning", None),
    );

    let report = run(&mut app, &gate_pr_action());

    let matched: Vec<&Value> = report["questions"]
        .as_array()
        .expect("the report carries questions")
        .iter()
        .filter(|question| question["kind"] == json!("occurred"))
        .map(|question| &question["matched"])
        .collect();
    assert_eq!(report["passed"], json!(true));
    assert_eq!(matched, vec![&json!(1), &json!(1)]);
}

#[test]
fn it_should_attest_a_check_at_the_probe_snapshot_with_the_derived_id() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);
    let id = format!(
        "attestations.{}.{}.checks.run-tests.work-items.w1",
        "a".repeat(32),
        "a".repeat(8)
    );
    let sha = "a".repeat(40);

    let attested = run(&mut app, &attest_action(RUN_TESTS, "failed", Some("flaky")));
    let node = node_of(&mut app, &id);

    assert_eq!(
        attested["attestation"],
        json!({
            "id": id,
            "check": RUN_TESTS,
            "work_item": W1,
            "outcome": "failed",
            "ok": false,
            "snapshot": { "kind": "git-commit", "id": sha },
            "note": "flaky",
        })
    );
    assert_eq!(
        node["data"],
        json!({
            "check": RUN_TESTS,
            "work_item": W1,
            "outcome": "failed",
            "ok": false,
            "note": "flaky",
        })
    );
    assert_eq!(node["snapshot"], json!({ "kind": "git-commit", "id": sha }));
    assert_eq!(node["author"], json!({ "agent": "user", "client": "cli" }));
}

#[test]
fn it_should_refuse_to_attest_when_the_tree_is_dirty() {
    let fixture = Fixture::new(FakeProbe::dirty());
    let mut app = fixture.open();
    open_work_item(&mut app);

    let error = refuse(&mut app, &attest_action(RUN_TESTS, "passed", None));
    let attestations = run(&mut app, &list_action(Some("attestation"), None, None));

    assert_eq!(error.code(), "snapshot-dirty");
    assert_eq!(detail_paths(&error), vec!["/snapshot"]);
    assert_eq!(attestations["items"], json!([]));
}

#[test]
fn it_should_refuse_to_attest_for_a_closed_work_item() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);
    run(&mut app, &close_action());

    let error = refuse(&mut app, &attest_action(RUN_TESTS, "passed", None));

    assert_eq!(error.code(), "work-item-closed");
    assert_eq!(detail_paths(&error), vec!["/work_item"]);
}

#[test]
fn it_should_revise_with_the_same_outcome_and_refuse_a_different_one() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);
    let first = run(&mut app, &attest_action(RUN_TESTS, "passed", None));

    let second = run(&mut app, &attest_action(RUN_TESTS, "passed", None));
    let error = refuse(&mut app, &attest_action(RUN_TESTS, "failed", None));

    assert_eq!(second["attestation"]["id"], first["attestation"]["id"]);
    assert_eq!(second["replayed"], json!(false));
    assert_eq!(error.code(), "outcome-immutable");
    assert_eq!(detail_paths(&error), vec!["/outcome"]);
}

#[test]
fn it_should_close_a_work_item_even_when_the_gate_fails() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);

    let closed = run(&mut app, &close_action());
    let node = node_of(&mut app, W1);

    assert_eq!(closed["report"]["passed"], json!(false));
    assert_eq!(node["data"]["status"], json!("closed"));
    assert_eq!(node["data"]["gate"]["passed"], json!(false));
}

#[test]
fn it_should_refuse_closing_when_the_tree_is_dirty_in_the_app() {
    let fixture = Fixture::new(FakeProbe::dirty());
    let mut app = fixture.open();
    open_work_item(&mut app);

    let error = refuse(&mut app, &close_action());
    let node = node_of(&mut app, W1);

    assert_eq!(error.code(), "snapshot-dirty");
    assert_eq!(node["data"]["status"], json!("open"));
}

#[test]
fn it_should_append_an_attestation_with_a_git_tree_snapshot_through_the_wire_format() {
    let fixture = Fixture::new(tree_probe(false));
    let mut app = fixture.open();
    open_work_item(&mut app);

    run(&mut app, &tree_attestation_action("git-tree"));
    let node = node_of(&mut app, TREE_ATTESTATION);

    assert_eq!(
        node["snapshot"],
        json!({ "kind": "git-tree", "id": TREE_A })
    );
    assert_eq!(
        node["data"],
        json!({
            "check": RUN_TESTS,
            "work_item": W1,
            "outcome": "passed",
            "ok": true,
        })
    );
}

#[test]
fn it_should_refuse_an_unknown_snapshot_kind_through_the_wire_format() {
    let fixture = Fixture::new(tree_probe(false));
    let mut app = fixture.open();
    open_work_item(&mut app);

    let error = refuse(&mut app, &tree_attestation_action("git-blob"));
    let listed = run(&mut app, &list_action(Some("attestation"), None, None));

    assert_eq!(
        error.to_json(),
        append_invalid("/items/0/snapshot/kind", "unknown kind")
    );
    assert_eq!(listed["items"], json!([]));
}

#[test]
fn it_should_still_refuse_a_git_tree_snapshot_in_the_work_item_gate() {
    let fixture = Fixture::new(tree_probe(true));
    let mut app = fixture.open();
    open_work_item(&mut app);

    let error = refuse(&mut app, &close_action());
    let node = node_of(&mut app, W1);

    assert_eq!(error.code(), "INVALID_RECORD");
    assert_eq!(
        error.to_json()["error"]["details"],
        json!([{
            "code": "const",
            "message": "const",
            "path": "/items/0/body/data/gate/snapshot/kind",
        }])
    );
    assert_eq!(node["data"]["status"], json!("open"));
    assert!(node["data"].get("gate").is_none());
}

// Erros em JSON

#[test]
fn it_should_render_a_rule_error_as_json() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();

    let error = refuse(&mut app, &append_action(vec![decision_wire("foo.bar")]));

    assert_eq!(error.to_json()["error"]["code"], json!("id-namespace"));
    assert_eq!(detail_paths(&error), vec!["/items/0/body/id"]);
}

#[test]
fn it_should_render_an_engine_error_with_its_wire_name_and_details() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let data = json!({
        "when": { "vocabulary": "closed", "event": "before-commit" },
        "blocking": true,
    });
    let item = node_wire("checks.no-how", "check", &data);

    let error = refuse(&mut app, &append_action(vec![item]));

    assert_eq!(error.to_json()["error"]["code"], json!("INVALID_RECORD"));
    assert!(!detail_paths(&error).is_empty());
}

#[test]
fn it_should_replace_the_storage_error_message() {
    let fixture = Fixture::clean();
    fs::write(fixture.raw.db_path(), [b'x'; 100]).expect("the garbage file is written");

    let error = fixture.open_error();

    let dir = fixture.raw.dir.path().display().to_string();
    assert_eq!(error.code(), "STORE_ERROR");
    assert_eq!(error.message(), "storage error; run the command again");
    assert!(!error.to_json().to_string().contains(&dir));
}

// Trava de escrita e tentativas

#[test]
fn it_should_report_busy_after_the_retries_with_the_run_again_message() {
    let fixture = Fixture::clean();
    let retry = RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: vec![Duration::from_millis(100), Duration::from_millis(200)],
    };
    let mut app = fixture.open_with(&retry);

    let (error, elapsed) = with_write_lock(&fixture.raw.config(), &acme(), |_release| {
        let started = Instant::now();
        let error = refuse(&mut app, &append_action(vec![decision_wire(USE_WAL)]));

        (error, started.elapsed())
    });

    assert_eq!(error.code(), "BUSY");
    assert_eq!(error.message(), BUSY_MESSAGE);
    assert!(elapsed >= Duration::from_millis(300), "elapsed {elapsed:?}");
}

#[test]
fn it_should_succeed_when_the_lock_is_released_during_the_retries() {
    let fixture = Fixture::clean();
    let retry = RetryPolicy {
        busy_timeout: Duration::from_millis(50),
        pauses: vec![Duration::from_millis(150), Duration::from_millis(300)],
    };
    {
        // o tenant nasce antes: criá-lo leria o `AttemptClock` sem ninguém para responder
        let _created = fixture.open();
    }
    let (clock, attempts) = AttemptClock::new();
    let deps = OmhlDeps {
        clock: &clock,
        ..fixture.deps()
    };
    let mut app = App::open(&fixture.raw.db_path(), &acme(), deps, &retry)
        .unwrap_or_else(|error| panic!("the app opens: {error}"));

    let result = with_write_lock_until_attempt(&fixture.raw.config(), &acme(), attempts, 2, || {
        app.dispatch(&Caller::cli(), &append_action(vec![decision_wire(USE_WAL)]))
    });

    assert!(result.is_ok());
}

#[test]
fn it_should_read_while_another_connection_holds_the_write_lock() {
    let fixture = Fixture::clean();
    {
        let _created = fixture.open();
    }

    let (opened, listed, got) = with_write_lock(&fixture.raw.config(), &acme(), |_release| {
        match fixture.try_open(&no_retry()) {
            Ok(mut app) => (
                true,
                app.dispatch(&Caller::cli(), &list_action(None, None, None))
                    .is_ok(),
                app.dispatch(&Caller::cli(), &get_action(USE_WAL)).is_ok(),
            ),
            Err(_) => (false, false, false),
        }
    });

    assert_eq!((opened, listed, got), (true, true, true));
}

#[test]
fn it_should_create_the_missing_tenant_on_open() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();

    let listed = run(&mut app, &list_action(None, None, None));
    let report = run(&mut app, &Action::Verify);

    assert_eq!(listed, json!({ "items": [] }));
    assert_eq!(report["ok"], json!(true));
}

// Runtime

#[test]
fn it_should_report_no_marker_when_discovering_outside_a_project() {
    let dir = TempDir::new().expect("temp dir");
    let xdg = TempDir::new().expect("temp dir");
    assert!(
        dir.path()
            .ancestors()
            .all(|parent| !parent.join(".omhl.toml").exists()),
        "an ancestor of the temp dir holds a .omhl.toml marker"
    );
    let env = Env {
        xdg_data_home: Some(xdg.path().as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
    };

    let error = match Runtime::discover(dir.path(), None, &env) {
        Ok(_) => panic!("discovery must fail without a marker"),
        Err(error) => error,
    };

    let created = fs::read_dir(xdg.path()).expect("data dir is read").count();
    assert_eq!(error.code(), "no-marker");
    assert_eq!(created, 0);
}

#[test]
fn it_should_open_the_database_from_the_environment_default() {
    let project = Project::new("acme");
    let env = Env {
        xdg_data_home: Some(project.xdg().as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
    };
    let runtime = match Runtime::discover(project.dir(), None, &env) {
        Ok(runtime) => runtime,
        Err(error) => panic!("discovery works: {error}"),
    };
    let mut app = match runtime.app() {
        Ok(app) => app,
        Err(error) => panic!("the app opens: {error}"),
    };

    let listed = app.dispatch(&Caller::cli(), &list_action(None, None, None));

    assert!(listed.is_ok());
    assert!(project.db_path().exists());
}

// Render

#[test]
fn it_should_render_the_action_with_project_nodes_and_the_native_after_relation() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let lint = node_wire(
        "checks.lint",
        "check",
        &check_data("before-commit", "run lint", true),
    );
    run(&mut app, &append_action(vec![lint]));

    let rendered = run(&mut app, &Action::Render);

    let text = rendered["text"].as_str().expect("the render carries text");
    assert!(text.contains("### checks.lint"));
    assert!(text.contains(
        "### checks.ci-green-before-merge\n- event: before-merge\n- blocking: true\n- after: checks.run-tests\n"
    ));
    assert_eq!(rendered["omitted"], json!(0));
}

#[test]
fn it_should_read_every_page_of_nodes_for_the_render_action() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    let ids: Vec<String> = (1..=60).map(|n| format!("guidelines.g{n:02}")).collect();
    let items: Vec<Value> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let data = json!({ "text": format!("rule {:02}", index + 1) });

            node_wire(id, "guideline", &data)
        })
        .collect();
    for batch in items.chunks(50) {
        run(&mut app, &append_action(batch.to_vec()));
    }

    let rendered = run(&mut app, &Action::Render);

    let text = rendered["text"].as_str().expect("the render carries text");
    let missing: Vec<&String> = ids
        .iter()
        .filter(|id| !text.contains(id.as_str()))
        .collect();
    assert!(missing.is_empty(), "missing from the render: {missing:?}");
    assert_eq!(rendered["omitted"], json!(0));
}

#[test]
fn it_should_never_render_work_items_or_attestations() {
    let fixture = Fixture::clean();
    let mut app = fixture.open();
    open_work_item(&mut app);
    run(&mut app, &attest_action(RUN_TESTS, "passed", None));

    let rendered = run(&mut app, &Action::Render);

    let text = rendered["text"].as_str().expect("the render carries text");
    assert_eq!(
        (text.contains(W1), text.contains("attestations.")),
        (false, false)
    );
}
