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
    WorkItemTypeUnknown,
    TypeChangeLoosens,
    CheckLooseningRefused,
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
    OriginInvalid,
    OriginImmutable,
    OriginRequired,
    RestsOnArchitecture,
    RestsOnTargetType,
    GuidelineRetireByCommand,
    RoleUnknown,
    RoleRecordForbidden,
    RoleRevisionForbidden,
    RoleAnswerForbidden,
    RoleRetirementForbidden,
    InvalidSessionId,
    ReviewBaseInvalid,
}

impl RuleCode {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleCode::IdNamespace => "id-namespace",
            RuleCode::UseCloseWorkItem => "use-close-work-item",
            RuleCode::InvalidTransition => "invalid-transition",
            RuleCode::WorkItemNotFound => "work-item-not-found",
            RuleCode::WorkItemTypeUnknown => "work-item-type-unknown",
            RuleCode::TypeChangeLoosens => "type-change-loosens",
            RuleCode::CheckLooseningRefused => "check-loosening-refused",
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
            RuleCode::OriginInvalid => "origin-invalid",
            RuleCode::OriginImmutable => "origin-immutable",
            RuleCode::OriginRequired => "origin-required",
            RuleCode::RestsOnArchitecture => "rests-on-architecture",
            RuleCode::RestsOnTargetType => "rests-on-target-type",
            RuleCode::GuidelineRetireByCommand => "guideline-retire-by-command",
            RuleCode::RoleUnknown => "role-unknown",
            RuleCode::RoleRecordForbidden => "role-record-forbidden",
            RuleCode::RoleRevisionForbidden => "role-revision-forbidden",
            RuleCode::RoleAnswerForbidden => "role-answer-forbidden",
            RuleCode::RoleRetirementForbidden => "role-retirement-forbidden",
            RuleCode::InvalidSessionId => "invalid-session-id",
            RuleCode::ReviewBaseInvalid => "review-base-invalid",
        }
    }

    pub fn guidance(self) -> Option<Guidance> {
        match self {
            RuleCode::WorkItemTypeUnknown => Some(Guidance {
                cause: "the work item has no type the project's matrix knows",
                next_step: "record the type with omhl append (field type) on the work item, or name the branch with its prefix in the table",
            }),
            RuleCode::KindMissing => Some(Guidance {
                cause: "the guideline has no kind",
                next_step: "set kind to business or architecture in the item data and append again",
            }),
            RuleCode::KindInvalid => Some(Guidance {
                cause: "the guideline kind must be business or architecture",
                next_step: "set kind to business or architecture in the item data and append again",
            }),
            RuleCode::OriginInvalid => Some(Guidance {
                cause: "only a business premise can carry origin late; a technical premise or one without kind cannot",
                next_step: "drop origin from the premise, or set kind to business when the premise is a late business one, and append again",
            }),
            RuleCode::OriginImmutable => Some(Guidance {
                cause: "a revision cannot add, change or drop the origin of a premise",
                next_step: "keep the origin the premise already has, or append a new premise with the origin you need",
            }),
            RuleCode::OriginRequired => Some(Guidance {
                cause: "the work item already has a decision, so a new business premise is late and must carry origin late",
                next_step: "on creation, add origin late to the premise data and append again; when a revision turns a premise into a business one, append a new premise with origin late instead, because adding origin in a revision gives origin-immutable",
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
                cause: "the role is not allowed to record this node or relation type, or this content (a divergence is created open), or the relation origin does not exist or is not its own",
                next_step: "append only the types and content the role may write, or leave this record to the role that owns it",
            }),
            RuleCode::RoleRevisionForbidden => Some(Guidance {
                cause: "the role may only revise nodes it recorded, and never change a premise or gap kind, accept a divergence, or move a contradicted node out of its work item; a confirmed premise or a closed divergence is not revised again",
                next_step: "append a new node of your own instead, or ask the owner to make the change",
            }),
            RuleCode::RoleAnswerForbidden => Some(Guidance {
                cause: "the critic answers only a gap whose kind is technical; a business gap, a gap without kind or a node that is not a gap is not its to answer",
                next_step: "leave the gap to the owner, or point the answers relation at a technical gap",
            }),
            RuleCode::RoleRetirementForbidden => Some(Guidance {
                cause: "this caller may not retire this node or relation",
                next_step: "ask the verifier or the owner to retire the contradicts relation; nobody retires what they recorded",
            }),
            RuleCode::InvalidSessionId => Some(Guidance {
                cause: "CLAUDE_CODE_SESSION_ID is set but empty, is not UTF-8, is longer than 100 UTF-16 units or equals outside-claude-code",
                next_step: "unset CLAUDE_CODE_SESSION_ID, or run omhl inside a Claude Code session",
            }),
            RuleCode::ReviewBaseInvalid => Some(Guidance {
                cause: "--base does not name a commit, or it is the same commit as HEAD, so there is no range to review",
                next_step: "pass --base with the integration branch the pull request targets, such as develop",
            }),
            RuleCode::CheckLooseningRefused => Some(Guidance {
                cause: "the revised check turns blocking off, removes its run or moves a blocking check out of the required events, which loosens a check already stored",
                next_step: "keep the check blocking, with its run and on a required event, or retire the check and declare a new one",
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
