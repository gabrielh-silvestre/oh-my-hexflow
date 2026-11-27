use crate::support;

use std::fs;
use std::os::unix::fs::symlink;

use omhl::app::INVALID_WORKFLOW_CONFIG;
use omhl::ports::git::GitProbe;
use omhl::probe::{NativeRefusal, NativeResolver};
use serde_json::{Value, json};
use support::appfx::AppFixture;
use support::git::GitRepo;
use support::omhl::{RUN_TESTS, acme, work_item_data};
use support::pr_gate::ALL_NATIVE_CHECKS;
use support::{FakeNative, FakeProbe};
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const ENABLING: &str = "version = 1\n\n[native]\nchecks = [\"checks.run-tests\"]\n";
const DISABLING: &str = "version = 1\n\n[native]\nchecks = []\n";
const WORKFLOW: &str = ".omhl/workflow.toml";
const NEXT_STEP: &str = "omhl load --dry-run";

fn repo_with(workflow: &str) -> GitRepo {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    repo.write(WORKFLOW, workflow);
    repo.commit_all("initial");

    repo
}

/// Worktree ligada numa branch nova, cujo commit leva o `workflow` dado.
fn linked_worktree(repo: &GitRepo, workflow: &str) -> TempDir {
    let parent = TempDir::new().expect("temp dir");
    let path = parent.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "linked",
        path.to_str().expect("utf-8 path"),
    ]);
    fs::write(path.join(WORKFLOW), workflow).expect("the linked copy is written");
    support::git::git_ok(&path, &["add", "-A"]);
    support::git::git_ok(&path, &["commit", "-m", "linked copy"]);

    parent
}

fn resolved_ids(resolver: &dyn NativeResolver) -> Vec<&'static str> {
    resolver
        .resolve()
        .expect("the activation resolves")
        .checks
        .into_iter()
        .collect()
}

// CMP-008: a ativação vem da árvore de trabalho do diretório principal

#[test]
fn it_should_resolve_from_the_main_directory_when_the_linked_copy_disables() {
    let repo = repo_with(ENABLING);
    let linked = linked_worktree(&repo, DISABLING);

    let enabled = resolved_ids(&GitProbe::new(&linked.path().join("linked")));

    assert_eq!(enabled, [RUN_TESTS]);
}

#[test]
fn it_should_resolve_nothing_from_the_main_directory_when_the_linked_copy_enables() {
    let repo = repo_with(DISABLING);
    let linked = linked_worktree(&repo, ENABLING);

    let enabled = resolved_ids(&GitProbe::new(&linked.path().join("linked")));

    assert!(enabled.is_empty());
}

#[test]
fn it_should_resolve_from_the_working_tree_of_the_main_directory_not_from_the_commit() {
    let repo = repo_with(DISABLING);
    repo.write(WORKFLOW, ENABLING);

    let enabled = resolved_ids(&GitProbe::new(repo.path()));

    assert_eq!(enabled, [RUN_TESTS]);
}

#[test]
fn it_should_resolve_nothing_when_the_working_tree_disables_what_the_commit_enabled() {
    let repo = repo_with(ENABLING);
    repo.write(WORKFLOW, DISABLING);

    let enabled = resolved_ids(&GitProbe::new(repo.path()));

    assert!(enabled.is_empty());
}

#[test]
fn it_should_resolve_nothing_when_the_workflow_file_is_absent() {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    repo.commit_all("initial");

    let enabled = resolved_ids(&GitProbe::new(repo.path()));

    assert!(enabled.is_empty());
}

#[test]
fn it_should_not_use_the_copy_of_the_current_directory_when_the_main_directory_is_unresolvable() {
    let outside = TempDir::new().expect("temp dir");
    fs::create_dir(outside.path().join(".omhl")).expect(".omhl is created");
    fs::write(outside.path().join(WORKFLOW), ENABLING).expect("the copy is written");

    let resolved = GitProbe::new(outside.path()).resolve();

    assert_eq!(resolved, Err(NativeRefusal::MainDirUnknown));
}

#[test]
fn it_should_refuse_with_the_invalid_workflow_code_when_the_main_copy_is_not_accepted() {
    let repo = repo_with("version = 1\ncolour = true\n");

    let resolved = GitProbe::new(repo.path()).resolve();

    let Err(NativeRefusal::InvalidWorkflow(error)) = resolved else {
        panic!("the invalid file is refused, got {resolved:?}");
    };
    assert_eq!(error.code, INVALID_WORKFLOW_CONFIG);
}

// O `gate-pr` e o `close` valem-se da ativação do diretório principal

fn open_work_item(fixture: &AppFixture) {
    // Um tipo sem átomo: sem `type` gravado o `close` procuraria o branch, e a fixture não tem repositório.
    let mut data = work_item_data("Ship it");
    data["type"] = json!("chore");
    let item = json!({ "body": {
        "kind": "node", "id": W1, "type": "work-item", "data": data,
    } });
    fixture
        .append_as_user(json!({ "items": [item] }))
        .expect("the work item is opened");
}

fn asked(report: &Value) -> Vec<String> {
    report["questions"]
        .as_array()
        .expect("the report lists its questions")
        .iter()
        .map(|question| question["label"].as_str().expect("a label").to_owned())
        .collect()
}

#[test]
fn it_should_ask_the_native_question_in_gate_pr_and_close_when_only_the_main_directory_enables_it()
{
    let repo = repo_with(ENABLING);
    let linked = linked_worktree(&repo, DISABLING);
    let fixture = AppFixture::new().with_native(GitProbe::new(&linked.path().join("linked")));
    open_work_item(&fixture);

    let gate = fixture.gate_pr(W1);
    let closed = fixture.close(W1).expect("the work item is closed");

    assert!(asked(&gate).contains(&RUN_TESTS.to_owned()));
    assert!(asked(&closed["report"]).contains(&RUN_TESTS.to_owned()));
}

#[test]
fn it_should_not_ask_the_native_question_in_gate_pr_and_close_when_only_the_worktree_copy_enables_it()
 {
    let repo = repo_with(DISABLING);
    let linked = linked_worktree(&repo, ENABLING);
    let fixture = AppFixture::new().with_native(GitProbe::new(&linked.path().join("linked")));
    open_work_item(&fixture);

    let gate = fixture.gate_pr(W1);
    let closed = fixture.close(W1).expect("the work item is closed");

    assert!(!asked(&gate).contains(&RUN_TESTS.to_owned()));
    assert!(!asked(&closed["report"]).contains(&RUN_TESTS.to_owned()));
}

#[test]
fn it_should_ask_no_native_question_in_gate_pr_and_close_without_the_workflow_file() {
    let repo = GitRepo::init();
    repo.write("tracked.txt", "one\n");
    repo.commit_all("initial");
    let fixture = AppFixture::new().with_native(GitProbe::new(repo.path()));
    open_work_item(&fixture);

    let gate = fixture.gate_pr(W1);
    let closed = fixture.close(W1).expect("the work item is closed");

    for report in [&gate, &closed["report"]] {
        let labels = asked(report);
        assert!(
            ALL_NATIVE_CHECKS
                .iter()
                .all(|id| !labels.contains(&(*id).to_owned())),
            "{labels:?}"
        );
    }
}

// CMP-009 e CMP-010: um arquivo que o `omhl` não aceita recusa `gate-pr` e `close`

fn chain_seq(fixture: &AppFixture) -> u64 {
    fixture
        .omhl()
        .chain_head(&acme())
        .expect("the head is read")
        .seq
}

fn assert_refused_with_invalid_workflow(repo: &GitRepo) {
    let fixture = AppFixture::new().with_native(GitProbe::new(repo.path()));
    open_work_item(&fixture);
    let head_before = chain_seq(&fixture);

    let gate = fixture.try_gate_pr(W1).expect_err("gate-pr is refused");
    let closed = fixture.close(W1).expect_err("close is refused");

    for error in [&gate, &closed] {
        assert_eq!(error["error"]["code"], json!(INVALID_WORKFLOW_CONFIG));
    }
    assert_eq!(chain_seq(&fixture), head_before);
    assert_eq!(fixture.get(W1)["data"]["status"], json!("open"));
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_has_a_syntax_error() {
    assert_refused_with_invalid_workflow(&repo_with("version = 1\n[native\n"));
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_has_an_unknown_top_level_key() {
    assert_refused_with_invalid_workflow(&repo_with("version = 1\ncolour = true\n"));
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_lists_a_wrong_native_id() {
    let workflow = "version = 1\n[native]\nchecks = [\"checks.lint\"]\n";

    assert_refused_with_invalid_workflow(&repo_with(workflow));
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_has_an_unsupported_version() {
    assert_refused_with_invalid_workflow(&repo_with("version = 2\n"));
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_is_not_utf8() {
    let repo = repo_with(DISABLING);
    fs::write(repo.path().join(WORKFLOW), [0xff, 0xfe, 0x00]).expect("the bytes are written");

    assert_refused_with_invalid_workflow(&repo);
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_is_a_symlink() {
    let repo = repo_with(DISABLING);
    let target = repo.path().join("elsewhere.toml");
    fs::write(&target, DISABLING).expect("the target is written");
    fs::remove_file(repo.path().join(WORKFLOW)).expect("the file is removed");
    symlink(&target, repo.path().join(WORKFLOW)).expect("the link is created");

    assert_refused_with_invalid_workflow(&repo);
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_file_is_above_the_size_cap() {
    let repo = repo_with(DISABLING);
    let oversized = format!("version = 1\n#{}\n", "x".repeat(65_536));
    fs::write(repo.path().join(WORKFLOW), oversized).expect("the file is written");

    assert_refused_with_invalid_workflow(&repo);
}

#[test]
fn it_should_refuse_gate_pr_and_close_when_the_workflow_path_is_a_directory() {
    let repo = repo_with(DISABLING);
    fs::remove_file(repo.path().join(WORKFLOW)).expect("the file is removed");
    fs::create_dir(repo.path().join(WORKFLOW)).expect("the directory is created");

    assert_refused_with_invalid_workflow(&repo);
}

#[test]
fn it_should_name_the_file_and_the_next_step_in_the_refusal() {
    let repo = repo_with("version = 1\ncolour = true\n");
    let fixture = AppFixture::new().with_native(GitProbe::new(repo.path()));
    open_work_item(&fixture);

    let gate = fixture.try_gate_pr(W1).expect_err("gate-pr is refused");
    let closed = fixture.close(W1).expect_err("close is refused");

    for error in [&gate, &closed] {
        let message = error["error"]["message"].as_str().expect("a message");
        let detail = &error["error"]["details"][0];
        assert!(message.contains(WORKFLOW), "{message}");
        assert_eq!(detail["code"], json!(INVALID_WORKFLOW_CONFIG));
        assert!(
            detail["next_step"]
                .as_str()
                .expect("a next step")
                .contains(NEXT_STEP),
            "{detail}"
        );
    }
}

#[test]
fn it_should_refuse_gate_pr_and_close_with_main_dir_unknown_when_the_resolver_cannot_find_it() {
    let fixture =
        AppFixture::new().with_native(FakeNative::refusing(NativeRefusal::MainDirUnknown));
    open_work_item(&fixture);
    let head_before = chain_seq(&fixture);

    let gate = fixture.try_gate_pr(W1).expect_err("gate-pr is refused");
    let closed = fixture.close(W1).expect_err("close is refused");

    for error in [&gate, &closed] {
        assert_eq!(error["error"]["code"], json!("main-dir-unknown"));
    }
    assert_eq!(chain_seq(&fixture), head_before);
    assert_eq!(fixture.get(W1)["data"]["status"], json!("open"));
}

#[test]
fn it_should_refuse_with_main_dir_unknown_for_a_directory_outside_a_repository() {
    let outside = TempDir::new().expect("temp dir");
    let fixture = AppFixture::new().with_native(GitProbe::new(outside.path()));
    open_work_item(&fixture);

    let gate = fixture.try_gate_pr(W1).expect_err("gate-pr is refused");

    assert_eq!(gate["error"]["code"], json!("main-dir-unknown"));
}

#[test]
fn it_should_keep_the_probe_error_when_the_probe_fails_before_the_resolver_runs() {
    let fixture = AppFixture::new()
        .with_probe(FakeProbe::failing())
        .with_native(FakeNative::refusing(NativeRefusal::MainDirUnknown));
    open_work_item(&fixture);

    let gate = fixture.try_gate_pr(W1).expect_err("gate-pr is refused");
    let closed = fixture.close(W1).expect_err("close is refused");

    for error in [&gate, &closed] {
        assert_eq!(error["error"]["code"], json!("probe-failed"));
    }
}

#[test]
fn it_should_refuse_with_main_dir_unknown_when_no_resolver_was_injected() {
    let fixture = AppFixture::new().without_native();
    open_work_item(&fixture);

    let gate = fixture.try_gate_pr(W1).expect_err("gate-pr is refused");

    assert_eq!(gate["error"]["code"], json!("main-dir-unknown"));
}
