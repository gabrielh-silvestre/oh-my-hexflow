use crate::extensions::collect::collect;
use crate::extensions::model::ExtensionIssue;
use crate::test_support::ext::ExtTree;

const VALID_CHECK: &str =
    "blocking = true\nhow = \"Run it\"\n\n[when]\nevent = \"before-commit\"\n";
const VALID_GUIDELINE: &str = "kind = \"business\"\ntext = \"Be brief\"\n";
const INVALID_TOML: &str = "this is = [not valid";

fn summary(issues: &[ExtensionIssue]) -> Vec<(String, &'static str)> {
    issues
        .iter()
        .map(|issue| (issue.path.clone(), issue.code))
        .collect()
}

fn at(path: &str, code: &'static str) -> (String, &'static str) {
    (path.to_owned(), code)
}

fn ids(extensions: &[crate::extensions::model::Extension]) -> Vec<&str> {
    extensions
        .iter()
        .map(|extension| extension.id.as_str())
        .collect()
}

#[test]
fn it_should_collect_valid_checks_and_guidelines() {
    let tree = ExtTree::new();
    tree.check("lint", VALID_CHECK)
        .guideline("tone", VALID_GUIDELINE);

    let collected = collect(tree.root());

    assert_eq!(collected.extensions.len(), 2);
    assert!(collected.errors.is_empty(), "errors must be empty");
    assert!(collected.warnings.is_empty(), "warnings must be empty");
}

#[test]
fn it_should_return_extensions_sorted_by_kind_and_slug() {
    let tree = ExtTree::new();
    tree.guideline("a", VALID_GUIDELINE)
        .check("z", VALID_CHECK)
        .check("b", VALID_CHECK);

    let collected = collect(tree.root());

    assert_eq!(
        ids(&collected.extensions),
        vec!["checks.b", "checks.z", "guidelines.a"]
    );
}

#[test]
fn it_should_aggregate_errors_from_scan_and_parse() {
    let tree = ExtTree::new();
    tree.check("Bad Name", VALID_CHECK).check("x", INVALID_TOML);

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.errors),
        vec![
            at(".omhl/checks/Bad Name.toml", "bad-file-name"),
            at(".omhl/checks/x.toml", "invalid-toml"),
        ]
    );
}

#[test]
fn it_should_report_every_invalid_file_not_only_the_first() {
    let tree = ExtTree::new();
    tree.check("a", INVALID_TOML)
        .check("b", INVALID_TOML)
        .guideline("c", INVALID_TOML);

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.errors),
        vec![
            at(".omhl/checks/a.toml", "invalid-toml"),
            at(".omhl/checks/b.toml", "invalid-toml"),
            at(".omhl/guidelines/c.toml", "invalid-toml"),
        ]
    );
}

#[test]
fn it_should_cap_errors_at_20_and_count_the_omitted() {
    let tree = ExtTree::new();
    for index in 0..25 {
        tree.check(&format!("f{index:02}"), INVALID_TOML);
    }

    let collected = collect(tree.root());

    assert_eq!(collected.errors.len(), 20);
    assert_eq!(collected.omitted_errors, 5);
}

#[test]
fn it_should_reject_a_native_check_id() {
    let tree = ExtTree::new();
    tree.check("run-tests", VALID_CHECK);

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.errors),
        vec![at(".omhl/checks/run-tests.toml", "reserved-id")]
    );
    assert!(collected.extensions.is_empty(), "extensions must be empty");
}

#[test]
fn it_should_reject_a_native_guideline_id() {
    let tree = ExtTree::new();
    tree.guideline("commit-policy", VALID_GUIDELINE);

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.errors),
        vec![at(".omhl/guidelines/commit-policy.toml", "reserved-id")]
    );
    assert!(collected.extensions.is_empty(), "extensions must be empty");
}

#[test]
fn it_should_accept_a_slug_that_only_resembles_a_native_id() {
    let tree = ExtTree::new();
    tree.check("run-tests-2", VALID_CHECK);

    let collected = collect(tree.root());

    assert_eq!(ids(&collected.extensions), vec!["checks.run-tests-2"]);
    assert!(collected.errors.is_empty(), "errors must be empty");
}

#[test]
fn it_should_forward_scan_warnings() {
    let tree = ExtTree::new();
    tree.file(".omhl/checks/.gitkeep", "")
        .file(".omhl/notes.txt", "x");

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.warnings),
        vec![
            at(".omhl/checks/.gitkeep", "hidden-file-ignored"),
            at(".omhl/notes.txt", "unknown-entry"),
        ]
    );
}

#[test]
fn it_should_return_nothing_for_a_project_without_omhl() {
    let tree = ExtTree::new();

    let collected = collect(tree.root());

    assert!(collected.extensions.is_empty(), "extensions must be empty");
    assert!(collected.errors.is_empty(), "errors must be empty");
    assert!(collected.warnings.is_empty(), "warnings must be empty");
    assert_eq!(collected.omitted_errors, 0);
}

#[test]
fn it_should_still_return_the_valid_extensions_next_to_errors() {
    let tree = ExtTree::new();
    tree.check("good", VALID_CHECK)
        .check("broken", INVALID_TOML);

    let collected = collect(tree.root());

    assert_eq!(ids(&collected.extensions), vec!["checks.good"]);
    assert_eq!(collected.errors.len(), 1);
}

#[test]
fn it_should_order_errors_by_path_then_code() {
    let tree = ExtTree::new();
    tree.check("b", INVALID_TOML)
        .check("a", INVALID_TOML)
        .guideline("a", INVALID_TOML);

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.errors),
        vec![
            at(".omhl/checks/a.toml", "invalid-toml"),
            at(".omhl/checks/b.toml", "invalid-toml"),
            at(".omhl/guidelines/a.toml", "invalid-toml"),
        ]
    );
}

#[test]
fn it_should_ignore_the_specs_directory_named_by_the_workflow_file() {
    let tree = ExtTree::new();
    tree.file(
        ".omhl/workflow.toml",
        "version = 1\n[specs]\ndir = \".omhl/my-specs\"\n",
    )
    .file(".omhl/my-specs/wi-dir0__x/part-1.md", "spec text\n");

    let collected = collect(tree.root());

    assert!(collected.errors.is_empty(), "errors must be empty");
    assert!(collected.warnings.is_empty(), "warnings must be empty");
}

#[test]
fn it_should_fall_back_to_the_defaults_and_warn_when_the_workflow_file_is_invalid() {
    let tree = ExtTree::new();
    tree.file(".omhl/workflow.toml", "version = 1\ncolour = true\n")
        .file(".omhl/my-specs/wi-dir0__x/part-1.md", "spec text\n");

    let collected = collect(tree.root());

    assert!(collected.errors.is_empty(), "errors must be empty");
    assert_eq!(
        summary(&collected.warnings),
        vec![
            at(".omhl/my-specs", "unknown-entry"),
            at(".omhl/workflow.toml", "invalid-workflow-config")
        ]
    );
}

#[test]
fn it_should_leave_the_scan_unchanged_when_specs_dir_is_outside_omhl() {
    let tree = ExtTree::new();
    tree.file(
        ".omhl/workflow.toml",
        "version = 1\n[specs]\ndir = \"docs/specs\"\n",
    )
    .file(".omhl/my-specs/wi-dir0__x/part-1.md", "spec text\n");

    let collected = collect(tree.root());

    assert_eq!(
        summary(&collected.warnings),
        vec![at(".omhl/my-specs", "unknown-entry")]
    );
}
