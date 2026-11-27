use crate::native::NATIVE_CATALOG_VERSION;
use crate::native::NativeBase;
use crate::test_support::{rty, sid, tname};
use hexlog::NativeNodes;
use hexlog::{NativeNode, SemanticId};

#[test]
fn it_should_bump_the_native_catalog_version_for_the_new_check() {
    let native = NativeBase;

    let versions = (
        crate::native::NATIVE_CATALOG_VERSION,
        native.catalog_version(),
    );

    assert_eq!(versions, ("1.1.0", "1.1.0"));
}

const CHECK_IDS: [&str; 4] = [
    "checks.run-tests",
    "checks.ci-green-before-merge",
    "checks.verify-review-comment-anchoring",
    CONFIG_LOOSENING_REVIEW,
];
const GUIDELINE_IDS: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];

const CONFIG_LOOSENING_REVIEW: &str = "checks.config-loosening-review";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";

/// Os ids nativos na ordem do contrato, com o tipo de cada um.
fn native_ids_with_types() -> Vec<(&'static str, &'static str)> {
    let checks = CHECK_IDS.iter().map(|id| (*id, "check"));
    let guidelines = GUIDELINE_IDS.iter().map(|id| (*id, "guideline"));

    checks.chain(guidelines).collect()
}

#[test]
fn it_should_reserve_each_native_id_with_its_type() {
    let native = NativeBase;
    let expected: Vec<Option<NativeNode>> = native_ids_with_types()
        .iter()
        .map(|(_, type_name)| {
            Some(NativeNode {
                type_name: tname(type_name),
            })
        })
        .collect();

    let found: Vec<Option<NativeNode>> = native_ids_with_types()
        .iter()
        .map(|(id, _)| native.lookup(&sid(id)))
        .collect();

    assert_eq!(found, expected);
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
fn it_should_list_the_native_ids_in_the_contract_order() {
    let native = NativeBase;

    let check_ids: Vec<&str> = native.checks().iter().map(|check| check.id).collect();
    let guideline_ids: Vec<&str> = native
        .guidelines()
        .iter()
        .map(|guideline| guideline.id)
        .collect();

    assert_eq!(
        (check_ids, guideline_ids),
        (CHECK_IDS.to_vec(), GUIDELINE_IDS.to_vec())
    );
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
        (CONFIG_LOOSENING_REVIEW, "before-pr", true),
    ];

    let found: Vec<(&str, &str, bool)> = native
        .checks()
        .iter()
        .map(|check| (check.id, check.event, check.blocking))
        .collect();

    assert_eq!(found, expected);
}

#[test]
fn it_should_expose_the_native_catalog_version() {
    let native = NativeBase;

    let versions = (native.catalog_version(), NATIVE_CATALOG_VERSION);

    assert_eq!(versions, ("1.1.0", "1.1.0"));
}

#[test]
fn it_should_expose_the_guideline_and_check_data_as_non_empty_texts_without_carriage_returns() {
    let native = NativeBase;

    let texts: Vec<&str> = native
        .checks()
        .iter()
        .map(|check| check.how)
        .chain(native.guidelines().iter().map(|guideline| guideline.text))
        .collect();

    let malformed: Vec<&str> = texts
        .iter()
        .copied()
        .filter(|text| text.is_empty() || text.contains('\r'))
        .collect();
    assert_eq!((texts.len(), malformed), (6, Vec::new()));
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
