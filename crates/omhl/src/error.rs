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
    KindInvalid,
    RestsOnArchitecture,
    RestsOnTargetType,
    GuidelineRetireByCommand,
    RoleUnknown,
    RoleRecordForbidden,
    RoleRevisionForbidden,
    RoleRetirementForbidden,
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
            RuleCode::KindInvalid => "kind-invalid",
            RuleCode::RestsOnArchitecture => "rests-on-architecture",
            RuleCode::RestsOnTargetType => "rests-on-target-type",
            RuleCode::GuidelineRetireByCommand => "guideline-retire-by-command",
            RuleCode::RoleUnknown => "role-unknown",
            RuleCode::RoleRecordForbidden => "role-record-forbidden",
            RuleCode::RoleRevisionForbidden => "role-revision-forbidden",
            RuleCode::RoleRetirementForbidden => "role-retirement-forbidden",
        }
    }

    pub fn guidance(self) -> Option<Guidance> {
        match self {
            RuleCode::KindMissing => Some(Guidance {
                cause: "the guideline has no kind",
                next_step: "set kind to business or architecture in the item data and append again",
            }),
            RuleCode::KindInvalid => Some(Guidance {
                cause: "the guideline kind must be business or architecture",
                next_step: "set kind to business or architecture in the item data and append again",
            }),
            RuleCode::RestsOnArchitecture => Some(Guidance {
                cause: "a guideline cannot rest on an architecture guideline; only a decision or a premise can",
                next_step: "rest it on a business guideline, or record the link from a decision",
            }),
            RuleCode::RestsOnTargetType => Some(Guidance {
                cause: "a guideline can only rest on another guideline",
                next_step: "rest the guideline on a business guideline, or move the premise under a decision",
            }),
            RuleCode::GuidelineRetireByCommand => Some(Guidance {
                cause: "a guideline can only be retired by the retire command",
                next_step: "retire a guideline with omhl guideline retire",
            }),
            RuleCode::RoleUnknown => Some(Guidance {
                cause: "OMHL_ROLE must be one of the seven omhl roles and is only accepted under Claude Code",
                next_step: "unset OMHL_ROLE, or set it to a valid omhl role while running under Claude Code",
            }),
            RuleCode::RoleRecordForbidden => Some(Guidance {
                cause: "the role is not allowed to record this node or relation type, or this content",
                next_step: "append only the types and content the role may write, or leave this record to the role that owns it",
            }),
            RuleCode::RoleRevisionForbidden => Some(Guidance {
                cause: "the role may only revise nodes it recorded, and never change a premise or gap kind, accept a divergence, or move a contradicted node out of its work item",
                next_step: "append a new node of your own instead, or ask the owner to make the change",
            }),
            RuleCode::RoleRetirementForbidden => Some(Guidance {
                cause: "this caller may not retire this node or relation",
                next_step: "ask the verifier or the owner to retire the contradicts relation; nobody retires what they recorded",
            }),
            _ => None,
        }
    }
}
