mod support;

use hexlog::TypeName;
use omhl::error::{OmhlError, RuleCode};

#[test]
fn it_should_render_each_rule_code_as_its_contract_string() {
    let expected = [
        (RuleCode::IdNamespace, "id-namespace"),
        (RuleCode::UseCloseWorkItem, "use-close-work-item"),
        (RuleCode::InvalidTransition, "invalid-transition"),
        (RuleCode::WorkItemNotFound, "work-item-not-found"),
        (RuleCode::WorkItemClosed, "work-item-closed"),
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
