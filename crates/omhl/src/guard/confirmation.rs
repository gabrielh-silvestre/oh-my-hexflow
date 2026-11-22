use serde_json::Value;

use crate::error::{OmhlError, RuleCode};
use crate::workflow::gap::{CRITIC_CONFIRMATION, confirmer, is_person_confirmer};
use crate::workflow::spec::is_amendment_id;

use super::append::NodeItem;
use super::role::CRITIC;

const CONFIRMED_BY: &str = "body/data/confirmed_by";
const CONFIRMED_BY_AUTHOR: &str = "body/data/confirmed_by/by";
const AMENDMENT: &str = "body/data/amendment";

/// Prova humana de `premise`, `guideline` e `divergence`: o `pattern` é proibido no catálogo, então a forma é imposta aqui.
/// Só o `omhl:critic`, numa premissa técnica, confirma por `critic` em vez de por pessoa.
pub(super) fn check_confirmation(node: &NodeItem<'_>, author: &str) -> Result<(), OmhlError> {
    match node.type_name {
        "premise" => check_premise(node, author),
        "guideline" => require_confirmation(node, false),
        "divergence" => check_divergence(node),
        _ => Ok(()),
    }
}

fn check_premise(node: &NodeItem<'_>, author: &str) -> Result<(), OmhlError> {
    if node.has_field("kind") && !node.has_field("work_item") {
        return Err(node.error(RuleCode::WorkItemNotFound, "body/data/work_item"));
    }
    if node.field("status") == Some("confirmed") {
        let critic_may_confirm = author == CRITIC && node.field("kind") == Some("technical");
        require_confirmation(node, critic_may_confirm)
    } else {
        forbid_field(node, "confirmed_by", CONFIRMED_BY)
    }
}

fn check_divergence(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    if node.field("resolution") != Some("spec-stale") {
        forbid_field(node, "confirmed_by", CONFIRMED_BY)?;
        return forbid_field(node, "amendment", AMENDMENT);
    }
    require_confirmation(node, false)?;
    if !node.has_field("amendment") {
        return Err(node.error(RuleCode::ConfirmationRequired, AMENDMENT));
    }
    if node.field("amendment").is_some_and(is_amendment_id) {
        Ok(())
    } else {
        Err(node.error(RuleCode::ConfirmationInvalid, AMENDMENT))
    }
}

fn require_confirmation(node: &NodeItem<'_>, critic_may_confirm: bool) -> Result<(), OmhlError> {
    let Some(confirmation) = node.data.get("confirmed_by") else {
        return Err(node.error(RuleCode::ConfirmationRequired, CONFIRMED_BY));
    };
    let by_person = confirmer(node.data).is_some_and(is_person_confirmer);
    let by_critic = critic_may_confirm
        && confirmer(node.data) == Some(CRITIC_CONFIRMATION)
        && has_literal_exchange(confirmation);

    if by_person || by_critic {
        Ok(())
    } else {
        Err(node.error(RuleCode::ConfirmationInvalid, CONFIRMED_BY_AUTHOR))
    }
}

/// Pergunta e resposta literais, as palavras que ficam como prova da confirmação.
fn has_literal_exchange(confirmation: &Value) -> bool {
    ["question", "answer"].iter().all(|key| {
        confirmation
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    })
}

fn forbid_field(node: &NodeItem<'_>, key: &str, pointer: &str) -> Result<(), OmhlError> {
    if node.has_field(key) {
        Err(node.error(RuleCode::ConfirmationInvalid, pointer))
    } else {
        Ok(())
    }
}
