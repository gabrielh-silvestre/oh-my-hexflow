use hexlog::{
    ChainHead, Hexlog, ListFilter, NodeView, PageRequest, RelationQuery, RelationTypeName,
    RelationView, TenantId,
};

use super::GateSource;
use crate::error::OmhlError;

pub struct HexlogSource<'a, 'd> {
    hexlog: &'a mut Hexlog<'d>,
    page_size: u32,
}

impl<'a, 'd> HexlogSource<'a, 'd> {
    pub fn new(hexlog: &'a mut Hexlog<'d>, page_size: u32) -> HexlogSource<'a, 'd> {
        HexlogSource { hexlog, page_size }
    }
}

impl GateSource for HexlogSource<'_, '_> {
    fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, OmhlError> {
        Ok(self.hexlog.chain_head(tenant)?)
    }

    fn nodes(
        &mut self,
        tenant: &TenantId,
        filter: &ListFilter,
    ) -> Result<Vec<NodeView>, OmhlError> {
        let mut nodes = Vec::new();
        let mut after_seq = None;
        loop {
            let request = PageRequest {
                limit: self.page_size,
                after_seq,
            };
            let page = self.hexlog.list(tenant, filter, &request)?;
            nodes.extend(page.items);
            match page.next_after_seq {
                Some(next) => after_seq = Some(next),
                None => return Ok(nodes),
            }
        }
    }

    fn relations(
        &mut self,
        tenant: &TenantId,
        relation_type: &RelationTypeName,
    ) -> Result<Vec<RelationView>, OmhlError> {
        let query = RelationQuery {
            relation_type: Some(relation_type.clone()),
            from: None,
            to: None,
            include_retired: false,
        };

        Ok(self.hexlog.relations(tenant, &query)?)
    }
}
