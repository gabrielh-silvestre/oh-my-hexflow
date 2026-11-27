use crate::error::{OmhlError, RuleCode};
use hexlog::TypeName;

#[test]
fn it_should_render_each_rule_code_as_its_contract_string() {
    let expected = [
        (RuleCode::IdNamespace, "id-namespace"),
        (RuleCode::UseCloseWorkItem, "use-close-work-item"),
        (RuleCode::InvalidTransition, "invalid-transition"),
        (RuleCode::WorkItemNotFound, "work-item-not-found"),
        (RuleCode::WorkItemClosed, "work-item-closed"),
        (RuleCode::WorkItemTypeUnknown, "work-item-type-unknown"),
        (RuleCode::TypeChangeLoosens, "type-change-loosens"),
        (RuleCode::CheckNotFound, "check-not-found"),
        (RuleCode::AttestationIdMismatch, "attestation-id-mismatch"),
        (RuleCode::AttestationIdTooLong, "attestation-id-too-long"),
        (RuleCode::SnapshotNotHead, "snapshot-not-head"),
        (RuleCode::SnapshotDirty, "snapshot-dirty"),
        (RuleCode::ProbeFailed, "probe-failed"),
        (RuleCode::OutcomeImmutable, "outcome-immutable"),
        (
            RuleCode::ReactivationUnsupported,
            "reactivation-unsupported",
        ),
        (RuleCode::WhereEqTooMany, "where-eq-too-many"),
        (RuleCode::WhereEqValueTooLong, "where-eq-value-too-long"),
        (RuleCode::GateInconsistent, "gate-inconsistent"),
        (RuleCode::SnapshotTreeMismatch, "snapshot-tree-mismatch"),
        (RuleCode::CheckRunAuthor, "check-run-author"),
        (RuleCode::AttestationIncomplete, "attestation-incomplete"),
        (RuleCode::ConfirmationRequired, "confirmation-required"),
        (RuleCode::ConfirmationInvalid, "confirmation-invalid"),
        (RuleCode::KindMissing, "kind-missing"),
        (RuleCode::KindInvalid, "kind-invalid"),
        (RuleCode::OriginInvalid, "origin-invalid"),
        (RuleCode::OriginImmutable, "origin-immutable"),
        (RuleCode::OriginRequired, "origin-required"),
        (RuleCode::RestsOnArchitecture, "rests-on-architecture"),
        (RuleCode::RestsOnTargetType, "rests-on-target-type"),
        (
            RuleCode::GuidelineRetireByCommand,
            "guideline-retire-by-command",
        ),
        (RuleCode::RoleUnknown, "role-unknown"),
        (RuleCode::RoleRecordForbidden, "role-record-forbidden"),
        (RuleCode::RoleRevisionForbidden, "role-revision-forbidden"),
        (
            RuleCode::RoleRetirementForbidden,
            "role-retirement-forbidden",
        ),
        (RuleCode::RoleAnswerForbidden, "role-answer-forbidden"),
        (RuleCode::InvalidSessionId, "invalid-session-id"),
        (RuleCode::ReviewBaseInvalid, "review-base-invalid"),
        (RuleCode::CheckLooseningRefused, "check-loosening-refused"),
    ];

    let rendered: Vec<(RuleCode, &str)> = expected
        .iter()
        .map(|(code, _)| (*code, code.as_str()))
        .collect();

    assert_eq!(rendered, expected);
}

#[test]
fn it_should_wrap_a_hexlog_error_without_changing_it() {
    let hexlog_error = TypeName::parse("Bad").expect_err("an uppercase name is refused");

    let wrapped = OmhlError::from(hexlog_error.clone());

    assert_eq!(wrapped, OmhlError::Hexlog(hexlog_error));
}

#[test]
fn it_should_display_a_rule_error_with_its_code_and_its_pointer_when_there_is_one() {
    let with_pointer = OmhlError::rule(RuleCode::IdNamespace, "/items/0/body/id");
    let without_pointer = OmhlError::rule(RuleCode::GateInconsistent, "");

    let rendered = (with_pointer.to_string(), without_pointer.to_string());

    assert_eq!(
        rendered,
        (
            "id-namespace at /items/0/body/id".to_owned(),
            "gate-inconsistent".to_owned()
        )
    );
}

#[test]
fn it_should_give_every_new_rule_code_a_cause_and_a_next_step() {
    // Lista fechada: cada fatia que cria um código de regra o acrescenta aqui.
    let new_codes = [
        RuleCode::KindMissing,
        RuleCode::KindInvalid,
        RuleCode::OriginInvalid,
        RuleCode::OriginImmutable,
        RuleCode::OriginRequired,
        RuleCode::RestsOnArchitecture,
        RuleCode::RestsOnTargetType,
        RuleCode::GuidelineRetireByCommand,
        RuleCode::RoleUnknown,
        RuleCode::InvalidSessionId,
        RuleCode::CheckLooseningRefused,
    ];

    let incomplete: Vec<&str> = new_codes
        .iter()
        .filter(|code| {
            code.guidance()
                .is_none_or(|guidance| guidance.cause.is_empty() || guidance.next_step.is_empty())
        })
        .map(|code| code.as_str())
        .collect();

    assert_eq!(incomplete, Vec::<&str>::new());
}

#[test]
fn it_should_name_the_outside_claude_code_marker_in_the_cause_of_invalid_session_id() {
    let cause = RuleCode::InvalidSessionId
        .guidance()
        .expect("invalid-session-id has guidance")
        .cause;

    assert!(
        cause.contains(crate::facade::OUTSIDE_CLAUDE_CODE),
        "{cause}"
    );
}

#[test]
fn it_should_cover_both_paths_in_the_next_step_of_origin_required() {
    let next_step = RuleCode::OriginRequired
        .guidance()
        .expect("origin-required has guidance")
        .next_step;

    let covered = (
        next_step.contains("on creation, add origin late to the premise data"),
        next_step.contains("append a new premise with origin late instead"),
    );

    assert_eq!(covered, (true, true));
}
