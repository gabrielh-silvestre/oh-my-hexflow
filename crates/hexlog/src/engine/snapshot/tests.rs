use serde_json::json;

use crate::{ErrorCode, Snapshot, SnapshotKind};

#[test]
fn it_should_accept_a_lowercase_hex_snapshot_of_40_or_64_characters() {
    let cases = ["0123456789abcdef".repeat(4), "c".repeat(40)];

    for id in &cases {
        let snapshot = Snapshot::new(SnapshotKind::GitCommit, id).unwrap();

        assert_eq!(snapshot.kind(), SnapshotKind::GitCommit);
        assert_eq!(snapshot.id(), id);
    }
}

#[test]
fn it_should_reject_a_snapshot_with_7_39_or_41_characters_uppercase_or_non_hex() {
    let cases = [
        "a".repeat(7),
        "a".repeat(39),
        "a".repeat(41),
        "A".repeat(40),
        format!("{}g", "a".repeat(39)),
        "A".repeat(64),
    ];

    for id in &cases {
        let error = Snapshot::new(SnapshotKind::GitCommit, id).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidRecord, "code for {id:?}");
        assert_eq!(error.details.len(), 1, "details for {id:?}");
        assert_eq!(error.details[0].code, "snapshot-format", "{id:?}");
        assert_eq!(error.details[0].path, "", "path for {id:?}");
    }
}

#[test]
fn it_should_serialize_a_snapshot_as_kind_and_id() {
    let id = "c".repeat(40);
    let snapshot = Snapshot::new(SnapshotKind::GitCommit, &id).unwrap();

    let serialized = serde_json::to_value(&snapshot).unwrap();

    assert_eq!(serialized, json!({"kind": "git-commit", "id": id}));
}

#[test]
fn it_should_refuse_a_snapshot_id_that_is_not_40_or_64_lowercase_hex() {
    let ids = [
        "a".repeat(7),
        "a".repeat(39),
        "a".repeat(41),
        "A".repeat(40),
        format!("{}g", "a".repeat(39)),
        "A".repeat(64),
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
    ];

    for kind in [SnapshotKind::GitTree, SnapshotKind::GitCommit] {
        for id in &ids {
            let error = Snapshot::new(kind, id).unwrap_err();

            assert_eq!(
                error.code,
                ErrorCode::InvalidRecord,
                "code for {kind:?} {id:?}"
            );
            assert_eq!(error.details.len(), 1, "details for {kind:?} {id:?}");
            assert_eq!(error.details[0].code, "snapshot-format", "{kind:?} {id:?}");
            assert_eq!(error.details[0].path, "", "path for {kind:?} {id:?}");
        }
    }
}
