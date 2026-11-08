mod support;

use serde_json::{Value, json};
use support::child::omhl;
use support::git::git_in;
use support::mcp_session::McpSession;
use support::omhl::work_item_data;
use support::project::Project;
use tempfile::TempDir;

const TOOLS: [&str; 8] = [
    "list", "get", "append", "attest", "render", "gate_pr", "close", "verify",
];
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
fn it_should_list_exactly_the_eight_tools_in_a_fixed_order() {
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
