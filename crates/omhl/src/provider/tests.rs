use crate::provider::{
    AttestResult, CheckContext, CheckRead, CheckState, CiDecision, CiVerdict, RedCheck, RepoRef,
    Required, Strategy, ci_decision, ci_verdict, repo_from_origin_url,
};

fn widgets() -> RepoRef {
    RepoRef {
        host: "github.com".to_owned(),
        path: vec!["acme".to_owned(), "widgets".to_owned()],
    }
}

fn path_of(url: &str, host: &str) -> Option<Vec<String>> {
    repo_from_origin_url(url, host).map(|repo| repo.path)
}

#[test]
fn it_should_spell_the_slug_as_host_owner_and_repo() {
    let repo = widgets();

    let slug = repo.slug();

    assert_eq!(slug, "github.com/acme/widgets");
}

#[test]
fn it_should_read_owner_and_repo_from_an_https_origin() {
    let path = path_of("https://github.com/acme/widgets.git", "github.com");

    assert_eq!(path, Some(widgets().path));
}

#[test]
fn it_should_read_owner_and_repo_from_an_ssh_origin() {
    let path = path_of("git@github.com:acme/widgets.git", "github.com");

    assert_eq!(path, Some(widgets().path));
}

#[test]
fn it_should_read_nothing_from_an_origin_on_another_host() {
    let path = path_of("https://gitlab.com/acme/widgets.git", "github.com");

    assert_eq!(path, None);
}

#[test]
fn it_should_read_nothing_from_an_origin_with_more_than_two_segments() {
    let path = path_of("https://github.com/acme/group/widgets.git", "github.com");

    assert_eq!(path, None);
}

// Fatia 2: a leitura do origin (CA-022) e a política de CI (CMP-023, CMP-024)

fn repo(host: &str) -> Option<RepoRef> {
    Some(RepoRef {
        host: host.to_owned(),
        path: vec!["o".to_owned(), "r".to_owned()],
    })
}

#[test]
fn it_should_read_owner_repo_and_host_from_every_accepted_origin_form() {
    let table = [
        ("https://github.com/o/r", "github.com"),
        ("https://github.com/o/r.git", "github.com"),
        ("ssh://git@github.com/o/r.git", "github.com"),
        ("git@github.com:o/r.git", "github.com"),
        ("git@ghe.example.com:o/r.git", "ghe.example.com"),
        ("https://ghe.example.com/o/r", "ghe.example.com"),
    ];

    let read: Vec<Option<RepoRef>> = table
        .iter()
        .map(|(url, host)| repo_from_origin_url(url, host))
        .collect();

    let expected: Vec<Option<RepoRef>> = table.iter().map(|(_, host)| repo(host)).collect();
    assert_eq!(read, expected);
}

#[test]
fn it_should_read_nothing_from_a_github_origin_when_the_declared_host_is_another() {
    let read = repo_from_origin_url("https://github.com/o/r.git", "ghe.example.com");

    assert_eq!(read, None);
}

#[test]
fn it_should_read_nothing_from_an_origin_that_is_not_a_repository() {
    let urls = [
        "https://github.com/o",
        "https://github.com/",
        "/srv/git/r.git",
        "github.com",
    ];

    let read: Vec<Option<RepoRef>> = urls
        .iter()
        .map(|url| repo_from_origin_url(url, "github.com"))
        .collect();

    assert_eq!(read, vec![None, None, None, None]);
}

#[test]
fn it_should_spell_each_strategy_as_its_lowercase_name() {
    let strategies = [Strategy::Merge, Strategy::Squash, Strategy::Rebase];

    let names: Vec<&str> = strategies.into_iter().map(Strategy::as_str).collect();

    assert_eq!(names, ["merge", "squash", "rebase"]);
}

fn read_of(states: &[(&str, CheckState)]) -> CheckRead {
    CheckRead {
        head_commit: "a".repeat(40),
        contexts: states
            .iter()
            .map(|(name, state)| CheckContext {
                name: (*name).to_owned(),
                state: *state,
            })
            .collect(),
    }
}

#[test]
fn it_should_give_none_for_an_empty_rollup() {
    let verdict = ci_verdict(&read_of(&[]));

    assert_eq!(verdict, CiVerdict::None);
}

#[test]
fn it_should_give_green_when_every_context_is_green() {
    let read = read_of(&[("build", CheckState::Green), ("lint", CheckState::Green)]);

    let verdict = ci_verdict(&read);

    assert_eq!(verdict, CiVerdict::Green);
}

#[test]
fn it_should_give_running_when_a_context_is_running_and_none_is_red() {
    let read = read_of(&[("build", CheckState::Green), ("e2e", CheckState::Running)]);

    let verdict = ci_verdict(&read);

    assert_eq!(verdict, CiVerdict::Running);
}

#[test]
fn it_should_give_red_naming_the_contexts_that_are_not_green() {
    let read = read_of(&[("build", CheckState::Green), ("lint", CheckState::NotGreen)]);

    let verdict = ci_verdict(&read);

    assert_eq!(verdict, CiVerdict::Red(vec!["lint".to_owned()]));
}

const OWNER_PHRASE: &str = "owner approved merging with lint red";

fn red(name: &str, required: Required) -> RedCheck {
    RedCheck {
        name: name.to_owned(),
        required,
    }
}

fn lint_red() -> CiVerdict {
    CiVerdict::Red(vec!["lint".to_owned()])
}

fn attested(decision: &CiDecision) -> Option<(AttestResult, String)> {
    match decision {
        CiDecision::Attest { result, note } => Some((*result, note.clone())),
        CiDecision::Refuse(_) => None,
    }
}

#[test]
fn it_should_attest_passed_for_a_green_ci() {
    let decision = ci_decision(&CiVerdict::Green, &[], false, None);

    assert_eq!(
        attested(&decision).map(|(result, _)| result),
        Some(AttestResult::Passed)
    );
}

#[test]
fn it_should_attest_passed_with_warning_and_the_no_ci_note_for_none_without_require_ci() {
    let decision = ci_decision(&CiVerdict::None, &[], false, None);

    let (result, note) = attested(&decision).expect("an attestation");
    assert_eq!(result, AttestResult::PassedWithWarning);
    assert!(note.contains("no CI seen at that SHA"), "{note}");
}

#[test]
fn it_should_refuse_with_ci_required_missing_for_none_with_require_ci() {
    let decision = ci_decision(&CiVerdict::None, &[], true, None);

    assert_eq!(decision, CiDecision::Refuse("ci-required-missing"));
}

#[test]
fn it_should_refuse_with_ci_running_while_a_check_runs_even_with_force() {
    let decision = ci_decision(&CiVerdict::Running, &[], false, Some(OWNER_PHRASE));

    assert_eq!(decision, CiDecision::Refuse("ci-running"));
}

#[test]
fn it_should_refuse_with_ci_not_green_for_a_required_red_check_even_with_force() {
    let checks = [red("lint", Required::Yes)];

    let decision = ci_decision(&lint_red(), &checks, false, Some(OWNER_PHRASE));

    assert_eq!(decision, CiDecision::Refuse("ci-not-green"));
}

#[test]
fn it_should_refuse_with_ci_not_green_for_an_unrequired_red_check_without_force() {
    let checks = [red("lint", Required::No)];

    let decision = ci_decision(&lint_red(), &checks, false, None);

    assert_eq!(decision, CiDecision::Refuse("ci-not-green"));
}

#[test]
fn it_should_attest_passed_with_warning_naming_the_red_checks_and_the_owner_phrase_with_force() {
    let checks = [red("lint", Required::No)];

    let decision = ci_decision(&lint_red(), &checks, false, Some(OWNER_PHRASE));

    let (result, note) = attested(&decision).expect("an attestation");
    assert_eq!(result, AttestResult::PassedWithWarning);
    assert!(
        note.contains("lint") && note.contains(OWNER_PHRASE),
        "{note}"
    );
}

#[test]
fn it_should_count_a_red_check_of_unknown_requirement_as_required() {
    let checks = [red("lint", Required::Unknown)];

    let decision = ci_decision(&lint_red(), &checks, false, Some(OWNER_PHRASE));

    assert_eq!(decision, CiDecision::Refuse("ci-not-green"));
}

#[test]
fn it_should_count_a_red_check_the_reading_did_not_classify_as_required() {
    let checks = [red("build", Required::No)];

    let decision = ci_decision(&lint_red(), &checks, false, Some(OWNER_PHRASE));

    assert_eq!(decision, CiDecision::Refuse("ci-not-green"));
}
