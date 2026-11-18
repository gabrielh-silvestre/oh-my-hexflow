//! Helpers dos testes unitários (`tests.rs` ao lado da fonte).
//!
//! Cópias dos helpers de `tests/support/` (uma mudança aqui vale também em `tests/support/<arquivo>`, e vice-versa):
//! - `tests/support/mod.rs`: `DEFAULT_INSTANT`, `sid`, `tname`, `rty`, `tenant`, `author`, `loader_author`, `snap` e `snap64`;
//! - `tests/support/omhl.rs`: `check_data`, `decision_data`, `PR_7` e `RUN_TESTS`;
//! - `tests/support/gate.rs`: `node_view`, `RECORD_ID`, `LABEL`, `FakeSource`, `all_filter`, `type_filter`,
//!   `node_view_with`, `node_views`, `relation_view`, `evaluate_one`, `expected_report` e `chain_head`;
//! - `tests/support/views.rs`: o arquivo inteiro (`test_support/views.rs`);
//! - `tests/support/pr_gate.rs`: `CI_GREEN`, `NO_OPEN_CONTRADICTION`, `DECISIONS_ANCHORED`, `ANCHORS_CONFIRMED`, `RECORD_ID`,
//!   `native_check`, `default_natives`, `project_check`, `mechanical_project_check`, `required_pairs` e `snapshots`;
//! - `tests/support/ext.rs`: `ExtTree` sem `at` e sem `fifo` (o `mkfifo` fica fora de `src`) e sem `write_check_file` e `toml_value`.
//!
//! Helpers que um `tests.rs` copia do arquivo de integração/e2e de onde saíram (uma mudança num vale também no outro):
//! - `graphs/tests.rs`, de `tests/catalogs.rs`: `PREVIOUS_DECISIONS_CATALOG_V3`, `catalog`, `catalog_from`, `confirmation`,
//!   `confirmed`, `gap`, `is_accepted`, `old_decisions_guideline`, `old_decisions_premise`, `plain` e `with`;
//! - `workflow/spec/tests.rs`, de `tests/spec_validate.rs`: `AMENDMENT_MOTIVE_BODY_LINE`, `PART_CA_LINE`, `PART_CMP_LINE`,
//!   `PART_OBJECTIVE_LINE`, `text`, `valid_amendment`, `valid_part` e `with_line`;
//! - `workflow/spec/tests.rs`, de `tests/spec_validate_graph.rs`: os mesmos de cima (menos `PART_CA_LINE`, `PART_CMP_LINE`) mais
//!   `W1`, `W2`, `BUSINESS_PREMISE`, `DIVERGENCE`, `PART_FIRST_CITATION_LINE`, `PART_SECOND_CITATION_LINE`
//!   e `pending_marker_part`;
//! - `workflow/gap/tests.rs`, de `tests/report_answers.rs`: `W1`, `GAP`, `OTHER_GAP`, `ANSWER`, `DIRECTIVE` e `confirmed_by`;
//! - `workflow/lock/tests.rs`, de `tests/spec_lock.rs`: `FOLDER`, `DEFAULT_SPECS`, `CUSTOM_SPECS`, `FROZEN` e `IN_WORKTREE`;
//! - `workflow/matrix/tests.rs`, de `tests/gate_worktree.rs`: `missing` e `missing_with_spec`;
//! - `workflow/recurrence/tests.rs`, de `tests/report_recurrences.rs`: `W1`, `W2`, `W3` e `TLS`;
//! - `extensions/scan/tests.rs`, de `tests/ext_scan.rs`: `summary` e `at`;
//! - `project/marker/tests.rs`, de `tests/project.rs`: `MARKER_FILE`; `project/paths/tests.rs`, do mesmo: `mode_of`;
//! - `projection/write/tests.rs`, de `tests/projection_write.rs`: `HEADER`, `PROJECTION` e `generated`;
//! - `pr_gate/tests.rs`, de `tests/pr_gate.rs`: `field`, `tree` e `at_head`;
//! - `workflow/retro/tests.rs`, de `tests/close_retro.rs`: `DIVERGENCE`, `OTHER_DIVERGENCE`, `GAP` e `ANSWER`;
//! - `native/tests.rs`, de `tests/native.rs`: `CHECK_IDS`, `GUIDELINE_IDS`, `RUN_TESTS`, `CI_GREEN` e `native_ids_with_types`;
//! - `ports/process/tests.rs`, de `tests/process_runner.rs`: `TWENTY_SECS`; e `spec`, de `tests/proc_roles/mod.rs`;
//! - `guard/tests.rs`, de `tests/guard.rs`: `rule`;
//! - `ports/blob/tests.rs` e `byproduct/tests.rs`, ambos de `tests/byproduct_store.rs`: `ABC_SHA256` (uma cópia em cada).

pub mod ext;
pub mod gate;
pub mod omhl;
pub mod pr_gate;
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

/// Commit de 40 caracteres iguais a `fill`.
pub fn snap(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(40)).expect("valid snapshot")
}

/// Commit de 64 caracteres iguais a `fill`.
pub fn snap64(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(64)).expect("valid snapshot")
}
