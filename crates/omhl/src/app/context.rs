use hexlog::{NativeNodes, NodeView, RelationQuery, RelationTypeName, SemanticId, TenantId};
use serde_json::{Value, json};

use super::error::AppError;
use super::read::all_nodes;
use crate::facade::Omhl;
use crate::graphs;
use crate::native::NativeBase;
use crate::probe::NativeRefusal;
use crate::render::{CONTEXT_BUDGET, RenderInput, render};
use crate::workflow::config::NativeConfig;

const INVALID_WORKFLOW_NOTICE: &str = "WARNING: .omhl/workflow.toml is not accepted (invalid-workflow-config): gate-pr and close refuse until it is fixed and no native check is printed meanwhile; run omhl load --dry-run for details";
const MAIN_DIR_UNKNOWN_NOTICE: &str = "WARNING: the main directory was not resolved (main-dir-unknown): no native check is printed and gate-pr and close refuse; run git init if the project has no repository, or check the repository if it has one";

/// O `render` nunca recusa: sem ativação legível vale o conjunto vazio e o preâmbulo diz por quê.
pub(super) fn render_context(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
    native: Result<NativeConfig, NativeRefusal>,
) -> Result<Value, AppError> {
    let (enabled, notice) = match native {
        Ok(enabled) => (enabled, None),
        Err(refusal) => (NativeConfig::default(), Some(refusal_notice(&refusal))),
    };
    let checks = all_nodes(omhl, tenant, "check")?;
    let guidelines = all_nodes(omhl, tenant, "guideline")?;
    let decisions = all_nodes(omhl, tenant, "decision")?;
    let premises = open_premises(
        all_nodes(omhl, tenant, "premise")?,
        &all_nodes(omhl, tenant, "work-item")?,
    );
    let after = after_pairs(omhl, tenant)?;
    let input = RenderInput {
        tenant: tenant.as_str(),
        native: &NativeBase,
        enabled: &enabled,
        notice,
        checks: &checks,
        guidelines: &guidelines,
        decisions: &decisions,
        premises: &premises,
        after: &after,
    };

    let rendered = render(&input, CONTEXT_BUDGET);

    Ok(json!({ "text": rendered.text, "omitted": rendered.omitted }))
}

fn refusal_notice(refusal: &NativeRefusal) -> &'static str {
    match refusal {
        NativeRefusal::MainDirUnknown => MAIN_DIR_UNKNOWN_NOTICE,
        NativeRefusal::InvalidWorkflow(_) => INVALID_WORKFLOW_NOTICE,
    }
}

fn open_premises(premises: Vec<NodeView>, work_items: &[NodeView]) -> Vec<NodeView> {
    premises
        .into_iter()
        .filter(|premise| {
            let Some(owner) = premise.data["work_item"].as_str() else {
                return true;
            };
            !work_items
                .iter()
                .any(|item| item.id.as_str() == owner && item.data["status"] == "closed")
        })
        .collect()
}

fn after_pairs(
    omhl: &mut Omhl<'_>,
    tenant: &TenantId,
) -> Result<Vec<(SemanticId, SemanticId)>, AppError> {
    let relation_type = RelationTypeName::parse(graphs::AFTER_RELATION)?;
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
