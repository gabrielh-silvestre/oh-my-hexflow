use hexlog::{Detail, ErrorCode, HexlogError, pointer_segment};

fn detail_at(path: &str) -> Detail {
    Detail {
        path: path.to_owned(),
        code: "some-rule".to_owned(),
        message: "refused".to_owned(),
        current: None,
        retired: None,
    }
}

#[test]
fn it_should_prefix_detail_paths_with_at() {
    let error = HexlogError {
        code: ErrorCode::InvalidInput,
        message: "refused".to_owned(),
        details: vec![detail_at(""), detail_at("/x/y")],
    };

    let prefixed = error.at("/items/3/body/id");

    let paths: Vec<&str> = prefixed.details.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, ["/items/3/body/id", "/items/3/body/id/x/y"]);
}

#[test]
fn it_should_escape_pointer_segments_per_rfc_6901() {
    let cases = [
        ("plain", "plain"),
        ("a/b", "a~1b"),
        ("a~b", "a~0b"),
        ("a/b~c", "a~1b~0c"),
        ("~1", "~01"),
        ("", ""),
    ];

    for (segment, escaped) in cases {
        assert_eq!(pointer_segment(segment), escaped, "segment {segment:?}");
    }
}

#[test]
fn it_should_keep_the_path_when_prefixing_with_an_empty_string() {
    let error = HexlogError {
        code: ErrorCode::InvalidInput,
        message: "refused".to_owned(),
        details: vec![detail_at(""), detail_at("/x/y")],
    };

    let prefixed = error.at("");

    let paths: Vec<&str> = prefixed.details.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, ["", "/x/y"]);
}

#[test]
fn it_should_map_every_error_code_to_its_wire_name() {
    let cases = [
        (ErrorCode::InvalidInput, "INVALID_INPUT"),
        (ErrorCode::InvalidRecord, "INVALID_RECORD"),
        (ErrorCode::InvalidSchema, "INVALID_SCHEMA"),
        (ErrorCode::TypeNotFound, "TYPE_NOT_FOUND"),
        (ErrorCode::RelationTypeNotFound, "RELATION_TYPE_NOT_FOUND"),
        (ErrorCode::TenantNotFound, "TENANT_NOT_FOUND"),
        (ErrorCode::ReservedId, "RESERVED_ID"),
        (ErrorCode::ForkRejected, "FORK_REJECTED"),
        (ErrorCode::CycleRejected, "CYCLE_REJECTED"),
        (ErrorCode::IdempotencyConflict, "IDEMPOTENCY_CONFLICT"),
        (ErrorCode::ChainBroken, "CHAIN_BROKEN"),
        (ErrorCode::Busy, "BUSY"),
        (ErrorCode::StoreError, "STORE_ERROR"),
        (ErrorCode::Internal, "INTERNAL"),
    ];

    for (code, wire) in cases {
        assert_eq!(code.as_str(), wire);
    }
}
