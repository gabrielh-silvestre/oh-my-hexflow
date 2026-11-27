use crate::support;

use std::fs;

use serde_json::{Value, json};
use support::child::{Cmd, Output, omhl_json};
use support::git::{git_ok, write_executable};
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const REVIEW: &str = "checks.config-loosening-review";
const COMMANDS: [&str; 2] = ["gate-pr", "close"];

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

    let output = run(&project, &["gate-pr", W1, "--base", "main"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_keep_probe_failed_when_git_fails_to_find_the_merge_base() {
    let project = project_with_review(true);
    commit_on_topic(&project, "src/a.rs");

    let output = with_git_that_fails_on(
        &project,
        &["merge-base", "diff"],
        &["gate-pr", W1, "--base", "main"],
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
        &["gate-pr", W1, "--base", "no-such-ref"],
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

    let output = run(&project, &["gate-pr", W1, "--base", "main"]);

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
