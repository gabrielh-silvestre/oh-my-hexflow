use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Cmd, Output, omhl_json};
use support::git::{git_ok, write_executable};
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const REVIEW: &str = "checks.config-loosening-review";
const COMMANDS: [&str; 2] = ["gate-delivery", "close"];

fn run(project: &Project, args: &[&str]) -> Output {
    cmd(project, args).run()
}

fn cmd(project: &Project, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(project.dir()).xdg(project.xdg())
}

fn open_work_item(project: &Project, work_item_type: &str) {
    let mut data = work_item_data("t");
    data["type"] = json!(work_item_type);
    let request = json!({ "items": [{ "body": {
        "kind": "node", "id": W1, "type": "work-item", "data": data,
    } }] });

    let appended = cmd(project, &["append"]).stdin(request.to_string()).run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn project_with_review(on: bool) -> Project {
    let project = Project::new("acme");
    project.enable_natives(if on { &[REVIEW] } else { &[] });
    support::git::commit_marker(project.dir());
    open_work_item(&project, "chore");

    project
}

fn commit_on_topic(project: &Project, path: &str) -> String {
    let fork_point = git_ok(project.dir(), &["rev-parse", "HEAD"]);
    git_ok(project.dir(), &["switch", "-q", "-c", "topic"]);
    let target = project.dir().join(path);
    fs::create_dir_all(target.parent().expect("a parent")).expect("the folder is created");
    // A comment keeps an existing workflow.toml valid, so the change reaches the base check.
    let previous = fs::read_to_string(&target).unwrap_or_default();
    fs::write(&target, format!("{previous}# changed\n")).expect("the file is written");
    git_ok(project.dir(), &["add", "-A"]);
    git_ok(project.dir(), &["commit", "-q", "-m", "change"]);

    fork_point.trim().to_owned()
}

fn error_of(output: &Output) -> Value {
    serde_json::from_str::<Value>(output.stderr.trim_end()).expect("stderr is JSON")["error"]
        .clone()
}

fn assert_refused(output: &Output, code: &str) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(error_of(output)["code"], json!(code), "{}", output.stderr);
}

fn text_of(output: &Output) -> String {
    format!("{}{}", output.stdout, output.stderr)
}

fn with_git_that_fails_on(project: &Project, words: &[&str], args: &[&str]) -> Output {
    let fakebin = project.dir().with_extension("fakebin");
    fs::create_dir_all(&fakebin).expect("fakebin is created");
    let inherited = std::env::var_os("PATH").expect("the test process has a PATH");
    let real_git = std::env::split_paths(&inherited)
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git is on the PATH");
    let tests = words
        .iter()
        .map(|word| format!("[ \"$arg\" = {word} ] && exit 1;"))
        .collect::<String>();
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do {tests} done\nexec {} \"$@\"\n",
        real_git.display()
    );
    write_executable(&fakebin.join("git"), &script);
    let path =
        std::env::join_paths(std::iter::once(fakebin).chain(std::env::split_paths(&inherited)))
            .expect("PATH is joined");

    cmd(project, args).env("PATH", path).run()
}

#[test]
fn it_should_refuse_a_base_that_does_not_resolve_naming_the_option() {
    let project = project_with_review(true);

    for command in COMMANDS {
        let output = run(&project, &[command, W1, "--base", "no-such-ref"]);

        assert_refused(&output, "review-base-invalid");
        let message = error_of(&output)["message"].to_string();
        assert!(message.contains("--base"), "{message}");
    }
}

#[test]
fn it_should_refuse_head_as_the_base_because_the_range_is_empty() {
    let project = project_with_review(true);

    for command in COMMANDS {
        let output = run(&project, &[command, W1, "--base", "HEAD"]);

        assert_refused(&output, "review-base-invalid");
    }
}

#[test]
fn it_should_refuse_an_invalid_base_even_with_the_review_check_off() {
    let project = project_with_review(false);

    for command in COMMANDS {
        let output = run(&project, &[command, W1, "--base", "no-such-ref"]);

        assert_refused(&output, "review-base-invalid");
    }
}

#[test]
fn it_should_accept_a_valid_base_with_the_review_check_off() {
    let project = project_with_review(false);
    commit_on_topic(&project, "src/a.rs");

    let output = run(&project, &["gate-delivery", W1, "--base", "main"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_keep_probe_failed_when_git_fails_to_find_the_merge_base() {
    let project = project_with_review(true);
    commit_on_topic(&project, "src/a.rs");

    let output = with_git_that_fails_on(
        &project,
        &["merge-base", "diff"],
        &["gate-delivery", W1, "--base", "main"],
    );

    assert_refused(&output, "probe-failed");
}

#[test]
fn it_should_refuse_with_the_workflow_error_before_judging_the_base() {
    let project = project_with_review(true);
    project.write_workflow("version = 1\ncolour = true\n");
    git_ok(project.dir(), &["commit", "-q", "-am", "break the file"]);

    for command in COMMANDS {
        let output = run(&project, &[command, W1, "--base", "no-such-ref"]);

        assert_refused(&output, "invalid-workflow-config");
    }
}

#[test]
fn it_should_refuse_with_the_main_directory_error_before_judging_the_base() {
    let project = project_with_review(true);

    let output = with_git_that_fails_on(
        &project,
        &["--git-common-dir"],
        &["gate-delivery", W1, "--base", "no-such-ref"],
    );

    assert_refused(&output, "main-dir-unknown");
}

#[test]
fn it_should_refuse_with_the_type_error_before_judging_the_base() {
    let other = Project::new("acme");
    other.enable_natives(&[REVIEW]);
    support::git::commit_marker(other.dir());
    open_work_item(&other, "xyz");

    for command in COMMANDS {
        let output = run(&other, &[command, W1, "--base", "no-such-ref"]);

        assert_refused(&output, "work-item-type-unknown");
    }
}

fn assert_names_the_trigger(project: &Project, fork_point: &str, path: &str) {
    for command in COMMANDS {
        let text = text_of(&run(project, &[command, W1, "--base", "main"]));

        for expected in ["main", fork_point, path] {
            assert!(text.contains(expected), "{command}: {expected} in {text}");
        }
    }
}

#[test]
fn it_should_print_the_base_the_merge_base_and_the_triggering_path_for_the_workflow_file() {
    let project = project_with_review(true);
    let fork_point = commit_on_topic(&project, ".omhl/workflow.toml");

    assert_names_the_trigger(&project, &fork_point, ".omhl/workflow.toml");
}

#[test]
fn it_should_print_the_base_the_merge_base_and_the_triggering_path_for_a_check_file() {
    let project = project_with_review(true);
    let fork_point = commit_on_topic(&project, ".omhl/checks/x.md");

    assert_names_the_trigger(&project, &fork_point, ".omhl/checks/x.md");
}

#[test]
fn it_should_leave_the_review_question_out_of_the_gate_when_only_a_source_file_changed() {
    let project = project_with_review(true);
    commit_on_topic(&project, "src/a.rs");

    let output = run(&project, &["gate-delivery", W1, "--base", "main"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!output.stdout.contains(REVIEW), "{}", output.stdout);
}

#[test]
fn it_should_say_that_no_base_was_given_when_the_review_check_is_on() {
    let project = project_with_review(true);

    for command in COMMANDS {
        let text = text_of(&run(&project, &[command, W1]));

        assert!(text.contains("no base"), "{command}: {text}");
    }
}

fn review_question(output: &Output) -> Option<Value> {
    let gate: Value = serde_json::from_str(output.stdout.trim_end()).expect("stdout is JSON");

    gate["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .find(|question| question["label"] == json!(REVIEW))
        .cloned()
}

fn attest_review(project: &Project) {
    let attested = run(project, &["attest", REVIEW, W1, "--outcome", "passed"]);

    assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
}

fn project_with_loosened_workflow() -> Project {
    let project = project_with_review(true);
    commit_on_topic(&project, ".omhl/workflow.toml");

    project
}

fn status_of_work_item(project: &Project) -> Value {
    let fetched = run(project, &["get", W1]);

    serde_json::from_str::<Value>(fetched.stdout.trim_end()).expect("stdout is JSON")["node"]
        ["data"]["status"]
        .clone()
}

#[test]
fn it_should_fail_the_review_question_as_refuse_when_the_workflow_file_changed_and_nothing_is_attested()
 {
    let project = project_with_loosened_workflow();

    let gate = run(&project, &["gate-delivery", W1, "--base", "main"]);

    let review = review_question(&gate).expect("the review question is in the gate");
    assert_eq!(review["passed"], json!(false));
    assert_eq!(review["class"], json!("refuse"));
}

#[test]
fn it_should_refuse_the_close_when_the_review_is_required_and_unattested() {
    let project = project_with_loosened_workflow();

    let closed = run(&project, &["close", W1, "--base", "main"]);

    assert_refused(&closed, "close-refused");
    assert_eq!(status_of_work_item(&project), json!("open"));
}

#[test]
fn it_should_not_let_force_with_a_note_past_the_unattested_review() {
    let project = project_with_loosened_workflow();

    let closed = run(
        &project,
        &[
            "close",
            W1,
            "--base",
            "main",
            "--force",
            "--note",
            "owner approved the override",
        ],
    );

    assert_refused(&closed, "close-refused");
    assert_eq!(status_of_work_item(&project), json!("open"));
}

#[test]
fn it_should_pass_the_review_question_once_it_is_attested_at_the_head() {
    let project = project_with_loosened_workflow();
    attest_review(&project);

    let gate = run(&project, &["gate-delivery", W1, "--base", "main"]);

    let review = review_question(&gate).expect("the review question is in the gate");
    assert_eq!(review["passed"], json!(true));
}

#[test]
fn it_should_close_once_the_review_is_attested_at_the_head() {
    let project = project_with_loosened_workflow();
    attest_review(&project);

    let closed = run(&project, &["close", W1, "--base", "main"]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(status_of_work_item(&project), json!("closed"));
}

#[test]
fn it_should_fail_the_review_question_again_when_a_commit_follows_the_attestation() {
    let project = project_with_loosened_workflow();
    attest_review(&project);
    git_ok(
        project.dir(),
        &["commit", "-q", "--allow-empty", "-m", "after the review"],
    );

    let gate = run(&project, &["gate-delivery", W1, "--base", "main"]);

    let review = review_question(&gate).expect("the review question is in the gate");
    assert_eq!(review["passed"], json!(false));
}

#[test]
fn it_should_require_the_review_when_no_base_is_given_and_the_check_is_on() {
    let project = project_with_review(true);

    let gate = run(&project, &["gate-delivery", W1]);

    let review = review_question(&gate).expect("the review question is in the gate");
    assert_eq!(review["passed"], json!(false));
}

#[test]
fn it_should_leave_the_review_question_out_when_the_check_is_off_even_if_the_workflow_file_changed()
{
    let project = project_with_review(false);
    commit_on_topic(&project, ".omhl/workflow.toml");

    let gate = run(&project, &["gate-delivery", W1, "--base", "main"]);

    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);
    assert_eq!(review_question(&gate), None);
}

#[test]
fn it_should_keep_the_review_required_when_the_evaluated_commit_turns_it_off_in_a_worktree() {
    let project = project_with_review(true);
    let linked = project.dir().with_extension("linked");
    git_ok(
        project.dir(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/wi-w1__x",
            linked.to_str().expect("utf-8"),
        ],
    );
    fs::write(
        linked.join(".omhl/workflow.toml"),
        "version = 1\n\n[native]\nchecks = []\n",
    )
    .expect("the worktree workflow file is written");
    git_ok(&linked, &["add", "-A"]);
    git_ok(&linked, &["commit", "-q", "-m", "turn the review off"]);

    let gate = cmd(&project, &["gate-delivery", W1, "--base", "main"])
        .cwd(&linked)
        .run();

    let review = review_question(&gate).expect("the review question is in the gate");
    assert_eq!(review["passed"], json!(false));
    fs::remove_dir_all(linked).expect("the worktree is removed");
}

const BROKEN_WORKFLOW: &str = "version = 1\ncolour = true\n";
const REPAIRED_WORKFLOW: &str =
    "# repaired\nversion = 1\n\n[native]\nchecks = [\"checks.config-loosening-review\"]\n";
const RUN_TESTS: &str = "checks.run-tests";

/// A worktree `feat/wi-w1__x` com um commit; `change` roda dentro dela antes do commit.
fn worktree_with_commit(project: &Project, change: impl FnOnce(&Path)) -> PathBuf {
    let linked = project.dir().with_extension("linked");
    git_ok(
        project.dir(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/wi-w1__x",
            linked.to_str().expect("utf-8"),
        ],
    );
    change(&linked);
    git_ok(&linked, &["add", "-A"]);
    git_ok(&linked, &["commit", "-q", "-m", "the pull request"]);

    linked
}

fn write_in(dir: &Path, rel: &str, bytes: impl AsRef<[u8]>) {
    let target = dir.join(rel);
    fs::create_dir_all(target.parent().expect("a parent")).expect("the folder is created");
    let _ = fs::remove_file(&target);
    fs::write(target, bytes).expect("the file is written");
}

/// O arquivo do diretório principal fica ilegível depois do commit do PR: a worktree já tem o seu.
fn break_main_workflow(project: &Project) {
    write_in(project.dir(), ".omhl/workflow.toml", BROKEN_WORKFLOW);
}

fn run_in(project: &Project, linked: &Path, args: &[&str]) -> Output {
    cmd(project, args).cwd(linked).run()
}

fn repair_pr_with(head_workflow: impl FnOnce(&Path)) -> (Project, PathBuf) {
    let project = project_with_review(true);
    let linked = worktree_with_commit(&project, head_workflow);
    break_main_workflow(&project);

    (project, linked)
}

fn assert_refused_as_invalid_workflow(project: &Project, linked: &Path, args: &[&str]) {
    for command in COMMANDS {
        let mut full = vec![command, W1];
        full.extend_from_slice(args);

        assert_refused(&run_in(project, linked, &full), "invalid-workflow-config");
    }
}

#[test]
fn it_should_keep_invalid_workflow_config_for_a_normal_pull_request_when_the_main_file_is_unreadable()
 {
    let (project, linked) = repair_pr_with(|dir| write_in(dir, "src/a.rs", "changed\n"));

    assert_refused_as_invalid_workflow(&project, &linked, &["--base", "main"]);
}

#[test]
fn it_should_evaluate_the_repair_pull_request_with_the_review_required_and_a_warning() {
    let (project, linked) =
        repair_pr_with(|dir| write_in(dir, ".omhl/workflow.toml", REPAIRED_WORKFLOW));

    let gate = run_in(&project, &linked, &["gate-delivery", W1, "--base", "main"]);

    let review = review_question(&gate).expect("the review question is in the gate");
    assert_eq!(review["passed"], json!(false));
    assert!(
        text_of(&gate).to_lowercase().contains("warning"),
        "{}",
        text_of(&gate)
    );
}

#[test]
fn it_should_refuse_the_close_of_the_repair_pull_request_with_close_refused_while_the_review_is_unattested()
 {
    let (project, linked) =
        repair_pr_with(|dir| write_in(dir, ".omhl/workflow.toml", REPAIRED_WORKFLOW));

    let closed = run_in(&project, &linked, &["close", W1, "--base", "main"]);

    assert_refused(&closed, "close-refused");
}

#[test]
fn it_should_close_the_repair_pull_request_once_the_review_is_attested_at_its_head() {
    let project = project_with_review(true);
    let linked = worktree_with_commit(&project, |dir| {
        write_in(dir, ".omhl/workflow.toml", REPAIRED_WORKFLOW)
    });
    let attested = run_in(
        &project,
        &linked,
        &["attest", REVIEW, W1, "--outcome", "passed"],
    );
    assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
    break_main_workflow(&project);

    let closed = run_in(&project, &linked, &["close", W1, "--base", "main"]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
}

#[test]
fn it_should_not_recognize_the_repair_pull_request_without_a_base() {
    let (project, linked) =
        repair_pr_with(|dir| write_in(dir, ".omhl/workflow.toml", REPAIRED_WORKFLOW));

    assert_refused_as_invalid_workflow(&project, &linked, &[]);
}

#[test]
fn it_should_ask_a_native_check_that_is_on_only_in_the_head_version_of_the_workflow_file() {
    let (project, linked) = repair_pr_with(|dir| {
        write_in(
            dir,
            ".omhl/workflow.toml",
            format!("version = 1\n\n[native]\nchecks = [\"{REVIEW}\", \"{RUN_TESTS}\"]\n"),
        )
    });

    let gate = run_in(&project, &linked, &["gate-delivery", W1, "--base", "main"]);

    let asked = text_of(&gate);
    assert!(asked.contains(RUN_TESTS), "{asked}");
}

#[test]
fn it_should_not_recognize_a_repair_whose_head_workflow_file_is_a_symlink() {
    let (project, linked) = repair_pr_with(|dir| {
        write_in(dir, "other.toml", REPAIRED_WORKFLOW);
        let target = dir.join(".omhl/workflow.toml");
        fs::remove_file(&target).expect("the file is removed");
        std::os::unix::fs::symlink("../other.toml", target).expect("the link is created");
    });

    assert_refused_as_invalid_workflow(&project, &linked, &["--base", "main"]);
}

#[test]
fn it_should_not_recognize_a_repair_whose_head_workflow_file_is_larger_than_the_limit() {
    let (project, linked) = repair_pr_with(|dir| {
        let padding = "#".repeat(::omhl::project::layout::EXT_FILE_MAX_BYTES as usize);
        write_in(
            dir,
            ".omhl/workflow.toml",
            format!("{REPAIRED_WORKFLOW}{padding}\n"),
        )
    });

    assert_refused_as_invalid_workflow(&project, &linked, &["--base", "main"]);
}

#[test]
fn it_should_not_recognize_a_repair_whose_head_workflow_file_is_not_utf8() {
    let (project, linked) = repair_pr_with(|dir| {
        let mut bytes = REPAIRED_WORKFLOW.as_bytes().to_vec();
        bytes.extend_from_slice(b"# \xff\xfe\n");
        write_in(dir, ".omhl/workflow.toml", bytes)
    });

    assert_refused_as_invalid_workflow(&project, &linked, &["--base", "main"]);
}

#[test]
fn it_should_not_recognize_a_repair_whose_head_workflow_file_the_parser_refuses() {
    let (project, linked) =
        repair_pr_with(|dir| write_in(dir, ".omhl/workflow.toml", BROKEN_WORKFLOW));

    assert_refused_as_invalid_workflow(&project, &linked, &["--base", "main"]);
}
