//! Cópias dos helpers de `tests/support/` e dos arquivos de `tests/integration` e `tests/e2e` que cada `tests.rs` copia:
//! uma mudança aqui vale também na origem, e vice-versa. O `ext.rs` daqui não tem o `mkfifo`, que fica fora de `src`.

pub mod doc_surface;
pub mod ext;
pub mod gate;
pub mod omhl;
pub mod plugin_labels;
pub mod pr_gate;
pub mod role_view;
pub mod views;

use hexlog::{Author, RelationTypeName, SemanticId, Snapshot, SnapshotKind, TenantId, TypeName};

pub const DEFAULT_INSTANT: &str = "2026-11-07T12:00:00.000Z";

pub fn sid(text: &str) -> SemanticId {
    SemanticId::parse(text).expect("valid semantic id")
}

pub fn rty(text: &str) -> RelationTypeName {
    RelationTypeName::parse(text).expect("valid relation type name")
}

pub fn tenant(text: &str) -> TenantId {
    TenantId::parse(text).expect("valid tenant")
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

pub fn loader_author() -> Author {
    Author {
        agent: "loader".to_owned(),
        model: None,
        client: "loader".to_owned(),
    }
}

pub fn snap(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(40)).expect("valid snapshot")
}

pub fn snap64(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(64)).expect("valid snapshot")
}
