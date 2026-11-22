use crate::error::OmhlError;
use crate::gate::{GateQuestion, GateSource, Question, QuestionKind, QuestionReport, evaluate};
use hexlog::{
    ChainHead, EndpointStatus, Hash, ListFilter, NodeView, Origin, RecordId, RelationTypeName,
    RelationView, TenantId,
};
use serde_json::{Value, json};

use super::{DEFAULT_INSTANT, author, rty, sid, tenant, tname};

pub const LABEL: &str = "q";

const RECORD_ID: &str = "acme:0192a000-0000-7000-8000-000000000001";

pub fn node_view(id: &str) -> NodeView {
    NodeView {
        id: sid(id),
        type_name: tname("note"),
        record_id: RecordId::parse(RECORD_ID).expect("valid record id"),
        seq: 1,
        at: DEFAULT_INSTANT.to_owned(),
        data: json!({}),
        snapshot: None,
        external_id: None,
        author: author(),
        shadowed: false,
    }
}

pub struct FakeSource {
    nodes: Vec<(ListFilter, Vec<NodeView>)>,
    relations: Vec<(RelationTypeName, Vec<RelationView>)>,
    heads: Vec<ChainHead>,
    heads_read: usize,
}

impl FakeSource {
    pub fn new() -> FakeSource {
        FakeSource {
            nodes: Vec::new(),
            relations: Vec::new(),
            heads: Vec::new(),
            heads_read: 0,
        }
    }

    pub fn with_nodes(mut self, filter: ListFilter, nodes: Vec<NodeView>) -> FakeSource {
        self.nodes.push((filter, nodes));
        self
    }

    pub fn with_relations(
        mut self,
        relation_type: &str,
        relations: Vec<RelationView>,
    ) -> FakeSource {
        self.relations.push((rty(relation_type), relations));
        self
    }

    pub fn with_heads(mut self, heads: Vec<ChainHead>) -> FakeSource {
        self.heads = heads;
        self
    }
}

impl Default for FakeSource {
    fn default() -> FakeSource {
        FakeSource::new()
    }
}

impl GateSource for FakeSource {
    fn chain_head(&mut self, _tenant: &TenantId) -> Result<ChainHead, OmhlError> {
        let head = self
            .heads
            .get(self.heads_read)
            .expect("the fake has a chain head programmed for this call")
            .clone();
        self.heads_read += 1;

        Ok(head)
    }

    fn nodes(
        &mut self,
        _tenant: &TenantId,
        filter: &ListFilter,
    ) -> Result<Vec<NodeView>, OmhlError> {
        let nodes = self
            .nodes
            .iter()
            .find(|(configured, _)| configured == filter)
            .map(|(_, nodes)| nodes.clone())
            .unwrap_or_default();

        Ok(nodes)
    }

    fn relations(
        &mut self,
        _tenant: &TenantId,
        relation_type: &RelationTypeName,
    ) -> Result<Vec<RelationView>, OmhlError> {
        let relations = self
            .relations
            .iter()
            .find(|(configured, _)| configured == relation_type)
            .map(|(_, relations)| relations.clone())
            .unwrap_or_default();

        Ok(relations)
    }
}

// Filtros

pub fn all_filter() -> ListFilter {
    ListFilter {
        type_name: None,
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    }
}

pub fn type_filter(type_name: &str) -> ListFilter {
    ListFilter {
        type_name: Some(tname(type_name)),
        ..all_filter()
    }
}

pub fn node_view_with(id: &str, data: Value) -> NodeView {
    NodeView {
        data,
        ..node_view(id)
    }
}

pub fn node_views(ids: &[&str]) -> Vec<NodeView> {
    ids.iter().map(|id| node_view(id)).collect()
}

pub fn relation_view(relation_type: &str, from: &str, to: &str) -> RelationView {
    RelationView {
        record_id: RecordId::parse(RECORD_ID).expect("valid record id"),
        seq: 1,
        at: DEFAULT_INSTANT.to_owned(),
        relation_type: rty(relation_type),
        from: sid(from),
        to: sid(to),
        from_origin: Origin::Tenant,
        to_origin: Origin::Tenant,
        from_status: EndpointStatus::Current,
        to_status: EndpointStatus::Current,
        native_catalog: None,
        external_id: None,
        author: author(),
        retired: false,
    }
}

// Avaliação e relatórios

pub fn evaluate_one(source: &mut FakeSource, question: Question) -> QuestionReport {
    let questions = [GateQuestion::new(LABEL, question)];

    evaluate(source, &tenant("acme"), &questions)
        .expect("the evaluation succeeds")
        .pop()
        .expect("one report per question")
}

pub fn expected_report(
    kind: QuestionKind,
    passed: bool,
    matched: usize,
    failing: &[&str],
) -> QuestionReport {
    QuestionReport {
        label: LABEL.to_owned(),
        kind,
        passed,
        matched,
        failing: failing.iter().map(|id| sid(id)).collect(),
        omitted: 0,
        next_step: None,
    }
}

pub fn chain_head(seq: u64, fill: char) -> ChainHead {
    ChainHead {
        seq,
        hash: Hash::parse(&fill.to_string().repeat(64)).expect("valid hash"),
    }
}
