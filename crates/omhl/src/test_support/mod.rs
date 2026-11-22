//! Cópias dos helpers de `tests/support/`: uma mudança aqui vale também no arquivo de mesmo nome em `tests/support/`
//! (`mod.rs`, `omhl.rs`, `gate.rs`, `views.rs`, `pr_gate.rs`, `ext.rs`), e vice-versa. O `ext.rs` daqui não tem o
//! `mkfifo`, que fica fora de `src`. O `prove_both_sides` de `tests/support/scan.rs` tem cópia em `role_view.rs`.
//!
//! Helpers que um `tests.rs` copia do arquivo de integração/e2e de onde saíram (uma mudança num vale também no outro):
//! - `role_view.rs`, de `workflow/role_allow.rs`, `tests/e2e/skills/coverage.rs` e `tests/e2e/skills/orchestration.rs`;
//! - `role_view/who_uses.rs`, de `tests/e2e/skills/workflow.rs`;
//! - `graphs/tests.rs`, de `tests/integration/catalogs.rs`;
//! - `workflow/spec/tests.rs`, de `tests/e2e/spec_validate.rs` e de `tests/e2e/spec_validate_graph.rs`;
//! - `workflow/gap/tests.rs`, de `tests/e2e/report_answers.rs`;
//! - `workflow/lock/tests.rs`, de `tests/e2e/spec_lock.rs`;
//! - `workflow/matrix/tests.rs`, de `tests/e2e/gate_worktree.rs`;
//! - `workflow/recurrence/tests.rs`, de `tests/e2e/report_recurrences.rs`;
//! - `extensions/scan/tests.rs`, de `tests/integration/ext_scan.rs`;
//! - `project/marker/tests.rs` e `project/paths/tests.rs`, de `tests/integration/project.rs`;
//! - `projection/write/tests.rs`, de `tests/integration/projection_write.rs`;
//! - `pr_gate/tests.rs`, de `tests/integration/pr_gate.rs`;
//! - `workflow/retro/tests.rs`, de `tests/e2e/close_retro.rs`;
//! - `native/tests.rs`, de `tests/integration/native.rs`;
//! - `ports/process/tests.rs`, de `tests/integration/process_runner.rs` e de `tests/integration/proc_roles/mod.rs`;
//! - `guard/tests.rs`, de `tests/integration/guard.rs`;
//! - `adapters/text/simple/tests.rs`, de `tests/support/cycle.rs`;
//! - `ports/blob/tests.rs` e `byproduct/tests.rs`, um do outro.

pub mod ext;
pub mod gate;
pub mod omhl;
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
