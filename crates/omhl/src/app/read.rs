use hexlog::{ListFilter, NodeView, PageRequest, TenantId, TypeName};

use super::error::AppError;
use crate::facade::Omhl;

const PAGE_LIMIT: u32 = 50;

/// Todos os nodes vigentes do tipo, página a página.
pub(super) fn all_nodes(
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
