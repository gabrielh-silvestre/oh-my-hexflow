use hexlog::{
    AppendOutcome, AppendRequest, ChainHead, ChainReport, Clock, Config, Deps, Hash, Hexlog,
    IdGenerator, ListFilter, NodeView, Page, PageRequest, RelationQuery, RelationView, SemanticId,
    TenantId, TenantInfo,
};

use crate::error::OmhlError;
use crate::native::NativeBase;
use crate::probe::SnapshotProbe;
use crate::{graphs, guard};

static NATIVE: NativeBase = NativeBase;

/// Portas que o `Omhl` empresta; o dono é quem chama.
#[derive(Clone, Copy)]
pub struct OmhlDeps<'d> {
    pub clock: &'d dyn Clock,
    pub ids: &'d dyn IdGenerator,
    pub probe: &'d dyn SnapshotProbe,
}

/// Dono do `Hexlog`: monta o catalog de `graphs::catalog()` e usa o `NativeBase` real.
pub struct Omhl<'d> {
    hexlog: Hexlog<'d>,
    probe: &'d dyn SnapshotProbe,
}

impl<'d> Omhl<'d> {
    pub fn open(config: &Config, deps: OmhlDeps<'d>) -> Result<Omhl<'d>, OmhlError> {
        let hexlog_deps = Deps {
            clock: deps.clock,
            ids: deps.ids,
            catalog: graphs::catalog()?,
            native: &NATIVE,
        };

        Ok(Omhl {
            hexlog: Hexlog::open(config, hexlog_deps)?,
            probe: deps.probe,
        })
    }

    pub fn ensure_tenant(&mut self, tenant: &TenantId) -> Result<TenantInfo, OmhlError> {
        Ok(self.hexlog.ensure_tenant(tenant)?)
    }

    /// A guarda valida o lote inteiro antes de qualquer escrita.
    pub fn append(&mut self, req: &AppendRequest) -> Result<AppendOutcome, OmhlError> {
        guard::check_append(&mut self.hexlog, self.probe, req)?;

        Ok(self.hexlog.append(req)?)
    }

    pub fn get(
        &mut self,
        tenant: &TenantId,
        id: &SemanticId,
    ) -> Result<Option<NodeView>, OmhlError> {
        Ok(self.hexlog.get(tenant, id)?)
    }

    pub fn list(
        &mut self,
        tenant: &TenantId,
        filter: &ListFilter,
        page: &PageRequest,
    ) -> Result<Page<NodeView>, OmhlError> {
        guard::check_where_eq(filter)?;

        Ok(self.hexlog.list(tenant, filter, page)?)
    }

    pub fn relations(
        &mut self,
        tenant: &TenantId,
        query: &RelationQuery,
    ) -> Result<Vec<RelationView>, OmhlError> {
        Ok(self.hexlog.relations(tenant, query)?)
    }

    pub fn chain_head(&mut self, tenant: &TenantId) -> Result<ChainHead, OmhlError> {
        Ok(self.hexlog.chain_head(tenant)?)
    }

    pub fn verify_chain(
        &mut self,
        tenant: &TenantId,
        expected_chain_head: Option<&Hash>,
    ) -> Result<ChainReport, OmhlError> {
        Ok(self.hexlog.verify_chain(tenant, expected_chain_head)?)
    }
}
