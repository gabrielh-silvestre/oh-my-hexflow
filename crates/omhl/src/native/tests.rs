use crate::native::NATIVE_CATALOG_VERSION;
use crate::native::NativeBase;
use crate::test_support::{rty, sid, tname};
use hexlog::NativeNodes;
use hexlog::{NativeNode, SemanticId};

const CHECK_IDS: [&str; 4] = [
    "checks.run-tests",
    "checks.ci-green-before-merge",
    "checks.verify-review-comment-anchoring",
    CONFIG_LOOSENING_REVIEW,
];
const FORMER_GUIDELINE_IDS: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];

const CONFIG_LOOSENING_REVIEW: &str = "checks.config-loosening-review";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";

#[test]
fn it_should_reserve_each_native_check_id_with_the_check_type() {
    let native = NativeBase;
    let expected = vec![
        Some(NativeNode {
            type_name: tname("check")
        });
        4
    ];

    let found: Vec<Option<NativeNode>> =
        CHECK_IDS.iter().map(|id| native.lookup(&sid(id))).collect();

    assert_eq!(found, expected);
}

#[test]
fn it_should_not_reserve_the_two_former_native_guideline_ids() {
    let native = NativeBase;

    let found: Vec<Option<NativeNode>> = FORMER_GUIDELINE_IDS
        .iter()
        .map(|id| native.lookup(&sid(id)))
        .collect();

    assert_eq!(found, vec![None, None]);
}

#[test]
fn it_should_not_reserve_an_unknown_id() {
    let native = NativeBase;
    let unknown = ["checks.unknown", "checks.run-test", "guidelines.run-tests"];

    let found: Vec<Option<NativeNode>> = unknown.iter().map(|id| native.lookup(&sid(id))).collect();

    assert_eq!(found, vec![None, None, None]);
}

#[test]
fn it_should_expose_the_ci_after_run_tests_native_relation() {
    let native = NativeBase;

    let pairs = native.native_relations(&rty("after"));

    assert_eq!(pairs, vec![(sid(CI_GREEN), sid(RUN_TESTS))]);
}

#[test]
fn it_should_return_no_pairs_for_other_relation_types() {
    let native = NativeBase;

    let pairs: Vec<Vec<(SemanticId, SemanticId)>> = ["supports", "contradicts", "answers"]
        .iter()
        .map(|relation_type| native.native_relations(&rty(relation_type)))
        .collect();

    assert_eq!(pairs, vec![Vec::new(), Vec::new(), Vec::new()]);
}

#[test]
fn it_should_list_the_same_four_native_check_ids_in_the_contract_order() {
    let native = NativeBase;

    let check_ids: Vec<&str> = native.checks().iter().map(|check| check.id).collect();

    assert_eq!(check_ids, CHECK_IDS.to_vec());
}

#[test]
fn it_should_mark_the_four_native_checks_blocking_with_the_contract_events() {
    let native = NativeBase;
    let expected = vec![
        (RUN_TESTS, "before-commit", true),
        (CI_GREEN, "before-merge", true),
        (
            "checks.verify-review-comment-anchoring",
            "before-publishing-review-comments",
            true,
        ),
        (CONFIG_LOOSENING_REVIEW, "before-push", true),
    ];

    let found: Vec<(&str, &str, bool)> = native
        .checks()
        .iter()
        .map(|check| (check.id, check.event, check.blocking))
        .collect();

    assert_eq!(found, expected);
}

// Valor em develop antes da entrega: dois ids nativos saíram e um evento mudou.
const DEVELOP_CATALOG_VERSION: (u32, u32, u32) = (1, 1, 0);

fn parse_version(version: &str) -> (u32, u32, u32) {
    let parts: Vec<u32> = version
        .split('.')
        .map(|part| part.parse().expect("numeric version part"))
        .collect();

    (parts[0], parts[1], parts[2])
}

#[test]
fn it_should_raise_the_native_catalog_version_above_the_develop_value() {
    let native = NativeBase;

    let versions = (
        parse_version(native.catalog_version()),
        parse_version(NATIVE_CATALOG_VERSION),
    );

    assert!(
        versions.0 > DEVELOP_CATALOG_VERSION && versions.1 > DEVELOP_CATALOG_VERSION,
        "{versions:?}"
    );
}

#[test]
fn it_should_expose_the_native_check_data_as_non_empty_texts_without_carriage_returns() {
    let native = NativeBase;

    let texts: Vec<&str> = native.checks().iter().map(|check| check.how).collect();

    let malformed: Vec<&str> = texts
        .iter()
        .copied()
        .filter(|text| text.is_empty() || text.contains('\r'))
        .collect();
    assert_eq!((texts.len(), malformed), (4, Vec::new()));
}

#[test]
fn it_should_end_the_native_check_catalog_with_the_config_loosening_review() {
    let native = NativeBase;

    let last = native.checks().last().map(|check| check.id);

    assert_eq!(last, Some(CONFIG_LOOSENING_REVIEW));
}

#[test]
fn it_should_describe_the_config_loosening_review_in_one_english_sentence() {
    let native = NativeBase;
    let check = native
        .checks()
        .iter()
        .find(|check| check.id == CONFIG_LOOSENING_REVIEW)
        .expect("the review check is in the catalog");

    let how = check.how;

    assert!(how.is_ascii(), "{how}");
    assert!(how.contains("oh-my-hexflow:code-reviewer"), "{how}");
    assert!(!how.contains(". "), "{how}");
    assert!(!how.contains('\n'), "{how}");
}

fn config_loosening_review_how() -> &'static str {
    NativeBase
        .checks()
        .iter()
        .find(|check| check.id == CONFIG_LOOSENING_REVIEW)
        .expect("the review check is in the catalog")
        .how
}

#[test]
fn it_should_name_the_two_reviewed_paths_in_the_config_loosening_review_how() {
    let how = config_loosening_review_how();

    assert!(how.contains(".omhl/workflow.toml"), "{how}");
    assert!(how.contains(".omhl/checks/"), "{how}");
}

#[test]
fn it_should_leave_the_attestation_to_the_human_in_the_config_loosening_review_how() {
    let how = config_loosening_review_how();

    assert!(how.contains("the human attest"), "{how}");
}

#[test]
fn it_should_not_tell_the_agent_to_attest_in_the_config_loosening_review_how() {
    let how = config_loosening_review_how();

    assert!(!how.contains("attest the result"), "{how}");
    assert!(
        !how.contains("any check, gate or lint configuration"),
        "{how}"
    );
}

#[test]
fn it_should_reserve_the_config_loosening_review_id_as_a_check() {
    let native = NativeBase;

    let found = native.lookup(&sid(CONFIG_LOOSENING_REVIEW));

    assert_eq!(
        found,
        Some(NativeNode {
            type_name: tname("check")
        })
    );
}

#[test]
fn it_should_keep_the_config_loosening_review_out_of_the_native_after_pair() {
    let native = NativeBase;

    let pairs = native.native_relations(&rty("after"));

    assert!(pairs.iter().all(|(from, to)| {
        from.as_str() != CONFIG_LOOSENING_REVIEW && to.as_str() != CONFIG_LOOSENING_REVIEW
    }));
}

fn names(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn it_should_trigger_the_review_for_each_config_path_that_changed() {
    let changed = names(&[
        ".omhl/workflow.toml",
        ".omhl/checks/x.toml",
        ".omhl/checks/x.md",
    ]);

    let triggers = crate::native::review_trigger_paths(&changed);

    assert_eq!(triggers, changed);
}

#[test]
fn it_should_not_trigger_the_review_for_a_source_file() {
    let changed = names(&["src/a.rs", "README.md"]);

    let triggers = crate::native::review_trigger_paths(&changed);

    assert!(triggers.is_empty(), "{triggers:?}");
}

#[test]
fn it_should_not_trigger_the_review_for_a_nested_file_or_another_extension_in_the_checks_folder() {
    let changed = names(&[
        ".omhl/checks/sub/x.toml",
        ".omhl/checks/x.rs",
        ".omhl/guidelines/g.toml",
    ]);

    let triggers = crate::native::review_trigger_paths(&changed);

    assert!(triggers.is_empty(), "{triggers:?}");
}

#[test]
fn it_should_trigger_the_review_only_by_the_old_path_of_a_check_moved_out_of_the_folder() {
    let changed = names(&["docs/x.toml", ".omhl/checks/x.toml"]);

    let triggers = crate::native::review_trigger_paths(&changed);

    assert_eq!(triggers, names(&[".omhl/checks/x.toml"]));
}

#[test]
fn it_should_not_trigger_the_review_for_an_empty_change_list() {
    let triggers = crate::native::review_trigger_paths(&[]);

    assert!(triggers.is_empty(), "{triggers:?}");
}
