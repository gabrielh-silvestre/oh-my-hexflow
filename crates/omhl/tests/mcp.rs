mod support;

use std::fs;

use ::omhl::byproduct::ByproductStore;
use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::ports::blob::BlobStore;
use hexlog::{AppendRequest, Config};
use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::ext::ExtTree;
use support::git::git_in;
use support::mcp_session::McpSession;
use support::omhl::{
    acme, attestation_id_for, divergence_wires, divergence_work_item_wires, node_item,
    with_snapshot, work_item_data,
};
use support::project::Project;
use support::{FakeProbe, FixedClock, SeqIds, executor_attestation_data, executor_author, snap};
use tempfile::TempDir;

const TOOLS: [&str; 11] = [
    "list",
    "get",
    "append",
    "attest",
    "render",
    "gate_pr",
    "close",
    "verify",
    "byproduct",
    "project",
    "report_answers",
];
const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const LINT_RUN_TOML: &str = r#"blocking = true
how = "Run lint"
run = ["cargo", "clippy"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;
const CHECK: &str = "checks.lint";
const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const BSMALL_SHA: &str = "a834c640cc27439d8a70b00733ebf8165a1a9d5aac5ccb6a48cb33dff45c48cd";
const MIB: usize = 1_048_576;

// Apoio

fn modern() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "0" },
    })
}

fn rpc(id: impl Into<Value>, method: &str, params: impl Into<Value>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.into(), "method": method, "params": params.into() })
}

fn ping(id: impl Into<Value>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.into(), "method": "ping" })
}

fn tool_call(id: i64, name: &str, arguments: impl Into<Value>, meta: Option<Value>) -> Value {
    let mut params = json!({ "name": name, "arguments": arguments.into() });
    if let Some(meta) = meta {
        params["_meta"] = meta;
    }

    rpc(json!(id), "tools/call", params)
}

fn serve() -> (Project, McpSession) {
    let project = Project::new("acme");
    let session = McpSession::start(project.dir(), project.xdg());

    (project, session)
}

fn text_of(result: &Value) -> Value {
    let text = result["content"][0]["text"]
        .as_str()
        .expect("the first content block is text");

    serde_json::from_str(text).expect("the text block is JSON")
}

fn commit_marker(project: &Project) -> String {
    let steps: [&[&str]; 5] = [
        &["init", "-b", "main"],
        &["config", "user.name", "Test Author"],
        &["config", "user.email", "author@example.com"],
        &["add", "-A"],
        &["commit", "-m", "add marker"],
    ];
    for args in steps {
        let output = git_in(project.dir(), args);
        assert!(output.status.success(), "git {args:?} failed");
    }
    let head = git_in(project.dir(), &["rev-parse", "HEAD"]);

    String::from_utf8_lossy(&head.stdout).trim().to_owned()
}

fn open_work_item(project: &Project) {
    let request = json!({
        "items": [{ "body": {
            "kind": "node",
            "id": "work-items.w1",
            "type": "work-item",
            "data": work_item_data("t"),
        } }]
    });

    let appended = omhl(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(request.to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

/// Projeto fora de repositório git, com `checks.lint` carregado e os work items `w1` e `w2` abertos.
struct Fixture {
    project: Project,
    clock: FixedClock,
    ids: SeqIds,
}

impl Fixture {
    fn new() -> Fixture {
        let project = Project::new("acme");
        ExtTree::at(project.dir()).check("lint", LINT_RUN_TOML);
        let fixture = Fixture {
            project,
            clock: FixedClock::default(),
            ids: SeqIds::new(),
        };
        let loaded = fixture.cli(&["load"]);
        assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
        let items: Vec<Value> = [W1, W2]
            .iter()
            .map(|id| {
                json!({ "body": {
                    "kind": "node", "id": id, "type": "work-item", "data": work_item_data("t"),
                } })
            })
            .collect();
        let appended = omhl(&["append"])
            .cwd(fixture.project.dir())
            .xdg(fixture.project.xdg())
            .stdin(json!({ "items": items }).to_string())
            .run();
        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);

        fixture
    }

    fn cli(&self, args: &[&str]) -> Output {
        omhl(args)
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .run()
    }

    fn session(&self) -> McpSession {
        McpSession::start(self.project.dir(), self.project.xdg())
    }

    /// Grava o blob `BSMALL` (`one`, `two` no stdout e `three` no stderr) pelo `BlobStore` real.
    fn put_bsmall(&self) -> String {
        let bytes = b"omhl-byproduct v1 stdout=8 stderr=6\none\ntwo\nthree\n";
        let store = BlobStore::new(&self.project.xdg().join("omhl"));

        let hash = store.put(bytes).expect("the blob is stored");

        assert_eq!(hash.as_str(), BSMALL_SHA);
        hash.as_str().to_owned()
    }

    fn remove_blob(&self, sha: &str) {
        let file = self
            .project
            .xdg()
            .join("omhl/blobs")
            .join(&sha[..2])
            .join(sha);
        fs::remove_file(file).expect("the blob file is removed");
    }

    /// Grava a attestation `failed` de `checks.lint` em `snap('a')` para o work item.
    fn attest_failed(&self, work_item: &str, byproduct: &str) {
        let probe = FakeProbe::clean(snap('a'));
        let deps = OmhlDeps {
            clock: &self.clock,
            ids: &self.ids,
            probe: &probe,
        };
        let mut db = Omhl::open(&Config::new(self.project.db_path()), deps).expect("omhl opens");
        db.ensure_tenant(&acme()).expect("the tenant is created");
        let id = attestation_id_for(&snap('a'), CHECK, work_item);
        let data = executor_attestation_data(CHECK, work_item, "failed", byproduct);
        let item = with_snapshot(node_item(&id, "attestation", data), &snap('a'));

        db.append(&AppendRequest {
            tenant: acme(),
            author: executor_author(),
            key: None,
            items: vec![item],
        })
        .expect("the attestation is appended");
    }
}

fn json_line(text: &str) -> Value {
    assert_eq!(text.lines().count(), 1, "expected one line, got: {text:?}");

    serde_json::from_str(text).expect("the line is JSON")
}

fn tool_names(listed: &Value) -> Vec<&str> {
    listed["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("the name is text"))
        .collect()
}

// Ciclo de vida e descoberta

#[test]
fn it_should_answer_server_discover_with_the_modern_result() {
    let (_project, session) = serve();

    session.send(&rpc(
        json!(1),
        "server/discover",
        json!({ "_meta": modern() }),
    ));

    let response = session.recv();
    assert_eq!(
        response["result"],
        json!({
            "resultType": "complete",
            "supportedVersions": ["2026-07-28"],
            "capabilities": { "tools": {} },
            "_meta": {
                "io.modelcontextprotocol/serverInfo": {
                    "name": "omhl",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            },
            "ttlMs": 60000,
            "cacheScope": "private",
        })
    );
}

#[test]
fn it_should_answer_initialize_with_the_legacy_version_echo() {
    let (_project, session) = serve();
    let cases = [
        ("2025-11-25", "2025-11-25"),
        ("2025-06-18", "2025-06-18"),
        ("1999-01-01", "2025-11-25"),
    ];

    let mut answers = Vec::new();
    for (index, (requested, _)) in cases.iter().enumerate() {
        let params = json!({
            "protocolVersion": requested,
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" },
        });
        session.send(&rpc(json!(index), "initialize", params));
        answers.push(session.recv());
    }

    for (answer, (_, expected)) in answers.iter().zip(cases) {
        assert_eq!(answer["result"]["protocolVersion"], json!(expected));
        assert_eq!(answer["result"]["capabilities"], json!({ "tools": {} }));
        assert_eq!(answer["result"]["serverInfo"]["name"], json!("omhl"));
    }
}

#[test]
fn it_should_answer_ping_with_an_empty_result() {
    let (_project, session) = serve();

    session.send(&ping(json!(1)));

    assert_eq!(session.recv()["result"], json!({}));
}

// Ferramentas

#[test]
fn it_should_list_exactly_the_eleven_tools_in_a_fixed_order() {
    let (_project, session) = serve();

    session.send(&rpc(json!(1), "tools/list", json!({})));

    let response = session.recv();
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools is an array");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("the name is text"))
        .collect();
    assert_eq!(names, TOOLS);
    for tool in tools {
        let name = tool["name"].as_str().expect("the name is text");
        assert_eq!(
            tool["inputSchema"]["type"],
            json!("object"),
            "{name} schema"
        );
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{name} has a description"
        );
        assert!(name.len() <= 128, "{name} is at most 128 characters");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')),
            "{name} uses only the allowed characters"
        );
    }
}

#[test]
fn it_should_add_result_type_and_cache_fields_only_in_the_modern_era() {
    let (_project, session) = serve();

    session.send(&rpc(json!(1), "tools/list", json!({ "_meta": modern() })));
    let modern_response = session.recv();
    session.send(&rpc(json!(2), "tools/list", json!({})));
    let legacy_response = session.recv();

    let modern_result = &modern_response["result"];
    assert_eq!(modern_result["resultType"], json!("complete"));
    assert_eq!(modern_result["ttlMs"], json!(60000));
    assert_eq!(modern_result["cacheScope"], json!("private"));
    let legacy_result = legacy_response["result"]
        .as_object()
        .expect("the legacy result is an object");
    for key in ["resultType", "ttlMs", "cacheScope"] {
        assert!(
            !legacy_result.contains_key(key),
            "legacy result has no {key}"
        );
    }
}

#[test]
fn it_should_add_result_type_to_a_modern_tool_call_only() {
    let (_project, session) = serve();

    session.send(&tool_call(1, "list", json!({}), Some(modern())));
    let modern_response = session.recv();
    session.send(&tool_call(2, "list", json!({}), None));
    let legacy_response = session.recv();

    assert_eq!(modern_response["result"]["resultType"], json!("complete"));
    assert!(legacy_response["result"].get("resultType").is_none());
}

#[test]
fn it_should_return_structured_content_equal_to_the_text_block() {
    let (_project, session) = serve();

    session.send(&tool_call(1, "list", json!({}), None));

    let result = &session.recv()["result"];
    let text = result["content"][0]["text"]
        .as_str()
        .expect("the first content block is text");
    assert_eq!(result["content"][0]["type"], json!("text"));
    assert_eq!(result["structuredContent"], text_of(result));
    assert_eq!(
        serde_json::to_string(&text_of(result)).expect("the value serializes"),
        text,
        "the text block is compact JSON"
    );
}

#[test]
fn it_should_return_a_domain_error_as_is_error() {
    let (_project, session) = serve();

    session.send(&tool_call(1, "get", json!({ "id": "Bad Id" }), None));

    let result = &session.recv()["result"];
    assert_eq!(result["isError"], json!(true));
    assert_eq!(text_of(result)["error"]["code"], json!("INVALID_INPUT"));
    assert_eq!(result["structuredContent"], text_of(result));
}

#[test]
fn it_should_return_a_missing_argument_as_is_error() {
    let (_project, session) = serve();

    session.send(&tool_call(1, "get", json!({}), None));

    let response = session.recv();
    assert!(response.get("error").is_none(), "not a protocol error");
    let result = &response["result"];
    assert_eq!(result["isError"], json!(true));
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        json!("invalid-arguments")
    );
    assert_eq!(
        result["structuredContent"]["error"]["details"][0]["path"],
        json!("/id")
    );
}

#[test]
fn it_should_attest_through_a_tool_call() {
    let project = Project::new("acme");
    let head = commit_marker(&project);
    open_work_item(&project);
    let session = McpSession::start(project.dir(), project.xdg());

    session.send(&tool_call(
        1,
        "attest",
        json!({
            "check": "checks.run-tests",
            "work_item": "work-items.w1",
            "outcome": "passed",
        }),
        None,
    ));

    let result = &session.recv()["result"];
    assert!(result.get("isError").is_none(), "the call succeeds");
    let attestation = &result["structuredContent"]["attestation"];
    assert_eq!(attestation["outcome"], json!("passed"));
    assert_eq!(attestation["ok"], json!(true));
    assert_eq!(attestation["snapshot"]["id"], json!(head));
    assert_eq!(text_of(result), result["structuredContent"]);
}

#[test]
fn it_should_report_no_marker_as_is_error_without_dying() {
    let dir = TempDir::new().expect("project directory is created");
    let xdg = TempDir::new().expect("data directory is created");
    let session = McpSession::start(dir.path(), xdg.path());

    session.send(&tool_call(1, "list", json!({}), None));
    let failed = session.recv();
    session.send(&ping(json!(2)));
    let pinged = session.recv();

    assert_eq!(failed["result"]["isError"], json!(true));
    assert_eq!(
        failed["result"]["structuredContent"]["error"]["code"],
        json!("no-marker")
    );
    assert_eq!(pinged["id"], json!(2));
    assert_eq!(pinged["result"], json!({}));
}

// project

#[test]
fn it_should_call_project_through_mcp() {
    let project = Project::new("acme");
    open_work_item(&project);
    let session = McpSession::start(project.dir(), project.xdg());

    session.send(&tool_call(1, "project", json!({}), None));

    let result = &session.recv()["result"];
    assert!(result.get("isError").is_none(), "the call succeeds");
    assert_eq!(text_of(result)["written"], json!(["work-items.w1.md"]));
}

// byproduct e a superfície de ferramentas

#[test]
fn it_should_expose_byproduct_through_the_mcp_tool_with_the_same_json_as_the_cli() {
    let fixture = Fixture::new();
    let blob = fixture.put_bsmall();
    fixture.attest_failed(W1, &blob);
    let session = fixture.session();
    let expected = json!({
        "check": CHECK,
        "work_item": W1,
        "outcome": "failed",
        "byproduct": BSMALL_SHA,
        "lines": ["one", "two", "three"],
    });

    let cli = fixture.cli(&["byproduct", CHECK, W1]);
    session.send(&tool_call(
        1,
        "byproduct",
        json!({ "check": CHECK, "work_item": W1 }),
        None,
    ));
    let result = session.recv()["result"].clone();
    session.send(&rpc(json!(2), "tools/list", json!({})));
    let listed = session.recv();

    assert_eq!(cli.code, Some(0), "stderr: {}", cli.stderr);
    assert!(result.get("isError").is_none(), "the call succeeds");
    assert_eq!(result["structuredContent"], expected);
    assert_eq!(text_of(&result), expected);
    assert_eq!(json_line(&cli.stdout), expected);
    let tool = listed["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .find(|tool| tool["name"] == json!("byproduct"))
        .expect("the byproduct tool is listed");
    assert_eq!(
        tool["inputSchema"]["properties"]["tail"]["type"],
        json!("integer")
    );
    assert_eq!(
        tool["inputSchema"]["required"],
        json!(["check", "work_item"])
    );
}

#[test]
fn it_should_not_expose_check_run_through_the_mcp_tools() {
    let (_project, session) = serve();
    let calls = ["check_run", "check-run", "check_sweep", "check-sweep"];

    session.send(&rpc(json!(1), "tools/list", json!({})));
    let listed = session.recv();
    let mut answers = Vec::new();
    for (index, name) in (2..).zip(calls) {
        session.send(&tool_call(index, name, json!({}), None));
        answers.push(session.recv());
    }

    assert_eq!(tool_names(&listed), TOOLS);
    for (name, answer) in calls.iter().zip(&answers) {
        assert_eq!(answer["error"]["code"], json!(-32602), "{name}");
        assert!(answer.get("result").is_none(), "{name} has no result");
    }
}

#[test]
fn it_should_not_expose_hooks_install_through_the_mcp_tools() {
    let (_project, session) = serve();
    let calls = [
        "hooks_install",
        "hooks-install",
        "hooks_post_commit",
        "hooks-post-commit",
    ];

    session.send(&rpc(json!(1), "tools/list", json!({})));
    let listed = session.recv();
    let mut answers = Vec::new();
    for (index, name) in (2..).zip(calls) {
        session.send(&tool_call(index, name, json!({}), None));
        answers.push(session.recv());
    }

    let names = tool_names(&listed);
    assert!(
        names.iter().all(|name| !name.starts_with("hooks")),
        "no hooks tool is listed: {names:?}"
    );
    for (name, answer) in calls.iter().zip(&answers) {
        assert_eq!(answer["error"]["code"], json!(-32602), "{name}");
    }
}

#[test]
fn it_should_report_the_same_error_codes_in_the_cli_and_in_the_mcp() {
    let fixture = Fixture::new();
    let blob = fixture.put_bsmall();
    fixture.attest_failed(W2, &blob);
    fixture.remove_blob(&blob);
    let session = fixture.session();
    let cases = [
        (
            "check-not-found",
            vec!["byproduct", "checks.nope", W1],
            json!({ "check": "checks.nope", "work_item": W1 }),
        ),
        (
            "attestation-not-found",
            vec!["byproduct", CHECK, W1],
            json!({ "check": CHECK, "work_item": W1 }),
        ),
        (
            "byproduct-missing",
            vec!["byproduct", CHECK, W2],
            json!({ "check": CHECK, "work_item": W2 }),
        ),
        (
            "invalid-arguments",
            vec!["byproduct", CHECK, W2, "--tail", "0"],
            json!({ "check": CHECK, "work_item": W2, "tail": 0 }),
        ),
    ];

    for (index, (code, cli_args, mcp_args)) in (1..).zip(cases) {
        let cli = fixture.cli(&cli_args);
        session.send(&tool_call(index, "byproduct", mcp_args, None));
        let result = session.recv()["result"].clone();

        assert_eq!(cli.code, Some(1), "{code}: stdout {}", cli.stdout);
        assert_eq!(cli.stdout, "", "{code}");
        assert_eq!(result["isError"], json!(true), "{code}");
        assert_eq!(
            result["structuredContent"],
            json_line(&cli.stderr),
            "{code}"
        );
        assert_eq!(result["structuredContent"]["error"]["code"], json!(code));
    }
}

#[test]
fn it_should_not_expose_load_through_the_mcp_tools() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    let session = McpSession::start(project.dir(), project.xdg());

    session.send(&rpc(json!(1), "tools/list", json!({})));
    let listed = session.recv();
    session.send(&tool_call(2, "load", json!({}), None));
    let plain = session.recv();
    session.send(&tool_call(3, "load", json!({ "dry_run": true }), None));
    let dry = session.recv();
    let items = omhl(&["list", "--type", "check"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .run();

    let names = tool_names(&listed);
    assert!(!names.contains(&"load"), "load is not listed: {names:?}");
    assert_eq!(plain["error"]["code"], json!(-32602));
    assert_eq!(dry["error"]["code"], json!(-32602));
    assert_eq!(items.code, Some(0), "stderr: {}", items.stderr);
    assert!(
        !items.stdout.contains(CHECK),
        "nothing was loaded: {}",
        items.stdout
    );
}

// list com where

/// Projeto com os work items `w1` e `w2` e as três divergências de `DIVERGENCE_ROWS`, já com a
/// sessão MCP aberta.
fn session_with_divergences() -> (Project, McpSession) {
    let project = Project::new("acme");
    for items in [divergence_work_item_wires(), divergence_wires()] {
        let appended = omhl(&["append"])
            .cwd(project.dir())
            .xdg(project.xdg())
            .stdin(json!({ "items": items }).to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }
    let session = McpSession::start(project.dir(), project.xdg());

    (project, session)
}

fn list_divergences(session: &McpSession, filter: &Value) -> Value {
    let arguments = json!({ "type": "divergence", "where": filter });
    session.send(&tool_call(1, "list", arguments, None));

    session.recv()["result"].clone()
}

fn divergence_ids_where(session: &McpSession, filter: &Value) -> Vec<String> {
    let result = list_divergences(session, filter);

    assert_ne!(result["isError"], json!(true), "result: {result}");
    result["structuredContent"]["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|item| item["id"].as_str().expect("the id is text").to_owned())
        .collect()
}

fn where_error(session: &McpSession, filter: &Value) -> Value {
    let result = list_divergences(session, filter);

    assert_eq!(result["isError"], json!(true), "result: {result}");

    result["structuredContent"]["error"].clone()
}

fn where_error_code(session: &McpSession, filter: &Value) -> Value {
    where_error(session, filter)["code"].clone()
}

fn eight_pairs() -> serde_json::Map<String, Value> {
    (1..=8).map(|n| (format!("f{n}"), json!("v"))).collect()
}

#[test]
fn it_should_declare_where_as_an_object_of_text_in_the_list_tool_schema() {
    let (_project, session) = serve();

    session.send(&rpc(json!(1), "tools/list", json!({})));

    let response = session.recv();
    let list = response["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .find(|tool| tool["name"] == json!("list"))
        .expect("the list tool is listed");
    assert_eq!(
        list["inputSchema"]["properties"]["where"]["type"],
        json!("object")
    );
    assert_eq!(
        list["inputSchema"]["properties"]["where"]["additionalProperties"]["type"],
        json!("string")
    );
}

#[test]
fn it_should_list_the_divergences_of_a_work_item_with_where() {
    let (_project, session) = session_with_divergences();

    let ids = divergence_ids_where(&session, &json!({ "work_item": "work-items.w2" }));

    assert_eq!(ids, ["divergences.d2", "divergences.d3"]);
}

#[test]
fn it_should_list_the_divergences_of_a_kind_with_where() {
    let (_project, session) = session_with_divergences();

    let ids = divergence_ids_where(&session, &json!({ "kind": "missing" }));

    assert_eq!(ids, ["divergences.d1", "divergences.d2"]);
}

#[test]
fn it_should_list_the_divergences_of_a_resolution_with_where() {
    let (_project, session) = session_with_divergences();

    let ids = divergence_ids_where(&session, &json!({ "resolution": "open" }));

    assert_eq!(ids, ["divergences.d1", "divergences.d3"]);
}

#[test]
fn it_should_combine_every_where_field_into_one_match() {
    let (_project, session) = session_with_divergences();
    let filter = json!({
        "work_item": "work-items.w2",
        "kind": "missing",
        "resolution": "code-fixed",
    });

    let ids = divergence_ids_where(&session, &filter);

    assert_eq!(ids, ["divergences.d2"]);
}

#[test]
fn it_should_return_nothing_when_no_divergence_matches_every_where_field() {
    let (_project, session) = session_with_divergences();
    let filter = json!({
        "work_item": "work-items.w2",
        "kind": "missing",
        "resolution": "open",
    });

    let ids = divergence_ids_where(&session, &filter);

    assert!(
        ids.is_empty(),
        "each field matches a different row: {ids:?}"
    );
}

#[test]
fn it_should_accept_eight_where_fields() {
    let (_project, session) = session_with_divergences();

    let ids = divergence_ids_where(&session, &Value::Object(eight_pairs()));

    assert!(ids.is_empty(), "no divergence has those fields: {ids:?}");
}

#[test]
fn it_should_refuse_more_than_eight_where_fields() {
    let (_project, session) = session_with_divergences();
    let mut pairs = eight_pairs();
    pairs.insert("f9".to_owned(), json!("v"));

    let code = where_error_code(&session, &Value::Object(pairs));

    assert_eq!(code, json!("where-eq-too-many"));
}

#[test]
fn it_should_accept_a_where_value_of_256_characters() {
    let (_project, session) = session_with_divergences();

    let ids = divergence_ids_where(&session, &json!({ "text": "x".repeat(256) }));

    assert!(ids.is_empty(), "no divergence has that text: {ids:?}");
}

#[test]
fn it_should_refuse_a_where_value_longer_than_256_characters() {
    let (_project, session) = session_with_divergences();

    let code = where_error_code(&session, &json!({ "text": "x".repeat(257) }));

    assert_eq!(code, json!("where-eq-value-too-long"));
}

#[test]
fn it_should_refuse_a_where_argument_that_is_not_an_object() {
    let (_project, session) = session_with_divergences();

    let error = where_error(&session, &json!("kind=missing"));

    assert_eq!(
        error["details"],
        json!([{ "path": "/where", "code": "invalid-arguments", "message": "wrong type" }])
    );
}

#[test]
fn it_should_refuse_a_where_value_that_is_not_text() {
    let (_project, session) = session_with_divergences();

    let error = where_error(&session, &json!({ "kind": 1 }));

    assert_eq!(
        error["details"],
        json!([{ "path": "/where/kind", "code": "invalid-arguments", "message": "wrong type" }])
    );
}

// Erros de protocolo

#[test]
fn it_should_return_invalid_params_for_an_unknown_tool() {
    let (_project, session) = serve();

    session.send(&tool_call(1, "nope", json!({}), None));

    assert_eq!(session.recv()["error"]["code"], json!(-32602));
}

#[test]
fn it_should_return_method_not_found() {
    let (_project, session) = serve();

    session.send(&rpc(json!(1), "foo/bar", json!({})));

    assert_eq!(session.recv()["error"]["code"], json!(-32601));
}

#[test]
fn it_should_answer_invalid_json_with_parse_error_and_keep_serving() {
    let (_project, session) = serve();

    session.send_raw(b"{\n");
    let parse_error = session.recv();
    session.send(&ping(json!(1)));
    let pinged = session.recv();

    assert_eq!(parse_error["error"]["code"], json!(-32700));
    assert_eq!(parse_error["id"], Value::Null);
    assert_eq!(pinged["result"], json!({}));
}

#[test]
fn it_should_answer_a_batch_array_with_invalid_request() {
    let (_project, session) = serve();

    session.send(&json!([{ "jsonrpc": "2.0", "id": 1, "method": "ping" }]));

    let response = session.recv();
    assert_eq!(response["error"]["code"], json!(-32600));
    assert_eq!(response["id"], Value::Null);
}

#[test]
fn it_should_reject_an_unsupported_modern_version() {
    let (_project, session) = serve();
    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "1900-01-01" });

    session.send(&rpc(json!(1), "tools/list", json!({ "_meta": meta })));

    let error = &session.recv()["error"];
    assert_eq!(error["code"], json!(-32022));
    assert_eq!(error["data"]["supported"], json!(["2026-07-28"]));
    assert_eq!(error["data"]["requested"], json!("1900-01-01"));
}

#[test]
fn it_should_never_answer_a_message_without_an_id() {
    let (_project, session) = serve();
    let silent = [
        json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "method": "foo/bar" }),
    ];

    for message in &silent {
        session.send(message);
    }
    session.send(&ping(json!(7)));

    assert_eq!(session.recv()["id"], json!(7));
}

#[test]
fn it_should_echo_numeric_zero_and_string_ids_unchanged() {
    let (_project, session) = serve();

    session.send(&ping(json!(0)));
    let numeric = session.recv();
    session.send(&ping(json!("a")));
    let textual = session.recv();

    assert_eq!(numeric["id"], json!(0));
    assert_eq!(textual["id"], json!("a"));
}

// Robustez do laço

#[test]
fn it_should_answer_invalid_utf8_with_parse_error_and_keep_serving() {
    let (_project, session) = serve();

    session.send_raw(&[0xff, 0xfe, b'\n']);
    let parse_error = session.recv();
    session.send(&ping(json!(1)));
    let pinged = session.recv();

    assert_eq!(parse_error["error"]["code"], json!(-32700));
    assert_eq!(parse_error["id"], Value::Null);
    assert_eq!(pinged["result"], json!({}));
}

#[test]
fn it_should_discard_a_line_above_1_mib_and_keep_serving() {
    let (_project, session) = serve();
    let prefix = br#"{"jsonrpc":"2.0","id":1,"method":"ping","x":""#;
    let suffix = br#""}"#;
    let mut line = prefix.to_vec();
    line.resize(MIB + 1 - suffix.len(), b'a');
    line.extend_from_slice(suffix);
    assert_eq!(line.len(), MIB + 1, "the line is one byte above the limit");
    line.push(b'\n');

    session.send_raw(&line);
    let rejected = session.recv();
    session.send(&ping(json!(2)));
    let pinged = session.recv();

    assert_eq!(rejected["error"]["code"], json!(-32600));
    assert_eq!(rejected["id"], Value::Null);
    assert_eq!(pinged["id"], json!(2));
}

#[test]
fn it_should_exit_zero_on_stdin_eof() {
    let (_project, session) = serve();
    session.send(&ping(json!(1)));
    session.recv();

    let code = session.finish();

    assert_eq!(code, Some(0));
}
