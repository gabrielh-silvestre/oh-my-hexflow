use ::omhl::error::OmhlError;
use ::omhl::gate::{
    GateQuestion, GateSource, HexlogSource, Question, QuestionKind, QuestionReport, evaluate,
};
use hexlog::{
    AppendItem, AppendRequest, Catalog, ChainHead, EndpointStatus, Hash, Hexlog, ItemBody, Lineage,
    ListFilter, NodeView, Origin, RecordId, RelationTypeDef, RelationTypeName, RelationView,
    Snapshot, TenantId, TypeDef,
};
use serde_json::{Value, json};

use super::raw::RawHarness;
use super::{DEFAULT_INSTANT, EmptyNative, author, rty, sid, tenant, tname};

/// Rótulo de toda pergunta montada por `evaluate_one`.
pub const LABEL: &str = "q";

const RECORD_ID: &str = "acme:0192a000-0000-7000-8000-000000000001";

/// Fonte falsa de porta própria: sem lógica de filtro. Devolve a lista configurada para um filtro
/// `==` ao pedido (e vazia para qualquer outro), as relations configuradas por tipo e os chain
/// heads programados, um por chamada de `chain_head`.
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

/// Filtro que não restringe nada.
pub fn all_filter() -> ListFilter {
    ListFilter {
        type_name: None,
        id_prefix: None,
        snapshot: None,
        where_eq: Vec::new(),
    }
}

/// Filtro só por tipo; filtros de tipos diferentes são diferentes para o `FakeSource`.
pub fn type_filter(type_name: &str) -> ListFilter {
    ListFilter {
        type_name: Some(tname(type_name)),
        ..all_filter()
    }
}

// Views montadas por literal

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

/// Node com o `data` dado.
pub fn node_view_with(id: &str, data: Value) -> NodeView {
    NodeView {
        data,
        ..node_view(id)
    }
}

pub fn node_views(ids: &[&str]) -> Vec<NodeView> {
    ids.iter().map(|id| node_view(id)).collect()
}

/// Relation vigente, com os dois extremos vigentes.
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

/// Avalia uma pergunta só, com o rótulo `LABEL`, e devolve o relatório dela.
pub fn evaluate_one(source: &mut FakeSource, question: Question) -> QuestionReport {
    let questions = [GateQuestion::new(LABEL, question)];

    evaluate(source, &tenant("acme"), &questions)
        .expect("the evaluation succeeds")
        .pop()
        .expect("one report per question")
}

/// Relatório esperado, com o rótulo `LABEL` e nenhuma evidência omitida.
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

// Hexlog real (testes 20 a 22)

/// Catalog mínimo: `note` aberto, `snap-note` e `snap-memo` abertos com snapshot obrigatório (só
/// tipos que exigem snapshot o aceitam), e os relation types `supports` e `contradicts` sem restrição.
pub fn local_catalog() -> Catalog {
    let open_type = |name: &str, requires_snapshot: bool| TypeDef {
        name: tname(name),
        schema: json!({"type": "object"}),
        requires_snapshot,
        equivalent: None,
    };
    let open_relation = |name: &str| RelationTypeDef {
        name: rty(name),
        from: None,
        to: None,
        acyclic: false,
        equivalent: None,
    };

    Catalog::new(
        vec![
            open_type("note", false),
            open_type("snap-note", true),
            open_type("snap-memo", true),
        ],
        vec![open_relation("supports"), open_relation("contradicts")],
    )
    .expect("the local definitions are accepted by the catalog")
}

/// Abre o `Hexlog` do harness com `local_catalog` e o tenant `acme` já criado.
pub fn open_acme<'a>(harness: &'a RawHarness, native: &'a EmptyNative) -> Hexlog<'a> {
    let mut hexlog = harness.hexlog(local_catalog(), native);
    hexlog
        .ensure_tenant(&tenant("acme"))
        .expect("the tenant is created");

    hexlog
}

pub fn node_item(id: &str, type_name: &str, data: Value) -> AppendItem {
    AppendItem {
        body: ItemBody::Node {
            id: sid(id),
            type_name: tname(type_name),
            data,
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

pub fn snapshot_node_item(
    id: &str,
    type_name: &str,
    data: Value,
    snapshot: Snapshot,
) -> AppendItem {
    AppendItem {
        snapshot: Some(snapshot),
        ..node_item(id, type_name, data)
    }
}

pub fn relation_item(relation_type: &str, from: &str, to: &str) -> AppendItem {
    AppendItem {
        body: ItemBody::Relation {
            relation_type: rty(relation_type),
            from: sid(from),
            to: sid(to),
        },
        base: None,
        snapshot: None,
        external_id: None,
    }
}

pub fn retirement_item(relation_type: &str, from: &str, to: &str, base: &RecordId) -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: Lineage::Relation {
                relation_type: rty(relation_type),
                from: sid(from),
                to: sid(to),
            },
        },
        base: Some(base.clone()),
        snapshot: None,
        external_id: None,
    }
}

/// Grava os itens no tenant `acme` num lote só e devolve os ids técnicos, na ordem dos itens.
pub fn append_acme(hexlog: &mut Hexlog<'_>, items: Vec<AppendItem>) -> Vec<RecordId> {
    let request = AppendRequest {
        tenant: tenant("acme"),
        author: author(),
        key: None,
        items,
    };

    hexlog
        .append(&request)
        .expect("the items are appended")
        .records
        .into_iter()
        .map(|record| record.id)
        .collect()
}

/// Avalia uma pergunta só sobre o `Hexlog` real, paginando de `page_size` em `page_size`.
pub fn evaluate_on_hexlog(
    hexlog: &mut Hexlog<'_>,
    page_size: u32,
    question: Question,
) -> QuestionReport {
    let questions = [GateQuestion::new(LABEL, question)];
    let mut source = HexlogSource::new(hexlog, page_size);

    evaluate(&mut source, &tenant("acme"), &questions)
        .expect("the evaluation succeeds")
        .pop()
        .expect("one report per question")
}
