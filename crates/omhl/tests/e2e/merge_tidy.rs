//! A arrumação do clone depois do `omhl merge` (CMP-034 a CMP-037, CA-043 a CA-050), com o binário
//! real, um `origin` local e o `gh` falso.

use crate::support;

use serde_json::{Value, json};
use support::child::Output;
use support::gh::{MERGE_COMMIT, pr, state};
use support::merge::{MergeFixture, PROVIDER_ON, W1_BRANCH, code_of, json_line, step};
use support::tidy::{Remote, TidyFixture};

const STAGES: [&str; 4] = ["worktree", "switch", "fast-forward", "branch"];
const LOCK_REASON: &str = "held by the owner";

fn success(output: &Output) -> Value {
    assert_eq!(
        output.code,
        Some(0),
        "stdout: {} stderr: {}",
        output.stdout,
        output.stderr
    );

    json_line(&output.stdout)
}

fn stage<'v>(report: &'v Value, name: &str) -> &'v Value {
    step(&report["steps"], "tidy")["stages"]
        .as_array()
        .and_then(|stages| stages.iter().find(|stage| stage["stage"] == name))
        .unwrap_or_else(|| panic!("no stage {name} in {report}"))
}

fn result_of(report: &Value, name: &str) -> Value {
    stage(report, name)["result"].clone()
}

fn reason_of(report: &Value, name: &str) -> String {
    let stage = stage(report, name);
    assert_eq!(stage["result"], json!("skipped"), "{stage}");

    stage["reason"].as_str().unwrap_or_default().to_owned()
}

fn current_branch(fixture: &TidyFixture) -> String {
    fixture.git(&["symbolic-ref", "--short", "HEAD"])
}

fn branch_ref() -> String {
    format!("refs/heads/{W1_BRANCH}")
}

fn assert_no_forcing_flag(fixture: &TidyFixture) {
    for (_, argv) in fixture.git_calls() {
        let words: Vec<&str> = argv.split(' ').collect();
        assert!(
            !words
                .iter()
                .any(|word| ["--force", "-f", "-D"].contains(word)),
            "forcing flag in: {argv}"
        );
    }
}

/// A releitura depois do merge com a base dada: `OPEN` quando `open`, senão `MERGED`.
fn answer_reread(fixture: &TidyFixture, open: bool, base: &str) {
    let head = fixture.git(&["rev-parse", W1_BRANCH]);
    let reread = if open {
        state("OPEN", &head, base, None, false)
    } else {
        state("MERGED", &head, base, Some(MERGE_COMMIT), false)
    };
    fixture.merge.gh.answer_json("merged", &reread);
}

// CA-043: a ordem, o diretório principal e a linha `Next:`

#[test]
fn it_should_remove_the_worktree_the_command_runs_from() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    success(&merged);
    assert!(!fixture.cwd.exists());
}

#[test]
fn it_should_end_the_text_output_with_next_cd_to_the_main_directory() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);

    let merged = fixture.merge_text();

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    let last = merged
        .stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    assert_eq!(last, format!("Next: cd {}", fixture.main_dir().display()));
}

#[test]
fn it_should_point_next_to_cd_the_main_directory_in_json() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    assert_eq!(
        success(&merged)["next"],
        json!(format!("cd {}", fixture.main_dir().display()))
    );
}

#[test]
fn it_should_record_the_close_before_removing_the_worktree() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    success(&merged);
    assert_eq!(fixture.status_at_removal().as_deref(), Some("closed"));
}

#[test]
fn it_should_run_every_git_call_after_the_removal_with_dash_c_on_the_main_directory() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    let main = format!("-C {}", fixture.main_dir().display());

    let merged = fixture.merge_json(&[]);

    success(&merged);
    let calls = fixture.git_calls();
    let removal = calls
        .iter()
        .position(|(_, argv)| argv.contains("worktree remove"))
        .expect("the worktree is removed");
    let stray: Vec<&String> = calls[removal..]
        .iter()
        .map(|(_, argv)| argv)
        .filter(|argv| !argv.starts_with(&main))
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

#[test]
fn it_should_refuse_a_command_run_from_the_deleted_directory_with_invalid_project_dir() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    success(&fixture.merge_json(&[]));

    let listed = fixture.omhl_from_deleted(&fixture.cwd, &["list"]);

    assert_eq!(listed.code, Some(1), "stdout: {}", listed.stdout);
    assert_eq!(
        json_line(&listed.stderr)["error"]["code"],
        json!("invalid-project-dir")
    );
}

#[test]
fn it_should_not_tidy_while_the_merge_is_pending() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    answer_reread(&fixture, true, "main");

    let merged = fixture.merge_json(&[]);

    assert_eq!(code_of(&merged), json!("merge-pending"));
    assert!(fixture.cwd.is_dir());
    assert!(!fixture.called("worktree remove"));
}

#[test]
fn it_should_not_tidy_after_a_merge_into_another_base() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    answer_reread(&fixture, false, "release");

    let merged = fixture.merge_json(&[]);

    assert_eq!(merged.code, Some(1), "stdout: {}", merged.stdout);
    assert!(fixture.cwd.is_dir());
    assert!(!fixture.called("worktree remove"));
}

// CA-044: as variantes da remoção

#[test]
fn it_should_find_and_remove_a_worktree_outside_the_default_folder() {
    let fixture = TidyFixture::in_outside_worktree(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "worktree"), json!("done"));
    assert!(!fixture.cwd.exists());
}

#[test]
fn it_should_keep_a_worktree_with_a_modified_file_and_still_close() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.before(
        "before-remove",
        &format!("printf 'x\\n' >> '{}/.gitignore'", fixture.cwd.display()),
    );

    let merged = fixture.merge_json(&[]);

    let reason = reason_of(&success(&merged), "worktree");
    assert!(reason.contains(".gitignore"), "{reason}");
    assert!(reason.contains("ask the user"), "{reason}");
    assert!(fixture.cwd.is_dir());
    assert_eq!(fixture.merge.work_item_status(), json!("closed"));
}

#[test]
fn it_should_keep_a_worktree_with_an_untracked_file_and_still_close() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.before(
        "before-remove",
        &format!("printf 'x\\n' > '{}/notes.txt'", fixture.cwd.display()),
    );

    let merged = fixture.merge_json(&[]);

    let reason = reason_of(&success(&merged), "worktree");
    assert!(reason.contains("notes.txt"), "{reason}");
    assert!(reason.contains("ask the user"), "{reason}");
    assert!(fixture.cwd.join("notes.txt").is_file());
    assert_eq!(fixture.merge.work_item_status(), json!("closed"));
}

#[test]
fn it_should_keep_a_locked_worktree_and_say_the_lock_reason() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    let path = fixture.cwd.display().to_string();
    fixture.git(&["worktree", "lock", "--reason", LOCK_REASON, &path]);

    let merged = fixture.merge_json(&[]);

    let reason = reason_of(&success(&merged), "worktree");
    assert!(reason.contains(LOCK_REASON), "{reason}");
    assert!(fixture.cwd.is_dir());
}

#[test]
fn it_should_never_remove_the_main_worktree() {
    let fixture = TidyFixture::in_main_checkout(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    assert_eq!(
        result_of(&success(&merged), "worktree"),
        json!("not-needed")
    );
    assert!(fixture.main_dir().join(".git").is_dir());
    assert!(!fixture.called("worktree remove"));
}

#[test]
fn it_should_pass_no_forcing_flag_to_git_when_the_worktree_is_dirty() {
    let fixture = TidyFixture::in_default_worktree(Remote::Squash);
    fixture.before(
        "before-remove",
        &format!("printf 'x\\n' > '{}/notes.txt'", fixture.cwd.display()),
    );

    let merged = fixture.merge_json(&[]);

    success(&merged);
    assert_no_forcing_flag(&fixture);
}

// CA-045: os ignorados somem com a worktree

#[test]
fn it_should_remove_a_worktree_that_only_has_an_ignored_file_and_the_file_with_it() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    let env_file = fixture.cwd.join(".env");
    std::fs::write(&env_file, "TOKEN=x\n").expect("the ignored file is written");

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "worktree"), json!("done"));
    assert!(!env_file.exists());
}

// CA-046: o avanço rápido

#[test]
fn it_should_fast_forward_the_integration_in_use_by_the_main_checkout_without_a_merge_commit() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "fast-forward"), json!("done"));
    assert_eq!(fixture.rev("refs/heads/main"), Some(fixture.remote_main()));
}

#[test]
fn it_should_fast_forward_the_integration_not_in_use_by_fetching_into_it() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.git(&["switch", "-q", "-c", "other"]);

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "fast-forward"), json!("done"));
    assert_eq!(fixture.rev("refs/heads/main"), Some(fixture.remote_main()));
}

#[test]
fn it_should_skip_the_fast_forward_of_a_diverged_integration_without_rewriting_it() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.git(&["commit", "-q", "--allow-empty", "-m", "local only"]);
    let before = fixture.rev("refs/heads/main");

    let merged = fixture.merge_json(&[]);

    assert!(!reason_of(&success(&merged), "fast-forward").is_empty());
    assert_eq!(fixture.rev("refs/heads/main"), before);
}

#[test]
fn it_should_skip_the_fast_forward_of_an_integration_in_use_by_another_linked_worktree() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.git(&["switch", "-q", "-c", "other"]);
    let other = fixture.merge.scratch().join("int");
    fixture.git(&[
        "worktree",
        "add",
        "-q",
        &other.display().to_string(),
        "main",
    ]);
    let before = fixture.rev("refs/heads/main");

    let merged = fixture.merge_json(&[]);

    assert!(!reason_of(&success(&merged), "fast-forward").is_empty());
    assert_eq!(fixture.rev("refs/heads/main"), before);
}

#[test]
fn it_should_not_create_a_missing_local_integration_without_a_declared_one() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.git(&["switch", "-q", "-c", "other"]);
    fixture.git(&["branch", "-q", "-D", "main"]);

    let merged = fixture.merge_json(&[]);

    assert!(!reason_of(&success(&merged), "fast-forward").is_empty());
    assert_eq!(fixture.rev("refs/heads/main"), None);
}

#[test]
fn it_should_refuse_before_the_merge_when_the_declared_integration_has_no_local_branch() {
    let workflow = format!("{PROVIDER_ON}\n[branch]\nintegration = \"develop\"\n");
    let fixture = MergeFixture::with_workflow(Some(&workflow));
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(42, "OPEN", &head, "develop", false, false)]);
    fixture
        .gh
        .answer_json("state", &state("OPEN", &head, "develop", None, false));

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("review-base-invalid"));
    assert!(!fixture.gh.merge_called());
}

// CA-047: a troca do checkout principal

#[test]
fn it_should_switch_the_main_checkout_to_the_integration_when_the_work_was_done_there() {
    let fixture = TidyFixture::in_main_checkout(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "switch"), json!("done"));
    assert_eq!(current_branch(&fixture), "main");
}

#[test]
fn it_should_fast_forward_and_delete_the_branch_after_switching_the_main_checkout() {
    let fixture = TidyFixture::in_main_checkout(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    success(&merged);
    assert_eq!(fixture.rev("refs/heads/main"), Some(fixture.remote_main()));
    assert_eq!(fixture.rev(&branch_ref()), None);
}

#[test]
fn it_should_skip_the_switch_and_the_branch_deletion_when_the_tree_blocks_the_switch() {
    let fixture = TidyFixture::in_main_checkout(Remote::MergeCommit);
    fixture.before(
        "before-switch",
        &format!(
            "printf 'x\\n' >> '{}/.gitignore'",
            fixture.main_dir().display()
        ),
    );

    let merged = fixture.merge_json(&[]);

    let report = success(&merged);
    assert!(!reason_of(&report, "switch").is_empty());
    assert!(!reason_of(&report, "branch").is_empty());
    assert_eq!(current_branch(&fixture), W1_BRANCH);
    assert!(fixture.rev(&branch_ref()).is_some());
}

#[test]
fn it_should_leave_the_main_checkout_on_its_branch_when_a_worktree_existed() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.git(&["switch", "-q", "-c", "other"]);

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "switch"), json!("not-needed"));
    assert_eq!(current_branch(&fixture), "other");
}

// CA-048: a exclusão da branch

#[test]
fn it_should_delete_the_branch_after_a_merge_commit() {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);

    let merged = fixture.merge_json(&[]);

    assert_eq!(result_of(&success(&merged), "branch"), json!("done"));
    assert_eq!(fixture.rev(&branch_ref()), None);
}

fn assert_branch_left_with_the_limit(remote: Remote, strategy: &str) {
    let fixture = TidyFixture::in_default_worktree(remote);
    fixture.delete_remote_branch();

    let merged = fixture.merge_json(&[strategy]);

    let reason = reason_of(&success(&merged), "branch");
    assert!(reason.contains("squash"), "{reason}");
    assert!(reason.contains("rebase"), "{reason}");
    assert!(fixture.rev(&branch_ref()).is_some());
}

#[test]
fn it_should_leave_the_branch_with_a_warning_after_squash_with_the_tracking_branch_gone() {
    assert_branch_left_with_the_limit(Remote::Squash, "--squash");
}

#[test]
fn it_should_leave_the_branch_with_a_warning_after_rebase_with_the_tracking_branch_gone() {
    assert_branch_left_with_the_limit(Remote::Rebase, "--rebase");
}

#[test]
fn it_should_delete_the_branch_after_squash_while_the_tracking_branch_is_at_the_tip() {
    let fixture = TidyFixture::in_default_worktree(Remote::Squash);

    let merged = fixture.merge_json(&["--squash"]);

    assert_eq!(result_of(&success(&merged), "branch"), json!("done"));
    assert_eq!(fixture.rev(&branch_ref()), None);
}

#[test]
fn it_should_never_pass_capital_d_to_git_after_squash() {
    let fixture = TidyFixture::in_default_worktree(Remote::Squash);
    fixture.delete_remote_branch();

    let merged = fixture.merge_json(&["--squash"]);

    success(&merged);
    assert_no_forcing_flag(&fixture);
}

// CA-049: as etapas puladas não mudam o fechamento nem o código de saída

/// A worktree suja e a integração local divergida: worktree, avanço e branch pulados.
fn every_stage_refused() -> TidyFixture {
    let fixture = TidyFixture::in_default_worktree(Remote::MergeCommit);
    fixture.git(&["commit", "-q", "--allow-empty", "-m", "local only"]);
    fixture.before(
        "before-remove",
        &format!("printf 'x\\n' > '{}/notes.txt'", fixture.cwd.display()),
    );

    fixture
}

#[test]
fn it_should_list_the_four_stages_with_their_results_in_json() {
    let fixture = every_stage_refused();

    let merged = fixture.merge_json(&[]);

    let report = success(&merged);
    let results: Vec<Value> = STAGES.iter().map(|name| result_of(&report, name)).collect();
    assert_eq!(
        results,
        [
            json!("skipped"),
            json!("not-needed"),
            json!("skipped"),
            json!("skipped")
        ]
    );
}

#[test]
fn it_should_give_a_reason_for_every_skipped_stage() {
    let fixture = every_stage_refused();

    let merged = fixture.merge_json(&[]);

    let report = success(&merged);
    for name in ["worktree", "fast-forward", "branch"] {
        assert!(!reason_of(&report, name).is_empty(), "{name}");
    }
}

#[test]
fn it_should_keep_the_merge_and_the_close_recorded_when_stages_are_skipped() {
    let fixture = every_stage_refused();

    let merged = fixture.merge_json(&[]);

    success(&merged);
    assert_eq!(fixture.merge.work_item_status(), json!("closed"));
    assert_eq!(fixture.merge.list("pull-request").len(), 1);
}

#[test]
fn it_should_print_the_four_stages_and_their_results_in_text() {
    let fixture = every_stage_refused();

    let merged = fixture.merge_text();

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    for word in STAGES.iter().chain(&["skipped", "not-needed"]) {
        assert!(merged.stdout.contains(word), "{word} in {}", merged.stdout);
    }
}

// CA-050: sem modo só de arrumação, e a repetição não toca o clone

#[test]
fn it_should_offer_no_tidy_only_mode_in_the_merge_help() {
    let fixture = MergeFixture::new();

    let help = fixture.omhl_cmd(&["merge", "--help"]).run();

    assert_eq!(help.code, Some(0), "stderr: {}", help.stderr);
    assert!(
        !help.stdout.to_lowercase().contains("tidy"),
        "{}",
        help.stdout
    );
}

#[test]
fn it_should_refuse_a_second_run_with_invalid_transition_without_touching_the_clone() {
    let fixture = TidyFixture::in_main_checkout(Remote::MergeCommit);
    success(&fixture.merge_json(&[]));
    let before = fixture.git_calls().len();

    let second = fixture.merge_json(&[]);

    assert_eq!(code_of(&second), json!("invalid-transition"));
    let touched: Vec<String> = fixture.git_calls()[before..]
        .iter()
        .map(|(_, argv)| argv.clone())
        .filter(|argv| {
            [
                "worktree remove",
                "worktree prune",
                "switch",
                "fetch",
                "merge --ff-only",
                "branch -d",
            ]
            .iter()
            .any(|command| argv.contains(command))
        })
        .collect();
    assert!(touched.is_empty(), "{touched:?}");
}
