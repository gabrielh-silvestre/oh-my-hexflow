use crate::native::NativeCheck;
use crate::pr_gate::GateSnapshots;
use crate::workflow::config::NativeConfig;
use hexlog::{NodeView, RecordId, SemanticId, Snapshot, SnapshotKind};
use serde_json::json;

use super::omhl::{RUN_TESTS, check_data};
use super::{DEFAULT_INSTANT, author, loader_author, sid, tname};

pub const CI_GREEN: &str = "checks.ci-green-before-merge";
pub const NO_OPEN_CONTRADICTION: &str = "no-open-contradiction";
pub const DECISIONS_ANCHORED: &str = "decisions-anchored";
pub const ANCHORS_CONFIRMED: &str = "anchors-confirmed";

const RECORD_ID: &str = "acme:0192a000-0000-7000-8000-000000000001";

pub fn native_check(id: &'static str, event: &'static str, blocking: bool) -> NativeCheck {
    NativeCheck {
        id,
        event,
        how: "run it",
        blocking,
    }
}

pub const VERIFY_ANCHORING: &str = "checks.verify-review-comment-anchoring";
pub const ALL_NATIVE_CHECKS: [&str; 3] = [RUN_TESTS, CI_GREEN, VERIFY_ANCHORING];

pub fn enabled(ids: &[&'static str]) -> NativeConfig {
    NativeConfig {
        checks: ids.iter().copied().collect(),
        ..NativeConfig::default()
    }
}

pub fn all_enabled() -> NativeConfig {
    enabled(&ALL_NATIVE_CHECKS)
}

pub fn default_natives() -> Vec<NativeCheck> {
    vec![
        native_check(RUN_TESTS, "before-commit", true),
        native_check(CI_GREEN, "before-merge", true),
    ]
}

pub fn project_check(id: &str, event: &str, blocking: bool, shadowed: bool) -> NodeView {
    NodeView {
        id: sid(id),
        type_name: tname("check"),
        record_id: RecordId::parse(RECORD_ID).expect("valid record id"),
        seq: 1,
        at: DEFAULT_INSTANT.to_owned(),
        data: check_data(event, "run it", blocking),
        snapshot: None,
        external_id: None,
        author: author(),
        shadowed,
    }
}

pub fn mechanical_project_check(id: &str, event: &str, blocking: bool, shadowed: bool) -> NodeView {
    let mut data = check_data(event, "run it", blocking);
    data["run"] = json!(["true"]);
    data["timeout_secs"] = json!(60);
    data["expect"] = json!([{"exit": 0, "outcome": "passed"}]);

    NodeView {
        data,
        author: loader_author(),
        ..project_check(id, event, blocking, shadowed)
    }
}

pub fn required_pairs(pairs: &[(&str, SnapshotKind)]) -> Vec<(SemanticId, SnapshotKind)> {
    pairs.iter().map(|(id, kind)| (sid(id), *kind)).collect()
}

pub fn snapshots(commit: &Snapshot, tree: &Snapshot) -> GateSnapshots {
    GateSnapshots {
        head_commit: commit.clone(),
        head_tree: tree.clone(),
    }
}
