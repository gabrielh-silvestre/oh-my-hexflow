use crate::gate::{GateQuestion, Question, QuestionClass};
use crate::native::NativeBase;
use crate::pr_gate::requirement_questions;
use crate::pr_gate::{
    GateSnapshots, NATIVE_SNAPSHOT, anchor_questions, pr_gate_questions, required_checks,
    spec_digests,
};
use crate::test_support::omhl::{PR_7, RUN_TESTS};
use crate::test_support::pr_gate::{
    ALL_NATIVE_CHECKS, ANCHORS_CONFIRMED, CI_GREEN, DECISIONS_ANCHORED, NO_OPEN_CONTRADICTION,
    all_enabled, default_natives, enabled, mechanical_project_check, native_check, project_check,
    required_pairs, snapshots,
};
use crate::test_support::{rty, sid, snap, tname};
use crate::workflow::config::RequireAtom;
use hexlog::SnapshotKind::{GitCommit, GitTree};
use hexlog::{FieldName, ListFilter, NodeView, Scalar, Snapshot, SnapshotKind};

fn field(name: &str) -> FieldName {
    FieldName::parse(name).expect("valid field name")
}

fn tree(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitTree, &fill.to_string().repeat(40)).expect("valid snapshot")
}

fn at_head() -> GateSnapshots {
    snapshots(&snap('a'), &tree('c'))
}

fn occurred_question(check: &str, snapshot: &Snapshot, class: QuestionClass) -> GateQuestion {
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
        class,
    )
}

fn sealed_question(check: &str, snapshot: &Snapshot, digest: &str) -> GateQuestion {
    GateQuestion::new(
        format!("spec-digest-match:{check}"),
        Question::Occurred {
            select: ListFilter {
                type_name: Some(tname("attestation")),
                id_prefix: None,
                snapshot: Some(snapshot.clone()),
                where_eq: vec![
                    (field("check"), Scalar::Str(check.to_owned())),
                    (field("work_item"), Scalar::Str(PR_7.to_owned())),
                    (field("ok"), Scalar::Bool(true)),
                    (field("spec_digest"), Scalar::Str(digest.to_owned())),
                ],
            },
            min: 1,
        },
        QuestionClass::Refuse,
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
        QuestionClass::Warn,
    )
}

// Composição das perguntas e seleção dos checks (puros)

#[test]
fn it_should_build_the_pr_gate_as_one_occurred_per_required_check_then_one_no_open_contradiction() {
    let required = required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)]);
    let expected = vec![
        occurred_question(RUN_TESTS, &snap('a'), QuestionClass::Warn),
        occurred_question(CI_GREEN, &snap('a'), QuestionClass::Refuse),
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

    let required = required_checks(&natives, &all_enabled(), &[], false);

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

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

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

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

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

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

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

    let required = required_checks(&natives, &all_enabled(), &shadow, false);

    assert_eq!(required, required_pairs(&[(CI_GREEN, GitCommit)]));
}

#[test]
fn it_should_ask_for_the_before_commit_attestation_on_the_head_tree() {
    let project = [mechanical_project_check(
        "checks.fmt",
        "before-commit",
        true,
        false,
    )];
    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &spec_digests(&project));

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.fmt", GitTree)
        ])
    );
    assert_eq!(
        questions,
        vec![
            occurred_question(RUN_TESTS, &snap('a'), QuestionClass::Warn),
            occurred_question(CI_GREEN, &snap('a'), QuestionClass::Refuse),
            occurred_question("checks.fmt", &tree('c'), QuestionClass::Refuse),
            sealed_question("checks.fmt", &tree('c'), &spec_digests(&project)[0].1),
            no_open_contradiction_question(),
        ]
    );
}

#[test]
fn it_should_ask_for_the_before_pr_attestation_on_the_head_commit() {
    let project = [mechanical_project_check(
        "checks.lint",
        "before-pr",
        true,
        false,
    )];
    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &spec_digests(&project));

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.lint", GitCommit),
        ])
    );
    assert!(questions.contains(&occurred_question(
        "checks.lint",
        &snap('a'),
        QuestionClass::Refuse
    )));
}

#[test]
fn it_should_ask_for_the_before_merge_attestation_on_the_head_commit() {
    let project = [mechanical_project_check(
        "checks.ci",
        "before-merge",
        true,
        false,
    )];
    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &spec_digests(&project));

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.ci", GitCommit)
        ])
    );
    assert!(questions.contains(&occurred_question(
        "checks.ci",
        &snap('a'),
        QuestionClass::Refuse
    )));
}

#[test]
fn it_should_derive_the_snapshot_kind_of_each_required_check_with_the_shared_decoder() {
    let project = [
        project_check("checks.a-notes", "before-commit", true, false),
        mechanical_project_check("checks.b-fmt", "before-commit", true, false),
        mechanical_project_check("checks.c-lint", "before-pr", true, false),
        mechanical_project_check("checks.d-ci", "before-merge", true, false),
    ];

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.a-notes", GitCommit),
            ("checks.b-fmt", GitTree),
            ("checks.c-lint", GitCommit),
            ("checks.d-ci", GitCommit),
        ])
    );
}

#[test]
fn it_should_treat_a_native_check_as_a_git_commit_check_through_the_single_native_constant() {
    let required = required_checks(&default_natives(), &all_enabled(), &[], false);

    assert_eq!(NATIVE_SNAPSHOT, GitCommit);
    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_require_run_tests_beside_a_mechanical_before_commit_check() {
    let fmt = mechanical_project_check("checks.fmt", "before-commit", true, false);

    let required = required_checks(&default_natives(), &all_enabled(), &[fmt], false);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.fmt", GitTree)
        ])
    );
}

#[test]
fn it_should_require_ci_green_beside_a_mechanical_before_merge_check() {
    let ci = mechanical_project_check("checks.ci", "before-merge", true, false);

    let required = required_checks(&default_natives(), &all_enabled(), &[ci], false);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.ci", GitCommit)
        ])
    );
}

#[test]
fn it_should_require_a_native_check_beside_a_project_check_of_its_event_mechanical_or_not() {
    let project = [
        project_check("checks.notes", "before-commit", true, false),
        project_check("checks.deploy-notes", "before-merge", true, false),
    ];

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

    assert_eq!(
        required,
        required_pairs(&[
            (RUN_TESTS, GitCommit),
            (CI_GREEN, GitCommit),
            ("checks.notes", GitCommit),
            ("checks.deploy-notes", GitCommit),
        ])
    );
}

#[test]
fn it_should_require_a_non_blocking_project_check_never_and_the_native_beside_it() {
    let project = [mechanical_project_check(
        "checks.fmt",
        "before-commit",
        false,
        false,
    )];

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_require_no_native_check_when_none_is_enabled() {
    let required = required_checks(&default_natives(), &enabled(&[]), &[], false);

    assert!(required.is_empty());
}

#[test]
fn it_should_require_only_the_enabled_native_check_when_each_one_is_enabled_alone() {
    let run_tests = required_checks(&default_natives(), &enabled(&[RUN_TESTS]), &[], false);
    let ci_green = required_checks(&default_natives(), &enabled(&[CI_GREEN]), &[], false);

    assert_eq!(run_tests, required_pairs(&[(RUN_TESTS, GitCommit)]));
    assert_eq!(ci_green, required_pairs(&[(CI_GREEN, GitCommit)]));
}

#[test]
fn it_should_never_require_the_review_anchoring_check_even_when_it_is_enabled() {
    let required = required_checks(
        NativeBase.checks(),
        &enabled(&ALL_NATIVE_CHECKS),
        &[],
        false,
    );

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_keep_the_project_checks_required_with_every_native_check_disabled() {
    let project = [
        project_check("checks.lint-docs", "before-merge", true, false),
        mechanical_project_check("checks.fmt", "before-commit", true, false),
    ];

    let disabled = required_checks(&default_natives(), &enabled(&[]), &project, false);
    let enabled_all = required_checks(&default_natives(), &all_enabled(), &project, false);

    assert_eq!(
        disabled,
        required_pairs(&[("checks.lint-docs", GitCommit), ("checks.fmt", GitTree)])
    );
    assert_eq!(enabled_all[2..], disabled[..]);
}

#[test]
fn it_should_keep_requiring_the_native_checks_when_no_mechanical_check_covers_the_event() {
    let project = [
        project_check("checks.notes", "before-commit", true, false),
        mechanical_project_check("checks.lint", "before-pr", true, false),
        mechanical_project_check(RUN_TESTS, "before-commit", true, true),
    ];

    let required = required_checks(&default_natives(), &all_enabled(), &project, false);

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

    let required = required_checks(&default_natives(), &all_enabled(), &[fmt], false);

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

#[test]
fn it_should_ask_one_requires_question_per_atom_with_whether_it_holds() {
    let questions = requirement_questions(&[
        (RequireAtom::Spec, false),
        (RequireAtom::AgentPremise, true),
    ]);

    let labels: Vec<&str> = questions.iter().map(|entry| entry.label.as_str()).collect();
    assert_eq!(labels, ["requires:spec", "requires:agent-premise"]);
    assert_eq!(
        questions[0].question,
        Question::Requirement { holds: false }
    );
    assert_eq!(questions[1].question, Question::Requirement { holds: true });
}

#[test]
fn it_should_ask_no_requires_question_for_a_type_with_no_atom() {
    assert!(requirement_questions(&[]).is_empty());
}

fn class_of(questions: &[GateQuestion], label: &str) -> QuestionClass {
    questions
        .iter()
        .find(|entry| entry.label == label)
        .unwrap_or_else(|| panic!("no question labeled {label}"))
        .class
}

fn labels(questions: &[GateQuestion]) -> Vec<&str> {
    questions.iter().map(|entry| entry.label.as_str()).collect()
}

fn gate_questions(project: &[NodeView]) -> Vec<GateQuestion> {
    let required = required_checks(&default_natives(), &all_enabled(), project, false);

    pr_gate_questions(&required, &sid(PR_7), &at_head(), &spec_digests(project))
}

#[test]
fn it_should_set_the_class_of_each_question_family_when_it_builds_it() {
    let project = [
        project_check("checks.lint-docs", "before-merge", true, false),
        mechanical_project_check("checks.fmt", "before-commit", true, false),
    ];
    let mut questions = gate_questions(&project);
    questions.extend(anchor_questions(&sid(PR_7)));
    questions.extend(requirement_questions(&[(RequireAtom::Spec, false)]));

    assert_eq!(class_of(&questions, RUN_TESTS), QuestionClass::Warn);
    assert_eq!(class_of(&questions, CI_GREEN), QuestionClass::Refuse);
    assert_eq!(
        class_of(&questions, "checks.lint-docs"),
        QuestionClass::Warn
    );
    assert_eq!(class_of(&questions, "checks.fmt"), QuestionClass::Refuse);
    assert_eq!(
        class_of(&questions, "spec-digest-match:checks.fmt"),
        QuestionClass::Refuse
    );
    assert_eq!(
        class_of(&questions, NO_OPEN_CONTRADICTION),
        QuestionClass::Warn
    );
    assert_eq!(
        class_of(&questions, DECISIONS_ANCHORED),
        QuestionClass::Warn
    );
    assert_eq!(class_of(&questions, ANCHORS_CONFIRMED), QuestionClass::Warn);
    assert_eq!(class_of(&questions, "requires:spec"), QuestionClass::Refuse);
}

#[test]
fn it_should_class_the_config_loosening_review_question_as_refuse_when_it_builds_it() {
    let review = sid("checks.config-loosening-review");
    let required = vec![(review, GitCommit)];

    let questions = pr_gate_questions(&required, &sid(PR_7), &at_head(), &[]);

    assert_eq!(
        class_of(&questions, "checks.config-loosening-review"),
        QuestionClass::Refuse
    );
}

#[test]
fn it_should_class_a_project_check_by_its_run_without_changing_its_label() {
    let text = [project_check("checks.docs", "before-merge", true, false)];
    let mechanical = [mechanical_project_check(
        "checks.docs",
        "before-merge",
        true,
        false,
    )];

    let text_questions = gate_questions(&text);
    let mechanical_questions = gate_questions(&mechanical);

    assert_eq!(
        class_of(&text_questions, "checks.docs"),
        QuestionClass::Warn
    );
    assert_eq!(
        class_of(&mechanical_questions, "checks.docs"),
        QuestionClass::Refuse
    );
}

#[test]
fn it_should_ask_no_native_question_while_its_check_is_off_in_the_native_config() {
    let natives = default_natives();
    let run_tests_only = required_checks(&natives, &enabled(&[RUN_TESTS]), &[], false);
    let ci_green_only = required_checks(&natives, &enabled(&[CI_GREEN]), &[], false);

    let without_ci = pr_gate_questions(&run_tests_only, &sid(PR_7), &at_head(), &[]);
    let without_run_tests = pr_gate_questions(&ci_green_only, &sid(PR_7), &at_head(), &[]);

    assert!(!labels(&without_ci).contains(&CI_GREEN));
    assert!(!labels(&without_run_tests).contains(&RUN_TESTS));
}

const CONFIG_LOOSENING_REVIEW: &str = "checks.config-loosening-review";

#[test]
fn it_should_require_the_review_check_when_the_review_input_says_so() {
    let required = required_checks(
        NativeBase.checks(),
        &enabled(&[CONFIG_LOOSENING_REVIEW]),
        &[],
        true,
    );

    assert_eq!(
        required,
        required_pairs(&[(CONFIG_LOOSENING_REVIEW, GitCommit)])
    );
}

#[test]
fn it_should_leave_the_review_check_out_when_it_is_enabled_but_the_input_says_not_required() {
    let required = required_checks(
        NativeBase.checks(),
        &enabled(&[CONFIG_LOOSENING_REVIEW]),
        &[],
        false,
    );

    assert!(required.is_empty());
}

#[test]
fn it_should_not_ask_the_review_check_through_the_plain_enabled_filter() {
    let on = enabled(&[RUN_TESTS, CI_GREEN, CONFIG_LOOSENING_REVIEW]);

    let required = required_checks(NativeBase.checks(), &on, &[], false);

    assert_eq!(
        required,
        required_pairs(&[(RUN_TESTS, GitCommit), (CI_GREEN, GitCommit)])
    );
}

#[test]
fn it_should_keep_the_other_enabled_natives_and_ask_the_review_once_when_it_is_required() {
    let on = enabled(&[RUN_TESTS, CI_GREEN, CONFIG_LOOSENING_REVIEW]);

    let required = required_checks(NativeBase.checks(), &on, &[], true);

    let ids: Vec<&str> = required.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids.len(), 3, "{ids:?}");
    for expected in [RUN_TESTS, CI_GREEN, CONFIG_LOOSENING_REVIEW] {
        assert!(ids.contains(&expected), "{expected} in {ids:?}");
    }
}
