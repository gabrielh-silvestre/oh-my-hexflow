//! `omhl merge` do merge em diante (fatia 2b: CMP-026 a CMP-030 e CMP-033), com o binário real, um
//! repositório git de fixture e o `gh` falso no `PATH`.

use crate::support;

use serde_json::{Value, json};
use support::gh::{MERGE_COMMIT, check_run, pr, required_pages, rollup, state};
use support::merge::{
    CI_GREEN, MergeFixture, OWNER_NOTE, PR_ID, PROVIDER_ON, RUN_TESTS, W1, code_of, error_of,
    json_line, node, step, step_names,
};
use support::omhl::pull_request_data;

const LOGGING_CHECK: &str = r#"blocking = true
how = "Logs that it ran."
run = ["sh", "-c", "echo check before-merge >> '@ORDER@'"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-merge"
"#;

const GHE: &str = "version = 1\n\n[provider]\nname = \"github\"\nhost = \"ghe.example.com\"\n";
const ALL_STEPS: [&str; 9] = [
    "lookup", "head", "base", "checks", "ci", "gate", "merge", "close", "tidy",
];

fn with_natives(natives: &[&str]) -> String {
    let list = natives
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{PROVIDER_ON}\n[native]\nchecks = [{list}]\n")
}

fn success(output: &support::child::Output) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    json_line(&output.stdout)
}

fn merge_line(fixture: &MergeFixture) -> String {
    fixture
        .gh
        .lines()
        .into_iter()
        .find(|line| line.starts_with("pr merge ") && !line.contains("--help"))
        .expect("the merge is called")
}

fn ci_read(fixture: &MergeFixture) -> bool {
    fixture
        .gh
        .lines()
        .iter()
        .any(|line| line.starts_with("pr view") && line.contains("statusCheckRollup"))
}

fn answer_lint_red_unrequired(fixture: &MergeFixture) {
    let head = fixture.head();
    fixture.gh.answer_json(
        "checks",
        &rollup(
            &head,
            &[
                check_run("build", "COMPLETED", "SUCCESS"),
                check_run("lint", "COMPLETED", "FAILURE"),
            ],
        ),
    );
    fixture
        .gh
        .answer_json("graphql", &required_pages(&head, &[&[("lint", false)]]));
}

/// O PR já `MERGED` no `HEAD` com a base `base`, na busca e na releitura.
fn answer_already_merged(fixture: &MergeFixture, base: &str) {
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(42, "MERGED", &head, base, false, false)]);
    fixture.gh.answer_json(
        "state",
        &state("MERGED", &head, base, Some(MERGE_COMMIT), false),
    );
}

fn pull_request_nodes(fixture: &MergeFixture) -> Vec<Value> {
    fixture.list("pull-request")
}

// CA-026 e CA-038: a sequência inteira e o caminho feliz

#[test]
fn it_should_run_lookup_checks_ci_and_merge_in_that_order() {
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
                || (line.starts_with("gh pr merge ") && !line.contains("--help"))
        })
        .map(|line| line.split(' ').take(3).collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(
        order,
        [
            "gh pr list",
            "check before-merge",
            "gh pr view",
            "gh pr merge"
        ]
    );
}

#[test]
fn it_should_report_every_step_in_order_on_success() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&[]);

    assert_eq!(step_names(&success(&merged)["steps"]), ALL_STEPS);
}

#[test]
fn it_should_close_the_work_item_and_record_the_pull_request_on_the_happy_path() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&[]);

    let report = success(&merged);
    assert_eq!(report["work_item"], json!(W1));
    assert_eq!(report["pull_request"]["id"], json!(PR_ID));
    assert!(
        report["next"].as_str().is_some_and(|next| !next.is_empty()),
        "{report}"
    );
    assert_eq!(fixture.work_item_status(), json!("closed"));
    assert!(fixture.node_data(PR_ID).is_some());
}

#[test]
fn it_should_print_each_step_and_the_next_line_in_the_text_output() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge_text(&[]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    for name in ALL_STEPS {
        assert!(merged.stdout.contains(name), "{name} in {}", merged.stdout);
    }
    assert!(
        merged.stdout.lines().any(|line| line.starts_with("Next: ")),
        "{}",
        merged.stdout
    );
}

// CA-033: o argv do merge

#[test]
fn it_should_merge_with_the_pinned_head_and_the_merge_flag_by_default() {
    let fixture = MergeFixture::new();
    let head = fixture.head();

    let _ = fixture.merge(&[]);

    assert_eq!(
        merge_line(&fixture),
        format!("pr merge 42 --repo github.com/acme/widgets --match-head-commit {head} --merge")
    );
}

#[test]
fn it_should_merge_with_the_squash_flag_when_asked() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&["--squash"]);

    assert!(
        merge_line(&fixture).ends_with(" --squash"),
        "{}",
        merge_line(&fixture)
    );
}

#[test]
fn it_should_merge_with_the_rebase_flag_when_asked() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&["--rebase"]);

    assert!(
        merge_line(&fixture).ends_with(" --rebase"),
        "{}",
        merge_line(&fixture)
    );
}

#[test]
fn it_should_refuse_squash_together_with_rebase_on_the_command_line() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&["--squash", "--rebase"]);

    assert_eq!(merged.code, Some(2), "stderr: {}", merged.stderr);
    assert_eq!(fixture.gh.lines(), Vec::<String>::new());
}

#[test]
fn it_should_never_pass_admin_auto_or_delete_branch_to_gh() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&["--squash"]);

    let forbidden: Vec<String> = fixture
        .gh
        .calls()
        .into_iter()
        .flat_map(|call| call.args)
        .filter(|arg| ["--admin", "--auto", "--delete-branch"].contains(&arg.as_str()))
        .collect();
    assert_eq!(forbidden, Vec::<String>::new());
}

// CA-034: a releitura decide

#[test]
fn it_should_refuse_with_base_mismatch_when_the_reread_shows_a_merge_into_another_base() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture.gh.answer_json(
        "merged",
        &state("MERGED", &head, "release", Some(MERGE_COMMIT), false),
    );

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("pull-request-base-mismatch"));
    assert_eq!(error["context"]["pr_base"], json!("release"));
    assert_eq!(error["context"]["expected_base"], json!("main"));
    let next = error["context"]["next"].as_str().unwrap_or_default();
    assert!(
        next.contains("ask the owner") && !next.contains("change the base"),
        "{next}"
    );
}

#[test]
fn it_should_keep_the_work_item_open_and_the_ci_attestation_when_the_reread_shows_another_base() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture.gh.answer_json(
        "merged",
        &state("MERGED", &head, "release", Some(MERGE_COMMIT), false),
    );

    let _ = fixture.merge(&[]);

    assert_eq!(fixture.work_item_status(), json!("open"));
    assert_eq!(pull_request_nodes(&fixture), Vec::<Value>::new());
    assert_eq!(fixture.ci_attestations().len(), 1);
}

#[test]
fn it_should_answer_merge_pending_and_record_no_pull_request_when_the_reread_is_still_open() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture
        .gh
        .answer_json("merged", &state("OPEN", &head, "main", None, true));

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("merge-pending"));
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains("pending")),
        "{error}"
    );
    assert_eq!(fixture.work_item_status(), json!("open"));
    assert_eq!(pull_request_nodes(&fixture), Vec::<Value>::new());
}

#[test]
fn it_should_refuse_with_merge_refused_when_the_reread_is_closed_without_merge() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture
        .gh
        .answer_json("merged", &state("CLOSED", &head, "main", None, false));

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("merge-refused"));
    assert_eq!(fixture.work_item_status(), json!("open"));
}

#[test]
fn it_should_refuse_with_the_gh_stderr_when_the_call_fails_and_the_pull_request_stays_open() {
    let fixture = MergeFixture::new();
    std::fs::remove_file(fixture.scratch().join("gh-answers/merged")).expect("the flip is removed");
    fixture.gh.exit("merge", 1);
    fixture.gh.stderr(
        "merge",
        "GraphQL: Pull request is not mergeable (mergePullRequest)\n",
    );

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("merge-refused"));
    assert!(
        error.to_string().contains("Pull request is not mergeable"),
        "{error}"
    );
    assert_eq!(fixture.work_item_status(), json!("open"));
}

#[test]
fn it_should_close_when_the_call_fails_but_the_pull_request_is_merged_at_the_evaluated_commit() {
    let fixture = MergeFixture::new();
    fixture.gh.exit("merge", 1);
    fixture.gh.stderr("merge", "error: connection reset\n");

    let merged = fixture.merge(&[]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    assert_eq!(fixture.work_item_status(), json!("closed"));
}

#[test]
fn it_should_refuse_without_closing_when_github_refuses_a_moved_head() {
    let fixture = MergeFixture::new();
    std::fs::remove_file(fixture.scratch().join("gh-answers/merged")).expect("the flip is removed");
    fixture.gh.exit("merge", 1);
    fixture.gh.stderr(
        "merge",
        "Head branch was modified. Review and try the merge again.\n",
    );

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("merge-refused"));
    assert_eq!(fixture.work_item_status(), json!("open"));
}

// CA-035: a gravação única

#[test]
fn it_should_record_the_pull_request_node_with_work_item_strategy_and_merge_commit() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&["--squash"]);

    let data = fixture
        .node_data(PR_ID)
        .expect("the pull request node exists");
    assert_eq!(data["work_item"], json!(W1));
    assert_eq!(data["strategy"], json!("squash"));
    assert_eq!(data["merge_commit"], json!(MERGE_COMMIT));
}

#[test]
fn it_should_record_neither_the_close_nor_the_node_when_the_write_fails_inside() {
    let fixture = MergeFixture::new();
    let head = fixture.head();
    fixture.gh.answer_json(
        "merged",
        &state("MERGED", &head, "main", Some("abc"), false),
    );

    let merged = fixture.merge(&[]);

    assert_eq!(merged.code, Some(1), "stderr: {}", merged.stderr);
    assert_eq!(fixture.work_item_status(), json!("open"));
    assert_eq!(pull_request_nodes(&fixture), Vec::<Value>::new());
}

// CA-036: o nó `pull-request`

#[test]
fn it_should_name_the_node_under_the_work_item_with_the_provider_and_the_number() {
    let fixture = MergeFixture::new();

    let merged = fixture.merge(&[]);

    assert_eq!(
        success(&merged)["pull_request"]["id"],
        json!("work-items.w1.github-42")
    );
}

#[test]
fn it_should_copy_the_title_type_and_closed_gate_of_the_work_item() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    let work_item = fixture.node_data(W1).expect("the work item exists");
    let data = fixture.node_data(PR_ID).expect("the node exists");
    assert_eq!(data["title"], work_item["title"]);
    assert_eq!(data["type"], json!("chore"));
    assert_eq!(data["gate"], work_item["gate"]);
    assert_eq!(data["status"], json!("closed"));
    assert_eq!(data["provider"], json!("github"));
    assert_eq!(data["number"], json!(42));
    assert_eq!(data["branch"], json!(support::merge::W1_BRANCH));
}

#[test]
fn it_should_spell_the_repo_without_the_host_on_github_com() {
    let fixture = MergeFixture::new();

    let _ = fixture.merge(&[]);

    assert_eq!(
        fixture.node_data(PR_ID).expect("the node exists")["repo"],
        json!("acme/widgets")
    );
}

#[test]
fn it_should_spell_the_repo_with_the_host_in_front_on_another_host() {
    let fixture = MergeFixture::with_workflow(Some(GHE));
    fixture.git(&[
        "remote",
        "set-url",
        "origin",
        "https://ghe.example.com/acme/widgets.git",
    ]);

    let _ = fixture.merge(&[]);

    assert_eq!(
        fixture.node_data(PR_ID).expect("the node exists")["repo"],
        json!("ghe.example.com/acme/widgets")
    );
}

#[test]
fn it_should_keep_the_ids_of_a_work_item_named_like_a_pull_request_and_the_node_apart() {
    let fixture = MergeFixture::new();
    fixture.append(&[node(
        "work-items.w1-pr-42",
        "work-item",
        json!({ "title": "look-alike", "status": "open", "type": "chore" }),
    )]);

    let _ = fixture.merge(&[]);

    assert_eq!(
        fixture
            .node_data("work-items.w1-pr-42")
            .expect("the look-alike stays")["title"],
        json!("look-alike")
    );
    assert!(fixture.node_data(PR_ID).is_some());
}

#[test]
fn it_should_refuse_with_pull_request_record_exists_when_any_node_has_the_computed_id() {
    let fixture = MergeFixture::new();
    fixture.append(&[node(
        PR_ID,
        "pull-request",
        pull_request_data("already here"),
    )]);

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("pull-request-record-exists"));
    assert_eq!(
        fixture.node_data(PR_ID).expect("the node stays")["title"],
        json!("already here")
    );
    assert_eq!(fixture.work_item_status(), json!("open"));
}

// CA-031 e CA-032: o fechamento forçado

#[test]
fn it_should_close_forced_with_the_dlv1_record_when_a_warn_question_fails_with_force() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    let gate = fixture.node_data(W1).expect("the work item exists")["gate"].clone();
    assert_eq!(gate["passed"], json!(false));
    assert_eq!(gate["forced"], json!(true));
    assert_eq!(gate["overridden"], json!([RUN_TESTS]));
    assert_eq!(gate["note"], json!(OWNER_NOTE));
}

#[test]
fn it_should_record_the_same_forced_gate_as_omhl_close_force_on_the_same_snapshot() {
    let by_merge = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));
    let by_close = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));
    let _ = by_merge.merge(&["--force", "--note", OWNER_NOTE]);

    let closed = by_close.run_json(&["close", W1, "--force", "--note", OWNER_NOTE]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let comparable = |fixture: &MergeFixture| {
        let gate = fixture.node_data(W1).expect("the work item exists")["gate"].clone();
        json!([
            gate["name"],
            gate["passed"],
            gate["forced"],
            gate["overridden"],
            gate["note"]
        ])
    };
    assert_eq!(comparable(&by_merge), comparable(&by_close));
}

#[test]
fn it_should_copy_the_forced_gate_into_the_pull_request_node_unchanged() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));

    let _ = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let work_item = fixture.node_data(W1).expect("the work item exists");
    let data = fixture.node_data(PR_ID).expect("the node exists");
    assert_eq!(data["gate"], work_item["gate"]);
}

#[test]
fn it_should_close_unforced_when_force_only_passed_an_unrequired_red_check() {
    let fixture = MergeFixture::new();
    answer_lint_red_unrequired(&fixture);

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    let gate = fixture.node_data(W1).expect("the work item exists")["gate"].clone();
    assert_eq!(gate["passed"], json!(true));
    for key in ["forced", "overridden", "note"] {
        assert!(gate.get(key).is_none(), "{key} in {gate}");
    }
}

#[test]
fn it_should_close_forced_naming_only_gate_labels_when_a_warn_question_also_fails() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));
    answer_lint_red_unrequired(&fixture);

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    let gate = fixture.node_data(W1).expect("the work item exists")["gate"].clone();
    assert_eq!(gate["forced"], json!(true));
    assert_eq!(gate["overridden"], json!([RUN_TESTS]));
}

#[test]
fn it_should_name_the_red_checks_in_the_ci_step_and_the_overridden_labels_in_the_gate_step() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));
    answer_lint_red_unrequired(&fixture);

    let merged = fixture.merge(&["--force", "--note", OWNER_NOTE]);

    let steps = success(&merged)["steps"].clone();
    assert_eq!(step(&steps, "ci")["forced"], json!(["lint"]));
    assert_eq!(step(&steps, "gate")["overridden"], json!([RUN_TESTS]));
}

// CA-037: a retomada

#[test]
fn it_should_skip_the_merge_call_and_close_a_pull_request_already_merged_at_the_evaluated_commit() {
    let fixture = MergeFixture::new();
    answer_already_merged(&fixture, "main");

    let merged = fixture.merge(&[]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    assert!(!fixture.gh.merge_called());
    assert_eq!(fixture.work_item_status(), json!("closed"));
}

#[test]
fn it_should_not_reread_the_ci_nor_rerun_the_checks_on_a_resume_after_the_merge() {
    let fixture = MergeFixture::with_checks(Some(PROVIDER_ON), &[("order", LOGGING_CHECK)]);
    fixture.answer_open_pull_request();
    std::fs::remove_file(fixture.scratch().join("gh-answers/merged")).expect("the flip is removed");
    let first = fixture.merge(&[]);
    assert_eq!(code_of(&first), json!("merge-pending"));
    let checks_before = fixture
        .gh
        .order()
        .iter()
        .filter(|line| line.starts_with("check"))
        .count();
    let reads_before = fixture
        .gh
        .lines()
        .iter()
        .filter(|line| line.contains("statusCheckRollup"))
        .count();
    answer_already_merged(&fixture, "main");

    let resumed = fixture.merge(&[]);

    assert_eq!(resumed.code, Some(0), "stderr: {}", resumed.stderr);
    let checks_after = fixture
        .gh
        .order()
        .iter()
        .filter(|line| line.starts_with("check"))
        .count();
    let reads_after = fixture
        .gh
        .lines()
        .iter()
        .filter(|line| line.contains("statusCheckRollup"))
        .count();
    assert_eq!((checks_after, reads_after), (checks_before, reads_before));
}

#[test]
fn it_should_skip_only_the_merge_call_with_auto_merge_on_and_leave_the_ci_attestation() {
    let fixture = MergeFixture::with_checks(Some(PROVIDER_ON), &[("order", LOGGING_CHECK)]);
    let head = fixture.head();
    fixture.answer_pull_requests(&[pr(42, "OPEN", &head, "main", false, true)]);
    fixture
        .gh
        .answer_json("state", &state("OPEN", &head, "main", None, true));

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("merge-pending"));
    assert!(!fixture.gh.merge_called());
    assert_eq!(fixture.ci_attestations().len(), 1);
    assert!(
        fixture
            .list("attestation")
            .iter()
            .any(|item| item["data"]["check"] == "checks.order"),
        "the before-merge check ran"
    );
}

#[test]
fn it_should_call_the_merge_and_report_pending_for_a_queued_pull_request_without_auto_merge() {
    let fixture = MergeFixture::new();
    std::fs::remove_file(fixture.scratch().join("gh-answers/merged")).expect("the flip is removed");
    fixture
        .gh
        .stderr("merge", "! Pull request #42 is already queued to merge\n");

    let merged = fixture.merge(&[]);

    assert_eq!(code_of(&merged), json!("merge-pending"));
    assert!(fixture.gh.merge_called());
    assert_eq!(fixture.work_item_status(), json!("open"));
}

#[test]
fn it_should_refuse_a_resume_of_a_pull_request_merged_into_another_base_without_recording() {
    let fixture = MergeFixture::new();
    answer_already_merged(&fixture, "release");
    let before = fixture.chain();

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("pull-request-base-mismatch"));
    let next = error["context"]["next"].as_str().unwrap_or_default();
    assert!(
        next.contains("ask the owner") && !next.contains("change the base"),
        "{next}"
    );
    assert_eq!(fixture.chain(), before);
    assert_eq!(fixture.work_item_status(), json!("open"));
}

#[test]
fn it_should_refuse_a_second_run_after_success_with_invalid_transition() {
    let fixture = MergeFixture::new();
    let first = fixture.merge(&[]);
    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);

    let second = fixture.merge(&[]);

    assert_eq!(code_of(&second), json!("invalid-transition"));
    assert_eq!(pull_request_nodes(&fixture).len(), 1);
}

#[test]
fn it_should_refuse_a_resume_without_the_ci_attestation_like_close_does() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[CI_GREEN])));
    answer_already_merged(&fixture, "main");

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("close-refused"));
    assert!(error.to_string().contains("omhl close"), "{error}");
    assert_eq!(fixture.work_item_status(), json!("open"));
}

#[test]
fn it_should_let_a_hand_written_attestation_close_a_resume_of_a_merged_pull_request() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[CI_GREEN])));
    answer_already_merged(&fixture, "main");
    fixture.attest(CI_GREEN, "passed");

    let merged = fixture.merge(&[]);

    assert_eq!(merged.code, Some(0), "stderr: {}", merged.stderr);
    assert!(!ci_read(&fixture));
}

// CA-041: uma recusa de cada família no texto e no JSON

fn assert_text_refusal(output: &support::child::Output, code: &str) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(
        output.stderr.starts_with(&format!("✘ {code} · ")),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.lines().count() >= 2,
        "a hint follows: {}",
        output.stderr
    );
}

#[test]
fn it_should_print_a_precondition_refusal_as_code_and_message_with_the_hint() {
    let fixture = MergeFixture::with_workflow(None);

    let merged = fixture.merge_text(&[]);

    assert_text_refusal(&merged, "integration-off");
}

#[test]
fn it_should_print_a_ci_refusal_as_code_and_message_with_the_hint() {
    let fixture = MergeFixture::new();
    fixture.gh.answer_json(
        "checks",
        &rollup(&fixture.head(), &[check_run("e2e", "IN_PROGRESS", "")]),
    );

    let merged = fixture.merge_text(&[]);

    assert_text_refusal(&merged, "ci-running");
}

#[test]
fn it_should_print_a_gate_refusal_as_code_and_message_with_the_hint() {
    let fixture = MergeFixture::with_workflow(Some(&with_natives(&[RUN_TESTS])));

    let merged = fixture.merge_text(&[]);

    assert_text_refusal(&merged, "close-warnings");
}

#[test]
fn it_should_print_a_merge_refusal_as_code_and_message_with_the_hint() {
    let fixture = MergeFixture::new();
    std::fs::remove_file(fixture.scratch().join("gh-answers/merged")).expect("the flip is removed");
    fixture.gh.exit("merge", 1);
    fixture
        .gh
        .stderr("merge", "GraphQL: Pull request is not mergeable\n");

    let merged = fixture.merge_text(&[]);

    assert_text_refusal(&merged, "merge-refused");
}

#[test]
fn it_should_give_each_refusal_family_a_json_error_with_steps_and_next() {
    let fixture = MergeFixture::new();
    fixture.gh.answer_json(
        "checks",
        &rollup(&fixture.head(), &[check_run("e2e", "IN_PROGRESS", "")]),
    );

    let merged = fixture.merge(&[]);

    let error = error_of(&merged);
    assert_eq!(error["code"], json!("ci-running"));
    assert_eq!(
        step_names(&error["context"]["steps"])
            .last()
            .map(String::as_str),
        Some("ci")
    );
    assert!(error["context"]["next"].is_string(), "{error}");
}
