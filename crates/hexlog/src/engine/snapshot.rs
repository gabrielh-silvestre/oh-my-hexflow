use serde::{Serialize, Serializer};

use crate::error::{ErrorCode, HexlogError};
use crate::ids::is_lower_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotKind {
    GitCommit,
    GitTree,
}

impl SnapshotKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SnapshotKind::GitCommit => "git-commit",
            SnapshotKind::GitTree => "git-tree",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<SnapshotKind> {
        match text {
            "git-commit" => Some(SnapshotKind::GitCommit),
            "git-tree" => Some(SnapshotKind::GitTree),
            _ => None,
        }
    }
}

impl Serialize for SnapshotKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    kind: SnapshotKind,
    id: String,
}

impl Snapshot {
    /// O motor confere só o formato (40 ou 64 hexadecimais minúsculos), nunca a existência.
    pub fn new(kind: SnapshotKind, id: &str) -> Result<Snapshot, HexlogError> {
        if matches!(id.len(), 40 | 64) && is_lower_hex(id) {
            return Ok(Snapshot {
                kind,
                id: id.to_owned(),
            });
        }
        Err(HexlogError::refused(
            ErrorCode::InvalidRecord,
            "snapshot-format",
            "invalid snapshot",
        ))
    }

    pub fn kind(&self) -> SnapshotKind {
        self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}
