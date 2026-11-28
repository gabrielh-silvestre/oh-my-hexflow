//! `omhl merge` até o gate (fatia 2a: CMP-017 a CMP-025), com o binário real, um repositório git de
//! fixture e o `gh` falso no `PATH`.

use crate::support;

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use support::gh::{HELP_WITHOUT_PIN, check_run, pr, required_pages, rollup, status_context};
use support::merge::{
    BROKEN, CI_GREEN, MergeFixture, OWNER_NOTE, PROVIDER_ON, PROVIDER_REQUIRE_CI, RUN_TESTS, TOKEN,
    W1, W1_BRANCH, W2_BRANCH, code_of, error_of, step_names,
};
use support::project::DEVELOP_BRANCH_TABLE;

const LOGGING_CHECK: &str = r#"blocking = true
how = "Logs that it ran."
run = ["sh", "-c", "echo check before-merge >> '@ORDER@'"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-merge"
"#;

const FAILING_CHECK: &str = r#"blocking = true
how = "Always fails."
run = ["sh", "-c", "echo check before-merge >> '@ORDER@'; exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-merge"
"#;

const OTHER_HOST: &str =
    "version = 1\n\n[provider]\nname = \"github\"\nhost = \"ghe.example.com\"\n";

fn declared_develop() -> String {
    format!("{PROVIDER_ON}{DEVELOP_BRANCH_TABLE}")
}

fn with_natives(natives: &[&str]) -> String {
    let list = natives
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{PROVIDER_ON}\n[native]\nchecks = [{list}]\n")
}

/// Repositório com a `develop` declarada e criada no mesmo commit da `main`.
fn develop_fixture() -> MergeFixture {
    let fixture = MergeFixture::with_workflow(Some(&declared_develop()));
    fixture.git(&["branch", "develop", "main"]);

    fixture
}

fn answer_red(fixture: &MergeFixture, red: &[Value]) {
    let head = fixture.head();
    let mut items = vec![check_run("build", "COMPLETED", "SUCCESS")];
    items.extend_from_slice(red);
    fixture.gh.answer_json("checks", &rollup(&head, &items));
}

fn answer_required(fixture: &MergeFixture, contexts: &[(&str, bool)]) {
    let head = fixture.head();
    fixture
        .gh
        .answer_json("graphql", &required_pages(&head, &[contexts]));
}

fn lint_red() -> Value {
    check_run("lint", "COMPLETED", "FAILURE")
}

fn ci_read(fixture: &MergeFixture) -> bool {
    fixture
        .gh
        .lines()
        .iter()
        .any(|line| line.starts_with("pr view") && line.contains("statusCheckRollup"))
}

fn message_of(output: &support::child::Output) -> String {
    error_of(output).to_string()
}

// CA-018 e CA-020: a conferência do `gh` e a integração desligada

#[test]
fn it_should_refuse_with_provider_cli_missing_and_the_install_hint_without_gh_on_the_path() {
    let fixture = MergeFixture::new();
    let path = fixture.path_without_gh();
    let before = fixture.chain();

    let merged = fixture.omhl_cmd_with_path(&["merge", W1], &path).run();

    assert_text_code(&merged, "provider-cli-missing");
    assert!(
        merged.stderr.to_lowercase().contains("install"),
        "{}",
        merged.stderr
    );
    assert_eq!(fixture.chain(), before);
}

/// Saída 1 com a primeira linha `✘ <código> · `: a dica só é conferida no texto, onde ela sempre sai.
fn assert_text_code(output: &support::child::Output, code: &str) {
    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert!(
        output.stderr.starts_with(&format!("✘ {code} · ")),
        "{}",
        output.stderr
    );
}

#[test]
fn it_should_refuse_with_provider_not_authenticated_and_the_login_command_when_auth_status_fails() {
    let fixture = MergeFixture::new();
    fixture.gh.exit("auth", 1);
    fixture.gh.stderr(
        "auth",
        "You are not logged into any GitHub hosts. To log in, run: gh auth login\n",
    );
    let before = fixture.chain();

    let merged = fixture.merge_text(&[]);

    assert_text_code(&merged, "provider-not-authenticated");
    assert!(
        merged
            .stderr
            .contains("gh auth login --hostname github.com"),
        "{}",
        merged.stderr
    );
    assert_eq!(fixture.chain(), before);
    assert!(!fixture.gh.called("pr list"));
}

#[test]
fn it_should_refuse_with_provider_cli_too_old_when_the_merge_help_lacks_match_head_commit() {
    let fixture = MergeFixture::new();
    fixture.gh.answer("help", HELP_WITHOUT_PIN);
    let before = fixture.chain();

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("provider-cli-too-old"));
    assert_eq!(fixture.chain(), before);
    assert!(!fixture.gh.called("pr list"));
}

#[test]
fn it_should_refuse_with_integration_off_without_starting_gh() {
    let fixture = MergeFixture::with_workflow(None);

    let merged = fixture.merge_text(&[]);

    assert_text_code(&merged, "integration-off");
    assert!(merged.stderr.contains("omhl close"), "{}", merged.stderr);
    assert_eq!(fixture.gh.lines(), Vec::<String>::new());
}

#[test]
fn it_should_never_start_gh_from_any_other_command_with_the_integration_off() {
    let fixture = MergeFixture::with_workflow(None);

    for args in [
        &["render"][..],
        &["verify"],
        &["gate-delivery", W1],
        &["check", "run", "--event", "before-push"],
        &["check", "run", "--event", "before-merge"],
        &["close", W1],
    ] {
        let _ = fixture.run_json(args);
    }

    assert_eq!(fixture.gh.lines(), Vec::<String>::new());
}

// CA-021: as precondições, na ordem, sem gravar nada e sem iniciar o `gh`

fn assert_precondition_refusal(
    fixture: &MergeFixture,
    merged: &support::child::Output,
    code: &str,
) {
    assert_eq!(code_of(merged), json!(code), "stderr: {}", merged.stderr);
    assert_eq!(
        fixture.gh.lines(),
        Vec::<String>::new(),
        "gh is not started before its preflight"
    );
}

#[test]
fn it_should_refuse_an_unreadable_main_workflow_with_invalid_workflow_config() {
    let fixture = MergeFixture::new();
    let before = fixture.chain();
    fixture.project.write_workflow(BROKEN);
    fixture.git(&["commit", "-q", "-am", "break the workflow"]);

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "invalid-workflow-config");
    fixture.project.write_workflow(PROVIDER_ON);
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_unknown_work_item_with_work_item_not_found() {
    let fixture = MergeFixture::new();
    let before = fixture.chain();

    let merged = fixture.run_json(&["merge", "work-items.ghost"]);

    assert_precondition_refusal(&fixture, &merged, "work-item-not-found");
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_a_closed_work_item_with_invalid_transition() {
    let fixture = MergeFixture::new();
    let closed = fixture.run_json(&["close", W1]);
    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let before = fixture.chain();

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "invalid-transition");
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_a_detached_head_with_head_detached() {
    let fixture = MergeFixture::new();
    fixture.git(&["switch", "-q", "--detach"]);

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "head-detached");
}

#[test]
fn it_should_refuse_the_branch_of_another_work_item_with_branch_not_work_item() {
    let fixture = MergeFixture::new();
    fixture.git(&["switch", "-q", "-c", W2_BRANCH]);

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "branch-not-work-item");
}

#[test]
fn it_should_refuse_the_main_branch_with_branch_not_work_item() {
    let fixture = MergeFixture::new();
    fixture.git(&["switch", "-q", "main"]);

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "branch-not-work-item");
}

#[test]
fn it_should_refuse_a_modified_tracked_file_with_snapshot_dirty() {
    let fixture = MergeFixture::new();
    let marker = fixture.dir().join(".omhl.toml");
    let text = fs::read_to_string(&marker).expect("the marker is read");
    fs::write(&marker, format!("{text}# edited\n")).expect("the marker is edited");

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "snapshot-dirty");
}

#[test]
fn it_should_refuse_an_untracked_file_with_snapshot_dirty() {
    let fixture = MergeFixture::new();
    fs::write(fixture.dir().join("notes.txt"), "draft\n").expect("the file is written");

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "snapshot-dirty");
}

#[test]
fn it_should_refuse_a_repository_without_origin_with_origin_missing() {
    let fixture = MergeFixture::new();
    fixture.git(&["remote", "remove", "origin"]);

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "origin-missing");
}

#[test]
fn it_should_refuse_an_origin_on_another_host_with_origin_host_mismatch() {
    let fixture = MergeFixture::with_workflow(Some(OTHER_HOST));

    let merged = fixture.merge(&[]);

    assert_precondition_refusal(&fixture, &merged, "origin-host-mismatch");
}

#[test]
fn it_should_read_only_the_origin_when_another_remote_points_elsewhere() {
    let fixture = MergeFixture::new();
    fixture.git(&[
        "remote",
        "add",
        "upstream",
        "https://github.com/someone/else.git",
    ]);

    let _ = fixture.merge(&[]);

    let list = fixture
        .gh
        .lines()
        .into_iter()
        .find(|line| line.starts_with("pr list"))
        .expect("the lookup runs");
    assert!(list.contains("--repo github.com/acme/widgets"), "{list}");
}

// CA-002: as declarações vêm do diretório principal

/// Volta o diretório principal à `main` e põe a branch do work item numa worktree ligada, onde o
/// `workflow.toml` da branch é trocado por `branch_workflow` e commitado.
fn linked_worktree(fixture: &MergeFixture, branch_workflow: &str) -> PathBuf {
    fixture.git(&["switch", "-q", "main"]);
    let worktree = fixture.scratch().join("wt");
    let path = worktree.display().to_string();
    fixture.git(&["worktree", "add", "-q", &path, W1_BRANCH]);
    fs::create_dir_all(worktree.join(".omhl")).expect("the .omhl folder exists");
    fs::write(worktree.join(".omhl/workflow.toml"), branch_workflow)
        .expect("the workflow is written");
    support::git::git_ok(&worktree, &["add", "-A"]);
    support::git::git_ok(&worktree, &["commit", "-q", "-m", "branch workflow"]);

    worktree
}

fn head_of(dir: &std::path::Path) -> String {
    support::git::git_ok(dir, &["rev-parse", "HEAD"])
        .trim()
        .to_owned()
}

#[test]
fn it_should_answer_integration_off_when_the_provider_is_declared_only_in_the_linked_worktree() {
    let fixture = MergeFixture::with_workflow(None);
    let worktree = linked_worktree(&fixture, PROVIDER_ON);

    let merged = fixture
        .omhl_cmd(&["--format", "json", "merge", W1])
        .cwd(&worktree)
        .run();

    assert_eq!(code_of(&merged), json!("integration-off"));
    assert_eq!(fixture.gh.lines(), Vec::<String>::new());
}

#[test]
fn it_should_keep_require_ci_of_the_main_directory_when_the_worktree_copy_drops_it() {
    let fixture = MergeFixture::with_workflow(Some(PROVIDER_REQUIRE_CI));
    let worktree = linked_worktree(&fixture, PROVIDER_ON);
    let head = head_of(&worktree);
    fixture.answer_pull_requests(&[pr(42, "OPEN", &head, "main", false, false)]);
    fixture.gh.answer_json("checks", &rollup(&head, &[]));

    let merged = fixture
        .omhl_cmd(&["--format", "json", "merge", W1])
        .cwd(&worktree)
        .run();

    assert_eq!(code_of(&merged), json!("ci-required-missing"));
}

// CA-019: cada chamada do `gh` com alvo explícito e ambiente isolado

#[test]
fn it_should_isolate_the_environment_of_every_gh_call_of_the_merge() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let calls = fixture.gh.calls();
    assert!(calls.len() >= 5, "{:?}", fixture.gh.lines());
    for call in &calls {
        for name in ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"] {
            assert_eq!(call.env_value(name), None, "{name} in {}", call.line());
        }
        assert_eq!(
            call.env_value("GH_TOKEN").as_deref(),
            Some(TOKEN),
            "{}",
            call.line()
        );
        assert_eq!(
            call.env_value("GH_PROMPT_DISABLED").as_deref(),
            Some("1"),
            "{}",
            call.line()
        );
    }
}

#[test]
fn it_should_name_the_target_explicitly_and_never_use_the_gh_api_placeholders() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let lines = fixture.gh.lines();
    let untargeted: Vec<&String> = lines
        .iter()
        .filter(|line| !line.starts_with("auth status") && !line.contains("--help"))
        .filter(|line| {
            !(line.contains("--repo github.com/acme/widgets")
                || line.starts_with("repo view github.com/acme/widgets")
                || line.contains("--hostname github.com"))
        })
        .collect();
    assert_eq!(untargeted, Vec::<&String>::new());
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("{owner}") || line.contains("{repo}")),
        "{lines:?}"
    );
}

#[test]
fn it_should_pass_the_graphql_variables_as_separate_f_arguments() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let graphql = fixture
        .gh
        .calls()
        .into_iter()
        .find(|call| call.line().starts_with("api graphql"))
        .expect("the required reading runs");
    for pair in [
        ["-f", "owner=acme"],
        ["-f", "name=widgets"],
        ["-F", "number=42"],
    ] {
        assert!(
            graphql.args.windows(2).any(|window| window == pair),
            "{pair:?} in {}",
            graphql.line()
        );
    }
    assert!(graphql.has("--paginate") && graphql.has("--slurp") && graphql.has("--hostname"));
}

// EMD-040: o `omhl merge` mantém as credenciais do Enterprise

#[test]
fn it_should_keep_the_enterprise_tokens_in_the_gh_calls_of_the_merge() {
    let fixture = MergeFixture::new();

    let _ = fixture
        .omhl_cmd(&["--format", "json", "merge", W1])
        .env("GH_ENTERPRISE_TOKEN", "enterprise-token")
        .run();

    let list = fixture
        .gh
        .calls()
        .into_iter()
        .find(|call| call.line().starts_with("pr list"))
        .expect("the lookup runs");
    assert_eq!(
        list.env_value("GH_ENTERPRISE_TOKEN").as_deref(),
        Some("enterprise-token")
    );
}

// EMD-041 e EMD-043: a branch padrão lida que o validador recusa

#[test]
fn it_should_refuse_a_default_branch_that_fails_the_validator_before_any_later_step() {
    let fixture = MergeFixture::with_checks(Some(PROVIDER_ON), &[("order", LOGGING_CHECK)]);
    fixture.answer_open_pull_request();
    fixture.gh.answer_json(
        "default",
        &json!({ "defaultBranchRef": { "name": "--upload-pack=x" } }),
    );
    let git_bin = fixture.scratch().join("git-bin");
    fs::create_dir_all(&git_bin).expect("the folder is created");
    let git_log = fixture.scratch().join("git.log");
    let found = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("sh runs");
    let real = String::from_utf8_lossy(&found.stdout).trim().to_owned();
    support::git::write_executable(
        &git_bin.join("git"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nexec \"{real}\" \"$@\"\n",
            git_log.display()
        ),
    );
    let path = format!(
        "{}:{}",
        fixture.gh.bin().display(),
        support::merge::path_with(&git_bin)
    );
    let before = fixture.chain();

    let merged = fixture
        .omhl_cmd_with_path(&["--format", "json", "merge", W1], &path)
        .run();

    assert_eq!(code_of(&merged), json!("probe-failed"));
    assert!(
        !fixture
            .gh
            .order()
            .iter()
            .any(|line| line.starts_with("check"))
    );
    assert!(!ci_read(&fixture));
    assert!(!fixture.gh.merge_called());
    assert_eq!(fixture.chain(), before);
    let git_calls = fs::read_to_string(&git_log).unwrap_or_default();
    assert!(!git_calls.contains("--upload-pack=x"), "{git_calls}");
}

// CA-023: a busca do PR pela branch

#[test]
fn it_should_look_the_pull_request_up_by_head_branch_with_every_state_and_auto_merge() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    let list = fixture
        .gh
        .lines()
        .into_iter()
        .find(|line| line.starts_with("pr list"))
        .expect("the lookup runs");
    assert!(list.contains(&format!("--head {W1_BRANCH}")), "{list}");
    assert!(list.contains("--state all"), "{list}");
    assert!(list.contains("autoMergeRequest"), "{list}");
}

#[test]
fn it_should_read_the_ci_of_the_chosen_pull_request_by_its_number() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    assert!(
        fixture.gh.called(
            "pr view 42 --repo github.com/acme/widgets --json headRefOid,statusCheckRollup"
        ),
        "{:?}",
        fixture.gh.lines()
    );
}

#[test]
fn it_should_refuse_with_pull_request_not_found_when_no_pull_request_comes_from_the_branch() {
    let fixture = MergeFixture::new();
    fixture.answer_pull_requests(&[]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("pull-request-not-found"));
}

#[test]
fn it_should_refuse_with_pull_request_ambiguous_naming_both_numbers() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture.answer_pull_requests(&[
        pr(42, "OPEN", &head, "main", false, false),
        pr(43, "OPEN", &head, "main", false, false),
    ]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("pull-request-ambiguous"));
    let message = message_of(&merged);
    assert!(
        message.contains("42") && message.contains("43"),
        "{message}"
    );
}

#[test]
fn it_should_drop_a_fork_pull_request_of_the_same_branch_name() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture.answer_pull_requests(&[
        pr(41, "OPEN", &head, "main", true, false),
        pr(42, "OPEN", &head, "main", false, false),
    ]);

    let _ = fixture.merge(&[]);

    assert!(fixture.gh.called("pr view 42 "), "{:?}", fixture.gh.lines());
    assert!(
        !fixture.gh.called("pr view 41 "),
        "{:?}",
        fixture.gh.lines()
    );
}

#[test]
fn it_should_not_take_a_pull_request_merged_at_another_commit_as_a_candidate() {
    let fixture = MergeFixture::new();
    fixture.answer_pull_requests(&[pr(42, "MERGED", &"e".repeat(40), "main", false, false)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("pull-request-not-found"));
}

// CA-024: sem suporte a fork

#[test]
fn it_should_refuse_with_fork_unsupported_naming_the_fork_pull_requests() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(41, "OPEN", &head, "main", true, false)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("fork-unsupported"));
    let message = message_of(&merged);
    assert!(
        message.contains("41") && message.contains("GitHub"),
        "{message}"
    );
    assert!(!fixture.gh.merge_called());
}

#[test]
fn it_should_refuse_with_fork_unsupported_naming_the_parent_when_the_origin_is_a_fork() {
    let fixture = MergeFixture::new();
    fixture.answer_pull_requests(&[]);
    fixture.gh.answer_json(
        "fork",
        &json!({ "isFork": true, "parent": { "name": "widgets", "owner": { "login": "upstream" } } }),
    );

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("fork-unsupported"));
    let message = message_of(&merged);
    assert!(
        message.contains("upstream/widgets") && message.contains("GitHub"),
        "{message}"
    );
    assert!(!fixture.gh.merge_called());
}

#[test]
fn it_should_say_the_merge_stays_on_github_when_no_pull_request_exists_and_the_origin_is_not_a_fork()
 {
    let fixture = MergeFixture::new();
    fixture.answer_pull_requests(&[]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("pull-request-not-found"));
    assert!(message_of(&merged).contains("GitHub"), "{}", merged.stderr);
    assert!(!fixture.gh.merge_called());
}

// CA-025: o `HEAD` empurrado e a base do PR

#[test]
fn it_should_refuse_with_head_not_pushed_for_a_local_commit_before_reading_the_ci() {
    let fixture = MergeFixture::new();
    fixture.commit_locally();

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("head-not-pushed"));
    assert!(!ci_read(&fixture));
}

#[test]
fn it_should_refuse_with_head_not_pushed_for_a_rewritten_commit() {
    let fixture = MergeFixture::new();
    fixture.git(&[
        "commit",
        "-q",
        "--amend",
        "--allow-empty",
        "-m",
        "rewritten",
    ]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("head-not-pushed"));
}

#[test]
fn it_should_go_past_the_head_and_base_steps_when_both_match() {
    let fixture = develop_fixture();
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(42, "OPEN", &head, "develop", false, false)]);

    let _ = fixture.merge(&[]);

    assert!(ci_read(&fixture), "{:?}", fixture.gh.lines());
}

#[test]
fn it_should_refuse_with_pull_request_base_mismatch_naming_both_bases_before_any_later_step() {
    let fixture = MergeFixture::with_checks(Some(&declared_develop()), &[("order", LOGGING_CHECK)]);
    fixture.git(&["branch", "develop", "main"]);
    let before = fixture.chain();

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("pull-request-base-mismatch"));
    assert_eq!(error["context"]["pr_base"], json!("main"));
    assert_eq!(error["context"]["expected_base"], json!("develop"));
    assert!(
        error["context"]["next"]
            .as_str()
            .is_some_and(|next| next.contains("change the base")),
        "{error}"
    );
    assert!(!ci_read(&fixture));
    assert!(!fixture.gh.merge_called());
    assert!(
        !fixture
            .gh
            .order()
            .iter()
            .any(|line| line.starts_with("check"))
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_with_head_not_pushed_when_both_the_head_and_the_base_differ() {
    let fixture = develop_fixture();
    fixture.commit_locally();

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("head-not-pushed"));
}

#[test]
fn it_should_read_the_base_from_the_lookup_without_another_call() {
    let fixture = develop_fixture();

    let _ = fixture.merge(&[]);

    assert_eq!(
        fixture
            .gh
            .lines()
            .iter()
            .map(|line| line.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>(),
        ["auth status", "pr merge", "pr list"]
    );
}

#[test]
fn it_should_expect_the_default_branch_read_by_gh_without_a_declaration() {
    let fixture = MergeFixture::new();
    fixture.gh.answer_json(
        "default",
        &json!({ "defaultBranchRef": { "name": "trunk" } }),
    );

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("pull-request-base-mismatch"));
    assert_eq!(error["context"]["expected_base"], json!("trunk"));
    assert!(
        fixture
            .gh
            .called("repo view github.com/acme/widgets --json defaultBranchRef")
    );
}

#[test]
fn it_should_never_read_origin_head_for_the_expected_base() {
    let fixture = MergeFixture::new();
    fixture.gh.answer_json(
        "default",
        &json!({ "defaultBranchRef": { "name": "trunk" } }),
    );
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(42, "OPEN", &head, "trunk", false, false)]);
    let origin_head = support::git::git_in(
        fixture.dir(),
        &["rev-parse", "--verify", "-q", "refs/remotes/origin/HEAD"],
    );
    assert!(
        !origin_head.status.success(),
        "the fixture has no origin/HEAD"
    );

    let _ = fixture.merge(&[]);

    assert!(
        ci_read(&fixture),
        "the base trunk is accepted: {:?}",
        fixture.gh.lines()
    );
}

// CA-026: a ordem até o gate

#[test]
fn it_should_run_lookup_then_before_merge_checks_then_the_ci_read() {
    let fixture = MergeFixture::with_checks(Some(PROVIDER_ON), &[("order", LOGGING_CHECK)]);
    fixture.answer_open_pull_request();

    let _ = fixture.merge(&[]);

    let order: Vec<String> = fixture
        .gh
        .order()
        .into_iter()
        .filter(|line| {
            line.starts_with("gh pr list")
                || line.starts_with("check")
                || line.contains("statusCheckRollup")
        })
        .map(|line| line.split(' ').take(3).collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(order, ["gh pr list", "check before-merge", "gh pr view"]);
}

#[test]
fn it_should_refuse_a_failing_before_merge_check_before_reading_the_ci() {
    let fixture = MergeFixture::with_checks(Some(PROVIDER_ON), &[("red", FAILING_CHECK)]);
    fixture.answer_open_pull_request();

    let merged = fixture.merge(&[]);

    assert_eq!(merged.code, Some(1), "stderr: {}", merged.stderr);
    assert!(!ci_read(&fixture));
    assert_eq!(fixture.ci_attestations(), Vec::<Value>::new());
}

#[test]
fn it_should_leave_the_gate_and_the_merge_unrun_when_the_ci_refuses() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[check_run("e2e", "IN_PROGRESS", "")]);

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("ci-running"));
    assert!(
        !step_names(&error["context"]["steps"]).contains(&"gate".to_owned()),
        "{error}"
    );
    assert!(!fixture.gh.merge_called());
}

#[test]
fn it_should_leave_the_merge_unrun_when_the_gate_refuses() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));

    let merged = fixture.merge(&[]);

    assert_eq!(merged.code, Some(1), "stderr: {}", merged.stderr);
    assert!(!fixture.gh.merge_called());
}

// CA-027: o veredito de cada valor, lido do JSON e não do código de saída

#[test]
fn it_should_read_the_rollup_json_when_gh_pr_view_exits_with_an_error() {
    let fixture = MergeFixture::new();
    fixture.gh.exit("checks", 1);
    fixture
        .gh
        .stderr("checks", "no checks reported on the branch\n");
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", true)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
}

#[test]
fn it_should_refuse_a_red_status_context_like_a_red_check_run() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[status_context("ci/legacy", "ERROR")]);
    answer_required(&fixture, &[("ci/legacy", true)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
}

// CA-028: a attestation do CI, uma linha da tabela por teste

fn only_ci_attestation(fixture: &MergeFixture) -> Value {
    let attestations = fixture.ci_attestations();
    assert_eq!(attestations.len(), 1, "{attestations:?}");

    attestations[0]["data"].clone()
}

#[test]
fn it_should_attest_passed_for_a_green_ci() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    let data = only_ci_attestation(&fixture);
    assert_eq!(data["outcome"], json!("passed"));
    assert_eq!(data["work_item"], json!(W1));
}

#[test]
fn it_should_attest_passed_with_warning_and_the_no_ci_note_without_require_ci() {
    let fixture = MergeFixture::new();
    fixture
        .gh
        .answer_json("checks", &rollup(&fixture.head(), &[]));

    let _ = fixture.merge(&[]);

    let data = only_ci_attestation(&fixture);
    assert_eq!(data["outcome"], json!("passed_with_warning"));
    assert!(
        data["note"]
            .as_str()
            .is_some_and(|note| note.contains("no CI seen at that SHA")),
        "{data}"
    );
}

#[test]
fn it_should_refuse_with_ci_required_missing_and_record_nothing_with_require_ci() {
    let fixture = MergeFixture::with_workflow(Some(PROVIDER_REQUIRE_CI));
    fixture
        .gh
        .answer_json("checks", &rollup(&fixture.head(), &[]));
    let before = fixture.chain();

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-required-missing"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_with_ci_running_and_record_nothing_while_a_check_runs() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[check_run("e2e", "QUEUED", "")]);
    let before = fixture.chain();

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-running"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_with_ci_not_green_for_a_required_red_check() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", true)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
}

#[test]
fn it_should_keep_refusing_a_required_red_check_with_force() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", true)]);
    let before = fixture.chain();

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_unrequired_red_check_without_force() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
}

#[test]
fn it_should_attest_passed_with_warning_naming_the_red_checks_and_the_note_with_force() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let data = only_ci_attestation(&fixture);
    assert_eq!(data["outcome"], json!("passed_with_warning"));
    let note = data["note"].as_str().unwrap_or_default();
    assert!(note.contains("lint") && note.contains(OWNER_NOTE), "{note}");
}

#[test]
fn it_should_count_a_red_check_the_reading_cannot_classify_as_required() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("build", false)]);

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
}

#[test]
fn it_should_refuse_force_without_note_on_the_command_line() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&["--force"]);

    assert_eq!(merged.code, Some(2), "stderr: {}", merged.stderr);
    assert_eq!(fixture.gh.lines(), Vec::<String>::new());
}

#[test]
fn it_should_refuse_note_without_force_on_the_command_line() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&["--note", OWNER_NOTE]);

    assert_eq!(merged.code, Some(2), "stderr: {}", merged.stderr);
}

// CA-029: a leitura de exigidos

#[test]
fn it_should_not_ask_graphql_for_a_green_rollup() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    assert!(
        !fixture.gh.called("api graphql"),
        "{:?}",
        fixture.gh.lines()
    );
}

#[test]
fn it_should_refuse_with_head_not_pushed_when_the_graphql_commit_differs() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    fixture.gh.answer_json(
        "graphql",
        &required_pages(&"e".repeat(40), &[&[("lint", false)]]),
    );

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(code_of(&merged), json!("head-not-pushed"));
}

#[test]
fn it_should_read_every_page_of_more_than_one_hundred_contexts() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[check_run("c100", "COMPLETED", "FAILURE")]);
    let names: Vec<String> = (0..100).map(|n| format!("c{n}")).collect();
    let first: Vec<(&str, bool)> = names.iter().map(|name| (name.as_str(), true)).collect();
    fixture.gh.answer_json(
        "graphql",
        &required_pages(&fixture.head(), &[&first, &[("c100", false)]]),
    );

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let data = only_ci_attestation(&fixture);
    assert_eq!(
        data["outcome"],
        json!("passed_with_warning"),
        "c100 is read from the second page"
    );
}

// CA-030: as propriedades da attestation

fn gate_question_passed(fixture: &MergeFixture, label: &str) -> bool {
    let gate = fixture.run_json(&["gate-delivery", W1]);
    assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);

    support::merge::json_line(&gate.stdout)["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .find(|question| question["label"] == label)
        .and_then(|question| question["passed"].as_bool())
        .unwrap_or(false)
}

/// Integração ligada com os nativos `run-tests` e `ci-green` ligados: o `run-tests` sem attestation
/// faz o gate recusar, e o merge para logo depois de gravar a attestation do CI.
fn stopped_at_gate() -> MergeFixture {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS, CI_GREEN])));
    let _ = fixture.merge(&[]);

    fixture
}

#[test]
fn it_should_bind_the_attestation_to_the_head_commit() {
    let fixture = stopped_at_gate();

    let at_head = gate_question_passed(&fixture, CI_GREEN);

    assert!(at_head);
}

#[test]
fn it_should_stop_answering_the_gate_question_after_a_new_commit() {
    let fixture = stopped_at_gate();
    fixture.commit_locally();

    let after = gate_question_passed(&fixture, CI_GREEN);

    assert!(!after);
}

#[test]
fn it_should_record_the_attestation_where_the_native_is_off() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    assert_eq!(fixture.ci_attestations().len(), 1);
}

#[test]
fn it_should_record_the_attestation_in_the_shape_of_a_hand_written_one() {
    let fixture = stopped_at_gate();
    let by_merge = only_ci_attestation(&fixture);
    // outro repositório: no mesmo HEAD a escrita à mão revisaria o mesmo nó
    let other = MergeFixture::new();
    other.attest(CI_GREEN, "passed");

    let by_hand = only_ci_attestation(&other);

    let keys = |data: &Value| -> Vec<String> {
        let mut keys: Vec<String> = data
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.retain(|key| key != "note");
        keys.sort();
        keys
    };
    assert_eq!(keys(&by_merge), keys(&by_hand));
}

#[test]
fn it_should_accept_a_hand_written_ci_attestation_with_the_integration_on() {
    let fixture = MergeFixture::new();

    let attested = fixture.run_json(&["attest", CI_GREEN, W1, "--outcome", "passed"]);

    assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
}

#[test]
fn it_should_ignore_an_earlier_hand_written_attestation_and_refuse_a_red_ci() {
    let fixture = MergeFixture::new();
    fixture.attest(CI_GREEN, "passed");
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", true)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
    assert!(ci_read(&fixture));
}

// CA-031: o gate no merge

#[test]
fn it_should_refuse_a_failed_refuse_question_without_a_way_out_by_force() {
    // a revisão de afrouxamento ligada e sem attestation é pergunta `refuse` que o merge não satisfaz
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[
        RUN_TESTS,
        "checks.config-loosening-review",
    ])));

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(code_of(&merged), json!("close-refused"));
    assert!(!fixture.gh.merge_called());
}

#[test]
fn it_should_refuse_a_failed_warn_question_without_force() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("close-warnings"));
    assert!(!fixture.gh.merge_called());
}

#[test]
fn it_should_refuse_a_base_that_does_not_resolve_with_review_base_invalid() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&["--base", "no-such-ref"]);

    assert_eq!(code_of(&merged), json!("review-base-invalid"));
    assert!(!fixture.gh.merge_called());
}

#[test]
fn it_should_refuse_with_review_base_invalid_when_the_declared_integration_is_missing_locally() {
    let fixture = MergeFixture::with_workflow(Some(&declared_develop()));
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(42, "OPEN", &head, "develop", false, false)]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("review-base-invalid"));
    assert!(!fixture.gh.merge_called());
}

// CA-032: o `--force` que só passa um check vermelho não exigido

#[test]
fn it_should_name_the_red_checks_in_the_ci_step_of_the_output() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("ci-not-green"));
    assert!(error.to_string().contains("lint"), "{error}");
}

#[test]
fn it_should_find_the_forced_ci_pass_among_the_passed_with_warning_attestations() {
    let fixture = MergeFixture::new();
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", false)]);

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let warned: Vec<Value> = fixture
        .ci_attestations()
        .into_iter()
        .filter(|item| item["data"]["outcome"] == "passed_with_warning")
        .collect();
    assert_eq!(warned.len(), 1);
}

#[test]
fn it_should_keep_refusing_a_required_red_check_when_the_gate_also_has_a_warn_question() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));
    answer_red(&fixture, &[lint_red()]);
    answer_required(&fixture, &[("lint", true)]);

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(code_of(&merged), json!("ci-not-green"));
}
