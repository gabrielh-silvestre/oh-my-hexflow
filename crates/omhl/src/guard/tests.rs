use crate::error::{OmhlError, RuleCode};
use crate::guard::attestation_id;
use crate::test_support::omhl::{PR_7, RUN_TESTS};
use crate::test_support::{sid, snap, snap64};

fn rule(code: RuleCode, pointer: &str) -> OmhlError {
    OmhlError::rule(code, pointer)
}

#[test]
fn it_should_derive_the_attestation_id_from_snapshot_check_and_work_item() {
    let expected = format!(
        "attestations.{}.{}.checks.run-tests.work-items.pr-7",
        "a".repeat(32),
        "a".repeat(8)
    );

    let id = attestation_id(&snap('a'), &sid(RUN_TESTS), &sid(PR_7));

    assert_eq!(id, Ok(sid(&expected)));
}

#[test]
fn it_should_split_a_sha256_snapshot_into_two_segments_of_32() {
    let expected = format!(
        "attestations.{}.{}.checks.run-tests.work-items.pr-7",
        "b".repeat(32),
        "b".repeat(32)
    );

    let id = attestation_id(&snap64('b'), &sid(RUN_TESTS), &sid(PR_7));

    assert_eq!(id, Ok(sid(&expected)));
}

#[test]
fn it_should_refuse_an_attestation_id_above_200_characters() {
    let check = sid(&format!("checks.{}", "c".repeat(63)));
    let work_item = sid(&format!("work-items.{}", "w".repeat(63)));

    let id = attestation_id(&snap64('b'), &check, &work_item);

    assert_eq!(id, Err(rule(RuleCode::AttestationIdTooLong, "")));
}
