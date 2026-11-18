use crate::support;

use std::fs;

use hexlog::{AppendRequest, ErrorCode};
use omhl::error::OmhlError;
use omhl::mechanical::spec::{CheckSpec, SpecError};
use serde_json::{Value, json};
use support::appfx::AppFixture;
use support::omhl::{OmhlHarness, acme, node_item};
use support::{FakeProbe, loader_author, snap};

const SECRET_MARKER: &str = "SECRET-MARKER-9f3";
const INVALID_TOML: &str = "this is = [not valid";
const A_TOML_PATH: &str = ".omhl/checks/a.toml";

// Montagem das extensions

fn check_toml(how: &str, after: &[&str]) -> String {
    let after_line = if after.is_empty() {
        String::new()
    } else {
        let targets: Vec<String> = after.iter().map(|id| format!("\"{id}\"")).collect();
        format!("after = [{}]\n", targets.join(", "))
    };

    format!("blocking = true\nhow = \"{how}\"\n{after_line}\n[when]\nevent = \"before-commit\"\n")
}

fn guideline_toml(text: &str) -> String {
    format!("kind = \"business\"\ntext = \"{text}\"\n")
}

fn check_data(how: &str) -> Value {
    json!({
        "blocking": true,
        "how": how,
        "when": {"vocabulary": "closed", "event": "before-commit"}
    })
}

/// 50 checks `c01..c50`; `c01 after c02` e `c02 after c01` fecham um ciclo no 2º lote.
fn fixture_with_a_cycle() -> AppFixture {
    let fx = AppFixture::new();
    for n in 1..=50 {
        let after: &[&str] = match n {
            1 => &["checks.c02"],
            2 => &["checks.c01"],
            _ => &[],
        };
        fx.ext()
            .check(&format!("c{n:02}"), &check_toml("Run", after));
    }

    fx
}

// Leitura das respostas

fn loaded(fx: &AppFixture, dry_run: bool) -> Value {
    fx.load(dry_run).expect("the load is accepted")
}

fn refused(fx: &AppFixture) -> Value {
    fx.load(false).expect_err("the load is refused")
}

fn ids_of(nodes: &Value) -> Vec<String> {
    nodes
        .as_array()
        .expect("the nodes are an array")
        .iter()
        .map(|node| node["id"].as_str().expect("the node has an id").to_owned())
        .collect()
}

fn details_of(error: &Value) -> &Vec<Value> {
    error["error"]["details"]
        .as_array()
        .expect("the error has details")
}

fn path_and_code(entry: &Value) -> (String, String) {
    (
        entry["path"].as_str().expect("entry has a path").to_owned(),
        entry["code"].as_str().expect("entry has a code").to_owned(),
    )
}

fn detail_keys(error: &Value) -> Vec<(String, String)> {
    details_of(error).iter().map(path_and_code).collect()
}

fn warning_keys(report: &Value) -> Vec<(String, String)> {
    report["warnings"]
        .as_array()
        .expect("the report has warnings")
        .iter()
        .map(path_and_code)
        .collect()
}

fn key(path: &str, code: &str) -> (String, String) {
    (path.to_owned(), code.to_owned())
}

fn numbers_in(text: &str) -> Vec<u32> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn total_records(fx: &AppFixture) -> u64 {
    fx.verify()["total_records"]
        .as_u64()
        .expect("verify counts the records")
}

fn retirement_request(fx: &AppFixture, id: &str) -> Value {
    let base = fx.get(id)["record_id"].clone();

    json!({
        "items": [{
            "body": {"kind": "retirement", "lineage": {"kind": "node", "id": id}},
            "base": base,
        }]
    })
}

fn retire(fx: &AppFixture, id: &str) {
    fx.append_as_user(retirement_request(fx, id))
        .expect("the retirement is accepted");
}

/// A guarda só deixa o client do comando retirar uma diretiva; o arquivo segue na árvore, então o comando recusaria.
fn retire_guideline(fx: &AppFixture, id: &str) {
    fx.append_as_guideline_retirer(retirement_request(fx, id))
        .expect("the retirement is accepted");
}

// Criação e autoria

#[test]
fn it_should_create_checks_and_guidelines() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .guideline("tone", &guideline_toml("Be brief"));

    let report = loaded(&fx, false);

    assert_eq!(report["created"], json!(["checks.a", "guidelines.tone"]));
    assert_eq!(ids_of(&fx.list("check")), vec!["checks.a"]);
    assert_eq!(ids_of(&fx.list("guideline")), vec!["guidelines.tone"]);
    assert_eq!(fx.get("checks.a")["data"], check_data("Run A"));
    assert_eq!(
        fx.get("guidelines.tone")["data"],
        json!({"text": "Be brief", "kind": "business"})
    );
}

#[test]
fn it_should_author_the_nodes_as_the_loader() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .guideline("tone", &guideline_toml("Be brief"));

    loaded(&fx, false);

    let loader = json!({"client": "loader", "agent": "loader"});
    assert_eq!(fx.get("checks.a")["author"], loader);
    assert_eq!(fx.get("guidelines.tone")["author"], loader);
}

// Dry run

#[test]
fn it_should_write_nothing_on_a_dry_run() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .guideline("tone", &guideline_toml("Be brief"));

    let report = loaded(&fx, true);

    assert_eq!(report["dry_run"], json!(true));
    assert_eq!(report["created"], json!(["checks.a", "guidelines.tone"]));
    assert_eq!(total_records(&fx), 0);
}

#[test]
fn it_should_report_on_a_dry_run_what_the_real_run_does() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .check("b", &check_toml("Run B", &["checks.a"]));

    let dry = loaded(&fx, true);
    let real = loaded(&fx, false);

    for field in ["created", "revised", "relations_created"] {
        assert_eq!(dry[field], real[field], "field {field} differs");
    }
}

// Idempotência e revisão

#[test]
fn it_should_report_unchanged_on_a_second_run() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .guideline("tone", &guideline_toml("Be brief"));
    loaded(&fx, false);
    let records_after_first = total_records(&fx);

    let second = loaded(&fx, false);

    assert_eq!(second["created"], json!([]));
    assert_eq!(second["unchanged"], json!(["checks.a", "guidelines.tone"]));
    assert_eq!(total_records(&fx), records_after_first);
}

#[test]
fn it_should_revise_a_node_when_the_file_changes() {
    let fx = AppFixture::new();
    fx.ext().check("a", &check_toml("Run A", &[]));
    loaded(&fx, false);
    fx.ext().check("a", &check_toml("Run A again", &[]));

    let report = loaded(&fx, false);

    assert_eq!(report["revised"], json!(["checks.a"]));
    assert_eq!(fx.get("checks.a")["data"], check_data("Run A again"));
}

#[test]
fn it_should_not_revise_for_a_comment_only_edit() {
    let fx = AppFixture::new();
    let original = check_toml("Run A", &[]);
    fx.ext().check("a", &original);
    loaded(&fx, false);
    let edited = format!(
        "{}\n\n# a comment added later\n",
        original.replace("blocking = true", "blocking    =    true  # still blocking")
    );
    fx.ext().check("a", &edited);

    let report = loaded(&fx, false);

    assert_eq!(report["unchanged"], json!(["checks.a"]));
    assert_eq!(report["revised"], json!([]));
}

#[test]
fn it_should_create_the_after_relation() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .check("b", &check_toml("Run B", &["checks.a"]));

    let report = loaded(&fx, false);

    assert_eq!(
        report["relations_created"],
        json!([{"from": "checks.b", "to": "checks.a"}])
    );
}

// Aposentadas

#[test]
fn it_should_skip_a_retired_check() {
    let fx = AppFixture::new();
    fx.ext().check("a", &check_toml("Run A", &[]));
    loaded(&fx, false);
    retire(&fx, "checks.a");

    let report = loaded(&fx, false);

    assert_eq!(
        report["skipped"],
        json!([{"id": "checks.a", "code": "extension-retired"}])
    );
    assert_eq!(fx.list("check"), json!([]));
}

#[test]
fn it_should_skip_a_retired_guideline() {
    let fx = AppFixture::new();
    fx.ext().guideline("tone", &guideline_toml("Be brief"));
    loaded(&fx, false);
    retire_guideline(&fx, "guidelines.tone");

    let report = loaded(&fx, false);

    assert_eq!(
        report["skipped"],
        json!([{"id": "guidelines.tone", "code": "extension-retired"}])
    );
    assert_eq!(fx.list("guideline"), json!([]));
}

#[test]
fn it_should_warn_and_still_load_the_rest_when_after_points_to_a_retired_check() {
    let fx = AppFixture::new();
    fx.ext().check("a", &check_toml("Run A", &[]));
    loaded(&fx, false);
    retire(&fx, "checks.a");
    fx.ext()
        .check("b", &check_toml("Run B", &["checks.a"]))
        .check("c", &check_toml("Run C", &[]));

    let report = loaded(&fx, false);

    assert_eq!(
        warning_keys(&report),
        vec![key(".omhl/checks/b.toml", "after-target-retired")]
    );
    assert_eq!(report["created"], json!(["checks.b", "checks.c"]));
    assert_eq!(report["relations_created"], json!([]));
}

// Órfãos

#[test]
fn it_should_report_orphans_without_retiring_them() {
    let fx = AppFixture::new();
    fx.ext().check("a", &check_toml("Run A", &[]));
    loaded(&fx, false);
    fs::remove_file(fx.ext().root().join(A_TOML_PATH)).expect("the file is removed");

    let report = loaded(&fx, false);

    assert_eq!(report["orphans"], json!(["checks.a"]));
    assert_eq!(ids_of(&fx.list("check")), vec!["checks.a"]);
}

#[test]
fn it_should_report_orphan_relations_without_retiring_them() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .check("b", &check_toml("Run B", &["checks.a"]));
    loaded(&fx, false);
    let records_before = total_records(&fx);
    fx.ext().check("b", &check_toml("Run B", &[]));

    let report = loaded(&fx, false);

    assert_eq!(
        report["orphan_relations"],
        json!([{"from": "checks.b", "to": "checks.a"}])
    );
    assert_eq!(total_records(&fx), records_before);
}

// Recusas

#[test]
fn it_should_refuse_invalid_extensions_and_write_nothing() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .check("x", INVALID_TOML);

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert_eq!(
        detail_keys(&error),
        vec![key(".omhl/checks/x.toml", "invalid-toml")]
    );
    assert_eq!(fx.list("check"), json!([]));
}

#[test]
fn it_should_refuse_a_guideline_without_kind_and_write_nothing() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .guideline("tone", "text = \"Be brief\"\n");

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert_eq!(
        detail_keys(&error),
        vec![key(".omhl/guidelines/tone.toml", "kind-missing")]
    );
    assert!(
        error["error"]["details"][0]["next_step"]
            .as_str()
            .is_some_and(|step| !step.is_empty()),
        "the detail has no next_step: {error}"
    );
    assert_eq!(fx.list("check"), json!([]));
    assert_eq!(fx.list("guideline"), json!([]));
}

#[test]
fn it_should_refuse_a_guideline_with_an_invalid_kind_and_write_nothing() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .guideline("tone", "kind = \"tecnico\"\ntext = \"Be brief\"\n");

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert_eq!(
        detail_keys(&error),
        vec![key(".omhl/guidelines/tone.toml", "kind-invalid")]
    );
    assert_eq!(fx.list("check"), json!([]));
    assert_eq!(fx.list("guideline"), json!([]));
}

#[test]
fn it_should_not_echo_file_content_in_the_error() {
    let fx = AppFixture::new();
    fx.ext().check(
        "x",
        &format!("blocking = true\nhow = \"Run\"\n{SECRET_MARKER} = 1\n\n[when]\nevent = \"e\"\n"),
    );

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert!(
        !error.to_string().contains(SECRET_MARKER),
        "the error echoes the file content: {error}"
    );
}

#[test]
fn it_should_cap_error_details_at_20() {
    let fx = AppFixture::new();
    for n in 1..=25 {
        fx.ext().check(&format!("bad{n:02}"), INVALID_TOML);
    }

    let error = refused(&fx);

    assert_eq!(details_of(&error).len(), 20);
    let message = error["error"]["message"]
        .as_str()
        .expect("the error has a message");
    assert!(
        numbers_in(message).contains(&5),
        "the message does not say 5 were omitted: {message}"
    );
}

#[test]
fn it_should_refuse_a_native_check_id() {
    let fx = AppFixture::new();
    fx.ext().check("run-tests", &check_toml("Run tests", &[]));

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert_eq!(
        detail_keys(&error),
        vec![key(".omhl/checks/run-tests.toml", "reserved-id")]
    );
}

#[test]
fn it_should_refuse_load_without_a_project_root() {
    let fx = AppFixture::without_root();

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("no-project-root"));
}

// Lotes e carga parcial

#[test]
fn it_should_load_more_than_50_changes_in_batches_and_be_idempotent() {
    let fx = AppFixture::new();
    for n in 1..=51 {
        fx.ext()
            .guideline(&format!("g{n:02}"), &guideline_toml(&format!("Rule {n}")));
    }

    let first = loaded(&fx, false);
    let second = loaded(&fx, false);

    assert_eq!(first["created"].as_array().map(Vec::len), Some(51));
    assert_eq!(second["created"], json!([]));
    assert_eq!(second["unchanged"].as_array().map(Vec::len), Some(51));
    assert_eq!(fx.verify()["ok"], json!(true));
}

#[test]
fn it_should_report_a_partial_load_when_a_later_batch_fails() {
    let fx = fixture_with_a_cycle();

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("CYCLE_REJECTED"));
    let last = details_of(&error).last().expect("the error has details");
    assert_eq!(last["code"], json!("load-partial"));
    assert!(
        last["message"]
            .as_str()
            .expect("the detail has a message")
            .contains("50 of 52"),
        "unexpected message: {last}"
    );
    assert_eq!(fx.list("check").as_array().map(Vec::len), Some(50));

    fx.ext().check("c02", &check_toml("Run", &[]));
    let retried = fx.load(false);
    assert!(
        retried.is_ok(),
        "the load after the fix failed: {retried:?}"
    );
}

#[test]
fn it_should_map_engine_errors_to_the_originating_file() {
    let fx = fixture_with_a_cycle();

    let error = refused(&fx);

    let first = details_of(&error).first().expect("the error has details");
    let path = first["path"].as_str().expect("the detail has a path");
    assert!(
        path == ".omhl/checks/c01.toml" || path == ".omhl/checks/c02.toml",
        "unexpected path: {path}"
    );
}

// Relatório

#[test]
fn it_should_include_the_tenant_and_warnings_in_the_report() {
    let fx = AppFixture::new();
    fx.ext().file(".omhl/checks/.gitkeep", "");

    let report = loaded(&fx, false);

    assert_eq!(report["tenant"], json!("acme"));
    assert_eq!(
        warning_keys(&report),
        vec![key(".omhl/checks/.gitkeep", "hidden-file-ignored")]
    );
}

#[test]
fn it_should_report_a_malformed_after_target_without_aborting_the_load() {
    let fx = AppFixture::new();
    fx.ext()
        .check("a", &check_toml("Run A", &[]))
        .check("b", &check_toml("Run B", &["checks.Bad Name"]));

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert_eq!(
        detail_keys(&error),
        vec![key(".omhl/checks/b.toml", "value-not-allowed")]
    );
}

// Campos mecânicos do check

const REPORT_LINT_TOML: &str = r#"concern = "style"
variant = "lint"
when = { event = "before-commit" }
blocking = true
run = ["cargo", "clippy", "--workspace", "--all-targets", "--locked"]
timeout_secs = 600
expect = [
  { exit = 0, stderr = "(?m)^warning", outcome = "passed_with_warning" },
  { exit = 0, outcome = "passed" },
]
"#;
const REPORT_LINT_MD: &str = "Run clippy on the workspace.\n";
const BAD_REGEX_TOML: &str = r#"blocking = true
how = "Run lint"
run = ["true"]
timeout_secs = 60
expect = [{ exit = 0, stdout = "(unclosed", outcome = "passed" }]
[when]
event = "before-pr"
"#;

type CatalogVerdict = Result<(), Vec<(String, String)>>;
type SpecVerdict = Result<(), SpecError>;

fn report_lint_data() -> Value {
    let mut data = support::omhl::check_data("before-commit", "Run clippy on the workspace.", true);
    data["concern"] = json!("style");
    data["variant"] = json!("lint");
    data["run"] = json!([
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked"
    ]);
    data["timeout_secs"] = json!(600);
    data["expect"] = json!([
        {"exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning"},
        {"exit": 0, "outcome": "passed"},
    ]);

    data
}

fn fx_with_report_lint() -> AppFixture {
    let fx = AppFixture::new();
    fx.ext()
        .check("lint", REPORT_LINT_TOML)
        .markdown("checks", "lint", REPORT_LINT_MD);

    fx
}

fn check_ok() -> Value {
    let mut data = support::omhl::check_data("before-commit", "Run the linter", true);
    data["concern"] = json!("style");
    data["variant"] = json!("lint");
    data["run"] = json!(["cargo", "clippy"]);
    data["timeout_secs"] = json!(120);
    data["expect"] = json!([{"exit": 0, "outcome": "passed"}]);

    data
}

fn check_with(key: &str, value: Value) -> Value {
    let mut data = check_ok();
    data[key] = value;

    data
}

/// Veredito do catálogo: `append` no banco real, como o `loader`; `Err` lista (path, code).
fn catalog_verdict(data: &Value) -> CatalogVerdict {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    let request = AppendRequest {
        tenant: acme(),
        author: loader_author(),
        key: None,
        items: vec![node_item("checks.probe", "check", data.clone())],
    };

    match omhl.append(&request) {
        Ok(_) => Ok(()),
        Err(OmhlError::Hexlog(error)) if error.code == ErrorCode::InvalidRecord => Err(error
            .details
            .iter()
            .map(|detail| (detail.path.clone(), detail.code.clone()))
            .collect()),
        Err(other) => panic!("unexpected refusal: {other:?}"),
    }
}

fn spec_verdict(data: &Value) -> SpecVerdict {
    CheckSpec::from_data(data).map(|_| ())
}

fn verdicts(data: &Value) -> (CatalogVerdict, SpecVerdict) {
    (catalog_verdict(data), spec_verdict(data))
}

fn catalog_refusal(path: &str, code: &str) -> CatalogVerdict {
    Err(vec![key(path, code)])
}

fn spec_refusal(path: &str, code: &'static str) -> SpecVerdict {
    Err(SpecError {
        path: path.to_owned(),
        code,
    })
}

#[test]
fn it_should_load_a_mechanical_check_into_the_database_with_all_its_fields() {
    let fx = fx_with_report_lint();

    let report = loaded(&fx, false);

    assert_eq!(report["created"], json!(["checks.lint"]));
    assert_eq!(fx.get("checks.lint")["data"], report_lint_data());
}

#[test]
fn it_should_report_invalid_extensions_for_a_bad_regex_and_write_nothing() {
    let fx = AppFixture::new();
    let records_before = total_records(&fx);
    fx.ext().check("lint", BAD_REGEX_TOML);

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("extensions-invalid"));
    assert_eq!(
        detail_keys(&error),
        vec![key(".omhl/checks/lint.toml", "regex-invalid")]
    );
    assert_eq!(
        error["error"]["details"][0]["message"],
        json!("regex-invalid at /expect/0/stdout")
    );
    assert_eq!(ids_of(&fx.list("check")), Vec::<String>::new());
    assert_eq!(total_records(&fx), records_before);
    assert!(!error.to_string().contains("unclosed"));
}

#[test]
fn it_should_author_the_loaded_mechanical_check_as_the_loader() {
    let fx = fx_with_report_lint();

    loaded(&fx, false);

    assert_eq!(
        fx.get("checks.lint")["author"],
        json!({"client": "loader", "agent": "loader"})
    );
}

#[test]
fn it_should_agree_with_the_catalog_on_the_number_of_run_items() {
    let at_limit = check_with("run", json!(vec!["x"; 64]));
    let over = check_with("run", json!(vec!["x"; 65]));

    let found = (verdicts(&at_limit), verdicts(&over));

    assert_eq!(
        found,
        (
            (Ok(()), Ok(())),
            (
                catalog_refusal("/items/0/body/data/run", "max-items"),
                spec_refusal("/run", "run-too-many-items")
            )
        )
    );
}

#[test]
fn it_should_agree_with_the_catalog_on_the_length_of_a_run_item() {
    let at_limit = check_with("run", json!(["r".repeat(1000)]));
    let over = check_with("run", json!(["r".repeat(1001)]));

    let found = (verdicts(&at_limit), verdicts(&over));

    assert_eq!(
        found,
        (
            (Ok(()), Ok(())),
            (
                catalog_refusal("/items/0/body/data/run/0", "max-length"),
                spec_refusal("/run/0", "run-item-too-long")
            )
        )
    );
}

#[test]
fn it_should_agree_with_the_catalog_on_the_timeout_range() {
    let mut missing = check_ok();
    missing
        .as_object_mut()
        .expect("the check data is an object")
        .remove("timeout_secs");

    let found = (
        verdicts(&check_with("timeout_secs", json!(1))),
        verdicts(&check_with("timeout_secs", json!(7200))),
        verdicts(&check_with("timeout_secs", json!(0))),
        verdicts(&check_with("timeout_secs", json!(7201))),
        verdicts(&missing),
    );

    assert_eq!(
        found,
        (
            (Ok(()), Ok(())),
            (Ok(()), Ok(())),
            (
                catalog_refusal("/items/0/body/data/timeout_secs", "minimum"),
                spec_refusal("/timeout_secs", "timeout-out-of-range")
            ),
            (
                catalog_refusal("/items/0/body/data/timeout_secs", "maximum"),
                spec_refusal("/timeout_secs", "timeout-out-of-range")
            ),
            (
                catalog_refusal("/items/0/body/data", "any-of"),
                spec_refusal("/timeout_secs", "timeout-missing")
            ),
        )
    );
}

#[test]
fn it_should_agree_with_the_catalog_on_the_number_of_expect_rules() {
    let rule = json!({"exit": 0, "outcome": "passed"});
    let at_limit = check_with("expect", json!(vec![rule.clone(); 16]));
    let over = check_with("expect", json!(vec![rule; 17]));

    let found = (verdicts(&at_limit), verdicts(&over));

    assert_eq!(
        found,
        (
            (Ok(()), Ok(())),
            (
                catalog_refusal("/items/0/body/data/expect", "max-items"),
                spec_refusal("/expect", "expect-too-many-rules")
            )
        )
    );
}
