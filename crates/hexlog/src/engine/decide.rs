use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use petgraph::algo::has_path_connecting;
use petgraph::graphmap::DiGraphMap;
use serde_json::Value;

use crate::engine::chain::{fingerprint, hash_record};
use crate::engine::limits::Limits;
use crate::engine::model::{
    AppendItem, AppendOutcome, AppendRequest, AppendedRecord, BodyAuthor, BodyBatch, BodyLineage,
    BodySnapshot, ChainHead, ItemBody, Lineage, Origin, RecordBody, invalid_body, lineage_columns,
    read_body,
};
use crate::engine::ports::{
    AppendPlan, BatchRow, Definitions, IdGenerator, LatestRow, LineageKind, NativeNodes, NewRecord,
    RecordColumns, RelationTypeInfo, WriteReads,
};
use crate::engine::rules::{
    Endpoint, endpoint_types_allowed, instant_is_well_formed, static_rules, validate_shape,
};
use crate::error::{ErrorCode, HexlogError};
use crate::ids::{Hash, Name, RecordId, RelationTypeName, SemanticId, TenantId, TypeName};

pub struct DecideCtx<'a> {
    pub defs: &'a dyn Definitions,
    pub native: &'a dyn NativeNodes,
    pub ids: &'a dyn IdGenerator,
    pub limits: &'a Limits,
    pub now: &'a str,
}

/// Os seis passos da escrita, todos dentro da transação do chamador (a ordem é a da recusa).
pub fn decide(
    ctx: &DecideCtx<'_>,
    req: &AppendRequest,
    reads: &mut dyn WriteReads,
) -> Result<AppendPlan, HexlogError> {
    validate_shape(req, ctx.limits)?;
    if reads.tenant(&req.tenant)?.is_none() {
        return Err(HexlogError::tenant_not_found());
    }
    let batch_fingerprint = fingerprint(&req.items);
    if let Some(replay) = find_replay(req, &batch_fingerprint, reads)? {
        return Ok(replay);
    }
    static_rules(req, ctx.defs, ctx.native, ctx.limits)?;
    // Número fora de ±2^53 já foi recusado pelo catálogo; chegar aqui com `Err` é defeito do motor.
    let batch_fingerprint = batch_fingerprint.map_err(|_| {
        HexlogError::refused(
            ErrorCode::Internal,
            "fingerprint-unreachable",
            "batch fingerprint failed after the static rules passed",
        )
    })?;
    let head = reads.chain_head(&req.tenant)?;
    validate_tail(&req.tenant, &head, reads)?;
    let lineages: Vec<Cow<'_, Lineage>> = req
        .items
        .iter()
        .map(|item| item_lineage(&item.body))
        .collect();
    let validated = validate_history(ctx, req, &lineages, reads)?;
    assemble(ctx, req, &lineages, &head, batch_fingerprint, &validated)
}

/// `Err` de impressão não tem replay possível: a consulta é pulada e a recusa vem dos passos seguintes.
fn find_replay(
    req: &AppendRequest,
    batch_fingerprint: &Result<Hash, HexlogError>,
    reads: &mut dyn WriteReads,
) -> Result<Option<AppendPlan>, HexlogError> {
    let (Some(key), Ok(batch_fingerprint)) = (&req.key, batch_fingerprint) else {
        return Ok(None);
    };
    let Some(stored) = reads.batch_by_key(&req.tenant, key)? else {
        return Ok(None);
    };
    if stored.fingerprint != *batch_fingerprint {
        return Err(HexlogError::refused(
            ErrorCode::IdempotencyConflict,
            "key-reused",
            "idempotency key was used with different items",
        )
        .at("/key"));
    }
    let records = reads
        .batch_records(&req.tenant, stored.first_seq, stored.item_count)?
        .into_iter()
        .map(|row| AppendedRecord {
            id: row.id,
            seq: row.seq,
        })
        .collect();
    let chain_head = reads.chain_head(&req.tenant)?;
    Ok(Some(AppendPlan {
        records: Vec::new(),
        batch: None,
        outcome: AppendOutcome {
            records,
            replayed: true,
            chain_head,
        },
    }))
}

/// Política `Tail`: o último record tem de bater com o seu hash e com o chain head.
fn validate_tail(
    tenant: &TenantId,
    head: &ChainHead,
    reads: &mut dyn WriteReads,
) -> Result<(), HexlogError> {
    if head.seq == 0 {
        return Ok(());
    }
    let tail = reads.batch_records(tenant, head.seq, 1)?;
    let intact = tail.first().is_some_and(|row| {
        hash_record(row.prev_hash.as_str(), &row.body) == row.hash && row.hash == head.hash
    });
    if intact {
        return Ok(());
    }
    Err(HexlogError::refused(
        ErrorCode::ChainBroken,
        "tail-hash-mismatch",
        "last record does not match its hash",
    ))
}

/// A linhagem do item; só a retirement já traz a sua pronta.
fn item_lineage(body: &ItemBody) -> Cow<'_, Lineage> {
    match body {
        ItemBody::Node { id, .. } => Cow::Owned(Lineage::Node(id.clone())),
        ItemBody::Relation {
            relation_type,
            from,
            to,
        } => Cow::Owned(Lineage::Relation {
            relation_type: relation_type.clone(),
            from: from.clone(),
            to: to.clone(),
        }),
        ItemBody::Retirement { lineage } => Cow::Borrowed(lineage),
    }
}

/// Extremos de uma relation: a origem de cada um e a versão do catálogo nativo que a resolveu.
struct Ends {
    from: Origin,
    to: Origin,
    native_catalog: Option<String>,
}

/// O que o passo 6 descobre de um item e a montagem usa.
struct Validated {
    latest: Option<LatestRow>,
    ends: Option<Ends>,
}

/// Situação de um node escrito por um item anterior do mesmo lote.
enum Overlay<'a> {
    Live(&'a TypeName),
    Retired,
}

/// Tipo de um extremo, vindo do catálogo nativo, do lote ou do latest gravado.
enum EndpointType<'a> {
    Native(TypeName),
    Batch(&'a TypeName),
    Stored(Name),
}

impl EndpointType<'_> {
    fn as_str(&self) -> &str {
        match self {
            EndpointType::Native(name) => name.as_str(),
            EndpointType::Batch(name) => name.as_str(),
            EndpointType::Stored(name) => name.as_str(),
        }
    }
}

struct Resolved<'a> {
    origin: Origin,
    type_name: EndpointType<'a>,
}

/// Arestas dos relation types `acyclic` do lote: vigentes no banco e nativas.
type PairsByType<'r> = HashMap<&'r str, Vec<(SemanticId, SemanticId)>>;

/// Passo 6: devolve o que cada item descobriu, na ordem dos itens.
fn validate_history(
    ctx: &DecideCtx<'_>,
    req: &AppendRequest,
    lineages: &[Cow<'_, Lineage>],
    reads: &mut dyn WriteReads,
) -> Result<Vec<Validated>, HexlogError> {
    let pairs = load_acyclic_pairs(ctx, req, reads)?;
    let mut graphs: HashMap<&str, DiGraphMap<&str, ()>> = pairs
        .iter()
        .map(|(&name, edges)| {
            let edges = edges.iter().map(|(from, to)| (from.as_str(), to.as_str()));
            (name, DiGraphMap::from_edges(edges))
        })
        .collect();
    let mut overlay: HashMap<&str, Overlay<'_>> = HashMap::new();
    let mut seen = HashSet::with_capacity(req.items.len());
    let mut validated = Vec::with_capacity(req.items.len());
    for (index, (item, lineage)) in req.items.iter().zip(lineages).enumerate() {
        if !seen.insert(lineage_columns(lineage)) {
            return Err(HexlogError::refused(
                ErrorCode::ForkRejected,
                "lineage-in-batch",
                "lineage already has an item in this batch",
            )
            .at_item(index, "body"));
        }
        let latest = reads.latest_record(&req.tenant, lineage)?;
        validate_item_history(index, item, latest.as_ref())?;
        let ends = ends_of_item(
            ctx,
            reads,
            &req.tenant,
            (index, &item.body),
            latest.as_ref(),
            &mut graphs,
            &mut overlay,
        )?;
        validated.push(Validated { latest, ends });
    }
    Ok(validated)
}

/// Extremos que o item descobriu; atualiza o grafo e o overlay para os itens seguintes do lote.
fn ends_of_item<'a>(
    ctx: &DecideCtx<'_>,
    reads: &mut dyn WriteReads,
    tenant: &TenantId,
    (index, body): (usize, &'a ItemBody),
    latest: Option<&LatestRow>,
    graphs: &mut HashMap<&'a str, DiGraphMap<&'a str, ()>>,
    overlay: &mut HashMap<&'a str, Overlay<'a>>,
) -> Result<Option<Ends>, HexlogError> {
    match body {
        ItemBody::Relation {
            relation_type,
            from,
            to,
        } => {
            let graph = graphs.get_mut(relation_type.as_str());
            let ends = validate_relation(
                ctx,
                reads,
                tenant,
                index,
                (relation_type, from, to),
                overlay,
                graph.as_deref(),
            )?;
            if let Some(graph) = graph {
                graph.add_edge(from.as_str(), to.as_str(), ());
            }
            Ok(Some(ends))
        }
        ItemBody::Retirement {
            lineage: Lineage::Relation { .. },
        } => Ok(Some(inherited_ends(reads, latest)?)),
        ItemBody::Node { id, type_name, .. } => {
            overlay.insert(id.as_str(), Overlay::Live(type_name));
            Ok(None)
        }
        ItemBody::Retirement {
            lineage: Lineage::Node(id),
        } => {
            overlay.insert(id.as_str(), Overlay::Retired);
            Ok(None)
        }
    }
}

/// Carrega antes de montar os grafos: eles emprestam deste mapa, que não pode mais crescer.
fn load_acyclic_pairs<'r>(
    ctx: &DecideCtx<'_>,
    req: &'r AppendRequest,
    reads: &mut dyn WriteReads,
) -> Result<PairsByType<'r>, HexlogError> {
    let mut pairs = PairsByType::new();
    for item in &req.items {
        let ItemBody::Relation { relation_type, .. } = &item.body else {
            continue;
        };
        let acyclic = relation_info(ctx.defs, relation_type)?.acyclic;
        if !acyclic || pairs.contains_key(relation_type.as_str()) {
            continue;
        }
        let mut edges = reads.live_relation_pairs(&req.tenant, relation_type)?;
        edges.extend(ctx.native.native_relations(relation_type));
        pairs.insert(relation_type.as_str(), edges);
    }
    Ok(pairs)
}

fn relation_info<'d>(
    defs: &'d dyn Definitions,
    relation_type: &RelationTypeName,
) -> Result<&'d RelationTypeInfo, HexlogError> {
    defs.relation_type(relation_type)
        .ok_or_else(|| HexlogError::internal("relation type is missing from the catalog"))
}

/// Regras 3 a 6 de uma relation: extremos, os dois nativos, tipos dos extremos e ciclo.
fn validate_relation(
    ctx: &DecideCtx<'_>,
    reads: &mut dyn WriteReads,
    tenant: &TenantId,
    index: usize,
    (relation_type, from, to): (&RelationTypeName, &SemanticId, &SemanticId),
    overlay: &HashMap<&str, Overlay<'_>>,
    graph: Option<&DiGraphMap<&str, ()>>,
) -> Result<Ends, HexlogError> {
    let source = resolve_endpoint(ctx, reads, tenant, from, overlay)?
        .ok_or_else(|| endpoint_not_found(index, Endpoint::From))?;
    let target = resolve_endpoint(ctx, reads, tenant, to, overlay)?
        .ok_or_else(|| endpoint_not_found(index, Endpoint::To))?;
    if source.origin == Origin::Native && target.origin == Origin::Native {
        return Err(HexlogError::refused(
            ErrorCode::InvalidRecord,
            "native-only-relation",
            "a relation between two native nodes cannot be written",
        )
        .at_item(index, "body"));
    }
    let info = relation_info(ctx.defs, relation_type)?;
    if let Err(side) =
        endpoint_types_allowed(info, source.type_name.as_str(), target.type_name.as_str())
    {
        return Err(HexlogError::refused(
            ErrorCode::InvalidRecord,
            "endpoint-type",
            "endpoint type is not allowed by the relation type",
        )
        .at_item(index, side.field()));
    }
    // O caminho de `to` até `from` faz a aresta nova fechar ciclo; nó fora do grafo não tem caminho.
    if graph.is_some_and(|edges| has_path_connecting(edges, to.as_str(), from.as_str(), None)) {
        return Err(HexlogError::refused(
            ErrorCode::CycleRejected,
            "cycle",
            "relation would close a cycle",
        )
        .at_item(index, "body"));
    }
    let any_native = source.origin == Origin::Native || target.origin == Origin::Native;
    Ok(Ends {
        from: source.origin,
        to: target.origin,
        native_catalog: any_native.then(|| ctx.native.catalog_version().to_owned()),
    })
}

fn endpoint_not_found(index: usize, side: Endpoint) -> HexlogError {
    HexlogError::refused(
        ErrorCode::InvalidRecord,
        "endpoint-not-found",
        "relation endpoint is not a current node",
    )
    .at_item(index, side.field())
}

/// O nativo prevalece; depois o lote; por último o latest gravado. `None` é ausente ou retirado.
fn resolve_endpoint<'o>(
    ctx: &DecideCtx<'_>,
    reads: &mut dyn WriteReads,
    tenant: &TenantId,
    id: &SemanticId,
    overlay: &HashMap<&str, Overlay<'o>>,
) -> Result<Option<Resolved<'o>>, HexlogError> {
    if let Some(node) = ctx.native.lookup(id) {
        return Ok(Some(Resolved {
            origin: Origin::Native,
            type_name: EndpointType::Native(node.type_name),
        }));
    }
    match overlay.get(id.as_str()) {
        Some(Overlay::Live(type_name)) => Ok(Some(Resolved {
            origin: Origin::Tenant,
            type_name: EndpointType::Batch(type_name),
        })),
        Some(Overlay::Retired) => Ok(None),
        None => Ok(reads
            .latest_record(tenant, &Lineage::Node(id.clone()))?
            .filter(|latest| !latest.retired)
            .map(|latest| Resolved {
                origin: Origin::Tenant,
                type_name: EndpointType::Stored(latest.type_name),
            })),
    }
}

/// Retirement de relation copia as origens e a versão nativa do latest: não revalida extremos.
fn inherited_ends(
    reads: &mut dyn WriteReads,
    latest: Option<&LatestRow>,
) -> Result<Ends, HexlogError> {
    let latest = latest.ok_or_else(|| HexlogError::internal("retirement has no latest record"))?;
    let row = reads
        .record_by_id(&latest.record_id)?
        .ok_or_else(|| HexlogError::internal("latest record is missing"))?;
    let (Some(from), Some(to)) = (row.columns.from_origin, row.columns.to_origin) else {
        return Err(invalid_body());
    };
    Ok(Ends {
        from,
        to,
        native_catalog: read_body(&row.body)?.native_catalog.map(Cow::into_owned),
    })
}

fn validate_item_history(
    index: usize,
    item: &AppendItem,
    latest: Option<&LatestRow>,
) -> Result<(), HexlogError> {
    let Some(latest) = latest else {
        return validate_first_record(index, item);
    };
    if item.base.as_ref() != Some(&latest.record_id) {
        return Err(not_current(index, latest));
    }
    match &item.body {
        ItemBody::Node { type_name, .. } if latest.type_name.as_str() != type_name.as_str() => {
            Err(HexlogError::refused(
                ErrorCode::InvalidRecord,
                "type-changed",
                "a revision cannot change the type",
            )
            .at_item(index, "body/type"))
        }
        ItemBody::Retirement { .. } if latest.retired => Err(HexlogError::refused(
            ErrorCode::InvalidRecord,
            "already-retired",
            "lineage is already retired",
        )
        .at_item(index, "body/lineage")),
        _ => Ok(()),
    }
}

/// Lineage sem records: um node só aceita raiz (sem `base`) e uma retirement não tem o que retirar.
fn validate_first_record(index: usize, item: &AppendItem) -> Result<(), HexlogError> {
    match (&item.body, &item.base) {
        (ItemBody::Retirement { .. }, _) => Err(HexlogError::refused(
            ErrorCode::InvalidRecord,
            "lineage-not-found",
            "lineage has no records to retire",
        )
        .at_item(index, "body/lineage")),
        (_, Some(_)) => Err(HexlogError::refused(
            ErrorCode::InvalidRecord,
            "base-unknown",
            "base refers to a lineage without records",
        )
        .at_item(index, "base")),
        _ => Ok(()),
    }
}

fn not_current(index: usize, latest: &LatestRow) -> HexlogError {
    let mut error = HexlogError::refused(
        ErrorCode::ForkRejected,
        "not-current",
        "base is not the current record of the lineage",
    )
    .at_item(index, "base");
    error.details[0].current = Some(latest.record_id.clone());
    error.details[0].retired = Some(latest.retired);
    error
}

/// O que um item grava: tipo, hash da definição e `data` vêm do catálogo (node, relation) ou do latest (retirement).
struct Subject<'a> {
    lineage: &'a Lineage,
    type_name: Name,
    def_hash: &'a Hash,
    data: Option<&'a Value>,
    retirement: bool,
}

fn subject<'a>(
    defs: &'a dyn Definitions,
    item: &'a AppendItem,
    lineage: &'a Lineage,
    latest: Option<&'a LatestRow>,
) -> Result<Subject<'a>, HexlogError> {
    match (&item.body, latest) {
        (
            ItemBody::Node {
                type_name, data, ..
            },
            _,
        ) => {
            let info = defs
                .node_type(type_name)
                .ok_or_else(|| HexlogError::internal("node type is missing from the catalog"))?;
            Ok(Subject {
                lineage,
                type_name: Name::parse(type_name.as_str())?,
                def_hash: &info.def_hash,
                data: Some(data),
                retirement: false,
            })
        }
        (ItemBody::Relation { relation_type, .. }, _) => {
            let info = relation_info(defs, relation_type)?;
            Ok(Subject {
                lineage,
                type_name: Name::parse(relation_type.as_str())?,
                def_hash: &info.def_hash,
                data: None,
                retirement: false,
            })
        }
        (ItemBody::Retirement { .. }, Some(latest)) => Ok(Subject {
            lineage,
            type_name: latest.type_name.clone(),
            def_hash: &latest.def_hash,
            data: None,
            retirement: true,
        }),
        _ => Err(HexlogError::internal("retirement has no latest record")),
    }
}

/// Monta os records do lote; `at` é o mesmo em todos e o gerador de ids só roda aqui, depois de todas as recusas.
fn assemble(
    ctx: &DecideCtx<'_>,
    req: &AppendRequest,
    lineages: &[Cow<'_, Lineage>],
    head: &ChainHead,
    batch_fingerprint: Hash,
    validated: &[Validated],
) -> Result<AppendPlan, HexlogError> {
    if !instant_is_well_formed(ctx.now) {
        return Err(HexlogError::refused(
            ErrorCode::Internal,
            "clock-format",
            "clock returned a malformed instant",
        ));
    }
    let count = u32::try_from(req.items.len())
        .map_err(|_| HexlogError::internal("batch item count out of range"))?;
    let mut records: Vec<NewRecord> = Vec::with_capacity(req.items.len());
    for (index, ((item, lineage), found)) in
        req.items.iter().zip(lineages).zip(validated).enumerate()
    {
        let (seq, prev_hash) = match records.last() {
            Some(last) => (last.seq + 1, last.hash.clone()),
            None => (head.seq + 1, head.hash.clone()),
        };
        let batch = (index == 0).then(|| BodyBatch {
            fingerprint: Cow::Borrowed(batch_fingerprint.as_str()),
            key: req.key.as_ref().map(|key| Cow::Borrowed(key.as_str())),
            count,
        });
        let link = Link { seq, prev_hash };
        records.push(build_record(ctx, req, (item, lineage), found, link, batch)?);
    }
    let last = records
        .last()
        .ok_or_else(|| HexlogError::internal("batch produced no record"))?;
    let chain_head = ChainHead {
        seq: last.seq,
        hash: last.hash.clone(),
    };
    let outcome_records = records
        .iter()
        .map(|record| AppendedRecord {
            id: record.id.clone(),
            seq: record.seq,
        })
        .collect();
    let batch = req.key.as_ref().map(|key| BatchRow {
        key: key.clone(),
        fingerprint: batch_fingerprint,
        first_seq: head.seq + 1,
        item_count: count,
    });
    Ok(AppendPlan {
        records,
        batch,
        outcome: AppendOutcome {
            records: outcome_records,
            replayed: false,
            chain_head,
        },
    })
}

/// Posição do record na cadeia: o seu `seq` e o hash do anterior (ou a âncora).
struct Link {
    seq: u64,
    prev_hash: Hash,
}

fn build_record(
    ctx: &DecideCtx<'_>,
    req: &AppendRequest,
    (item, lineage): (&AppendItem, &Lineage),
    validated: &Validated,
    link: Link,
    batch: Option<BodyBatch<'_>>,
) -> Result<NewRecord, HexlogError> {
    let latest = validated.latest.as_ref();
    let subject = subject(ctx.defs, item, lineage, latest)?;
    let id = RecordId::parse(&format!(
        "{}:{}",
        req.tenant.as_str(),
        ctx.ids.next().hyphenated()
    ))
    .map_err(|_| {
        HexlogError::refused(
            ErrorCode::Internal,
            "id-generator",
            "id generator returned a malformed id",
        )
    })?;
    let body = RecordBody {
        v: 1,
        tenant: Cow::Borrowed(req.tenant.as_str()),
        seq: link.seq,
        id: Cow::Borrowed(id.as_str()),
        at: Cow::Borrowed(ctx.now),
        type_name: Cow::Borrowed(subject.type_name.as_str()),
        lineage: body_lineage(subject.lineage, validated.ends.as_ref())?,
        revises: latest.map(|found| Cow::Borrowed(found.record_id.as_str())),
        retirement: subject.retirement,
        data: subject.data.map(Cow::Borrowed),
        author: BodyAuthor {
            agent: Cow::Borrowed(&req.author.agent),
            model: req.author.model.as_deref().map(Cow::Borrowed),
            client: Cow::Borrowed(&req.author.client),
        },
        snapshot: item.snapshot.as_ref().map(|found| BodySnapshot {
            kind: Cow::Borrowed(found.kind().as_str()),
            id: Cow::Borrowed(found.id()),
        }),
        external_id: item
            .external_id
            .as_ref()
            .map(|found| Cow::Borrowed(found.as_str())),
        native_catalog: validated
            .ends
            .as_ref()
            .and_then(|ends| ends.native_catalog.as_deref())
            .map(Cow::Borrowed),
        def_hash: Cow::Borrowed(subject.def_hash.as_str()),
        batch,
    };
    let text = body.canonical()?;
    let hash = hash_record(link.prev_hash.as_str(), &text);
    let columns = record_columns(ctx.now, item, &subject, validated);
    Ok(NewRecord {
        seq: link.seq,
        id,
        prev_hash: link.prev_hash,
        hash,
        body: text,
        columns,
    })
}

fn body_lineage<'a>(
    lineage: &'a Lineage,
    ends: Option<&Ends>,
) -> Result<BodyLineage<'a>, HexlogError> {
    match (lineage, ends) {
        (Lineage::Node(id), _) => Ok(BodyLineage::Node {
            id: Cow::Borrowed(id.as_str()),
        }),
        (
            Lineage::Relation {
                relation_type,
                from,
                to,
            },
            Some(ends),
        ) => Ok(BodyLineage::Relation {
            relation_type: Cow::Borrowed(relation_type.as_str()),
            from: Cow::Borrowed(from.as_str()),
            to: Cow::Borrowed(to.as_str()),
            from_origin: Cow::Borrowed(ends.from.as_str()),
            to_origin: Cow::Borrowed(ends.to.as_str()),
        }),
        (Lineage::Relation { .. }, None) => Err(HexlogError::internal(
            "relation item has no resolved endpoints",
        )),
    }
}

fn record_columns(
    now: &str,
    item: &AppendItem,
    subject: &Subject<'_>,
    validated: &Validated,
) -> RecordColumns {
    let (lineage_kind, relation) = match subject.lineage {
        Lineage::Node(_) => (LineageKind::Node, None),
        Lineage::Relation {
            relation_type,
            from,
            to,
        } => (LineageKind::Relation, Some((relation_type, from, to))),
    };
    RecordColumns {
        at: now.to_owned(),
        type_name: subject.type_name.clone(),
        lineage_kind,
        lineage_key: lineage_columns(subject.lineage).1.into_owned(),
        retired: subject.retirement,
        revises: validated
            .latest
            .as_ref()
            .map(|found| found.record_id.clone()),
        relation_type: relation.map(|(relation_type, _, _)| relation_type.clone()),
        from_id: relation.map(|(_, from, _)| from.clone()),
        to_id: relation.map(|(_, _, to)| to.clone()),
        from_origin: validated.ends.as_ref().map(|ends| ends.from),
        to_origin: validated.ends.as_ref().map(|ends| ends.to),
        snapshot: item.snapshot.clone(),
        external_id: item.external_id.clone(),
        def_hash: subject.def_hash.clone(),
    }
}
