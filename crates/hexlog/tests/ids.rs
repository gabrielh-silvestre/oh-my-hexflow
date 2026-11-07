mod support;

use std::fmt::Debug;

use hexlog::{
    ErrorCode, ExternalId, Hash, HexlogError, IdGenerator, IdempotencyKey, Name, RecordId,
    RelationTypeName, TenantId, TypeName,
};
use support::SeqIds;

fn assert_accepts<T: Debug>(parse: fn(&str) -> Result<T, HexlogError>, cases: &[String]) {
    for text in cases {
        assert!(parse(text).is_ok(), "should accept {text:?}");
    }
}

fn assert_rejects<T: Debug>(
    parse: fn(&str) -> Result<T, HexlogError>,
    cases: &[String],
    code: ErrorCode,
    detail_code: &str,
) {
    for text in cases {
        let error = parse(text).unwrap_err();

        assert_eq!(error.code, code, "code for {text:?}");
        assert_eq!(error.details.len(), 1, "details for {text:?}");
        assert_eq!(
            error.details[0].code, detail_code,
            "detail code for {text:?}"
        );
        assert_eq!(error.details[0].path, "", "path for {text:?}");
    }
}

fn texts<const N: usize>(cases: [&str; N]) -> Vec<String> {
    cases.iter().map(|text| (*text).to_owned()).collect()
}

fn assert_name_rules<T: Debug>(parse: fn(&str) -> Result<T, HexlogError>) {
    let mut accepted = texts(["a", "acme", "a-b", "0a", "a1b2", "a-"]);
    accepted.push("a".repeat(63));
    let mut rejected = texts(["", "A", "a.b", "a_b", "-a", "a b", "á", "a:b"]);
    rejected.push("a".repeat(64));

    assert_accepts(parse, &accepted);
    assert_rejects(parse, &rejected, ErrorCode::InvalidInput, "name-format");
}

#[test]
fn it_should_validate_names_per_the_contract_table() {
    assert_name_rules(Name::parse);
    assert_name_rules(TenantId::parse);
    assert_name_rules(TypeName::parse);
    assert_name_rules(RelationTypeName::parse);
}

#[test]
fn it_should_validate_record_ids_per_the_contract_table() {
    let ids = SeqIds::new();
    let generated = format!("acme:{}", ids.next());
    let mut accepted = texts([
        "acme:0192a000-0000-7000-8000-000000000001",
        "acme:0192a000-0000-7000-9000-000000000001",
        "acme:0192a000-0000-7000-a000-000000000001",
        "acme:0192a000-0000-7000-b000-000000000001",
        "a-b:0192a000-0000-7abc-8def-0123456789ab",
    ]);
    accepted.push(generated);
    let mut rejected = texts([
        "",
        "0192a000-0000-7000-8000-000000000001",
        ":0192a000-0000-7000-8000-000000000001",
        "ACME:0192a000-0000-7000-8000-000000000001",
        "acme:0192A000-0000-7000-8000-000000000001",
        "acme:0192a000-0000-4000-8000-000000000001",
        "acme:0192a000-0000-6000-8000-000000000001",
        "acme:0192a000-0000-7000-7000-000000000001",
        "acme:0192a000-0000-7000-c000-000000000001",
        "acme:0192a00000007000800000000000000001",
        "acme:0192a000-0000-7000-8000-00000000001",
        "acme:0192a000-0000-7000-8000-0000000000011",
        "acme:acme:0192a000-0000-7000-8000-000000000001",
        "acme-0192a000-0000-7000-8000-000000000001",
    ]);
    rejected.push(format!(
        "{}:0192a000-0000-7000-8000-000000000001",
        "a".repeat(64)
    ));

    assert_accepts(RecordId::parse, &accepted);
    assert_rejects(
        RecordId::parse,
        &rejected,
        ErrorCode::InvalidInput,
        "record-id-format",
    );
}

#[test]
fn it_should_validate_hashes_per_the_contract_table() {
    let accepted = vec!["0123456789abcdef".repeat(4), "f".repeat(64)];
    let mut rejected = texts(["", "abc"]);
    rejected.push("a".repeat(63));
    rejected.push("a".repeat(65));
    rejected.push("A".repeat(64));
    rejected.push(format!("{}g", "a".repeat(63)));
    rejected.push(format!("0x{}", "a".repeat(62)));

    assert_accepts(Hash::parse, &accepted);
    assert_rejects(
        Hash::parse,
        &rejected,
        ErrorCode::InvalidInput,
        "hash-format",
    );
}

#[test]
fn it_should_validate_idempotency_keys_in_utf16_units() {
    let accepted = vec![
        "k".to_owned(),
        "a".repeat(200),
        "😀".repeat(100),
        "é".repeat(200),
    ];
    let rejected = vec![String::new(), "a".repeat(201), "😀".repeat(101)];

    assert_accepts(IdempotencyKey::parse, &accepted);
    assert_rejects(
        IdempotencyKey::parse,
        &rejected,
        ErrorCode::InvalidInput,
        "idempotency-key-format",
    );
}

#[test]
fn it_should_validate_external_ids_in_utf16_units() {
    let accepted = vec![
        "x".to_owned(),
        "a".repeat(200),
        "😀".repeat(100),
        "é".repeat(200),
    ];
    let rejected = vec![String::new(), "a".repeat(201), "😀".repeat(101)];

    assert_accepts(ExternalId::parse, &accepted);
    assert_rejects(
        ExternalId::parse,
        &rejected,
        ErrorCode::InvalidRecord,
        "external-id-format",
    );
}
