use crate::extensions::model::{Extension, ExtensionIssue, ExtensionKind};
use crate::extensions::plan::{
    ExistingNode, ExistingRelation, Plan, PlanInput, PlannedChange, plan,
};
use serde_json::{Value, json};

const LOADER: &str = "loader";
const NATIVE_CHECK: &str = "checks.run-tests";

fn check_data(how: &str) -> Value {
    json!({
        "blocking": true,
        "how": how,
        "when": {"vocabulary": "closed", "event": "before-commit"}
    })
}

fn check_with_after(slug: &str, how: &str, after: &[&str]) -> Extension {
    Extension {
        kind: ExtensionKind::Check,
        id: format!("checks.{slug}"),
        path: format!(".omhl/checks/{slug}.toml"),
        data: check_data(how),
        after: after.iter().map(|id| (*id).to_owned()).collect(),
    }
}

fn check(slug: &str, how: &str) -> Extension {
    check_with_after(slug, how, &[])
}

fn guideline(slug: &str, text: &str) -> Extension {
    Extension {
        kind: ExtensionKind::Guideline,
        id: format!("guidelines.{slug}"),
        path: format!(".omhl/guidelines/{slug}.toml"),
        data: json!({"text": text}),
        after: Vec::new(),
    }
}

fn stored(extension: &Extension, record_id: &str) -> ExistingNode {
    ExistingNode {
        id: extension.id.clone(),
        type_name: extension.kind.type_name().to_owned(),
        data: extension.data.clone(),
        record_id: record_id.to_owned(),
        retired: false,
        author_client: LOADER.to_owned(),
    }
}

fn retired(node: ExistingNode) -> ExistingNode {
    ExistingNode {
        retired: true,
        ..node
    }
}

fn relation(from: &str, to: &str) -> ExistingRelation {
    ExistingRelation {
        from: from.to_owned(),
        to: to.to_owned(),
        retired: false,
        author_client: LOADER.to_owned(),
    }
}

fn run(
    extensions: &[Extension],
    nodes: &[ExistingNode],
    relations: &[ExistingRelation],
    natives: &[&str],
) -> Result<Plan, Vec<ExtensionIssue>> {
    let native_check_ids: Vec<String> = natives.iter().map(|id| (*id).to_owned()).collect();

    plan(&PlanInput {
        extensions,
        existing_nodes: nodes,
        existing_relations: relations,
        native_check_ids: &native_check_ids,
    })
}

fn planned(
    extensions: &[Extension],
    nodes: &[ExistingNode],
    relations: &[ExistingRelation],
    natives: &[&str],
) -> Plan {
    run(extensions, nodes, relations, natives)
        .unwrap_or_else(|issues| panic!("the plan is accepted, got {issues:?}"))
}

fn refused(
    extensions: &[Extension],
    nodes: &[ExistingNode],
    relations: &[ExistingRelation],
) -> Vec<ExtensionIssue> {
    run(extensions, nodes, relations, &[]).expect_err("the plan is refused")
}

fn create_node(extension: &Extension) -> PlannedChange {
    PlannedChange::CreateNode {
        id: extension.id.clone(),
        type_name: extension.kind.type_name().to_owned(),
        data: extension.data.clone(),
        path: extension.path.clone(),
    }
}

fn create_relation(from: &Extension, to: &str) -> PlannedChange {
    PlannedChange::CreateRelation {
        from: from.id.clone(),
        to: to.to_owned(),
        path: from.path.clone(),
    }
}

fn skipped(plan: &Plan) -> Vec<(&str, &str)> {
    plan.skipped
        .iter()
        .map(|entry| (entry.id.as_str(), entry.code))
        .collect()
}

fn warning_codes(plan: &Plan) -> Vec<&str> {
    plan.warnings.iter().map(|issue| issue.code).collect()
}

fn is_relation(change: &PlannedChange) -> bool {
    matches!(change, PlannedChange::CreateRelation { .. })
}

type Summary = (
    Vec<PlannedChange>,
    Vec<String>,
    Vec<(String, String)>,
    Vec<String>,
    Vec<(String, String)>,
    Vec<ExtensionIssue>,
);

fn summary(plan: &Plan) -> Summary {
    (
        plan.changes.clone(),
        plan.unchanged.clone(),
        plan.skipped
            .iter()
            .map(|entry| (entry.id.clone(), entry.code.to_owned()))
            .collect(),
        plan.orphans.clone(),
        plan.orphan_relations.clone(),
        plan.warnings.clone(),
    )
}

/// One declared set that exercises every plan list, with the three inputs ordered by `variant`.
fn scenario(variant: usize) -> (Vec<Extension>, Vec<ExistingNode>, Vec<ExistingRelation>) {
    let first = check_with_after("a", "Run a", &["checks.b", "checks.c", "checks.gone"]);
    let second = check("b", "Run b new");
    let third = check("c", "Run c");
    let fourth = check("d", "Run d");
    let fifth = guideline("g", "Be brief");
    let mut extensions = vec![first, second, third, fourth, fifth];
    let mut nodes = vec![
        stored(&check("a", "Run a"), "r-a"),
        stored(&check("b", "Run b old"), "r-b"),
        retired(stored(&check("gone", "Run gone"), "r-gone")),
        stored(&guideline("g", "Be brief"), "r-g"),
        stored(&check("old", "Run old"), "r-old"),
        retired(stored(&check("d", "Run d"), "r-d")),
    ];
    let mut relations = vec![
        relation("checks.x", "checks.y"),
        relation("checks.a", "checks.c"),
    ];

    match variant {
        0 => {}
        1 => {
            extensions.reverse();
            nodes.reverse();
            relations.reverse();
        }
        _ => {
            extensions.rotate_left(2);
            nodes.rotate_left(3);
            relations.rotate_left(1);
        }
    }

    (extensions, nodes, relations)
}

#[test]
fn it_should_create_a_node_for_a_new_extension() {
    let extension = check("a", "Run a");

    let plan = planned(std::slice::from_ref(&extension), &[], &[], &[]);

    assert_eq!(plan.changes, vec![create_node(&extension)]);
}

#[test]
fn it_should_create_a_guideline_node() {
    let extension = guideline("tone", "Be brief");

    let plan = planned(std::slice::from_ref(&extension), &[], &[], &[]);

    assert_eq!(
        plan.changes,
        vec![PlannedChange::CreateNode {
            id: "guidelines.tone".to_owned(),
            type_name: "guideline".to_owned(),
            data: json!({"text": "Be brief"}),
            path: ".omhl/guidelines/tone.toml".to_owned(),
        }]
    );
}

#[test]
fn it_should_not_change_an_identical_node() {
    let extension = check("a", "Run a");
    let nodes = [stored(&extension, "r1")];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert!(plan.changes.is_empty());
    assert_eq!(plan.unchanged, vec!["checks.a".to_owned()]);
}

#[test]
fn it_should_revise_a_node_whose_data_changed() {
    let extension = check("a", "Run a new");
    let nodes = [stored(&check("a", "Run a old"), "r1")];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert_eq!(
        plan.changes,
        vec![PlannedChange::ReviseNode {
            id: "checks.a".to_owned(),
            type_name: "check".to_owned(),
            data: extension.data.clone(),
            base: "r1".to_owned(),
            path: extension.path,
        }]
    );
}

#[test]
fn it_should_compare_data_by_value() {
    let extension = check("a", "Run a");
    let reordered: Value = serde_json::from_str(
        r#"{"when":{"event":"before-commit","vocabulary":"closed"},"how":"Run a","blocking":true}"#,
    )
    .expect("the stored data parses");
    let nodes = [ExistingNode {
        data: reordered,
        ..stored(&extension, "r1")
    }];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert_eq!(plan.unchanged, vec!["checks.a".to_owned()]);
}

#[test]
fn it_should_revise_a_node_authored_by_someone_else_when_a_file_declares_it() {
    let extension = check("a", "Run a new");
    let nodes = [ExistingNode {
        author_client: "cli".to_owned(),
        ..stored(&check("a", "Run a old"), "r1")
    }];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert_eq!(
        plan.changes,
        vec![PlannedChange::ReviseNode {
            id: "checks.a".to_owned(),
            type_name: "check".to_owned(),
            data: extension.data.clone(),
            base: "r1".to_owned(),
            path: extension.path,
        }]
    );
}

#[test]
fn it_should_skip_a_retired_check() {
    let extension = check("a", "Run a new");
    let nodes = [retired(stored(&check("a", "Run a old"), "r1"))];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert_eq!(skipped(&plan), vec![("checks.a", "extension-retired")]);
    assert!(plan.changes.is_empty());
}

#[test]
fn it_should_skip_a_retired_guideline() {
    let extension = guideline("tone", "Be brief");
    let nodes = [retired(stored(&guideline("tone", "Be long"), "r1"))];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert_eq!(
        skipped(&plan),
        vec![("guidelines.tone", "extension-retired")]
    );
    assert!(plan.changes.is_empty());
}

#[test]
fn it_should_not_plan_relations_of_a_skipped_extension() {
    let retired_extension = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");
    let nodes = [retired(stored(&retired_extension, "r1"))];

    let plan = planned(&[retired_extension, target.clone()], &nodes, &[], &[]);

    assert_eq!(plan.changes, vec![create_node(&target)]);
    assert!(plan.warnings.is_empty());
}

#[test]
fn it_should_create_an_after_relation() {
    let dependent = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");

    let plan = planned(&[dependent.clone(), target.clone()], &[], &[], &[]);

    assert_eq!(
        plan.changes,
        vec![
            create_node(&dependent),
            create_node(&target),
            create_relation(&dependent, "checks.b"),
        ]
    );
}

#[test]
fn it_should_not_recreate_a_live_relation() {
    let dependent = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");
    let nodes = [stored(&dependent, "r1"), stored(&target, "r2")];
    let relations = [relation("checks.a", "checks.b")];

    let plan = planned(&[dependent, target], &nodes, &relations, &[]);

    assert!(plan.changes.is_empty());
}

#[test]
fn it_should_warn_relation_retired_and_emit_no_item() {
    let dependent = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");
    let nodes = [stored(&dependent, "r1"), stored(&target, "r2")];
    let relations = [ExistingRelation {
        retired: true,
        ..relation("checks.a", "checks.b")
    }];

    let plan = planned(&[dependent, target], &nodes, &relations, &[]);

    assert_eq!(warning_codes(&plan), vec!["relation-retired"]);
    assert!(plan.changes.is_empty());
}

#[test]
fn it_should_accept_after_to_a_declared_check() {
    let dependent = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");
    let nodes = [stored(&dependent, "r1"), stored(&target, "r2")];

    let plan = planned(&[dependent.clone(), target], &nodes, &[], &[]);

    assert_eq!(plan.changes, vec![create_relation(&dependent, "checks.b")]);
}

#[test]
fn it_should_accept_after_to_a_native_check() {
    let dependent = check_with_after("a", "Run a", &[NATIVE_CHECK]);

    let plan = planned(std::slice::from_ref(&dependent), &[], &[], &[NATIVE_CHECK]);

    assert_eq!(
        plan.changes,
        vec![
            create_node(&dependent),
            create_relation(&dependent, NATIVE_CHECK)
        ]
    );
}

#[test]
fn it_should_accept_after_to_a_live_check_in_the_store() {
    let dependent = check_with_after("a", "Run a", &["checks.stored"]);
    let nodes = [stored(&check("stored", "Run stored"), "r1")];

    let plan = planned(std::slice::from_ref(&dependent), &nodes, &[], &[]);

    assert_eq!(
        plan.changes,
        vec![
            create_node(&dependent),
            create_relation(&dependent, "checks.stored")
        ]
    );
}

#[test]
fn it_should_warn_after_target_retired_for_a_retired_store_check() {
    let dependent = check_with_after("a", "Run a", &["checks.old"]);
    let nodes = [retired(stored(&check("old", "Run old"), "r1"))];

    let plan = planned(std::slice::from_ref(&dependent), &nodes, &[], &[]);

    assert_eq!(warning_codes(&plan), vec!["after-target-retired"]);
    assert_eq!(plan.changes, vec![create_node(&dependent)]);
}

#[test]
fn it_should_warn_after_target_retired_for_a_skipped_extension() {
    let dependent = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");
    let nodes = [retired(stored(&target, "r1"))];

    let plan = planned(&[dependent.clone(), target], &nodes, &[], &[]);

    assert_eq!(warning_codes(&plan), vec!["after-target-retired"]);
    assert_eq!(plan.changes, vec![create_node(&dependent)]);
}

#[test]
fn it_should_error_unknown_after_target_for_a_missing_id() {
    let dependent = check_with_after("a", "Run a", &["checks.ghost"]);

    let issues = refused(std::slice::from_ref(&dependent), &[], &[]);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "unknown-after-target");
    assert_eq!(issues[0].path, ".omhl/checks/a.toml");
}

#[test]
fn it_should_collect_every_unknown_after_target_error() {
    let extensions = [
        check_with_after("z", "Run z", &["checks.ghost"]),
        check_with_after("x", "Run x", &["checks.ghost"]),
        check_with_after("y", "Run y", &["checks.ghost"]),
    ];

    let issues = refused(&extensions, &[], &[]);

    let found: Vec<(&str, &str)> = issues
        .iter()
        .map(|issue| (issue.path.as_str(), issue.code))
        .collect();
    assert_eq!(
        found,
        vec![
            (".omhl/checks/x.toml", "unknown-after-target"),
            (".omhl/checks/y.toml", "unknown-after-target"),
            (".omhl/checks/z.toml", "unknown-after-target"),
        ]
    );
}

#[test]
fn it_should_report_orphan_nodes_authored_by_the_loader() {
    let extension = check("a", "Run a");
    let nodes = [stored(&check("old", "Run old"), "r1")];

    let plan = planned(std::slice::from_ref(&extension), &nodes, &[], &[]);

    assert_eq!(plan.orphans, vec!["checks.old".to_owned()]);
    assert_eq!(plan.changes, vec![create_node(&extension)]);
}

#[test]
fn it_should_not_report_orphans_authored_by_others() {
    let nodes = [ExistingNode {
        author_client: "cli".to_owned(),
        ..stored(&check("old", "Run old"), "r1")
    }];

    let plan = planned(&[], &nodes, &[], &[]);

    assert!(plan.orphans.is_empty());
}

#[test]
fn it_should_not_report_retired_nodes_as_orphans() {
    let nodes = [retired(stored(&check("old", "Run old"), "r1"))];

    let plan = planned(&[], &nodes, &[], &[]);

    assert!(plan.orphans.is_empty());
}

#[test]
fn it_should_report_orphan_relations_authored_by_the_loader() {
    let relations = [relation("checks.x", "checks.y")];

    let plan = planned(&[], &[], &relations, &[]);

    assert_eq!(
        plan.orphan_relations,
        vec![("checks.x".to_owned(), "checks.y".to_owned())]
    );
}

#[test]
fn it_should_not_report_a_declared_relation_as_orphan() {
    let dependent = check_with_after("a", "Run a", &["checks.b"]);
    let target = check("b", "Run b");
    let nodes = [stored(&dependent, "r1"), stored(&target, "r2")];
    let relations = [relation("checks.a", "checks.b")];

    let plan = planned(&[dependent, target], &nodes, &relations, &[]);

    assert!(plan.orphan_relations.is_empty());
}

#[test]
fn it_should_order_changes_nodes_by_id_then_relations_by_pair() {
    let c = check_with_after("c", "Run c", &["checks.b", "checks.a"]);
    let b = check_with_after("b", "Run b", &["checks.a"]);
    let a = check("a", "Run a");

    let plan = planned(&[c.clone(), b.clone(), a.clone()], &[], &[], &[]);

    assert_eq!(
        plan.changes,
        vec![
            create_node(&a),
            create_node(&b),
            create_node(&c),
            create_relation(&b, "checks.a"),
            create_relation(&c, "checks.a"),
            create_relation(&c, "checks.b"),
        ]
    );
}

#[test]
fn it_should_put_every_node_before_every_relation() {
    let dependent = check_with_after("a", "Run a", &["checks.b", "checks.c"]);
    let revised = check("b", "Run b new");
    let created = check("c", "Run c");
    let nodes = [stored(&check("b", "Run b old"), "r1")];

    let plan = planned(&[dependent, revised, created], &nodes, &[], &[]);

    let first_relation = plan
        .changes
        .iter()
        .position(is_relation)
        .expect("the plan holds relations");
    assert!(plan.changes[first_relation..].iter().all(is_relation));
    assert!(first_relation > 0);
}

#[test]
fn it_should_return_an_empty_plan_for_no_extensions() {
    let nodes = [stored(&check("old", "Run old"), "r1")];

    let plan = planned(&[], &nodes, &[], &[]);

    assert!(plan.changes.is_empty());
    assert!(plan.unchanged.is_empty());
    assert!(plan.skipped.is_empty());
    assert_eq!(plan.orphans, vec!["checks.old".to_owned()]);
}

#[test]
fn it_should_be_deterministic_for_shuffled_input() {
    let (extensions, nodes, relations) = scenario(0);
    let baseline = summary(&planned(&extensions, &nodes, &relations, &[]));

    let (extensions, nodes, relations) = scenario(1);
    let reversed = summary(&planned(&extensions, &nodes, &relations, &[]));

    let (extensions, nodes, relations) = scenario(2);
    let rotated = summary(&planned(&extensions, &nodes, &relations, &[]));

    assert_eq!(reversed, baseline);
    assert_eq!(rotated, baseline);
}
