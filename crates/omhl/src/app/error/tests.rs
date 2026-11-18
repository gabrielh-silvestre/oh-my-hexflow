use crate::app::error::AppError;
use crate::error::{OmhlError, RuleCode};
use serde_json::json;

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
            &json!("set kind to business or architecture in the item data and append again"),
            None,
        )
    );
}
