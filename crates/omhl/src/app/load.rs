use hexlog::chain::fingerprint;
use hexlog::{
    AppendItem, AppendRequest, IdempotencyKey, ItemBody, Lineage, NodeView, RecordId,
    RelationQuery, RelationTypeName, SemanticId, TenantId, TypeName,
};
use serde_json::{Value, json};

use super::action::Caller;
use super::engine::{Engine, require_project_root};
use super::error::AppError;
use super::read::all_nodes;
use crate::extensions::collect::collect;
use crate::extensions::model::{Extension, ExtensionIssue, ExtensionKind};
use crate::extensions::plan::{
    ExistingNode, ExistingRelation, Plan, PlanInput, PlannedChange, plan,
};
use crate::facade::Omhl;
use crate::native::NativeBase;

const BATCH_SIZE: usize = 50;
const AFTER: &str = "after";
const KEY_PREFIX: &str = "omhl-load-";

impl Engine<'_> {
    /// Reconcilia `.omhl/` com o tenant; `dry_run` devolve o mesmo relatório sem gravar.
    pub(super) fn load(&mut self, dry_run: bool) -> Result<Value, AppError> {
        let Engine {
            omhl,
            tenant,
            project_root,
            ..
        } = self;
        let root = require_project_root(project_root.as_deref())?;
        let collected = collect(root);
        if !collected.errors.is_empty() {
            return Err(AppError::extensions_invalid(
                &collected.errors,
                collected.omitted_errors,
            ));
        }
        let reconciled = reconcile(omhl, tenant, &collected.extensions)?;
        let report = report(tenant, dry_run, &reconciled, &collected.warnings);
        if !dry_run {
            write_changes(omhl, tenant, reconciled.changes)?;
        }

        Ok(report)
    }
}

fn reconcile(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    extensions: &[Extension],
) -> Result<Plan, AppError> {
    let natives: Vec<String> = NativeBase
        .checks()
        .iter()
        .map(|check| check.id.to_owned())
        .collect();
    let existing_nodes = existing_nodes(omhl, tenant, extensions, &natives)?;
    let existing_relations = existing_relations(omhl, tenant)?;
    let input = PlanInput {
        extensions,
        existing_nodes: &existing_nodes,
        existing_relations: &existing_relations,
        native_check_ids: &natives,
    };

    plan(&input).map_err(|issues| AppError::extensions_invalid(&issues, 0))
}

/// Os vigentes do banco mais as lineages aposentadas que as extensions declaram ou citam em `after`.
fn existing_nodes(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    extensions: &[Extension],
    natives: &[String],
) -> Result<Vec<ExistingNode>, AppError> {
    let mut nodes = Vec::new();
    for kind in [ExtensionKind::Check, ExtensionKind::Guideline] {
        let live = all_nodes(omhl, tenant, kind.type_name())?;
        nodes.extend(live.into_iter().map(live_node));
    }
    let mut ids: Vec<&str> = extensions
        .iter()
        .flat_map(|extension| {
            std::iter::once(extension.id.as_str()).chain(extension.after.iter().map(String::as_str))
        })
        .filter(|id| !nodes.iter().any(|node| node.id == *id) && !natives.iter().any(|n| n == id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        nodes.extend(retired_node(omhl, tenant, id)?);
    }

    Ok(nodes)
}

fn live_node(view: NodeView) -> ExistingNode {
    ExistingNode {
        id: view.id.as_str().to_owned(),
        type_name: view.type_name.as_str().to_owned(),
        data: view.data,
        record_id: view.record_id.as_str().to_owned(),
        retired: false,
        author_client: view.author.client,
    }
}

fn retired_node(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    id: &str,
) -> Result<Option<ExistingNode>, AppError> {
    let lineage = Lineage::Node(SemanticId::parse(id)?);
    let last = omhl.lineage_records(tenant, &lineage)?.pop();
    let kind = match id.starts_with(ExtensionKind::Guideline.id_prefix()) {
        true => ExtensionKind::Guideline,
        false => ExtensionKind::Check,
    };

    Ok(last
        .filter(|revision| revision.retired)
        .map(|revision| ExistingNode {
            id: id.to_owned(),
            type_name: kind.type_name().to_owned(),
            data: Value::Null,
            record_id: revision.record_id.as_str().to_owned(),
            retired: true,
            author_client: revision.author.client,
        }))
}

fn existing_relations(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
) -> Result<Vec<ExistingRelation>, AppError> {
    let query = RelationQuery {
        relation_type: Some(RelationTypeName::parse(AFTER)?),
        from: None,
        to: None,
        include_retired: true,
    };

    Ok(omhl
        .relations(tenant, &query)?
        .into_iter()
        .map(|relation| ExistingRelation {
            from: relation.from.as_str().to_owned(),
            to: relation.to.as_str().to_owned(),
            retired: relation.retired,
            author_client: relation.author.client,
        })
        .collect())
}

fn report(tenant: &TenantId, dry_run: bool, plan: &Plan, scanned: &[ExtensionIssue]) -> Value {
    let [created, revised, relations_created] = split_changes(&plan.changes);
    let skipped: Vec<Value> = plan
        .skipped
        .iter()
        .map(|skipped| json!({ "id": skipped.id, "code": skipped.code }))
        .collect();
    let orphan_relations: Vec<Value> = plan
        .orphan_relations
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    let mut warnings: Vec<&ExtensionIssue> = scanned.iter().chain(&plan.warnings).collect();
    warnings.sort_by_key(|issue| (&issue.path, issue.code));
    let warnings: Vec<Value> = warnings
        .iter()
        .map(|issue| json!({ "path": issue.path, "code": issue.code, "message": issue.message }))
        .collect();

    json!({
        "tenant": tenant,
        "dry_run": dry_run,
        "created": created,
        "revised": revised,
        "unchanged": plan.unchanged,
        "skipped": skipped,
        "relations_created": relations_created,
        "orphans": plan.orphans,
        "orphan_relations": orphan_relations,
        "warnings": warnings,
    })
}

fn split_changes(changes: &[PlannedChange]) -> [Vec<Value>; 3] {
    let mut split: [Vec<Value>; 3] = Default::default();
    for change in changes {
        match change {
            PlannedChange::CreateNode { id, .. } => split[0].push(json!(id)),
            PlannedChange::ReviseNode { id, .. } => split[1].push(json!(id)),
            PlannedChange::CreateRelation { from, to, .. } => {
                split[2].push(json!({ "from": from, "to": to }));
            }
        }
    }

    split
}

/// Lotes de até `BATCH_SIZE` na ordem do plano; falha depois do 1º lote vira `load-partial`.
fn write_changes(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    changes: Vec<PlannedChange>,
) -> Result<(), AppError> {
    let total = changes.len();
    let mut pending = changes
        .into_iter()
        .map(into_item)
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut written = 0;
    while !pending.is_empty() {
        let batch = pending.len().min(BATCH_SIZE);
        let (items, files): (Vec<AppendItem>, Vec<String>) = pending.drain(..batch).unzip();
        let size = items.len();
        append_batch(omhl, tenant, items, &files).map_err(|error| match written {
            0 => error,
            _ => error.with_detail(
                "",
                "load-partial",
                &format!(
                    "{written} of {total} change(s) written before the failure; run omhl load again"
                ),
            ),
        })?;
        written += size;
    }

    Ok(())
}

fn append_batch(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    items: Vec<AppendItem>,
    files: &[String],
) -> Result<(), AppError> {
    let key = format!("{KEY_PREFIX}{}", fingerprint(&items)?.as_str());
    let request = AppendRequest {
        tenant: tenant.clone(),
        author: Caller::loader().author(),
        key: Some(IdempotencyKey::parse(&key)?),
        items,
    };
    omhl.append(&request)
        .map_err(|error| AppError::from(error).remap_item_paths(files))?;

    Ok(())
}

fn into_item(change: PlannedChange) -> Result<(AppendItem, String), AppError> {
    let (body, base, path) = match change {
        PlannedChange::CreateNode {
            id,
            type_name,
            data,
            path,
        } => (node_body(&id, &type_name, data)?, None, path),
        PlannedChange::ReviseNode {
            id,
            type_name,
            data,
            base,
            path,
        } => (
            node_body(&id, &type_name, data)?,
            Some(RecordId::parse(&base)?),
            path,
        ),
        PlannedChange::CreateRelation { from, to, path } => {
            (relation_body(&from, &to)?, None, path)
        }
    };
    let item = AppendItem {
        body,
        base,
        snapshot: None,
        external_id: None,
    };

    Ok((item, path))
}

fn relation_body(from: &str, to: &str) -> Result<ItemBody, AppError> {
    Ok(ItemBody::Relation {
        relation_type: RelationTypeName::parse(AFTER)?,
        from: SemanticId::parse(from)?,
        to: SemanticId::parse(to)?,
    })
}

fn node_body(id: &str, type_name: &str, data: Value) -> Result<ItemBody, AppError> {
    Ok(ItemBody::Node {
        id: SemanticId::parse(id)?,
        type_name: TypeName::parse(type_name)?,
        data,
    })
}
