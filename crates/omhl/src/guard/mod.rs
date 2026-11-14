mod append;
mod attestation;
mod confirmation;
mod filter;
mod rests_on;

use hexlog::{SemanticId, Snapshot};

pub(crate) use self::append::{GUIDELINE_RETIRE_CLIENT, check_append};
pub(crate) use self::attestation::probe_refusal;
pub(crate) use self::filter::{check_question_filters, check_where_eq};
use crate::error::{OmhlError, RuleCode};

pub const WHERE_EQ_PAIRS_MAX: usize = 8;
/// Caracteres Unicode de um `Scalar::Str`.
pub const WHERE_EQ_TEXT_MAX: usize = 256;

/// Teto de um `SemanticId` no `hexlog`.
const ATTESTATION_ID_MAX: usize = 200;
/// O SHA entra em pedaços de 32 porque cada segmento do id aceita no máximo 63 caracteres.
const SNAPSHOT_SEGMENT: usize = 32;

/// `attestations.<id[..32]>.<id[32..]>.<check>.<work_item>`.
pub fn attestation_id(
    snapshot: &Snapshot,
    check: &SemanticId,
    work_item: &SemanticId,
) -> Result<SemanticId, OmhlError> {
    let (head, tail) = snapshot.id().split_at(SNAPSHOT_SEGMENT);
    let (check, work_item) = (check.as_str(), work_item.as_str());
    let text = format!("attestations.{head}.{tail}.{check}.{work_item}");
    if text.len() > ATTESTATION_ID_MAX {
        return Err(OmhlError::rule(RuleCode::AttestationIdTooLong, ""));
    }

    Ok(SemanticId::parse(&text)?)
}
