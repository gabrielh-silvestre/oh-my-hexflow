use hexlog::{NativeNodes, RelationQuery, RelationTypeName, SemanticId, TenantId};
use serde_json::{Value, json};

use super::error::AppError;
use super::read::all_nodes;
use crate::facade::Omhl;
use crate::native::NativeBase;
use crate::render::{CONTEXT_BUDGET, RenderInput, render};

const AFTER: &str = "after";

/// Ação `render`: lê todo o contexto do tenant (checks, guidelines, decisions, premises, `after`)
/// e entrega o texto do render. Work items e atestados não entram.
pub(super) fn render_context(omhl: &mut Omhl<'_>, tenant: &TenantId) -> Result<Value, AppError> {
    let checks = all_nodes(omhl, tenant, "check")?;
    let guidelines = all_nodes(omhl, tenant, "guideline")?;
    let decisions = all_nodes(omhl, tenant, "decision")?;
    let premises = all_nodes(omhl, tenant, "premise")?;
    let after = after_pairs(omhl, tenant)?;
    let input = RenderInput {
        tenant: tenant.as_str(),
        native: &NativeBase,
        checks: &checks,
        guidelines: &guidelines,
        decisions: &decisions,
        premises: &premises,
        after: &after,
    };

    let rendered = render(&input, CONTEXT_BUDGET);

    Ok(json!({ "text": rendered.text, "omitted": rendered.omitted }))
}

/// As relações `after` vigentes, mais a do par nativo se o motor não a devolveu.
fn after_pairs(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
) -> Result<Vec<(SemanticId, SemanticId)>, AppError> {
    let relation_type = RelationTypeName::parse(AFTER)?;
    let query = RelationQuery {
        relation_type: Some(relation_type.clone()),
        from: None,
        to: None,
        include_retired: false,
    };
    let mut pairs: Vec<(SemanticId, SemanticId)> = omhl
        .relations(tenant, &query)?
        .into_iter()
        .map(|relation| (relation.from, relation.to))
        .collect();
    for pair in NativeBase.native_relations(&relation_type) {
        if !pairs.contains(&pair) {
            pairs.push(pair);
        }
    }

    Ok(pairs)
}
