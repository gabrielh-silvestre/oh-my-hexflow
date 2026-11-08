mod support;

use std::fs;

use serde_json::{Value, json};
use support::appfx::AppFixture;
use support::omhl::{premise_data, pull_request_data, work_item_data};

const HEADER: &str = "<!-- omhl: generated file, do not edit -->";
const PROJECTION: &str = ".omhl/projection";
const WORK_ITEM_A: &str = "work-items.a";
const WORK_ITEM_B: &str = "work-items.b";
const FILE_A: &str = "work-items.a.md";
const FILE_B: &str = "work-items.b.md";

// Montagem do estado

fn node_request(id: &str, type_name: &str, data: &Value) -> Value {
    json!({
        "items": [{ "body": { "kind": "node", "id": id, "type": type_name, "data": data } }]
    })
}

fn add(fx: &AppFixture, id: &str, type_name: &str, data: &Value) {
    fx.append_as_user(node_request(id, type_name, data))
        .expect("the append is accepted");
}

fn add_work_item(fx: &AppFixture, id: &str, title: &str) {
    add(fx, id, "work-item", &work_item_data(title));
}

fn add_premise(fx: &AppFixture, id: &str, text: &str, owner: &str) {
    add(fx, id, "premise", &premise_data(text, Some(owner)));
}

fn retire(fx: &AppFixture, id: &str) {
    let base = fx.get(id)["record_id"].clone();
    let request = json!({
        "items": [{
            "body": {"kind": "retirement", "lineage": {"kind": "node", "id": id}},
            "base": base,
        }]
    });

    fx.append_as_user(request)
        .expect("the retirement is accepted");
}

fn fixture_with_two_work_items() -> AppFixture {
    let fx = AppFixture::new();
    add_work_item(&fx, WORK_ITEM_A, "First");
    add_work_item(&fx, WORK_ITEM_B, "Second");

    fx
}

// Leitura das respostas e do disco

fn projected(fx: &AppFixture) -> Value {
    fx.project().expect("the projection is accepted")
}

fn refused(fx: &AppFixture) -> Value {
    fx.project().expect_err("the projection is refused")
}

fn projected_file(fx: &AppFixture, name: &str) -> String {
    let bytes = fx.ext().read(&format!("{PROJECTION}/{name}"));

    String::from_utf8(bytes).expect("the projected file is UTF-8")
}

fn first_detail_code(error: &Value) -> &str {
    error["error"]["details"][0]["code"]
        .as_str()
        .expect("the error has a detail code")
}

#[test]
fn it_should_write_one_file_per_work_item() {
    let fx = fixture_with_two_work_items();

    let report = projected(&fx);

    assert_eq!(report["written"], json!([FILE_A, FILE_B]));
    assert!(projected_file(&fx, FILE_A).starts_with(HEADER));
    assert!(projected_file(&fx, FILE_B).starts_with(HEADER));
}

#[test]
fn it_should_report_unchanged_on_a_second_run() {
    let fx = fixture_with_two_work_items();
    projected(&fx);

    let report = projected(&fx);

    assert_eq!(report["unchanged"], json!([FILE_A, FILE_B]));
    assert_eq!(report["written"], json!([]));
}

#[test]
fn it_should_return_the_documented_json_shape() {
    let fx = AppFixture::new();
    add_work_item(&fx, WORK_ITEM_A, "First");

    let report = projected(&fx);

    let mut keys: Vec<&str> = report
        .as_object()
        .expect("the report is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["dir", "removed", "unchanged", "written"]);
    assert_eq!(report["dir"], json!(".omhl/projection"));
}

#[test]
fn it_should_project_pull_requests_and_work_items() {
    let fx = AppFixture::new();
    add_work_item(&fx, WORK_ITEM_A, "First");
    add(
        &fx,
        WORK_ITEM_B,
        "pull-request",
        &pull_request_data("Second"),
    );

    let report = projected(&fx);

    assert_eq!(report["written"], json!([FILE_A, FILE_B]));
    assert!(projected_file(&fx, FILE_A).contains("\n- type: work-item\n"));
    assert!(projected_file(&fx, FILE_B).contains("\n- type: pull-request\n"));
}

#[test]
fn it_should_remove_a_file_when_its_work_item_is_retired() {
    let fx = fixture_with_two_work_items();
    projected(&fx);
    retire(&fx, WORK_ITEM_A);

    let report = projected(&fx);

    assert_eq!(report["removed"], json!([FILE_A]));
    assert_eq!(report["unchanged"], json!([FILE_B]));
}

#[test]
fn it_should_create_only_the_directory_and_gitignore_for_an_empty_store() {
    let fx = AppFixture::new();

    let report = projected(&fx);

    assert_eq!(report["written"], json!([]));
    assert_eq!(
        projected_file(&fx, ".gitignore"),
        "# omhl: generated file, do not edit\n*\n"
    );
}

#[test]
fn it_should_refuse_a_project_without_a_root() {
    let fx = AppFixture::without_root();

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("no-project-root"));
}

#[test]
fn it_should_surface_a_symlinked_directory_as_projection_failed() {
    let fx = AppFixture::new();
    fx.ext()
        .dir("elsewhere")
        .symlink(PROJECTION, "../elsewhere");

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("projection-failed"));
    assert_eq!(first_detail_code(&error), "symlink");
}

#[test]
fn it_should_surface_projection_busy() {
    let fx = AppFixture::new();
    fx.ext().dir(PROJECTION);
    let held = fs::File::create(fx.ext().root().join(PROJECTION).join(".lock"))
        .expect("the lock file opens");
    held.try_lock().expect("the test takes the lock");

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("projection-failed"));
    assert_eq!(first_detail_code(&error), "projection-busy");
}

#[test]
fn it_should_surface_not_generated() {
    let fx = AppFixture::new();
    add_work_item(&fx, WORK_ITEM_A, "First");
    fx.ext()
        .file(&format!("{PROJECTION}/{FILE_A}"), "written by hand\n");

    let error = refused(&fx);

    assert_eq!(error["error"]["code"], json!("projection-failed"));
    assert_eq!(first_detail_code(&error), "not-generated");
    assert_eq!(projected_file(&fx, FILE_A), "written by hand\n");
}

#[test]
fn it_should_produce_the_same_bytes_whatever_the_insertion_order() {
    let forward = AppFixture::new();
    add_work_item(&forward, WORK_ITEM_A, "First");
    add_work_item(&forward, WORK_ITEM_B, "Second");
    add_premise(&forward, "premises.p1", "One", WORK_ITEM_A);
    add_premise(&forward, "premises.p2", "Two", WORK_ITEM_A);
    let backward = AppFixture::new();
    add_work_item(&backward, WORK_ITEM_B, "Second");
    add_work_item(&backward, WORK_ITEM_A, "First");
    add_premise(&backward, "premises.p2", "Two", WORK_ITEM_A);
    add_premise(&backward, "premises.p1", "One", WORK_ITEM_A);

    projected(&forward);
    projected(&backward);

    assert_eq!(
        projected_file(&forward, FILE_A),
        projected_file(&backward, FILE_A)
    );
    assert_eq!(
        projected_file(&forward, FILE_B),
        projected_file(&backward, FILE_B)
    );
}

#[test]
fn it_should_not_write_records() {
    let fx = AppFixture::new();
    add_work_item(&fx, WORK_ITEM_A, "First");
    add_premise(&fx, "premises.p1", "One", WORK_ITEM_A);
    let verify_before = fx.verify();
    let work_items_before = fx.list("work-item");
    let premises_before = fx.list("premise");

    projected(&fx);

    assert_eq!(fx.verify(), verify_before);
    assert_eq!(fx.list("work-item"), work_items_before);
    assert_eq!(fx.list("premise"), premises_before);
    assert_eq!(fx.verify()["ok"], json!(true));
}

#[test]
fn it_should_exclude_retired_nodes() {
    let fx = AppFixture::new();
    add_work_item(&fx, WORK_ITEM_A, "First");
    add_premise(&fx, "premises.p1", "Kept", WORK_ITEM_A);
    add_premise(&fx, "premises.p2", "Retired", WORK_ITEM_A);
    retire(&fx, "premises.p2");

    projected(&fx);

    let text = projected_file(&fx, FILE_A);
    assert!(text.contains("### premises.p1"));
    assert!(!text.contains("premises.p2"));
}
