use crate::error::{OmhlError, RuleCode};
use crate::workflow::spec::is_digits;

use super::append::NodeItem;

const CONFIRMED_BY: &str = "body/data/confirmed_by";
const CONFIRMED_BY_AUTHOR: &str = "body/data/confirmed_by/by";
const AMENDMENT: &str = "body/data/amendment";
const PERSON_PREFIX: &str = "person:";
const AMENDMENT_PREFIX: &str = "emenda-";

/// Prova humana de `premise`, `guideline` e `divergence`: o `pattern` é proibido no catálogo, então a forma é imposta aqui.
pub(super) fn check_confirmation(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    match node.type_name {
        "premise" => check_premise(node),
        "guideline" => require_confirmation(node),
        "divergence" => check_divergence(node),
        _ => Ok(()),
    }
}

fn check_premise(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    if node.has_field("kind") && !node.has_field("work_item") {
        return Err(node.error(RuleCode::WorkItemNotFound, "body/data/work_item"));
    }
    if node.field("status") == Some("confirmed") {
        require_confirmation(node)
    } else {
        forbid_field(node, "confirmed_by", CONFIRMED_BY)
    }
}

fn check_divergence(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    if node.field("resolution") != Some("spec-stale") {
        forbid_field(node, "confirmed_by", CONFIRMED_BY)?;
        return forbid_field(node, "amendment", AMENDMENT);
    }
    require_confirmation(node)?;
    if !node.has_field("amendment") {
        return Err(node.error(RuleCode::ConfirmationRequired, AMENDMENT));
    }
    if node.field("amendment").is_some_and(is_amendment_id) {
        Ok(())
    } else {
        Err(node.error(RuleCode::ConfirmationInvalid, AMENDMENT))
    }
}

fn require_confirmation(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    let Some(confirmation) = node.data.get("confirmed_by") else {
        return Err(node.error(RuleCode::ConfirmationRequired, CONFIRMED_BY));
    };
    let by_person = confirmation
        .get("by")
        .and_then(|by| by.as_str())
        .and_then(|by| by.strip_prefix(PERSON_PREFIX))
        .is_some_and(|name| !name.is_empty());

    if by_person {
        Ok(())
    } else {
        Err(node.error(RuleCode::ConfirmationInvalid, CONFIRMED_BY_AUTHOR))
    }
}

fn forbid_field(node: &NodeItem<'_>, key: &str, pointer: &str) -> Result<(), OmhlError> {
    if node.has_field(key) {
        Err(node.error(RuleCode::ConfirmationInvalid, pointer))
    } else {
        Ok(())
    }
}

/// `emenda-NN`, sempre com dois dígitos.
fn is_amendment_id(id: &str) -> bool {
    id.strip_prefix(AMENDMENT_PREFIX)
        .is_some_and(|number| is_digits(number, 2))
}
