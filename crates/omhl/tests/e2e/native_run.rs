use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use ::omhl::byproduct::ByproductStore;
use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::ports::blob::BlobStore;
use hexlog::{AppendRequest, Config, Snapshot, SnapshotKind};
use omhl::native::{NativeBase, RUN_TESTS_ID};
use omhl::workflow::config::{NativeRun, native_run_digest};
use serde_json::{Value, json};
use support::child::{Cmd, omhl, omhl_json};
use support::ext::ExtTree;
use support::git::git_ok;
use support::omhl::{acme, node_item, with_snapshot};
use support::project::Project;
use support::{FakeProbe, FixedClock, SeqIds, executor_attestation_data, executor_author};

const WORK_ITEM_APPEND: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.w1","type":"work-item","data":{"title":"t","status":"open","type":"chore"}}}]}"#;

const GATE_ON_COMMIT: &str = r#"blocking = true
how = "Always passes."
run = ["true"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-commit"
"#;

fn workflow(run: Option<(&str, u32)>) -> String {
    let mut text = format!("version = 1\n\n[native]\nchecks = [\"{RUN_TESTS_ID}\"]\n");
    if let Some((script, timeout_secs)) = run {
        text.push_str(&format!(
            "\n[native.run.\"{RUN_TESTS_ID}\"]\nrun = [\"sh\", \"-c\", \"{script}\"]\ntimeout_secs = {timeout_secs}\n"
        ));
    }

    text
}

fn declared(script: &str, timeout_secs: u32) -> NativeRun {
    NativeRun {
        run: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        timeout_secs,
        expect: json!([{ "exit": 0, "outcome": "passed" }]),
    }
}

fn omhl_in(p: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(p.dir()).xdg(p.xdg())
}

fn omhl_json_in(p: &Project, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(p.dir()).xdg(p.xdg())
}

/// Repositório limpo na branch do work item `w1`, com o `workflow.toml` e os checks de projeto já carregados.
fn repo(workflow_text: &str, checks: &[(&str, &str)]) -> Project {
    let p = Project::new("acme");
    p.write_workflow(workflow_text);
    git_ok(p.dir(), &["init", "-q", "-b", "main"]);
    git_ok(p.dir(), &["config", "user.name", "Test Author"]);
    git_ok(p.dir(), &["config", "user.email", "author@example.com"]);
    let tree = ExtTree::at(p.dir());
    for (slug, toml) in checks {
        tree.check(slug, toml);
    }
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "base"]);
    git_ok(p.dir(), &["switch", "-q", "-c", "feat/wi-w1__x"]);

    let appended = omhl_in(&p, &["append"]).stdin(WORK_ITEM_APPEND).run();
    assert_eq!(appended.code, Some(0), "append stderr: {}", appended.stderr);
    let loaded = omhl_json_in(&p, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);

    p
}

fn check_run(p: &Project) -> support::child::Output {
    omhl_in(
        p,
        &[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-commit",
        ],
    )
    .run()
}

fn native_attestations(p: &Project) -> Vec<Value> {
    let listed = omhl_json_in(p, &["list", "--type", "attestation"]).run();
    assert_eq!(listed.code, Some(0), "list stderr: {}", listed.stderr);
    let body: Value = serde_json::from_str(listed.stdout.trim_end()).expect("the list is JSON");

    body["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .filter(|item| item["data"]["check"] == json!(RUN_TESTS_ID))
        .cloned()
        .collect()
}

fn git_rev(p: &Project, rev: &str) -> String {
    git_ok(p.dir(), &["rev-parse", rev]).trim().to_owned()
}

fn result_of(stdout: &str) -> Value {
    serde_json::from_str(stdout.trim_end()).expect("the run result is JSON")
}

#[test]
fn it_should_record_an_ok_attestation_on_the_git_tree_when_the_declared_native_command_passes() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    let item = &items[0];
    assert_eq!(item["data"]["outcome"], json!("passed"));
    assert_eq!(item["data"]["ok"], json!(true));
    assert_eq!(item["data"]["work_item"], json!("work-items.w1"));
    assert_eq!(
        item["author"],
        json!({ "client": "executor", "agent": "executor" })
    );
    assert_eq!(
        item["data"]["spec_digest"],
        json!(native_run_digest(&declared("exit 0", 60)))
    );
    assert_eq!(
        item["snapshot"],
        json!({ "kind": "git-tree", "id": git_rev(&p, "HEAD^{tree}") })
    );
}

#[test]
fn it_should_store_the_native_command_output_in_byproduct_when_it_runs() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    check_run(&p);

    let items = native_attestations(&p);
    let byproduct = items[0]["data"]["byproduct"].as_str().unwrap_or_default();
    assert_eq!(byproduct.len(), 64, "byproduct: {byproduct}");
}

#[test]
fn it_should_list_the_native_result_when_the_declared_command_runs() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    let run = check_run(&p);

    let result = result_of(&run.stdout);
    assert_eq!(result["results"][0]["check"], json!(RUN_TESTS_ID));
    assert_eq!(result["results"][0]["outcome"], json!("passed"));
}

#[test]
fn it_should_record_failed_and_block_when_the_native_command_exits_non_zero() {
    let p = repo(&workflow(Some(("exit 1", 60))), &[]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert_eq!(items[0]["data"]["outcome"], json!("failed"));
    assert_eq!(items[0]["data"]["ok"], json!(false));
}

#[test]
fn it_should_record_timed_out_when_the_native_command_outlives_its_timeout() {
    let p = repo(&workflow(Some(("sleep 30", 1))), &[]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items[0]["data"]["outcome"], json!("failed"));
    assert_eq!(items[0]["data"]["timed_out"], json!(true));
}

#[test]
fn it_should_run_nothing_for_the_native_when_no_command_is_declared() {
    let p = repo(&workflow(None), &[]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert_eq!(result_of(&run.stdout)["results"], json!([]));
    assert_eq!(native_attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_accept_a_manual_commit_attestation_when_the_native_has_no_command() {
    let p = repo(&workflow(None), &[]);

    let attested = omhl_json_in(
        &p,
        &[
            "attest",
            RUN_TESTS_ID,
            "work-items.w1",
            "--outcome",
            "passed",
        ],
    )
    .run();

    assert_eq!(attested.code, Some(0), "attest stderr: {}", attested.stderr);
    let items = native_attestations(&p);
    assert_eq!(
        items[0]["snapshot"],
        json!({ "kind": "git-commit", "id": git_rev(&p, "HEAD") })
    );
}

#[test]
fn it_should_carry_no_command_data_in_the_native_catalog() {
    let programs = ["cargo", "npm", "pytest", "go test", "make", "pnpm", "yarn"];

    for check in NativeBase.checks() {
        let debug = format!("{check:?}");
        assert!(!debug.contains("run:"), "run field in {debug}");
        assert!(!debug.contains("argv"), "argv in {debug}");
        for program in programs {
            assert!(!check.how.contains(program), "{program} in {}", check.id);
        }
    }
}

#[test]
fn it_should_run_the_native_without_any_file_in_the_checks_directory() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert_eq!(
        result_of(&run.stdout)["results"][0]["check"],
        json!(RUN_TESTS_ID)
    );
    assert!(
        !run.stdout.contains("check-orphaned"),
        "stdout: {}",
        run.stdout
    );
}

#[test]
fn it_should_not_report_spec_stale_for_the_native_on_the_next_run() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);
    check_run(&p);

    let second = check_run(&p);

    assert_eq!(second.code, Some(0), "stderr: {}", second.stderr);
    assert!(
        !second.stderr.contains("spec-stale") && !second.stdout.contains("spec-stale"),
        "stdout: {} stderr: {}",
        second.stdout,
        second.stderr
    );
}

#[test]
fn it_should_still_drop_a_planned_project_check_without_a_file_as_orphan_next_to_the_native() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[("gate", GATE_ON_COMMIT)]);
    fs::remove_file(p.dir().join(".omhl/checks/gate.toml")).expect("the check file is deleted");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "drop gate"]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let result = result_of(&run.stdout);
    assert_eq!(result["warnings"][0]["code"], json!("check-orphaned"));
    assert_eq!(result["warnings"][0]["check"], json!("checks.gate"));
    assert_eq!(result["results"][0]["check"], json!(RUN_TESTS_ID));
}

const EVALUATED_ID_PREFIX: &str = "attestations.evaluated.";
const EVALUATED_ID_SUFFIX: &str = ".checks.run-tests.work-items.w1";

fn disabled_workflow() -> String {
    "version = 1\n".to_owned()
}

/// O diretório principal fica em `main` com `main_text`; a worktree da branch commita `index_text` e roda o `check run`.
fn repo_with_index(main_text: &str, index_text: &str) -> (Project, PathBuf) {
    let p = repo(main_text, &[]);
    git_ok(p.dir(), &["switch", "-q", "main"]);
    let linked = p.dir().with_extension("linked");
    git_ok(
        p.dir(),
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().expect("utf-8"),
            "feat/wi-w1__x",
        ],
    );
    fs::write(linked.join(".omhl/workflow.toml"), index_text).expect("the index text is written");
    git_ok(&linked, &["add", "-A"]);
    git_ok(
        &linked,
        &["commit", "-q", "--allow-empty", "-m", "change workflow"],
    );
    fs::write(linked.join(".omhl/workflow.toml"), index_text).expect("the text is kept");

    (p, linked)
}

fn check_run_in(p: &Project, dir: &Path) -> support::child::Output {
    omhl(&[
        "check",
        "run",
        "--format",
        "json",
        "--event",
        "before-commit",
    ])
    .cwd(dir)
    .xdg(p.xdg())
    .run()
}

fn is_evaluated(item: &Value) -> bool {
    let id = item["id"].as_str().unwrap_or_default();
    id.starts_with(EVALUATED_ID_PREFIX) && id.ends_with(EVALUATED_ID_SUFFIX)
}

fn in_force_of(items: &[Value]) -> Vec<&Value> {
    items.iter().filter(|item| !is_evaluated(item)).collect()
}

fn evaluated_of(items: &[Value]) -> Vec<&Value> {
    items.iter().filter(|item| is_evaluated(item)).collect()
}

#[test]
fn it_should_run_both_declarations_and_record_one_attestation_per_digest_when_the_index_changes_the_command()
 {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("true", 60))),
    );

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 2, "attestations: {items:?}");
    assert_eq!(
        in_force_of(&items)[0]["data"]["spec_digest"],
        json!(native_run_digest(&declared("exit 0", 60)))
    );
    assert_eq!(
        evaluated_of(&items)[0]["data"]["spec_digest"],
        json!(native_run_digest(&declared("true", 60)))
    );
}

#[test]
fn it_should_list_both_native_results_when_the_index_changes_the_command() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("true", 60))),
    );

    let run = check_run_in(&p, &linked);

    let result = result_of(&run.stdout);
    assert_eq!(
        result["results"].as_array().map(Vec::len),
        Some(2),
        "{result}"
    );
}

#[test]
fn it_should_not_refuse_an_evaluated_failed_after_the_in_force_passed_as_outcome_immutable() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 1", 60))),
    );

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    assert!(
        !run.stderr.contains("outcome-immutable"),
        "stderr: {}",
        run.stderr
    );
    let items = native_attestations(&p);
    assert_eq!(items.len(), 2, "attestations: {items:?}");
    assert_eq!(in_force_of(&items)[0]["data"]["outcome"], json!("passed"));
    assert_eq!(evaluated_of(&items)[0]["data"]["outcome"], json!("failed"));
}

#[test]
fn it_should_run_the_evaluated_command_under_the_evaluated_id_when_the_main_directory_has_no_command()
 {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(Some(("exit 0", 60))));

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert!(is_evaluated(&items[0]), "id: {}", items[0]["id"]);
    assert_eq!(
        items[0]["data"]["spec_digest"],
        json!(native_run_digest(&declared("exit 0", 60)))
    );
    assert_eq!(items[0]["snapshot"]["kind"], json!("git-tree"));
}

#[test]
fn it_should_keep_running_the_in_force_command_when_the_index_drops_the_table() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert!(!is_evaluated(&items[0]), "id: {}", items[0]["id"]);
    assert_eq!(
        items[0]["data"]["spec_digest"],
        json!(native_run_digest(&declared("exit 0", 60)))
    );
}

#[test]
fn it_should_run_once_under_the_base_id_when_both_declarations_have_the_same_command() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0", 60))),
    );

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert!(!is_evaluated(&items[0]), "id: {}", items[0]["id"]);
}

#[test]
fn it_should_run_the_command_under_the_evaluated_id_when_the_index_enables_a_native_the_main_directory_disabled()
 {
    let (p, linked) = repo_with_index(&disabled_workflow(), &workflow(Some(("exit 0", 60))));

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert!(is_evaluated(&items[0]), "id: {}", items[0]["id"]);
}

#[test]
fn it_should_run_nothing_when_the_index_enables_a_disabled_native_without_its_entry() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &workflow(None));

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert_eq!(result_of(&run.stdout)["results"], json!([]));
    assert_eq!(native_attestations(&p), Vec::<Value>::new());
}

// Gate: `gate-delivery` e `close` leem a declaração avaliada do `HEAD`

const SEALED: &str = "spec-digest-match:checks.run-tests";
const SEALED_HEAD: &str = "spec-digest-match:checks.run-tests@head";

fn gate_in(p: &Project, dir: &Path) -> Value {
    let gate = omhl_json(&["gate-delivery", "work-items.w1"])
        .cwd(dir)
        .xdg(p.xdg())
        .run();

    serde_json::from_str(gate.stdout.trim_end()).expect("the gate prints JSON")
}

fn question_of<'a>(gate: &'a Value, label: &str) -> &'a Value {
    gate["questions"]
        .as_array()
        .expect("the gate has questions")
        .iter()
        .find(|question| question["label"] == label)
        .unwrap_or_else(|| panic!("the gate asks {label}: {gate}"))
}

fn run_tests_labels(gate: &Value) -> Vec<String> {
    gate["questions"]
        .as_array()
        .expect("the gate has questions")
        .iter()
        .filter_map(|question| question["label"].as_str())
        .filter(|label| label.contains(RUN_TESTS_ID))
        .map(str::to_owned)
        .collect()
}

fn close_in(p: &Project, dir: &Path, extra: &[&str]) -> support::child::Output {
    let mut args = vec!["close", "work-items.w1"];
    args.extend_from_slice(extra);

    omhl_json(&args).cwd(dir).xdg(p.xdg()).run()
}

#[test]
fn it_should_ask_one_plain_and_two_sealed_questions_when_the_head_changes_the_command() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("true", 60))),
    );

    let gate = gate_in(&p, &linked);

    assert_eq!(run_tests_labels(&gate), [RUN_TESTS_ID, SEALED, SEALED_HEAD]);
}

#[test]
fn it_should_pass_the_command_questions_when_both_declarations_were_run_and_passed() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("true", 60))),
    );
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    for label in [RUN_TESTS_ID, SEALED, SEALED_HEAD] {
        assert_eq!(question_of(&gate, label)["passed"], json!(true), "{label}");
    }
}

#[test]
fn it_should_fail_the_gate_when_the_evaluated_run_failed_after_the_in_force_passed() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 1", 60))),
    );
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    assert_eq!(gate["passed"], json!(false), "{gate}");
    assert_eq!(question_of(&gate, SEALED_HEAD)["passed"], json!(false));
}

#[test]
fn it_should_close_when_both_declarations_were_run_and_passed() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("true", 60))),
    );
    check_run_in(&p, &linked);

    let closed = close_in(&p, &linked, &[]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
}

#[test]
fn it_should_refuse_the_close_when_the_evaluated_run_failed() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 1", 60))),
    );
    check_run_in(&p, &linked);

    let closed = close_in(&p, &linked, &[]);

    assert_eq!(closed.code, Some(1), "stdout: {}", closed.stdout);
    assert!(closed.stderr.contains("close-refused"), "{}", closed.stderr);
}

#[test]
fn it_should_refuse_a_forced_close_when_the_evaluated_run_failed() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 1", 60))),
    );
    check_run_in(&p, &linked);

    let closed = close_in(&p, &linked, &["--force", "--note", "owner override"]);

    assert_eq!(closed.code, Some(1), "stdout: {}", closed.stdout);
}

#[test]
fn it_should_class_every_native_question_with_a_command_as_refuse() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("true", 60))),
    );

    let gate = gate_in(&p, &linked);

    for label in [RUN_TESTS_ID, SEALED, SEALED_HEAD] {
        assert_eq!(
            question_of(&gate, label)["class"],
            json!("refuse"),
            "{label}"
        );
    }
}

#[test]
fn it_should_ask_the_evaluated_digest_labeled_head_when_the_head_adds_the_command() {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(Some(("exit 0", 60))));

    let gate = gate_in(&p, &linked);

    let labels = run_tests_labels(&gate);
    assert!(labels.contains(&SEALED_HEAD.to_owned()), "{labels:?}");
}

#[test]
fn it_should_keep_the_main_directory_instruction_a_separate_warn_question_when_the_head_adds_the_command()
 {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(Some(("exit 0", 60))));

    let gate = gate_in(&p, &linked);

    assert_eq!(question_of(&gate, RUN_TESTS_ID)["class"], json!("warn"));
    assert_eq!(
        question_of(&gate, &format!("{RUN_TESTS_ID}@head"))["class"],
        json!("refuse")
    );
}

#[test]
fn it_should_still_ask_the_in_force_sealed_question_when_the_head_drops_the_table() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));

    let gate = gate_in(&p, &linked);

    assert_eq!(question_of(&gate, SEALED)["class"], json!("refuse"));
}

#[test]
fn it_should_ask_the_evaluated_instruction_requirement_next_to_the_in_force_command_when_the_head_drops_the_table()
 {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));

    let gate = gate_in(&p, &linked);

    let plain = run_tests_labels(&gate)
        .into_iter()
        .filter(|label| !label.starts_with("spec-digest-match"))
        .count();
    assert_eq!(plain, 2, "{gate}");
}

// Attest: recusa `check-run-author` só quando todo requisito do nativo tem comando

fn attest_in(p: &Project, dir: &Path) -> support::child::Output {
    omhl_json(&[
        "attest",
        RUN_TESTS_ID,
        "work-items.w1",
        "--outcome",
        "passed",
    ])
    .cwd(dir)
    .xdg(p.xdg())
    .run()
}

fn attest_error(output: &support::child::Output) -> Value {
    let line = output.stderr.lines().last().unwrap_or_default();

    serde_json::from_str::<Value>(line).expect("stderr ends in a JSON error")["error"].clone()
}

fn assert_refused_as_run_author(attested: &support::child::Output) {
    assert_eq!(attested.code, Some(1), "stdout: {}", attested.stdout);
    let error = attest_error(attested);
    assert_eq!(error["code"], json!("check-run-author"), "{error}");
    assert_eq!(error["details"][0]["path"], json!("/check"), "{error}");
}

fn assert_accepted_as_commit(p: &Project, attested: &support::child::Output) {
    assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
    let items = native_attestations(p);
    assert_eq!(
        items[0]["snapshot"]["kind"],
        json!("git-commit"),
        "{items:?}"
    );
}

#[test]
fn it_should_refuse_attest_with_check_run_author_when_both_declarations_have_the_same_command() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0", 60))),
    );

    let attested = attest_in(&p, &linked);

    assert_refused_as_run_author(&attested);
}

#[test]
fn it_should_ask_one_sealed_question_without_the_head_label_when_both_declarations_are_equal() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0", 60))),
    );

    let gate = gate_in(&p, &linked);

    assert_eq!(run_tests_labels(&gate), [RUN_TESTS_ID, SEALED]);
}

/// Roda o `check run` com as declarações iguais e depois reescreve a tabela do diretório principal.
fn sealed_question_after_main_changes(main_after: &str) -> Value {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0", 60))),
    );
    check_run_in(&p, &linked);
    fs::write(p.dir().join(".omhl/workflow.toml"), main_after).expect("the main text is changed");

    question_of(&gate_in(&p, &linked), SEALED).clone()
}

#[test]
fn it_should_stop_meeting_the_sealed_question_when_the_main_directory_changes_run() {
    let question = sealed_question_after_main_changes(&workflow(Some(("exit 2", 60))));

    assert_eq!(question["passed"], json!(false), "{question}");
}

#[test]
fn it_should_stop_meeting_the_sealed_question_when_the_main_directory_changes_timeout_secs() {
    let question = sealed_question_after_main_changes(&workflow(Some(("exit 0", 90))));

    assert_eq!(question["passed"], json!(false), "{question}");
}

#[test]
fn it_should_stop_meeting_the_sealed_question_when_the_main_directory_changes_the_effective_expect()
{
    let changed = format!(
        "{}expect = [{{ exit = 0, outcome = \"failed\" }}]\n",
        workflow(Some(("exit 0", 60)))
    );

    let question = sealed_question_after_main_changes(&changed);

    assert_eq!(question["passed"], json!(false), "{question}");
}

#[test]
fn it_should_accept_a_hand_attest_when_the_head_adds_the_command() {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(Some(("exit 0", 60))));

    let attested = attest_in(&p, &linked);

    assert_accepted_as_commit(&p, &attested);
}

#[test]
fn it_should_accept_a_hand_attest_when_the_head_drops_the_table() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));

    let attested = attest_in(&p, &linked);

    assert_accepted_as_commit(&p, &attested);
}

#[test]
fn it_should_accept_a_hand_attest_after_the_declared_command_is_removed() {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(None));

    let attested = attest_in(&p, &linked);

    assert_accepted_as_commit(&p, &attested);
}

// `check run --check <nativo>`

fn check_run_id_in(p: &Project, dir: &Path, id: &str) -> support::child::Output {
    omhl(&["check", "run", "--format", "json", "--check", id])
        .cwd(dir)
        .xdg(p.xdg())
        .run()
}

fn stderr_error(output: &support::child::Output) -> Value {
    let line: Value = serde_json::from_str(output.stderr.trim_end()).expect("stderr is JSON");

    line["error"].clone()
}

fn ci_green_workflow() -> String {
    "version = 1\n\n[native]\nchecks = [\"checks.ci-green-before-merge\"]\n".to_owned()
}

#[test]
fn it_should_run_the_native_and_record_the_attestation_when_checked_by_id_with_a_declared_command()
{
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    let run = check_run_id_in(&p, p.dir(), RUN_TESTS_ID);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert_eq!(items[0]["data"]["outcome"], json!("passed"));
}

#[test]
fn it_should_record_failed_when_checked_by_id_and_the_native_command_fails() {
    let p = repo(&workflow(Some(("exit 1", 60))), &[]);

    let run = check_run_id_in(&p, p.dir(), RUN_TESTS_ID);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items[0]["data"]["outcome"], json!("failed"));
}

#[test]
fn it_should_run_the_evaluated_command_when_checked_by_id_and_the_index_enables_the_native() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &workflow(Some(("exit 0", 60))));

    let run = check_run_id_in(&p, &linked, RUN_TESTS_ID);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert!(is_evaluated(&items[0]), "id: {}", items[0]["id"]);
}

#[test]
fn it_should_record_one_attestation_per_digest_when_checked_by_id_and_the_declarations_differ() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0 # changed", 60))),
    );

    let run = check_run_id_in(&p, &linked, RUN_TESTS_ID);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 2, "attestations: {items:?}");
    assert_eq!(evaluated_of(&items).len(), 1, "attestations: {items:?}");
}

#[test]
fn it_should_exit_check_not_mechanical_when_checked_by_id_and_the_native_has_no_command() {
    let p = repo(&workflow(None), &[]);

    let run = check_run_id_in(&p, p.dir(), RUN_TESTS_ID);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    assert_eq!(stderr_error(&run)["code"], json!("check-not-mechanical"));
    assert_eq!(native_attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_exit_check_not_mechanical_when_checked_by_id_and_the_native_is_not_runnable() {
    let p = repo(&ci_green_workflow(), &[]);

    let run = check_run_id_in(&p, p.dir(), "checks.ci-green-before-merge");

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    assert_eq!(stderr_error(&run)["code"], json!("check-not-mechanical"));
}

#[test]
fn it_should_exit_check_not_found_citing_the_native_table_when_the_native_is_disabled_in_both() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &disabled_workflow());

    let run = check_run_id_in(&p, &linked, RUN_TESTS_ID);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    let error = stderr_error(&run);
    assert_eq!(error["code"], json!("check-not-found"));
    assert!(
        error["details"][0]["next_step"]
            .as_str()
            .unwrap_or_default()
            .contains("[native] checks"),
        "error: {error}"
    );
    assert_eq!(native_attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_exit_check_not_found_without_a_hint_when_the_id_is_neither_native_nor_stored() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    let run = check_run_id_in(&p, p.dir(), "checks.nope");

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    let error = stderr_error(&run);
    assert_eq!(error["code"], json!("check-not-found"));
    assert_eq!(error["details"][0].get("next_step"), None, "error: {error}");
}

const IGNORED: &str = "native-run-ignored";
const IGNORED_HINT: &str =
    r#"enable the native in [native] checks or delete the table [native.run."checks.run-tests"]"#;

/// Nativo desligado, com a tabela de comando ainda no arquivo.
fn disabled_with_table() -> String {
    format!(
        "version = 1\n\n[native.run.\"{RUN_TESTS_ID}\"]\nrun = [\"sh\", \"-c\", \"exit 0\"]\ntimeout_secs = 60\n"
    )
}

fn gate_output(p: &Project, dir: &Path) -> support::child::Output {
    omhl_json(&["gate-delivery", "work-items.w1"])
        .cwd(dir)
        .xdg(p.xdg())
        .run()
}

/// Os avisos saem no `warnings` do JSON do stdout; o `close` recusado os leva no JSON do erro.
fn warnings_of(output: &support::child::Output) -> Vec<Value> {
    [&output.stdout, &output.stderr]
        .into_iter()
        .filter_map(|text| serde_json::from_str::<Value>(text.trim_end()).ok())
        .find_map(|body| {
            body["warnings"]
                .as_array()
                .or_else(|| body["error"]["warnings"].as_array())
                .cloned()
        })
        .unwrap_or_default()
}

fn assert_ignored_warning(output: &support::child::Output, from: &str) {
    let expected = json!({
        "code": IGNORED,
        "check": RUN_TESTS_ID,
        "from": from,
        "hint": IGNORED_HINT,
    });

    assert!(
        warnings_of(output).contains(&expected),
        "stdout: {} stderr: {}",
        output.stdout,
        output.stderr
    );
}

#[test]
fn it_should_warn_native_run_ignored_on_check_run_when_the_main_directory_disables_the_native_with_a_table()
 {
    let p = repo(&disabled_with_table(), &[]);

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert_ignored_warning(&run, "main-directory");
}

#[test]
fn it_should_run_no_command_when_the_main_directory_disables_the_native_with_a_table() {
    let p = repo(&disabled_with_table(), &[]);

    check_run(&p);

    assert!(native_attestations(&p).is_empty());
}

#[test]
fn it_should_warn_native_run_ignored_on_gate_delivery_when_the_main_directory_disables_the_native_with_a_table()
 {
    let p = repo(&disabled_with_table(), &[]);

    let gate = gate_output(&p, p.dir());

    assert_ignored_warning(&gate, "main-directory");
}

#[test]
fn it_should_ask_no_native_question_when_the_main_directory_disables_the_native_with_a_table() {
    let p = repo(&disabled_with_table(), &[]);

    let gate = gate_in(&p, p.dir());

    assert_eq!(run_tests_labels(&gate), Vec::<String>::new());
}

#[test]
fn it_should_warn_native_run_ignored_on_close_when_the_main_directory_disables_the_native_with_a_table()
 {
    let p = repo(&disabled_with_table(), &[]);

    let closed = close_in(&p, p.dir(), &[]);

    assert_ignored_warning(&closed, "main-directory");
}

#[test]
fn it_should_not_warn_native_run_ignored_when_the_native_is_enabled_with_its_table() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);

    let run = check_run(&p);

    assert!(
        !warnings_of(&run)
            .iter()
            .any(|w| w["code"] == json!(IGNORED)),
        "stdout: {}",
        run.stdout
    );
}

#[test]
fn it_should_warn_naming_the_index_when_the_index_disables_the_native_with_a_table() {
    let (p, linked) = repo_with_index(&workflow(None), &disabled_with_table());

    let run = check_run_in(&p, &linked);

    assert_ignored_warning(&run, "index");
}

#[test]
fn it_should_warn_naming_the_head_when_the_head_disables_the_native_with_a_table() {
    let (p, linked) = repo_with_index(&workflow(None), &disabled_with_table());

    let gate = gate_output(&p, &linked);

    assert_ignored_warning(&gate, "HEAD");
}

#[test]
fn it_should_not_warn_native_run_ignored_when_the_index_enables_the_native_with_its_entry() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &workflow(Some(("exit 0", 60))));

    let run = check_run_in(&p, &linked);

    assert!(
        !warnings_of(&run)
            .iter()
            .any(|w| w["code"] == json!(IGNORED)),
        "stdout: {}",
        run.stdout
    );
}

fn repo_with_invalid_workflow() -> Project {
    let p = repo(&workflow(Some(("exit 0", 60))), &[("gate", GATE_ON_COMMIT)]);
    fs::write(
        p.dir().join(".omhl/workflow.toml"),
        format!("version = 1\n\n[native.run]\n\"{RUN_TESTS_ID}\" = \"not a table\"\n"),
    )
    .expect("the invalid workflow is written");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "break workflow"]);

    p
}

#[test]
fn it_should_run_the_project_checks_and_no_native_when_the_main_directory_workflow_is_invalid() {
    let p = repo_with_invalid_workflow();

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert!(
        warnings_of(&run)
            .iter()
            .any(|w| w["code"] == json!("invalid-workflow-config")),
        "stdout: {}",
        run.stdout
    );
    assert!(native_attestations(&p).is_empty());
}

#[test]
fn it_should_attest_the_project_check_when_the_main_directory_workflow_is_invalid() {
    let p = repo_with_invalid_workflow();

    let run = check_run(&p);

    assert!(
        result_of(&run.stdout)["results"]
            .as_array()
            .is_some_and(|results| results.len() == 1),
        "stdout: {}",
        run.stdout
    );
}

// Entrada avaliada inválida e arquivo avaliado ilegível

fn empty_run_workflow(expect: Option<&str>) -> String {
    let mut text = format!(
        "version = 1\n\n[native]\nchecks = [\"{RUN_TESTS_ID}\"]\n\n[native.run.\"{RUN_TESTS_ID}\"]\nrun = []\ntimeout_secs = 60\n"
    );
    if let Some(expect) = expect {
        text.push_str(&format!("expect = {expect}\n"));
    }

    text
}

#[test]
fn it_should_run_no_evaluated_command_when_the_evaluated_entry_fails_validation() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &empty_run_workflow(None));

    check_run_in(&p, &linked);

    let items = native_attestations(&p);
    assert!(evaluated_of(&items).is_empty(), "{items:?}");
}

#[test]
fn it_should_fail_the_gate_when_the_evaluated_entry_fails_validation_even_after_the_in_force_run() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &empty_run_workflow(None));
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    assert_eq!(gate["passed"], json!(false), "{gate}");
    assert_eq!(question_of(&gate, SEALED_HEAD)["passed"], json!(false));
}

#[test]
fn it_should_ask_the_evaluated_question_unanswered_when_the_evaluated_entry_is_invalid_and_in_force_is_absent()
 {
    let (p, linked) = repo_with_index(&workflow(None), &empty_run_workflow(None));
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    assert_eq!(
        question_of(&gate, SEALED_HEAD)["passed"],
        json!(false),
        "{gate}"
    );
}

const UNREADABLE: &str = "evaluated-workflow-unreadable";

fn commit_non_utf8_workflow(linked: &Path) {
    fs::write(
        linked.join(".omhl/workflow.toml"),
        [0xff_u8, 0xfe, 0x00, 0xc3],
    )
    .expect("bytes");
    git_ok(linked, &["add", "-A"]);
    git_ok(linked, &["commit", "-q", "-m", "binary workflow"]);
}

fn assert_unreadable_warning(output: &support::child::Output, from: &str) {
    let found = warnings_of(output).iter().any(|w| {
        w["code"] == json!(UNREADABLE)
            && w["from"] == json!(from)
            && w["reason"] == json!("not-utf8")
    });

    assert!(found, "stdout: {} stderr: {}", output.stdout, output.stderr);
}

#[test]
fn it_should_warn_the_unreadable_head_on_gate_delivery_when_the_evaluated_workflow_is_not_utf8() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));
    commit_non_utf8_workflow(&linked);

    let gate = gate_output(&p, &linked);

    assert_unreadable_warning(&gate, "HEAD");
}

#[test]
fn it_should_warn_the_unreadable_index_on_check_run_when_the_evaluated_workflow_is_not_utf8() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));
    commit_non_utf8_workflow(&linked);

    let run = check_run_in(&p, &linked);

    assert_unreadable_warning(&run, "index");
}

#[test]
fn it_should_require_only_the_in_force_declaration_when_the_evaluated_workflow_is_not_utf8() {
    let (p, linked) = repo_with_index(&workflow(Some(("exit 0", 60))), &workflow(None));
    fs::write(
        linked.join(".omhl/workflow.toml"),
        [0xff_u8, 0xfe, 0x00, 0xc3],
    )
    .expect("bytes");
    git_ok(&linked, &["add", "-A"]);
    git_ok(&linked, &["commit", "-q", "-m", "binary workflow"]);

    let gate = gate_in(&p, &linked);

    assert_eq!(run_tests_labels(&gate), [RUN_TESTS_ID, SEALED]);
}

// Cenários ponta a ponta: PR que liga o nativo, PR de reparo e segunda worktree

/// O diretório principal passa a ter `text` em `main`, como depois do merge de um PR.
fn main_receives(p: &Project, text: &str) {
    fs::write(p.dir().join(".omhl/workflow.toml"), text).expect("the main text is written");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(
        p.dir(),
        &["commit", "-q", "-m", "merge the workflow change"],
    );
}

/// Novo commit na worktree, com árvore diferente, para o próximo `check run` gravar attestation nova.
fn commit_new_tree(dir: &Path) {
    fs::write(dir.join("next.txt"), "next\n").expect("the file is written");
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", "next commit"]);
}

fn enabling_pr() -> (Project, PathBuf) {
    repo_with_index(&disabled_workflow(), &workflow(Some(("exit 0", 60))))
}

#[test]
fn it_should_ask_the_head_questions_and_fail_the_gate_when_the_enabling_pr_has_no_attestation() {
    let (p, linked) = enabling_pr();

    let gate = gate_in(&p, &linked);

    assert_eq!(
        run_tests_labels(&gate),
        [
            SEALED_HEAD.replace("spec-digest-match:", ""),
            SEALED_HEAD.to_owned()
        ]
    );
    assert_eq!(gate["passed"], json!(false), "{gate}");
}

#[test]
fn it_should_pass_the_gate_when_the_enabling_pr_ran_the_command() {
    let (p, linked) = enabling_pr();
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    assert_eq!(gate["passed"], json!(true), "{gate}");
}

#[test]
fn it_should_refuse_attest_with_check_run_author_when_the_enabling_pr_adds_the_command() {
    let (p, linked) = enabling_pr();

    let attested = attest_in(&p, &linked);

    assert_refused_as_run_author(&attested);
}

/// O índice nunca decide: o `attest` recusa antes, porque o probe do snapshot acusa a mudança staged.
fn stage_workflow(dir: &Path, text: &str) {
    fs::write(dir.join(".omhl/workflow.toml"), text).expect("the staged text is written");
    git_ok(dir, &["add", ".omhl/workflow.toml"]);
}

fn assert_refused_as_dirty(p: &Project, attested: &support::child::Output) {
    assert_eq!(attested.code, Some(1), "stdout: {}", attested.stdout);
    assert_eq!(attest_error(attested)["code"], json!("snapshot-dirty"));
    assert!(
        native_attestations(p).is_empty(),
        "no attestation is written"
    );
}

#[test]
fn it_should_refuse_attest_as_dirty_when_the_index_drops_the_command_that_head_declares() {
    let command = workflow(Some(("exit 0", 60)));
    let (p, linked) = repo_with_index(&command, &command);
    stage_workflow(&linked, &workflow(None));

    let attested = attest_in(&p, &linked);

    assert_refused_as_dirty(&p, &attested);
}

#[test]
fn it_should_refuse_attest_as_dirty_when_the_index_adds_the_command_that_head_lacks() {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(None));
    stage_workflow(&linked, &workflow(Some(("exit 0", 60))));

    let attested = attest_in(&p, &linked);

    assert_refused_as_dirty(&p, &attested);
}

#[test]
fn it_should_close_the_enabling_pr_before_the_main_directory_is_updated() {
    let (p, linked) = enabling_pr();
    check_run_in(&p, &linked);

    let closed = close_in(&p, &linked, &[]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
}

#[test]
fn it_should_ask_one_plain_question_without_label_when_the_main_directory_receives_the_table() {
    let (p, linked) = enabling_pr();
    check_run_in(&p, &linked);
    main_receives(&p, &workflow(Some(("exit 0", 60))));

    let gate = gate_in(&p, &linked);

    assert_eq!(run_tests_labels(&gate), [RUN_TESTS_ID, SEALED]);
}

#[test]
fn it_should_meet_the_single_requirement_with_the_pr_attestation_when_the_main_directory_receives_the_table()
 {
    let (p, linked) = enabling_pr();
    check_run_in(&p, &linked);
    main_receives(&p, &workflow(Some(("exit 0", 60))));

    let gate = gate_in(&p, &linked);

    assert_eq!(gate["passed"], json!(true), "{gate}");
}

#[test]
fn it_should_add_no_requirement_when_the_enabling_pr_has_no_entry() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &workflow(None));

    let gate = gate_in(&p, &linked);

    assert_eq!(run_tests_labels(&gate), Vec::<String>::new());
}

// PR de reparo

fn repair_pr() -> (Project, PathBuf) {
    let (p, linked) = repo_with_index(&workflow(None), &workflow(Some(("exit 0", 60))));
    fs::write(
        p.dir().join(".omhl/workflow.toml"),
        "version = 1\ncolour = true\n",
    )
    .expect("the broken text is written");

    (p, linked)
}

fn base_gate_in(p: &Project, dir: &Path) -> Value {
    let gate = omhl_json(&["gate-delivery", "work-items.w1", "--base", "main"])
        .cwd(dir)
        .xdg(p.xdg())
        .run();

    serde_json::from_str(gate.stdout.trim_end()).expect("the gate prints JSON")
}

#[test]
fn it_should_record_the_evaluated_attestation_when_the_repair_pr_runs_the_command() {
    let (p, linked) = repair_pr();

    let run = check_run_in(&p, &linked);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let items = native_attestations(&p);
    assert_eq!(items.len(), 1, "attestations: {items:?}");
    assert!(is_evaluated(&items[0]), "id: {}", items[0]["id"]);
}

#[test]
fn it_should_pass_the_gate_with_base_when_the_repair_pr_attested_the_command() {
    let (p, linked) = repair_pr();
    check_run_in(&p, &linked);

    let gate = base_gate_in(&p, &linked);

    assert_eq!(
        question_of(&gate, SEALED_HEAD)["passed"],
        json!(true),
        "{gate}"
    );
}

#[test]
fn it_should_close_the_repair_pr_when_the_command_was_attested() {
    let (p, linked) = repair_pr();
    check_run_in(&p, &linked);
    let reviewed = omhl_json(&[
        "attest",
        "checks.config-loosening-review",
        "work-items.w1",
        "--outcome",
        "passed",
    ])
    .cwd(&linked)
    .xdg(p.xdg())
    .run();
    assert_eq!(reviewed.code, Some(0), "stderr: {}", reviewed.stderr);
    let attested = attest_in(&p, &linked);
    assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);

    let gate = base_gate_in(&p, &linked);
    assert_eq!(
        run_tests_labels(&gate),
        ["checks.run-tests@head", SEALED_HEAD, RUN_TESTS_ID]
    );
    let closed = close_in(&p, &linked, &["--base", "main"]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
}

#[test]
fn it_should_accept_a_hand_attest_of_the_instruction_requirement_when_the_repair_pr_has_the_command()
 {
    let (p, linked) = repair_pr();

    let attested = attest_in(&p, &linked);

    assert_accepted_as_commit(&p, &attested);
    let ids: Vec<String> = native_attestations(&p)
        .iter()
        .map(|item| item["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        ids.iter().all(|id| id.starts_with("attestations.")
            && !id.starts_with(EVALUATED_ID_PREFIX)
            && id.ends_with(EVALUATED_ID_SUFFIX)),
        "ids: {ids:?}"
    );
}

#[test]
fn it_should_pass_the_native_questions_with_base_when_the_repair_pr_attested_by_hand_and_ran_the_check()
 {
    let (p, linked) = repair_pr();
    let attested = attest_in(&p, &linked);
    assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
    check_run_in(&p, &linked);

    let gate = base_gate_in(&p, &linked);

    let passed: Vec<Value> = [RUN_TESTS_ID, "checks.run-tests@head", SEALED_HEAD]
        .iter()
        .map(|label| question_of(&gate, label)["passed"].clone())
        .collect();
    assert_eq!(passed, vec![json!(true); 3], "{gate}");
}

// Segunda worktree depois da mudança do diretório principal

#[test]
fn it_should_fail_the_sealed_question_when_the_main_directory_merges_a_command_change() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0", 60))),
    );
    check_run_in(&p, &linked);
    main_receives(&p, &workflow(Some(("exit 2", 60))));

    let gate = gate_in(&p, &linked);

    assert_eq!(question_of(&gate, SEALED)["passed"], json!(false), "{gate}");
}

#[test]
fn it_should_pass_the_gate_after_the_next_commit_records_the_new_digest() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("exit 0", 60))),
    );
    check_run_in(&p, &linked);
    main_receives(&p, &workflow(Some(("true", 60))));
    commit_new_tree(&linked);
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    assert_eq!(question_of(&gate, SEALED)["passed"], json!(true), "{gate}");
}

#[test]
fn it_should_fail_the_sealed_question_when_the_main_directory_merges_an_activation_with_command() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &disabled_workflow());
    main_receives(&p, &workflow(Some(("exit 0", 60))));

    let gate = gate_in(&p, &linked);

    assert_eq!(question_of(&gate, SEALED)["passed"], json!(false), "{gate}");
}

#[test]
fn it_should_pass_the_gate_after_the_next_commit_when_the_main_directory_merged_an_activation() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &disabled_workflow());
    main_receives(&p, &workflow(Some(("exit 0", 60))));
    commit_new_tree(&linked);
    check_run_in(&p, &linked);

    let gate = gate_in(&p, &linked);

    assert_eq!(question_of(&gate, SEALED)["passed"], json!(true), "{gate}");
}

fn on_develop(p: &Project) {
    git_ok(p.dir(), &["switch", "-q", "-c", "develop", "main"]);
    fs::write(
        p.dir().join(".git").join("omhl-develop-mark"),
        "omhl-develop-mark v1\n",
    )
    .expect("the mark is written");
}

#[test]
fn it_should_run_the_native_command_on_develop_without_recording_when_it_passes() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);
    on_develop(&p);

    let run = check_run(&p);

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let result = result_of(&run.stdout);
    assert_eq!(result["recorded"], json!(false));
    assert_eq!(result["results"][0]["check"], json!(RUN_TESTS_ID));
    assert_eq!(result["results"][0]["outcome"], json!("passed"));
}

#[test]
fn it_should_record_no_native_attestation_on_develop() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);
    on_develop(&p);

    check_run(&p);

    assert_eq!(native_attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_fail_on_develop_with_the_native_output_in_the_output_tail() {
    let p = repo(&workflow(Some(("echo boom-marker; exit 1", 60))), &[]);
    on_develop(&p);

    let run = check_run(&p);

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("output_tail"), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("boom-marker"), "stderr: {}", run.stderr);
    assert_eq!(native_attestations(&p), Vec::<Value>::new());
}

// omhl byproduct do nativo

const W1_ID: &str = "work-items.w1";

fn byproduct_in(p: &Project, dir: &Path, check: &str) -> support::child::Output {
    omhl_json(&["byproduct", check, W1_ID])
        .cwd(dir)
        .xdg(p.xdg())
        .run()
}

fn byproduct_result(output: &support::child::Output) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    serde_json::from_str(output.stdout.trim_end()).expect("the byproduct result is JSON")
}

fn lines_text(result: &Value) -> String {
    result["lines"]
        .as_array()
        .expect("lines is an array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

fn tree_snapshot(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn in_force_id(check: &str, fill: char) -> String {
    let sha = fill.to_string().repeat(40);

    format!("attestations.{}.{}.{check}.{W1_ID}", &sha[..32], &sha[32..])
}

fn evaluated_id(check: &str, fill: char) -> String {
    let sha = fill.to_string().repeat(40);

    format!(
        "attestations.evaluated.{}.{}.{check}.{W1_ID}",
        &sha[..32],
        &sha[32..]
    )
}

/// A ordem das chamadas define o `seq`; a saída vai num blob de só stdout.
fn record(
    p: &Project,
    ids: &SeqIds,
    id: &str,
    check: &str,
    fill: char,
    outcome: &str,
    stdout: &str,
) {
    let mut bytes = format!("omhl-byproduct v1 stdout={} stderr=0\n", stdout.len()).into_bytes();
    bytes.extend_from_slice(stdout.as_bytes());
    let hash = BlobStore::new(&p.db_path(), &acme())
        .put(&bytes)
        .expect("the blob is stored");
    let snapshot = tree_snapshot(fill);
    let probe = FakeProbe::clean(snapshot.clone());
    let clock = FixedClock::default();
    let deps = OmhlDeps {
        clock: &clock,
        ids,
        probe: &probe,
        session: &support::NO_SESSION,
    };
    let mut db = Omhl::open(&Config::new(p.db_path()), deps).expect("omhl opens");
    let data = executor_attestation_data(check, W1_ID, outcome, hash.as_str());

    db.append(&AppendRequest {
        tenant: acme(),
        author: executor_author(),
        key: None,
        items: vec![with_snapshot(node_item(id, "attestation", data), &snapshot)],
    })
    .expect("the attestation is appended");
}

#[test]
fn it_should_show_the_failed_outcome_and_output_lines_when_byproduct_reads_the_failing_native() {
    let p = repo(&workflow(Some(("echo boom; exit 1", 60))), &[]);
    check_run(&p);

    let output = byproduct_in(&p, p.dir(), RUN_TESTS_ID);

    let result = byproduct_result(&output);
    assert_eq!(result["outcome"], json!("failed"), "{result}");
    assert_eq!(result["byproduct"].as_str().map(str::len), Some(64));
    assert!(lines_text(&result).contains("boom"), "{result}");
}

#[test]
fn it_should_print_a_failure_hint_that_byproduct_accepts_when_the_native_command_fails() {
    let p = repo(&workflow(Some(("echo boom; exit 1", 60))), &[]);
    let run = omhl_in(&p, &["check", "run", "--event", "before-commit"]).run();
    let hint = "omhl byproduct checks.run-tests work-items.w1";
    assert!(
        run.stdout.contains(hint) || run.stderr.contains(hint),
        "stdout: {} stderr: {}",
        run.stdout,
        run.stderr
    );

    let output = byproduct_in(&p, p.dir(), RUN_TESTS_ID);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_show_the_evaluated_failed_when_it_was_recorded_after_the_in_force_passed() {
    let (p, linked) = repo_with_index(
        &workflow(Some(("exit 0", 60))),
        &workflow(Some(("echo evaluated-boom; exit 1", 60))),
    );
    check_run_in(&p, &linked);

    let output = byproduct_in(&p, &linked, RUN_TESTS_ID);

    let result = byproduct_result(&output);
    assert_eq!(result["outcome"], json!("failed"), "{result}");
    assert!(lines_text(&result).contains("evaluated-boom"), "{result}");
}

#[test]
fn it_should_show_the_evaluated_failed_when_the_in_force_passed_was_recorded_after_it() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);
    let ids = SeqIds::new();
    let evaluated = evaluated_id(RUN_TESTS_ID, 'a');
    let in_force = in_force_id(RUN_TESTS_ID, 'a');
    record(
        &p,
        &ids,
        &evaluated,
        RUN_TESTS_ID,
        'a',
        "failed",
        "evaluated-boom",
    );
    record(
        &p,
        &ids,
        &in_force,
        RUN_TESTS_ID,
        'a',
        "passed",
        "in-force-ok",
    );

    let output = byproduct_in(&p, p.dir(), RUN_TESTS_ID);

    let result = byproduct_result(&output);
    assert_eq!(result["outcome"], json!("failed"), "{result}");
    assert!(lines_text(&result).contains("evaluated-boom"), "{result}");
}

#[test]
fn it_should_show_the_passed_of_the_newer_snapshot_when_an_older_snapshot_failed() {
    let p = repo(&workflow(Some(("echo old-failure; exit 1", 60))), &[]);
    check_run(&p);
    main_receives(&p, &workflow(Some(("echo new-pass; exit 0", 60))));
    check_run(&p);

    let output = byproduct_in(&p, p.dir(), RUN_TESTS_ID);

    let result = byproduct_result(&output);
    assert_eq!(result["outcome"], json!("passed"), "{result}");
    assert!(lines_text(&result).contains("new-pass"), "{result}");
}

#[test]
fn it_should_exit_check_not_found_when_byproduct_reads_an_id_neither_native_nor_stored() {
    let p = repo(&workflow(Some(("exit 1", 60))), &[]);
    check_run(&p);

    let output = byproduct_in(&p, p.dir(), "checks.nope");

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(stderr_error(&output)["code"], json!("check-not-found"));
}

#[test]
fn it_should_exit_attestation_not_found_when_the_native_has_no_attestation_of_the_work_item() {
    let p = repo(&workflow(Some(("exit 1", 60))), &[]);

    let output = byproduct_in(&p, p.dir(), RUN_TESTS_ID);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(
        stderr_error(&output)["code"],
        json!("attestation-not-found")
    );
}

#[test]
fn it_should_read_the_highest_seq_attestation_when_a_project_check_has_two_snapshots() {
    let p = repo(&workflow(None), &[("lint", GATE_ON_COMMIT)]);
    let ids = SeqIds::new();
    let older = in_force_id("checks.lint", 'a');
    let newer = in_force_id("checks.lint", 'b');
    record(&p, &ids, &older, "checks.lint", 'a', "passed", "lint-ok");
    record(
        &p,
        &ids,
        &newer,
        "checks.lint",
        'b',
        "failed",
        "lint-failure",
    );

    let output = byproduct_in(&p, p.dir(), "checks.lint");

    let result = byproduct_result(&output);
    assert_eq!(result["outcome"], json!("failed"), "{result}");
    assert!(lines_text(&result).contains("lint-failure"), "{result}");
}

fn blob_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                blob_files(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

#[test]
fn it_should_keep_both_native_outputs_and_still_show_the_failed_when_prune_runs_after_seven_days() {
    let p = repo(&workflow(Some(("exit 0", 60))), &[]);
    let ids = SeqIds::new();
    let evaluated = evaluated_id(RUN_TESTS_ID, 'a');
    let in_force = in_force_id(RUN_TESTS_ID, 'a');
    record(
        &p,
        &ids,
        &evaluated,
        RUN_TESTS_ID,
        'a',
        "failed",
        "evaluated-boom",
    );
    record(
        &p,
        &ids,
        &in_force,
        RUN_TESTS_ID,
        'a',
        "passed",
        "in-force-ok",
    );
    let blobs = blob_files(&p.xdg().join("omhl/blobs"));
    assert_eq!(blobs.len(), 2, "blobs: {blobs:?}");
    for blob in &blobs {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(blob)
            .expect("the blob opens");
        file.set_modified(
            std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 86_400),
        )
        .expect("the date is set");
    }

    let pruned = omhl_json_in(&p, &["byproduct", "prune"]).run();

    assert_eq!(pruned.code, Some(0), "stderr: {}", pruned.stderr);
    assert!(blobs.iter().all(|blob| blob.exists()), "blobs: {blobs:?}");
    let result = byproduct_result(&byproduct_in(&p, p.dir(), RUN_TESTS_ID));
    assert_eq!(result["outcome"], json!("failed"), "{result}");
    assert!(lines_text(&result).contains("evaluated-boom"), "{result}");
}

fn disabled_with_run(run: &str) -> String {
    format!(
        "version = 1\n\n[native.run.\"{RUN_TESTS_ID}\"]\nrun = [\"sh\", \"-c\", \"{run}\"]\ntimeout_secs = 60\n"
    )
}

fn ignored_froms(output: &support::child::Output) -> Vec<String> {
    let mut froms: Vec<String> = warnings_of(output)
        .iter()
        .filter(|w| w["code"] == json!(IGNORED))
        .map(|w| w["from"].as_str().unwrap_or_default().to_owned())
        .collect();
    froms.sort();
    froms
}

#[test]
fn it_should_warn_once_on_check_run_when_the_main_directory_and_the_index_have_the_same_table() {
    let (p, linked) = repo_with_index(&disabled_with_table(), &disabled_with_table());

    let run = check_run_in(&p, &linked);

    assert_eq!(
        ignored_froms(&run),
        ["main-directory"],
        "stdout: {}",
        run.stdout
    );
}

#[test]
fn it_should_warn_for_both_sources_on_check_run_when_the_index_table_differs_from_the_main_directory()
 {
    let (p, linked) = repo_with_index(&disabled_with_table(), &disabled_with_run("exit 3"));

    let run = check_run_in(&p, &linked);

    assert_eq!(
        ignored_froms(&run),
        ["index", "main-directory"],
        "stdout: {}",
        run.stdout
    );
}

#[test]
fn it_should_warn_once_naming_the_index_on_check_run_when_only_the_index_has_the_table() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &disabled_with_table());

    let run = check_run_in(&p, &linked);

    assert_eq!(ignored_froms(&run), ["index"], "stdout: {}", run.stdout);
}

#[test]
fn it_should_warn_once_on_gate_delivery_when_the_main_directory_and_head_have_the_same_table() {
    let (p, linked) = repo_with_index(&disabled_with_table(), &disabled_with_table());

    let gate = gate_output(&p, &linked);

    assert_eq!(
        ignored_froms(&gate),
        ["main-directory"],
        "stdout: {}",
        gate.stdout
    );
}

#[test]
fn it_should_warn_for_both_sources_on_gate_delivery_when_the_head_table_differs_from_the_main_directory()
 {
    let (p, linked) = repo_with_index(&disabled_with_table(), &disabled_with_run("exit 3"));

    let gate = gate_output(&p, &linked);

    assert_eq!(
        ignored_froms(&gate),
        ["HEAD", "main-directory"],
        "stdout: {}",
        gate.stdout
    );
}

#[test]
fn it_should_warn_once_naming_the_head_on_gate_delivery_when_only_the_head_has_the_table() {
    let (p, linked) = repo_with_index(&disabled_workflow(), &disabled_with_table());

    let gate = gate_output(&p, &linked);

    assert_eq!(ignored_froms(&gate), ["HEAD"], "stdout: {}", gate.stdout);
}
