use std::fmt;

use hexlog::HexlogError;

use crate::guidance::Guidance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmhlError {
    Hexlog(HexlogError),
    /// `pointer` é um JSON Pointer (RFC 6901); vazio quando não se aplica.
    Rule {
        code: RuleCode,
        pointer: String,
    },
}

impl OmhlError {
    pub fn rule(code: RuleCode, pointer: &str) -> OmhlError {
        OmhlError::Rule {
            code,
            pointer: pointer.to_owned(),
        }
    }
}

impl From<HexlogError> for OmhlError {
    fn from(error: HexlogError) -> Self {
        OmhlError::Hexlog(error)
    }
}

impl fmt::Display for OmhlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OmhlError::Hexlog(error) => write!(f, "{error}"),
            OmhlError::Rule { code, pointer } if pointer.is_empty() => {
                write!(f, "{}", code.as_str())
            }
            OmhlError::Rule { code, pointer } => write!(f, "{} at {pointer}", code.as_str()),
        }
    }
}

impl std::error::Error for OmhlError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCode {
    IdNamespace,
    UseCloseWorkItem,
    InvalidTransition,
    WorkItemNotFound,
    WorkItemClosed,
    CheckNotFound,
    AttestationIdMismatch,
    AttestationIdTooLong,
    SnapshotNotHead,
    SnapshotDirty,
    ProbeFailed,
    OutcomeImmutable,
    ReactivationUnsupported,
    WhereEqTooMany,
    WhereEqValueTooLong,
    GateInconsistent,
    SnapshotTreeMismatch,
    CheckRunAuthor,
    AttestationIncomplete,
    ConfirmationRequired,
    ConfirmationInvalid,
    KindMissing,
}

impl RuleCode {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleCode::IdNamespace => "id-namespace",
            RuleCode::UseCloseWorkItem => "use-close-work-item",
            RuleCode::InvalidTransition => "invalid-transition",
            RuleCode::WorkItemNotFound => "work-item-not-found",
            RuleCode::WorkItemClosed => "work-item-closed",
            RuleCode::CheckNotFound => "check-not-found",
            RuleCode::AttestationIdMismatch => "attestation-id-mismatch",
            RuleCode::AttestationIdTooLong => "attestation-id-too-long",
            RuleCode::SnapshotNotHead => "snapshot-not-head",
            RuleCode::SnapshotDirty => "snapshot-dirty",
            RuleCode::ProbeFailed => "probe-failed",
            RuleCode::OutcomeImmutable => "outcome-immutable",
            RuleCode::ReactivationUnsupported => "reactivation-unsupported",
            RuleCode::WhereEqTooMany => "where-eq-too-many",
            RuleCode::WhereEqValueTooLong => "where-eq-value-too-long",
            RuleCode::GateInconsistent => "gate-inconsistent",
            RuleCode::SnapshotTreeMismatch => "snapshot-tree-mismatch",
            RuleCode::CheckRunAuthor => "check-run-author",
            RuleCode::AttestationIncomplete => "attestation-incomplete",
            RuleCode::ConfirmationRequired => "confirmation-required",
            RuleCode::ConfirmationInvalid => "confirmation-invalid",
            RuleCode::KindMissing => "kind-missing",
        }
    }

    pub fn guidance(self) -> Option<Guidance> {
        match self {
            RuleCode::KindMissing => Some(Guidance {
                cause: "the guideline has no kind",
                next_step: "set kind to negocio or arquitetura in the item data and append again",
            }),
            _ => None,
        }
    }
}
