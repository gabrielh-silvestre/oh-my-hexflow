use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::model::{Extension, ExtensionIssue};

pub(crate) const LOADER_CLIENT: &str = "loader";
const CHECK_TYPE: &str = "check";

#[derive(Clone, Debug, PartialEq)]
pub struct ExistingNode {
    pub id: String,
    pub type_name: String,
    pub data: Value,
    pub record_id: String,
    pub retired: bool,
    pub author_client: String,
}

/// Só a relação `after`: `from` depende de `to`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExistingRelation {
    pub from: String,
    pub to: String,
    pub retired: bool,
    pub author_client: String,
}

pub struct PlanInput<'a> {
    pub extensions: &'a [Extension],
    pub existing_nodes: &'a [ExistingNode],
    pub existing_relations: &'a [ExistingRelation],
    pub native_check_ids: &'a [String],
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlannedChange {
    CreateNode {
        id: String,
        type_name: String,
        data: Value,
        path: String,
    },
    ReviseNode {
        id: String,
        type_name: String,
        data: Value,
        base: String,
        path: String,
    },
    CreateRelation {
        from: String,
        to: String,
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub id: String,
    pub code: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub changes: Vec<PlannedChange>,
    pub unchanged: Vec<String>,
    pub skipped: Vec<Skipped>,
    pub orphans: Vec<String>,
    pub orphan_relations: Vec<(String, String)>,
    pub warnings: Vec<ExtensionIssue>,
}

enum Target {
    Live,
    Retired,
    Unknown,
}

struct Context<'a> {
    nodes: HashMap<&'a str, &'a ExistingNode>,
    /// Declared id mapped to "the file is skipped because the store retired it".
    declared: HashMap<&'a str, bool>,
    natives: &'a [String],
    live: HashSet<(&'a str, &'a str)>,
    retired: HashSet<(&'a str, &'a str)>,
}

/// Reconciles the declared extensions with the store without writing anything.
pub fn plan(input: &PlanInput) -> Result<Plan, Vec<ExtensionIssue>> {
    let mut extensions: Vec<&Extension> = input.extensions.iter().collect();
    extensions.sort_by(|a, b| a.id.cmp(&b.id));
    let nodes: HashMap<&str, &ExistingNode> = input
        .existing_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let mut plan = Plan::default();
    let mut declared = HashMap::new();
    for extension in &extensions {
        let skipped = plan_node(
            extension,
            nodes.get(extension.id.as_str()).copied(),
            &mut plan,
        );
        declared.insert(extension.id.as_str(), skipped);
    }
    let context = Context {
        nodes,
        declared,
        natives: input.native_check_ids,
        live: relation_pairs(input.existing_relations, false),
        retired: relation_pairs(input.existing_relations, true),
    };

    plan_relations(&extensions, &context, &mut plan)?;
    plan.orphans = orphan_nodes(input.existing_nodes, &context.declared);
    plan.orphan_relations = orphan_relations(&extensions, input.existing_relations);
    plan.warnings
        .sort_by(|a, b| (&a.path, &a.message).cmp(&(&b.path, &b.message)));
    Ok(plan)
}

/// Records what happens to the node of `extension`; returns true when it is skipped.
fn plan_node(extension: &Extension, existing: Option<&ExistingNode>, plan: &mut Plan) -> bool {
    let id = extension.id.clone();
    match existing {
        None => plan.changes.push(PlannedChange::CreateNode {
            id,
            type_name: extension.kind.type_name().to_owned(),
            data: extension.data.clone(),
            path: extension.path.clone(),
        }),
        Some(node) if node.retired => {
            plan.skipped.push(Skipped {
                id,
                code: "extension-retired",
            });
            return true;
        }
        Some(node) if node.data == extension.data => plan.unchanged.push(id),
        Some(node) => plan.changes.push(PlannedChange::ReviseNode {
            id,
            type_name: extension.kind.type_name().to_owned(),
            data: extension.data.clone(),
            base: node.record_id.clone(),
            path: extension.path.clone(),
        }),
    }
    false
}

fn resolve_target(id: &str, context: &Context) -> Target {
    if let Some(&skipped) = context.declared.get(id) {
        return if skipped {
            Target::Retired
        } else {
            Target::Live
        };
    }
    if context.natives.iter().any(|native| native == id) {
        return Target::Live;
    }
    match context.nodes.get(id) {
        Some(node) if node.type_name == CHECK_TYPE && node.retired => Target::Retired,
        Some(node) if node.type_name == CHECK_TYPE => Target::Live,
        _ => Target::Unknown,
    }
}

fn plan_relations(
    extensions: &[&Extension],
    context: &Context,
    plan: &mut Plan,
) -> Result<(), Vec<ExtensionIssue>> {
    let mut creations: Vec<(&str, &str, &str)> = Vec::new();
    let mut errors = Vec::new();

    for extension in extensions
        .iter()
        .filter(|e| !context.declared[e.id.as_str()])
    {
        plan_after_targets(extension, context, plan, &mut creations, &mut errors);
    }

    if !errors.is_empty() {
        errors.sort_by(|a, b| (&a.path, &a.message).cmp(&(&b.path, &b.message)));
        return Err(errors);
    }
    push_relation_creations(creations, plan);
    Ok(())
}

fn plan_after_targets<'e>(
    extension: &'e Extension,
    context: &Context,
    plan: &mut Plan,
    creations: &mut Vec<(&'e str, &'e str, &'e str)>,
    errors: &mut Vec<ExtensionIssue>,
) {
    for target in &extension.after {
        let pair = (extension.id.as_str(), target.as_str());
        let issue = |code, message| ExtensionIssue {
            path: extension.path.clone(),
            code,
            message,
            next_step: None,
        };
        match resolve_target(target, context) {
            Target::Unknown => errors.push(issue(
                "unknown-after-target",
                format!("{} is not declared, native or live in the store", target),
            )),
            Target::Retired => plan.warnings.push(issue(
                "after-target-retired",
                format!("{} is retired; no relation planned", target),
            )),
            Target::Live if context.live.contains(&pair) => {}
            Target::Live if context.retired.contains(&pair) => plan.warnings.push(issue(
                "relation-retired",
                format!("the relation to {} is retired; no relation planned", target),
            )),
            Target::Live => creations.push((pair.0, pair.1, extension.path.as_str())),
        }
    }
}

fn push_relation_creations(mut creations: Vec<(&str, &str, &str)>, plan: &mut Plan) {
    creations.sort();
    creations.dedup_by_key(|(from, to, _)| (*from, *to));
    plan.changes
        .extend(
            creations
                .into_iter()
                .map(|(from, to, path)| PlannedChange::CreateRelation {
                    from: from.to_owned(),
                    to: to.to_owned(),
                    path: path.to_owned(),
                }),
        );
}

fn relation_pairs(relations: &[ExistingRelation], retired: bool) -> HashSet<(&str, &str)> {
    relations
        .iter()
        .filter(|relation| relation.retired == retired)
        .map(|relation| (relation.from.as_str(), relation.to.as_str()))
        .collect()
}

fn orphan_nodes(nodes: &[ExistingNode], declared: &HashMap<&str, bool>) -> Vec<String> {
    let mut orphans: Vec<String> = nodes
        .iter()
        .filter(|node| !node.retired && node.author_client == LOADER_CLIENT)
        .filter(|node| !declared.contains_key(node.id.as_str()))
        .map(|node| node.id.clone())
        .collect();
    orphans.sort();
    orphans
}

fn orphan_relations(
    extensions: &[&Extension],
    relations: &[ExistingRelation],
) -> Vec<(String, String)> {
    let declared: HashSet<(&str, &str)> = extensions
        .iter()
        .flat_map(|e| e.after.iter().map(|to| (e.id.as_str(), to.as_str())))
        .collect();
    let mut orphans: Vec<(String, String)> = relations
        .iter()
        .filter(|relation| !relation.retired && relation.author_client == LOADER_CLIENT)
        .filter(|relation| !declared.contains(&(relation.from.as_str(), relation.to.as_str())))
        .map(|relation| (relation.from.clone(), relation.to.clone()))
        .collect();
    orphans.sort();
    orphans
}
