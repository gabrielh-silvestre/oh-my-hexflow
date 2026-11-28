mod append;
mod attestation;
mod confirmation;
mod filter;
mod origin;
mod rests_on;
mod role;
mod role_guard;

use hexlog::{SemanticId, Snapshot};

pub(crate) use self::append::{GUIDELINE_RETIRE_CLIENT, check_append};
pub(crate) use self::attestation::probe_refusal;
pub(crate) use self::filter::{check_question_filters, check_where_eq};
pub(crate) use self::role::known_role;
pub use self::role::{ROLE_PREFIX, ROLE_REFERENCE_COMMANDS, ROLE_REFERENCE_MATRIX, ROLE_VARIABLE};
use crate::error::{OmhlError, RuleCode};

pub const WHERE_EQ_PAIRS_MAX: usize = 8;
pub const WHERE_EQ_TEXT_MAX: usize = 256;

const ATTESTATION_ID_MAX: usize = 200;
/// O SHA entra em pedaços de 32 porque cada segmento do id aceita no máximo 63 caracteres.
const SNAPSHOT_SEGMENT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    InForce,
    Evaluated,
}

pub fn attestation_id(
    snapshot: &Snapshot,
    check: &SemanticId,
    work_item: &SemanticId,
    origin: Origin,
) -> Result<SemanticId, OmhlError> {
    let (head, tail) = snapshot.id().split_at(SNAPSHOT_SEGMENT);
    let (check, work_item) = (check.as_str(), work_item.as_str());
    let prefix = match origin {
        Origin::InForce => "attestations",
        Origin::Evaluated => "attestations.evaluated",
    };
    let text = format!("{prefix}.{head}.{tail}.{check}.{work_item}");
    if text.len() > ATTESTATION_ID_MAX {
        return Err(OmhlError::rule(RuleCode::AttestationIdTooLong, ""));
    }

    Ok(SemanticId::parse(&text)?)
}

#[cfg(test)]
mod tests;
