#![allow(dead_code)]

pub mod gate;
pub mod omhl;
pub mod pr_gate;
pub mod raw;

use std::sync::atomic::{AtomicU64, Ordering};

use ::omhl::probe::{ProbeError, ProbeOutcome, SnapshotProbe};
use hexlog::{
    Author, Clock, IdGenerator, NativeNode, NativeNodes, RelationTypeName, SemanticId, Snapshot,
    SnapshotKind, TenantId, TypeName,
};
use uuid::Uuid;

pub const DEFAULT_INSTANT: &str = "2026-11-07T12:00:00.000Z";
const UUID_V7_BASE: u128 = 0x0192_a000_0000_7000_8000_0000_0000_0000;

pub struct FixedClock {
    instant: String,
}

impl FixedClock {
    pub fn at(instant: &str) -> FixedClock {
        FixedClock {
            instant: instant.to_owned(),
        }
    }
}

impl Default for FixedClock {
    fn default() -> FixedClock {
        FixedClock::at(DEFAULT_INSTANT)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> String {
        self.instant.clone()
    }
}

pub struct SeqIds(AtomicU64);

impl SeqIds {
    pub fn new() -> SeqIds {
        SeqIds(AtomicU64::new(1))
    }
}

impl Default for SeqIds {
    fn default() -> SeqIds {
        SeqIds::new()
    }
}

impl IdGenerator for SeqIds {
    fn next(&self) -> Uuid {
        let n = self.0.fetch_add(1, Ordering::Relaxed);
        Uuid::from_u128(UUID_V7_BASE | u128::from(n))
    }
}

/// Sonda sem estado mutável: devolve sempre o mesmo resultado. Para mudá-lo no meio de um teste,
/// larga-se o `Omhl`, troca-se o falso e reabre-se.
pub struct FakeProbe {
    result: Result<ProbeOutcome, ProbeError>,
}

impl FakeProbe {
    pub fn clean(snapshot: Snapshot) -> FakeProbe {
        FakeProbe {
            result: Ok(ProbeOutcome::Reproducible(snapshot)),
        }
    }

    pub fn dirty() -> FakeProbe {
        FakeProbe {
            result: Ok(ProbeOutcome::Dirty),
        }
    }

    pub fn failing() -> FakeProbe {
        FakeProbe {
            result: Err(ProbeError {
                message: "probe unavailable".to_owned(),
            }),
        }
    }
}

impl SnapshotProbe for FakeProbe {
    fn probe(&self) -> Result<ProbeOutcome, ProbeError> {
        self.result.clone()
    }
}

/// Base nativa sem nenhum id.
pub struct EmptyNative;

impl NativeNodes for EmptyNative {
    fn lookup(&self, _id: &SemanticId) -> Option<NativeNode> {
        None
    }

    fn native_relations(&self, _relation_type: &RelationTypeName) -> Vec<(SemanticId, SemanticId)> {
        Vec::new()
    }

    fn catalog_version(&self) -> &str {
        "test"
    }
}

pub fn tenant(text: &str) -> TenantId {
    TenantId::parse(text).expect("valid tenant")
}

pub fn sid(text: &str) -> SemanticId {
    SemanticId::parse(text).expect("valid semantic id")
}

pub fn rty(text: &str) -> RelationTypeName {
    RelationTypeName::parse(text).expect("valid relation type name")
}

pub fn tname(text: &str) -> TypeName {
    TypeName::parse(text).expect("valid type name")
}

pub fn author() -> Author {
    Author {
        agent: "tester".to_owned(),
        model: None,
        client: "tests".to_owned(),
    }
}

/// Commit de 40 caracteres iguais a `fill`.
pub fn snap(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(40)).expect("valid snapshot")
}

/// Commit de 64 caracteres iguais a `fill`.
pub fn snap64(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(64)).expect("valid snapshot")
}
