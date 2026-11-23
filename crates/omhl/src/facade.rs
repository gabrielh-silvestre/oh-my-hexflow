use hexlog::{
    AppendOutcome, AppendRequest, Author, Catalog, ChainHead, ChainReport, Clock, Config, Deps,
    Hash, Hexlog, IdGenerator, Lineage, ListFilter, NodeView, Page, PageRequest, RelationQuery,
    RelationTypeName, RelationView, RevisionView, SemanticId, TenantId, TenantInfo, TypeName,
};

use crate::error::{OmhlError, RuleCode};
use crate::gate::{self, GateQuestion, GateReport, GateSource, HexlogSource, read_consistent};
use crate::graphs::WORK_ITEM_TYPES;
use crate::mechanical::plan::CheckNode;
use crate::native::NativeBase;
use crate::pr_gate::{
    GATE_NAME, GateSnapshots, anchor_questions, closing_request, pr_gate_questions,
    project_checks_filter, required_checks, spec_digests,
};
use crate::probe::{SnapshotProbe, TreeSource};
use crate::projection::ProjectionState;
use crate::{graphs, guard};

static NATIVE: NativeBase = NativeBase;

const PROJECTED_NODE_TYPES: [&str; 7] = [
    "work-item",
    "pull-request",
    "premise",
    "decision",
    "gap",
    "divergence",
    "attestation",
];
const PROJECTED_RELATIONS: [&str; 3] = ["supports", "contradicts", "answers"];

#[derive(Clone, Copy)]
pub struct OmhlDeps<'d> {
    pub clock: &'d dyn Clock,
    pub ids: &'d dyn IdGenerator,
    pub probe: &'d dyn SnapshotProbe,
}

/// Work item fechado: o que foi gravado e o gate avaliado no fechamento.
pub struct ClosedWorkItem {
    pub outcome: AppendOutcome,
    pub report: GateReport,
}

pub struct Omhl<'d> {
    hexlog: Hexlog<'d>,
    probe: &'d dyn SnapshotProbe,
    page_size: u32,
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
            page_size: config.limits.page_default,
        })
    }

    pub fn ensure_tenant(&mut self, tenant: &TenantId) -> Result<TenantInfo, OmhlError> {
        Ok(self.hexlog.ensure_tenant(tenant)?)
    }

    pub fn catalog(&self) -> &Catalog {
        self.hexlog.catalog()
    }

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

    pub fn lineage_records(
        &mut self,
        tenant: &TenantId,
        lineage: &Lineage,
    ) -> Result<Vec<RevisionView>, OmhlError> {
        Ok(self.hexlog.lineage_records(tenant, lineage)?)
    }

    pub fn list(
        &mut self,
        tenant: &TenantId,
        filter: &ListFilter,
        page: &PageRequest,
    ) -> Result<Page<NodeView>, OmhlError> {
        guard::check_where_eq(filter, "/filter")?;

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

    pub fn evaluate(
        &mut self,
        tenant: &TenantId,
        gate: &str,
        questions: &[GateQuestion],
    ) -> Result<GateReport, OmhlError> {
        guard::check_question_filters(questions)?;
        let mut source = HexlogSource::new(&mut self.hexlog, self.page_size);
        let (head, reports) = read_consistent(&mut source, tenant, |source| {
            gate::evaluate(source, tenant, questions)
        })?;

        Ok(GateReport::assemble(gate, head, reports))
    }

    pub fn evaluate_pr_gate(
        &mut self,
        tenant: &TenantId,
        work_item: &SemanticId,
        snapshots: &GateSnapshots,
    ) -> Result<GateReport, OmhlError> {
        self.find_work_item(tenant, work_item)?;

        self.gate_report(tenant, work_item, snapshots)
    }

    pub fn read_projection_state(
        &mut self,
        tenant: &TenantId,
    ) -> Result<ProjectionState, OmhlError> {
        let mut source = HexlogSource::new(&mut self.hexlog, self.page_size);
        let (_, state) = read_consistent(&mut source, tenant, |source| {
            let mut nodes = Vec::new();
            for type_name in PROJECTED_NODE_TYPES {
                let filter = ListFilter {
                    type_name: Some(TypeName::parse(type_name)?),
                    id_prefix: None,
                    snapshot: None,
                    where_eq: Vec::new(),
                };
                nodes.extend(source.nodes(tenant, &filter)?);
            }
            let mut relations = Vec::new();
            for name in PROJECTED_RELATIONS {
                relations.extend(source.relations(tenant, &RelationTypeName::parse(name)?)?);
            }

            Ok(ProjectionState { nodes, relations })
        })?;

        Ok(state)
    }

    pub fn check_nodes(&mut self, tenant: &TenantId) -> Result<Vec<CheckNode>, OmhlError> {
        let mut source = HexlogSource::new(&mut self.hexlog, self.page_size);
        let views = source.nodes(tenant, &project_checks_filter())?;

        Ok(views
            .into_iter()
            .map(|view| CheckNode {
                id: view.id,
                data: view.data,
                shadowed: view.shadowed,
            })
            .collect())
    }

    pub fn check_after_relations(
        &mut self,
        tenant: &TenantId,
    ) -> Result<Vec<(SemanticId, SemanticId)>, OmhlError> {
        let mut source = HexlogSource::new(&mut self.hexlog, self.page_size);
        let relations =
            source.relations(tenant, &RelationTypeName::parse(graphs::AFTER_RELATION)?)?;

        Ok(relations
            .into_iter()
            .map(|relation| (relation.from, relation.to))
            .collect())
    }

    fn gate_report(
        &mut self,
        tenant: &TenantId,
        work_item: &SemanticId,
        snapshots: &GateSnapshots,
    ) -> Result<GateReport, OmhlError> {
        let mut source = HexlogSource::new(&mut self.hexlog, self.page_size);
        let (head, reports) = read_consistent(&mut source, tenant, |source| {
            let checks = source.nodes(tenant, &project_checks_filter())?;
            let required = required_checks(NATIVE.checks(), &checks);
            let digests = spec_digests(&checks);
            let mut questions = pr_gate_questions(&required, work_item, snapshots, &digests);
            questions.extend(anchor_questions(work_item));

            gate::evaluate(source, tenant, &questions)
        })?;

        Ok(GateReport::assemble(GATE_NAME, head, reports))
    }

    /// Os snapshots têm de ser o `HEAD` e a árvore dele, de uma árvore limpa, conferidos na sonda antes do gate.
    pub fn close_work_item(
        &mut self,
        tenant: &TenantId,
        work_item: &SemanticId,
        snapshots: &GateSnapshots,
        author: &Author,
    ) -> Result<ClosedWorkItem, OmhlError> {
        let current = self.find_work_item(tenant, work_item)?;
        if current.data["status"] == "closed" {
            return Err(OmhlError::rule(RuleCode::InvalidTransition, "/work_item"));
        }
        let head_commit = guard::probe_refusal(&self.probe.probe(), &snapshots.head_commit);
        let head_tree = guard::probe_refusal(
            &self.probe.probe_tree(TreeSource::Head),
            &snapshots.head_tree,
        );
        if let Some(code) = head_commit.or(head_tree) {
            return Err(OmhlError::rule(code, "/snapshot"));
        }
        let report = self.gate_report(tenant, work_item, snapshots)?;
        let request = closing_request(tenant, author, current, &report, &snapshots.head_commit);
        let outcome = self.hexlog.append(&request)?;

        Ok(ClosedWorkItem { outcome, report })
    }

    pub fn find_work_item(
        &mut self,
        tenant: &TenantId,
        work_item: &SemanticId,
    ) -> Result<NodeView, OmhlError> {
        match self.hexlog.get(tenant, work_item)? {
            Some(view) if WORK_ITEM_TYPES.contains(&view.type_name.as_str()) => Ok(view),
            _ => Err(OmhlError::rule(RuleCode::WorkItemNotFound, "/work_item")),
        }
    }
}
