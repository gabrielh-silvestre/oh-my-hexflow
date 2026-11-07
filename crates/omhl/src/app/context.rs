use hexlog::{
    ListFilter, NativeNodes, NodeView, PageRequest, RelationQuery, RelationTypeName, SemanticId,
    TenantId, TypeName,
};
use serde_json::Value;

use super::error::AppError;
use super::output;
use crate::facade::Omhl;
use crate::native::NativeBase;
use crate::render::{CONTEXT_BUDGET, RenderInput, render};

const PAGE_LIMIT: u32 = 50;
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

    Ok(output::rendered(&render(&input, CONTEXT_BUDGET)))
}

fn all_nodes(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    type_name: &str,
) -> Result<Vec<NodeView>, AppError> {
    let filter = ListFilter {
        type_name: Some(TypeName::parse(type_name)?),
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    };
    let mut nodes = Vec::new();
    let mut after_seq = None;
    loop {
        let request = PageRequest {
            limit: PAGE_LIMIT,
            after_seq,
        };
        let page = omhl.list(tenant, &filter, &request)?;
        nodes.extend(page.items);
        match page.next_after_seq {
            Some(next) => after_seq = Some(next),
            None => return Ok(nodes),
        }
    }
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
