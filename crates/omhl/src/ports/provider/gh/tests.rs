use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use super::{GhProvider, map_check_run, map_status_context};
use crate::provider::{
    CheckContext, CheckRead, CheckState, ForkParent, MergeCall, PrState, Provider, ProviderError,
    PullRequest, RedCheck, RepoRef, Required, StateRead, Strategy,
};
use crate::runner::{CommandRunner, RunError, RunErrorKind, RunReport, RunSpec};

const LIMIT: Duration = Duration::from_millis(50);

/// Runner local: manda cada `RunSpec` ao `Receiver` do teste e devolve sempre a mesma resposta.
struct Canned {
    answer: Result<RunReport, RunError>,
    sent: Sender<RunSpec>,
}

impl Canned {
    fn new(answer: Result<RunReport, RunError>) -> (Canned, Receiver<RunSpec>) {
        let (sent, seen) = mpsc::channel();

        (Canned { answer, sent }, seen)
    }
}

impl CommandRunner for Canned {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        let _ = self.sent.send(spec.clone());
        self.answer.clone()
    }
}

fn only_spec(seen: &Receiver<RunSpec>) -> RunSpec {
    let specs: Vec<RunSpec> = seen.try_iter().collect();
    assert_eq!(specs.len(), 1, "one gh call");

    specs[0].clone()
}

fn report(stdout: &str, timed_out: bool) -> Result<RunReport, RunError> {
    Ok(RunReport {
        exit_code: Some(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out,
        truncated: false,
        duration: Duration::ZERO,
    })
}

fn widgets() -> RepoRef {
    RepoRef {
        host: "github.com".to_owned(),
        path: vec!["acme".to_owned(), "widgets".to_owned()],
    }
}

fn read(runner: &Canned) -> Result<String, ProviderError> {
    GhProvider::new(runner, "github.com", LIMIT).default_branch(&widgets())
}

#[test]
fn it_should_read_the_default_branch_name_from_the_json_of_gh() {
    let (runner, _seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"trunk\"}}\n",
        false,
    ));

    let branch = read(&runner);

    assert!(matches!(branch.as_deref(), Ok("trunk")));
}

#[test]
fn it_should_call_gh_repo_view_with_the_slug_and_the_json_field() {
    let (runner, seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"main\"}}\n",
        false,
    ));

    let _ = read(&runner);

    assert_eq!(
        only_spec(&seen).argv,
        [
            "gh",
            "repo",
            "view",
            "github.com/acme/widgets",
            "--json",
            "defaultBranchRef"
        ]
    );
}

#[test]
fn it_should_give_gh_the_timeout_of_the_constructor() {
    let (runner, seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"main\"}}\n",
        false,
    ));

    let _ = read(&runner);

    assert_eq!(only_spec(&seen).timeout, LIMIT);
}

#[test]
fn it_should_isolate_the_gh_environment() {
    let (runner, seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"main\"}}\n",
        false,
    ));

    let _ = read(&runner);

    let spec = only_spec(&seen);
    for name in ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"] {
        assert!(
            spec.env_remove.iter().any(|removed| removed == name),
            "{name}"
        );
    }
    assert!(!spec.env_remove.iter().any(|removed| removed == "GH_TOKEN"));
    assert!(
        spec.env_set
            .contains(&("GH_PROMPT_DISABLED".to_owned(), "1".to_owned()))
    );
}

#[test]
fn it_should_answer_timeout_when_gh_runs_past_the_limit() {
    let (runner, _seen) = Canned::new(report("", true));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::Timeout)));
}

#[test]
fn it_should_answer_cli_missing_when_gh_is_not_found() {
    let (runner, _seen) = Canned::new(Err(RunError {
        kind: RunErrorKind::NotFound,
        message: "gh not found".to_owned(),
    }));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::CliMissing)));
}

#[test]
fn it_should_answer_malformed_for_a_json_without_the_branch() {
    let (runner, _seen) = Canned::new(report("{}\n", false));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::Malformed)));
}

// Fatia 2: a porta completa (CMP-017, CMP-018, CMP-020, CMP-023, CMP-024, CMP-026, CMP-027)

const HEAD: &str = "1111111111111111111111111111111111111111";
const MOVED: &str = "2222222222222222222222222222222222222222";
const HELP_WITH_PIN: &str = "Usage: gh pr merge [<number> | <url> | <branch>] [flags]\n      --match-head-commit SHA   Commit SHA that the pull request head must match to allow merge\n";
const HELP_WITHOUT_PIN: &str = "Usage: gh pr merge [<number> | <url> | <branch>] [flags]\n  -s, --squash   Squash the commits into one commit and merge it into the base branch\n";

/// Runner local: responde pelo primeiro prefixo de argv que casa (o resto sai 0 sem stdout) e manda cada `RunSpec` ao teste.
struct Scripted {
    answers: Vec<(Vec<String>, Result<RunReport, RunError>)>,
    sent: Sender<RunSpec>,
}

impl Scripted {
    fn new(answers: &[(&[&str], Result<RunReport, RunError>)]) -> (Scripted, Receiver<RunSpec>) {
        let (sent, seen) = mpsc::channel();
        let answers = answers
            .iter()
            .map(|(prefix, answer)| (words(prefix), answer.clone()))
            .collect();

        (Scripted { answers, sent }, seen)
    }
}

impl CommandRunner for Scripted {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        let _ = self.sent.send(spec.clone());
        self.answers
            .iter()
            .find(|(prefix, _)| spec.argv.starts_with(prefix))
            .map(|(_, answer)| answer.clone())
            .unwrap_or_else(|| exited(0, "", ""))
    }
}

fn exited(code: i32, stdout: &str, stderr: &str) -> Result<RunReport, RunError> {
    Ok(RunReport {
        exit_code: Some(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
        truncated: false,
        duration: Duration::ZERO,
    })
}

fn gh(runner: &Scripted) -> GhProvider<'_> {
    GhProvider::new(runner, "github.com", LIMIT)
}

fn argvs(seen: &Receiver<RunSpec>) -> Vec<Vec<String>> {
    seen.try_iter().map(|spec| spec.argv).collect()
}

fn words(argv: &[&str]) -> Vec<String> {
    argv.iter().map(|word| (*word).to_owned()).collect()
}

fn pr_list_json(cross: bool) -> String {
    format!(
        "[{{\"number\":42,\"state\":\"OPEN\",\"headRefOid\":\"{HEAD}\",\"isCrossRepository\":{cross},\"baseRefName\":\"main\",\"autoMergeRequest\":null}}]\n"
    )
}

fn rollup_json(items: &str) -> String {
    format!("{{\"headRefOid\":\"{HEAD}\",\"statusCheckRollup\":[{items}]}}\n")
}

/// Uma página da resposta GraphQL; o `--slurp` junta as páginas num array.
fn graphql_page(oid: &str, nodes: &str, next: bool) -> String {
    format!(
        "{{\"data\":{{\"repository\":{{\"pullRequest\":{{\"commits\":{{\"nodes\":[{{\"commit\":{{\"oid\":\"{oid}\",\"statusCheckRollup\":{{\"contexts\":{{\"pageInfo\":{{\"hasNextPage\":{next},\"endCursor\":\"c1\"}},\"nodes\":[{nodes}]}}}}}}}}]}}}}}}}}}}"
    )
}

fn check_run_node(name: &str, required: &str) -> String {
    format!("{{\"__typename\":\"CheckRun\",\"name\":\"{name}\",\"isRequired\":{required}}}")
}

fn every_call(runner: &Scripted) {
    let provider = gh(runner);
    let repo = widgets();
    let _ = provider.preflight();
    let _ = provider.default_branch(&repo);
    let _ = provider.pull_requests(&repo, "feat/wi-w1__x");
    let _ = provider.checks(&repo, 42);
    let _ = provider.classify_red(&repo, 42, HEAD);
    let _ = provider.merge(&repo, 42, HEAD, Strategy::Merge);
    let _ = provider.state(&repo, 42);
    let _ = provider.fork_parent(&repo);
}

#[test]
fn it_should_isolate_the_environment_of_every_gh_call_of_the_port() {
    let (runner, seen) = Scripted::new(&[(
        &["gh", "pr", "merge", "--help"],
        exited(0, HELP_WITH_PIN, ""),
    )]);

    every_call(&runner);

    let specs: Vec<RunSpec> = seen.try_iter().collect();
    assert!(specs.len() >= 8, "{} calls", specs.len());
    for spec in &specs {
        assert_eq!(spec.argv[0], "gh");
        for name in ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"] {
            assert!(
                spec.env_remove.iter().any(|removed| removed == name),
                "{name} in {:?}",
                spec.argv
            );
        }
        assert!(!spec.env_remove.iter().any(|removed| removed == "GH_TOKEN"));
        assert!(
            spec.env_set
                .contains(&("GH_PROMPT_DISABLED".to_owned(), "1".to_owned()))
        );
        assert_eq!(spec.timeout, LIMIT);
    }
}

#[test]
fn it_should_never_use_the_owner_or_repo_placeholders_of_gh_api() {
    let (runner, seen) = Scripted::new(&[]);

    every_call(&runner);

    let placeholders: Vec<String> = argvs(&seen)
        .concat()
        .into_iter()
        .filter(|word| word.contains("{owner}") || word.contains("{repo}"))
        .collect();
    assert_eq!(placeholders, Vec::<String>::new());
}

#[test]
fn it_should_name_the_target_explicitly_in_every_call_that_reads_a_repository() {
    let (runner, seen) = Scripted::new(&[]);

    every_call(&runner);

    let untargeted: Vec<Vec<String>> = argvs(&seen)
        .into_iter()
        .filter(|argv| argv[1] != "auth" && !argv.iter().any(|word| word == "--help"))
        .filter(|argv| {
            !argv.iter().any(|word| {
                word == "--repo" || word == "github.com/acme/widgets" || word == "--hostname"
            })
        })
        .collect();
    assert_eq!(untargeted, Vec::<Vec<String>>::new());
}

#[test]
fn it_should_check_the_login_for_the_host_and_then_the_merge_help_in_preflight() {
    let (runner, seen) = Scripted::new(&[(
        &["gh", "pr", "merge", "--help"],
        exited(0, HELP_WITH_PIN, ""),
    )]);

    let checked = gh(&runner).preflight();

    assert_eq!(checked, Ok(()));
    assert_eq!(
        argvs(&seen),
        [
            words(&["gh", "auth", "status", "--hostname", "github.com"]),
            words(&["gh", "pr", "merge", "--help"]),
        ]
    );
}

#[test]
fn it_should_answer_cli_missing_in_preflight_when_gh_is_not_found() {
    let (runner, _seen) = Canned::new(Err(RunError {
        kind: RunErrorKind::NotFound,
        message: "gh not found".to_owned(),
    }));

    let checked = GhProvider::new(&runner, "github.com", LIMIT).preflight();

    assert_eq!(checked, Err(ProviderError::CliMissing));
}

#[test]
fn it_should_answer_not_authenticated_when_gh_auth_status_exits_one() {
    let (runner, _seen) = Scripted::new(&[
        (
            &["gh", "auth", "status"],
            exited(
                1,
                "",
                "You are not logged into any GitHub hosts. To log in, run: gh auth login\n",
            ),
        ),
        (
            &["gh", "pr", "merge", "--help"],
            exited(0, HELP_WITH_PIN, ""),
        ),
    ]);

    let checked = gh(&runner).preflight();

    assert_eq!(checked, Err(ProviderError::NotAuthenticated));
}

#[test]
fn it_should_answer_cli_too_old_when_the_merge_help_lacks_match_head_commit() {
    let (runner, _seen) = Scripted::new(&[(
        &["gh", "pr", "merge", "--help"],
        exited(0, HELP_WITHOUT_PIN, ""),
    )]);

    let checked = gh(&runner).preflight();

    assert_eq!(checked, Err(ProviderError::CliTooOld));
}

#[test]
fn it_should_list_the_pull_requests_of_the_head_branch_with_every_state_and_field() {
    let (runner, seen) =
        Scripted::new(&[(&["gh", "pr", "list"], exited(0, &pr_list_json(false), ""))]);

    let _ = gh(&runner).pull_requests(&widgets(), "feat/wi-w1__x");

    assert_eq!(
        argvs(&seen),
        [words(&[
            "gh",
            "pr",
            "list",
            "--repo",
            "github.com/acme/widgets",
            "--head",
            "feat/wi-w1__x",
            "--state",
            "all",
            "--json",
            "number,state,headRefOid,isCrossRepository,baseRefName,autoMergeRequest",
        ])]
    );
}

#[test]
fn it_should_read_a_pull_request_of_the_list_with_its_head_base_and_fork_flag() {
    let (runner, _seen) =
        Scripted::new(&[(&["gh", "pr", "list"], exited(0, &pr_list_json(true), ""))]);

    let listed = gh(&runner).pull_requests(&widgets(), "feat/wi-w1__x");

    assert_eq!(
        listed,
        Ok(vec![PullRequest {
            number: 42,
            state: PrState::Open,
            head_commit: HEAD.to_owned(),
            base_branch: "main".to_owned(),
            cross_repository: true,
            auto_merge: false,
        }])
    );
}

#[test]
fn it_should_read_auto_merge_as_on_when_the_auto_merge_request_is_not_null() {
    let list = format!(
        "[{{\"number\":42,\"state\":\"OPEN\",\"headRefOid\":\"{HEAD}\",\"isCrossRepository\":false,\"baseRefName\":\"main\",\"autoMergeRequest\":{{\"mergeMethod\":\"MERGE\"}}}}]\n"
    );
    let (runner, _seen) = Scripted::new(&[(&["gh", "pr", "list"], exited(0, &list, ""))]);

    let listed = gh(&runner).pull_requests(&widgets(), "feat/wi-w1__x");

    assert!(listed.is_ok_and(|prs| prs[0].auto_merge));
}

#[test]
fn it_should_read_the_checks_with_one_pr_view_of_the_head_and_the_rollup() {
    let (runner, seen) = Scripted::new(&[(&["gh", "pr", "view"], exited(0, &rollup_json(""), ""))]);

    let _ = gh(&runner).checks(&widgets(), 42);

    assert_eq!(
        argvs(&seen),
        [words(&[
            "gh",
            "pr",
            "view",
            "42",
            "--repo",
            "github.com/acme/widgets",
            "--json",
            "headRefOid,statusCheckRollup",
        ])]
    );
}

#[test]
fn it_should_read_an_empty_rollup_as_no_context() {
    let (runner, _seen) =
        Scripted::new(&[(&["gh", "pr", "view"], exited(0, &rollup_json(""), ""))]);

    let read = gh(&runner).checks(&widgets(), 42);

    assert_eq!(
        read,
        Ok(CheckRead {
            head_commit: HEAD.to_owned(),
            contexts: Vec::new(),
        })
    );
}

#[test]
fn it_should_read_the_rollup_json_even_when_gh_pr_view_exits_with_an_error() {
    let rollup = rollup_json(
        "{\"__typename\":\"CheckRun\",\"name\":\"build\",\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\"}",
    );
    let (runner, _seen) = Scripted::new(&[(
        &["gh", "pr", "view"],
        exited(
            1,
            &rollup,
            "no checks reported on the 'feat/wi-w1__x' branch\n",
        ),
    )]);

    let read = gh(&runner).checks(&widgets(), 42);

    assert_eq!(
        read.map(|read| read.contexts),
        Ok(vec![CheckContext {
            name: "build".to_owned(),
            state: CheckState::Green,
        }])
    );
}

#[test]
fn it_should_read_a_status_context_by_its_context_name_and_state() {
    let rollup = rollup_json(
        "{\"__typename\":\"StatusContext\",\"context\":\"ci/legacy\",\"state\":\"ERROR\"}",
    );
    let (runner, _seen) = Scripted::new(&[(&["gh", "pr", "view"], exited(0, &rollup, ""))]);

    let read = gh(&runner).checks(&widgets(), 42);

    assert_eq!(
        read.map(|read| read.contexts),
        Ok(vec![CheckContext {
            name: "ci/legacy".to_owned(),
            state: CheckState::NotGreen,
        }])
    );
}

#[test]
fn it_should_map_each_check_run_value_to_its_verdict() {
    let table = [
        ("SUCCESS", CheckState::Green),
        ("NEUTRAL", CheckState::Green),
        ("SKIPPED", CheckState::Green),
        ("FAILURE", CheckState::NotGreen),
        ("CANCELLED", CheckState::NotGreen),
        ("TIMED_OUT", CheckState::NotGreen),
        ("ACTION_REQUIRED", CheckState::NotGreen),
        ("STALE", CheckState::NotGreen),
        ("STARTUP_FAILURE", CheckState::NotGreen),
        ("QUEUED", CheckState::Running),
        ("IN_PROGRESS", CheckState::Running),
        ("PENDING", CheckState::Running),
        ("EXPECTED", CheckState::Running),
        ("WAITING", CheckState::Running),
        ("REQUESTED", CheckState::Running),
        ("SOMETHING_NEW", CheckState::NotGreen),
    ];

    let mapped: Vec<(&str, CheckState)> = table
        .iter()
        .map(|(value, _)| (*value, map_check_run(value)))
        .collect();

    assert_eq!(mapped, table);
}

#[test]
fn it_should_map_each_status_context_value_to_its_verdict() {
    let table = [
        ("SUCCESS", CheckState::Green),
        ("FAILURE", CheckState::NotGreen),
        ("ERROR", CheckState::NotGreen),
        ("PENDING", CheckState::Running),
        ("EXPECTED", CheckState::Running),
        ("SOMETHING_NEW", CheckState::NotGreen),
    ];

    let mapped: Vec<(&str, CheckState)> = table
        .iter()
        .map(|(value, _)| (*value, map_status_context(value)))
        .collect();

    assert_eq!(mapped, table);
}

#[test]
fn it_should_ask_graphql_for_the_required_flag_with_the_host_and_the_variables() {
    let page = format!(
        "[{}]\n",
        graphql_page(HEAD, &check_run_node("lint", "false"), false)
    );
    let (runner, seen) = Scripted::new(&[(&["gh", "api", "graphql"], exited(0, &page, ""))]);

    let _ = gh(&runner).classify_red(&widgets(), 42, HEAD);

    let argv = argvs(&seen).remove(0);
    assert_eq!(
        argv[..7],
        words(&[
            "gh",
            "api",
            "graphql",
            "--paginate",
            "--slurp",
            "--hostname",
            "github.com"
        ])
    );
    for pair in [
        ["-f", "owner=acme"],
        ["-f", "name=widgets"],
        ["-F", "number=42"],
    ] {
        assert!(
            argv.windows(2).any(|window| window == pair),
            "{pair:?} in {argv:?}"
        );
    }
    let query = argv
        .iter()
        .find(|word| word.starts_with("query="))
        .expect("a query argument");
    assert!(
        query.replace(' ', "").contains("commits(last:1)"),
        "{query}"
    );
    assert!(query.contains("isRequired(pullRequestNumber"), "{query}");
}

fn graphql_argv_for(owner: &str, name: &str) -> Vec<String> {
    let repo = RepoRef {
        host: "github.com".to_owned(),
        path: vec![owner.to_owned(), name.to_owned()],
    };
    let (runner, seen) = Scripted::new(&[]);
    let _ = gh(&runner).classify_red(&repo, 42, HEAD);

    argvs(&seen).remove(0)
}

fn has_pair(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2)
        .any(|window| window[0] == flag && window[1] == value)
}

#[test]
fn it_should_pass_an_all_digit_owner_and_repo_as_raw_strings() {
    let argv = graphql_argv_for("123", "456");

    assert!(has_pair(&argv, "-f", "owner=123"), "{argv:?}");
    assert!(has_pair(&argv, "-f", "name=456"), "{argv:?}");
}

#[test]
fn it_should_pass_a_repo_named_true_as_a_raw_string() {
    let argv = graphql_argv_for("acme", "true");

    assert!(has_pair(&argv, "-f", "name=true"), "{argv:?}");
}

#[test]
fn it_should_pass_an_owner_starting_with_at_as_a_raw_string_that_reads_no_file() {
    let argv = graphql_argv_for("@etc", "widgets");

    assert!(has_pair(&argv, "-f", "owner=@etc"), "{argv:?}");
    assert!(!has_pair(&argv, "-F", "owner=@etc"), "{argv:?}");
}

#[test]
fn it_should_answer_malformed_for_a_default_branch_starting_with_a_dash() {
    let (runner, _seen) = Canned::new(report(
        "{\"defaultBranchRef\":{\"name\":\"--upload-pack=x\"}}\n",
        false,
    ));

    let branch = read(&runner);

    assert!(matches!(branch, Err(ProviderError::Malformed)));
}

#[test]
fn it_should_classify_a_red_check_as_required_or_not_from_the_graphql_answer() {
    let nodes = format!(
        "{},{}",
        check_run_node("build", "true"),
        check_run_node("lint", "false")
    );
    let page = format!("[{}]\n", graphql_page(HEAD, &nodes, false));
    let (runner, _seen) = Scripted::new(&[(&["gh", "api", "graphql"], exited(0, &page, ""))]);

    let red = gh(&runner).classify_red(&widgets(), 42, HEAD);

    let red = red.expect("the answer is read");
    assert!(red.contains(&RedCheck {
        name: "build".to_owned(),
        required: Required::Yes
    }));
    assert!(red.contains(&RedCheck {
        name: "lint".to_owned(),
        required: Required::No
    }));
}

#[test]
fn it_should_answer_head_moved_when_the_graphql_commit_is_not_the_expected_one() {
    let page = format!(
        "[{}]\n",
        graphql_page(MOVED, &check_run_node("build", "true"), false)
    );
    let (runner, _seen) = Scripted::new(&[(&["gh", "api", "graphql"], exited(0, &page, ""))]);

    let red = gh(&runner).classify_red(&widgets(), 42, HEAD);

    assert_eq!(
        red,
        Err(ProviderError::HeadMoved {
            expected: HEAD.to_owned(),
            seen: MOVED.to_owned(),
        })
    );
}

#[test]
fn it_should_read_every_page_of_more_than_one_hundred_contexts() {
    let first: Vec<String> = (0..100)
        .map(|n| check_run_node(&format!("c{n}"), "false"))
        .collect();
    let pages = format!(
        "[{},{}]\n",
        graphql_page(HEAD, &first.join(","), true),
        graphql_page(HEAD, &check_run_node("c100", "true"), false)
    );
    let (runner, _seen) = Scripted::new(&[(&["gh", "api", "graphql"], exited(0, &pages, ""))]);

    let red = gh(&runner).classify_red(&widgets(), 42, HEAD);

    let red = red.expect("the pages are read");
    assert!(red.contains(&RedCheck {
        name: "c100".to_owned(),
        required: Required::Yes
    }));
    assert!(red.contains(&RedCheck {
        name: "c0".to_owned(),
        required: Required::No
    }));
}

fn merge_argv(strategy: Strategy) -> Vec<String> {
    let (runner, seen) = Scripted::new(&[]);
    let _ = gh(&runner).merge(&widgets(), 42, HEAD, strategy);

    argvs(&seen).remove(0)
}

#[test]
fn it_should_merge_with_the_pinned_head_and_the_merge_flag_by_default() {
    let argv = merge_argv(Strategy::Merge);

    assert_eq!(
        argv,
        words(&[
            "gh",
            "pr",
            "merge",
            "42",
            "--repo",
            "github.com/acme/widgets",
            "--match-head-commit",
            HEAD,
            "--merge",
        ])
    );
}

#[test]
fn it_should_merge_with_the_squash_flag_when_the_strategy_is_squash() {
    let argv = merge_argv(Strategy::Squash);

    assert_eq!(argv.last().map(String::as_str), Some("--squash"));
}

#[test]
fn it_should_merge_with_the_rebase_flag_when_the_strategy_is_rebase() {
    let argv = merge_argv(Strategy::Rebase);

    assert_eq!(argv.last().map(String::as_str), Some("--rebase"));
}

#[test]
fn it_should_never_pass_admin_auto_or_delete_branch_to_the_merge() {
    let strategies = [Strategy::Merge, Strategy::Squash, Strategy::Rebase];

    let argvs: Vec<Vec<String>> = strategies.into_iter().map(merge_argv).collect();

    let forbidden: Vec<&String> = argvs
        .iter()
        .flatten()
        .filter(|word| ["--admin", "--auto", "--delete-branch"].contains(&word.as_str()))
        .collect();
    assert_eq!(forbidden, Vec::<&String>::new());
}

#[test]
fn it_should_hand_back_the_result_and_the_stderr_of_a_failed_merge_without_deciding() {
    let (runner, _seen) = Scripted::new(&[(
        &["gh", "pr", "merge"],
        exited(
            1,
            "",
            "Head branch was modified. Review and try the merge again.\n",
        ),
    )]);

    let call = gh(&runner).merge(&widgets(), 42, HEAD, Strategy::Merge);

    assert_eq!(
        call,
        Ok(MergeCall {
            success: false,
            stderr: "Head branch was modified. Review and try the merge again.\n".to_owned(),
        })
    );
}

#[test]
fn it_should_reread_the_state_with_one_pr_view_of_the_merge_fields() {
    let (runner, seen) = Scripted::new(&[]);

    let _ = gh(&runner).state(&widgets(), 42);

    assert_eq!(
        argvs(&seen),
        [words(&[
            "gh",
            "pr",
            "view",
            "42",
            "--repo",
            "github.com/acme/widgets",
            "--json",
            "state,autoMergeRequest,baseRefName,headRefOid,mergeCommit",
        ])]
    );
}

#[test]
fn it_should_read_a_merged_state_with_its_merge_commit() {
    let state = format!(
        "{{\"state\":\"MERGED\",\"autoMergeRequest\":null,\"baseRefName\":\"main\",\"headRefOid\":\"{HEAD}\",\"mergeCommit\":{{\"oid\":\"{MOVED}\"}}}}\n"
    );
    let (runner, _seen) = Scripted::new(&[(&["gh", "pr", "view"], exited(0, &state, ""))]);

    let read = gh(&runner).state(&widgets(), 42);

    assert_eq!(
        read,
        Ok(StateRead {
            state: PrState::Merged,
            pending: false,
            base_branch: "main".to_owned(),
            head_commit: HEAD.to_owned(),
            merge_commit: Some(MOVED.to_owned()),
        })
    );
}

#[test]
fn it_should_read_an_open_state_with_auto_merge_as_pending() {
    let state = format!(
        "{{\"state\":\"OPEN\",\"autoMergeRequest\":{{\"mergeMethod\":\"MERGE\"}},\"baseRefName\":\"main\",\"headRefOid\":\"{HEAD}\",\"mergeCommit\":null}}\n"
    );
    let (runner, _seen) = Scripted::new(&[(&["gh", "pr", "view"], exited(0, &state, ""))]);

    let read = gh(&runner).state(&widgets(), 42);

    assert!(read.is_ok_and(|read| read.state == PrState::Open && read.pending));
}

#[test]
fn it_should_read_the_fork_parent_with_the_positional_slug() {
    let (runner, seen) = Scripted::new(&[]);

    let _ = gh(&runner).fork_parent(&widgets());

    assert_eq!(
        argvs(&seen),
        [words(&[
            "gh",
            "repo",
            "view",
            "github.com/acme/widgets",
            "--json",
            "isFork,parent"
        ])]
    );
}

#[test]
fn it_should_name_the_parent_of_a_fork_as_owner_and_repo() {
    let fork =
        "{\"isFork\":true,\"parent\":{\"name\":\"widgets\",\"owner\":{\"login\":\"upstream\"}}}\n";
    let (runner, _seen) = Scripted::new(&[(&["gh", "repo", "view"], exited(0, fork, ""))]);

    let read = gh(&runner).fork_parent(&widgets());

    assert_eq!(
        read,
        Ok(ForkParent {
            is_fork: true,
            parent: Some("upstream/widgets".to_owned()),
        })
    );
}
