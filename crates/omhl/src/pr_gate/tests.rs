use crate::gate::{GateQuestion, Question};
use crate::pr_gate::{
    GateSnapshots, NATIVE_SNAPSHOT, anchor_questions, pr_gate_questions, required_checks,
};
use crate::test_support::omhl::{PR_7, RUN_TESTS};
use crate::test_support::pr_gate::{
    ANCHORS_CONFIRMED, CI_GREEN, DECISIONS_ANCHORED, NO_OPEN_CONTRADICTION, default_natives,
    mechanical_project_check, native_check, project_check, required_pairs, snapshots,
};
use crate::test_support::{rty, sid, snap, tname};
use hexlog::SnapshotKind::{GitCommit, GitTree};
use hexlog::{FieldName, ListFilter, Scalar, Snapshot, SnapshotKind};

fn field(name: &str) -> FieldName {
    FieldName::parse(name).expect("valid field name")
}

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn at_head() -> GateSnapshots {
    snapshots(&snap('a'), &tree('c'))
}

fn occurred_question(check: &str, snapshot: &Snapshot) -> GateQuestion {
    GateQuestion::new(
        check,
        Question::Occurred {
            select: ListFilter {
                type_name: Some(tname("attestation")),
                id_prefix: None,
                snapshot: Some(snapshot.clone()),
                where_eq: vec![
                    (field("check"), Scalar::Str(check.to_owned())),
                    (field("work_item"), Scalar::Str(PR_7.to_owned())),
                    (field("ok"), Scalar::Bool(true)),
                ],
            },
            min: 1,
        },
    )
}

fn no_open_contradiction_question() -> GateQuestion {
    GateQuestion::new(
        NO_OPEN_CONTRADICTION,
        Question::NoOpenContradiction {
            of: Some(ListFilter {
                type_name: Some(tname("decision")),
                id_prefix: None,
                snapshot: None,
                where_eq: vec![(field("work_item"), Scalar::Str(PR_7.to_owned()))],
            }),
        },
    )
}

// Composição das perguntas e seleção dos checks (puros)

#[test]
fn it_should_build_the_pr_gate_as_one_occurred_per_required_check_then_one_no_open_contradiction() {
    let required = required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)]);
    let expected = vec![
        occurred_question(RUN_TESTS, &snap('a')),
        occurred_question(CI_GREEN, &snap('a')),
        no_open_contradiction_question(),
    ];

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &[]);

    assert_eq!(questions, expected);
}

#[test]
fn it_should_require_only_blocking_native_checks_with_before_commit_or_before_merge_events() {
    let natives = vec![
        native_check(RUN_TESTS, "before-commit", true),
        native_check(CI_GREEN, "before-merge", true),
        native_check(
            "checks.verify-review-comment-anchoring",
            "before-publishing-review-comments",
            true,
        ),
        native_check("checks.advisory", "before-commit", false),
    ];

    let required = required_checks(&natives, &[]);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_require_a_blocking_project_check_with_a_covered_event() {
    let project = [project_check(
        "checks.lint-docs",
        "before-merge",
        true,
        false,
    )];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.lint-docs", GitCommit)
        ])
    );
}

#[test]
fn it_should_not_require_a_non_blocking_project_check() {
    let project = [project_check(
        "checks.lint-docs",
        "before-merge",
        false,
        false,
    )];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_not_require_a_project_check_whose_event_is_not_covered() {
    let project = [project_check(
        "checks.lint-docs",
        "before-deploy",
        true,
        false,
    )];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_not_require_a_shadowed_project_check() {
    // O nativo `run-tests` não é bloqueante; a sombra do projeto, bloqueante, não pode torná-lo obrigatório.
    let natives = vec![
        native_check(RUN_TESTS, "before-commit", false),
        native_check(CI_GREEN, "before-merge", true),
    ];
    let shadow = [project_check(RUN_TESTS, "before-commit", true, true)];

    let required = required_checks(&natives, &shadow);

    assert_eq!(required, required_pairs(&[(CI_GREEN, GitCommit)]));
}

#[test]
fn it_should_ask_for_the_before_commit_attestation_on_the_head_tree() {
    let fmt = mechanical_project_check("checks.fmt", "before-commit", true, false);
    let required = required_checks(&default_natives(), &[fmt]);

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &[]);

    assert_eq!(
        required,
        required_pairs(&[(CI_GREEN, GitCommit), ("checks.fmt", GitTree)])
    );
    assert_eq!(
        questions,
        vec![
            occurred_question(CI_GREEN, &snap('a')),
            occurred_question("checks.fmt", &tree('c')),
            no_open_contradiction_question(),
        ]
    );
}

#[test]
fn it_should_ask_for_the_before_pr_attestation_on_the_head_commit() {
    let lint = mechanical_project_check("checks.lint", "before-pr", true, false);
    let required = required_checks(&default_natives(), &[lint]);

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &[]);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.lint", GitCommit),
        ])
    );
    assert!(questions.contains(&occurred_question("checks.lint", &snap('a'))));
}

#[test]
fn it_should_ask_for_the_before_merge_attestation_on_the_head_commit() {
    let ci = mechanical_project_check("checks.ci", "before-merge", true, false);
    let required = required_checks(&default_natives(), &[ci]);

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &[]);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), ("checks.ci", GitCommit)])
    );
    assert!(questions.contains(&occurred_question("checks.ci", &snap('a'))));
}

#[test]
fn it_should_derive_the_snapshot_kind_of_each_required_check_with_the_shared_decoder() {
    let project = [
        project_check("checks.a-notes", "before-commit", true, false),
        mechanical_project_check("checks.b-fmt", "before-commit", true, false),
        mechanical_project_check("checks.c-lint", "before-pr", true, false),
        mechanical_project_check("checks.d-ci", "before-merge", true, false),
    ];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(
        required,
        required_pairs(&[
            ("checks.a-notes", GitCommit),
            ("checks.b-fmt", GitTree),
            ("checks.c-lint", GitCommit),
            ("checks.d-ci", GitCommit),
        ])
    );
}

#[test]
fn it_should_treat_a_native_check_as_a_git_commit_check_through_the_single_native_constant() {
    let required = required_checks(&default_natives(), &[]);

    assert_eq!(NATIVE_SNAPSHOT, GitCommit);
    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_not_require_run_tests_when_a_mechanical_before_commit_check_exists() {
    let fmt = mechanical_project_check("checks.fmt", "before-commit", true, false);

    let required = required_checks(&default_natives(), &[fmt]);

    assert_eq!(
        required,
        required_pairs(&[(CI_GREEN, GitCommit), ("checks.fmt", GitTree)])
    );
}

#[test]
fn it_should_not_require_ci_green_before_merge_when_a_mechanical_before_merge_check_exists() {
    let ci = mechanical_project_check("checks.ci", "before-merge", true, false);

    let required = required_checks(&default_natives(), &[ci]);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), ("checks.ci", GitCommit)])
    );
}

#[test]
fn it_should_keep_requiring_the_native_checks_when_no_mechanical_check_covers_the_event() {
    let project = [
        project_check("checks.notes", "before-commit", true, false),
        mechanical_project_check("checks.lint", "before-pr", true, false),
        mechanical_project_check(RUN_TESTS, "before-commit", true, true),
    ];

    let required = required_checks(&default_natives(), &project);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.notes", GitCommit),
            ("checks.lint", GitCommit),
        ])
    );
}

#[test]
fn it_should_keep_requiring_run_tests_when_the_only_mechanical_before_commit_check_is_not_blocking()
{
    let fmt = mechanical_project_check("checks.fmt", "before-commit", false, false);

    let required = required_checks(&default_natives(), &[fmt]);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_build_the_decisions_anchored_question_without_a_target_check_and_the_anchors_confirmed_one_with_it()
 {
    let shape: Vec<(String, bool)> = anchor_questions(&sid(PR_7))
        .into_iter()
        .map(|entry| {
            let Question::Anchored { target, .. } = entry.question else {
                panic!("the anchor questions are anchored questions");
            };

            (entry.label, target.is_some())
        })
        .collect();

    assert_eq!(
        shape,
        vec![
            (DECISIONS_ANCHORED.to_owned(), false),
            (ANCHORS_CONFIRMED.to_owned(), true),
        ]
    );
}

#[test]
fn it_should_select_the_decisions_of_the_work_item_resting_on_rests_on_in_both_anchor_questions() {
    let decisions = ListFilter {
        type_name: Some(tname("decision")),
        id_prefix: None,
        snapshot: None,
        where_eq: vec![(field("work_item"), Scalar::Str(PR_7.to_owned()))],
    };

    let questions = anchor_questions(&sid(PR_7));

    let selections: Vec<_> = questions
        .into_iter()
        .map(|entry| match entry.question {
            Question::Anchored { select, by, .. } => (select, by),
            other => panic!("the anchor questions are anchored questions, got {other:?}"),
        })
        .collect();
    assert_eq!(
        selections,
        vec![
            (decisions.clone(), rty("rests-on")),
            (decisions, rty("rests-on"))
        ]
    );
}
