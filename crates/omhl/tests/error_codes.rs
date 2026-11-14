mod support;

use hexlog::TypeName;
use omhl::app::error::AppError;
use omhl::error::{OmhlError, RuleCode};
use omhl::extensions::model::{ExtensionKind, RawExtension};
use omhl::extensions::parse::parse;
use serde_json::json;

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
        (RuleCode::CheckRunAuthor, "check-run-author"),
        (RuleCode::AttestationIncomplete, "attestation-incomplete"),
        (RuleCode::ConfirmationRequired, "confirmation-required"),
        (RuleCode::ConfirmationInvalid, "confirmation-invalid"),
        (RuleCode::KindMissing, "kind-missing"),
        (RuleCode::RestsOnArchitecture, "rests-on-architecture"),
        (RuleCode::RestsOnTargetType, "rests-on-target-type"),
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
fn it_should_keep_the_json_of_a_legacy_refusal_unchanged() {
    let error = OmhlError::rule(RuleCode::IdNamespace, "/items/0/body/id");

    let rendered = AppError::from(error).to_json();

    assert_eq!(
        rendered,
        json!({
            "error": {
                "code": "id-namespace",
                "message": "id-namespace at /items/0/body/id",
                "details": [{
                    "path": "/items/0/body/id",
                    "code": "id-namespace",
                    "message": "id-namespace"
                }]
            }
        })
    );
}

#[test]
fn it_should_expose_next_step_only_when_guidance_exists() {
    let guided = OmhlError::rule(RuleCode::KindMissing, "/items/0/body/kind");
    let legacy = OmhlError::rule(RuleCode::IdNamespace, "/items/0/body/id");

    let guided = AppError::from(guided).to_json();
    let legacy = AppError::from(legacy).to_json();

    assert_eq!(
        (
            &guided["error"]["message"],
            &guided["error"]["details"][0]["next_step"],
            legacy["error"]["details"][0].get("next_step"),
        ),
        (
            &json!("the guideline has no kind"),
            &json!("set kind to negocio or arquitetura in the item data and append again"),
            None,
        )
    );
}

#[test]
fn it_should_give_every_new_rule_code_a_cause_and_a_next_step() {
    // Lista fechada: cada fatia que cria um código de regra o acrescenta aqui.
    let new_codes = [
        RuleCode::KindMissing,
        RuleCode::RestsOnArchitecture,
        RuleCode::RestsOnTargetType,
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
fn it_should_give_every_new_app_refusal_and_extension_issue_a_next_step() {
    // Lista fechada: cada fatia que cria uma recusa do app ou um issue de extension a acrescenta aqui.
    let tomls = [
        ("kind-missing", "text = \"x\"\n"),
        ("kind-invalid", "kind = \"x\"\ntext = \"x\"\n"),
    ];

    let incomplete: Vec<&str> = tomls
        .iter()
        .filter(|(code, toml)| {
            let raw = RawExtension {
                kind: ExtensionKind::Guideline,
                slug: "tone".to_owned(),
                toml_path: ".omhl/guidelines/tone.toml".to_owned(),
                toml: (*toml).to_owned(),
                markdown_path: None,
                markdown: None,
            };
            let issues = parse(&raw).expect_err("the extension is refused");

            !issues.iter().any(|issue| {
                issue.code == *code
                    && !issue.message.is_empty()
                    && issue
                        .next_step
                        .is_some_and(|guidance| !guidance.next_step.is_empty())
            })
        })
        .map(|(code, _)| *code)
        .collect();

    assert_eq!(incomplete, Vec::<&str>::new());
}
