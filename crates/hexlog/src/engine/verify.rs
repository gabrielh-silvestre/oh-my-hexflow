use std::collections::HashMap;

use crate::engine::chain::{self, hash_record, sha256hex};
use crate::engine::model::{
    BODY_VERSION, BodyLineage, BodySnapshot, BreakReason, ChainBreak, ChainReport, RecordBody,
    read_body,
};
use crate::engine::ports::{LineageKind, StoredBatch, StoredRow, VerifyReads};
use crate::error::HexlogError;
use crate::ids::{Hash, TenantId};

const PAGE_SIZE: u32 = 500;
const BREAKS_SHOWN_MAX: usize = 100;

/// As colunas e os campos do corpo que `verify_chain` compara com o record gravado.
pub const CHECKED_FIELDS: &[&str] = &[
    "seq",
    "prev_hash",
    "hash",
    "body_content",
    "id",
    "at",
    "type",
    "lineage_kind",
    "lineage_key",
    "retired",
    "revises",
    "def_hash",
    "relation_type",
    "from_id",
    "to_id",
    "from_origin",
    "to_origin",
    "snapshot_kind",
    "snapshot_id",
    "external_id",
    "body_not_canonical",
    "body_not_json",
    "body_tenant",
    "body_seq",
    "tenants_anchor",
    "tenants_anchor_doc",
    "tenants_created_at",
    "batches_key",
    "batches_fingerprint",
    "batches_first_seq",
    "batches_item_count",
    "batches_row_missing",
    "batches_row_extra",
];

/// As primeiras quebras, até o teto, e a contagem de todas.
#[derive(Default)]
struct BreakLog {
    shown: Vec<ChainBreak>,
    total: u64,
}

impl BreakLog {
    fn push(&mut self, seq: Option<u64>, reason: BreakReason) {
        self.total += 1;
        if self.shown.len() < BREAKS_SHOWN_MAX {
            self.shown.push(ChainBreak { seq, reason });
        }
    }

    /// Acrescenta `later` depois das quebras já guardadas.
    fn absorb(&mut self, later: BreakLog) {
        let room = BREAKS_SHOWN_MAX - self.shown.len();
        self.shown.extend(later.shown.into_iter().take(room));
        self.total += later.total;
    }
}

/// O que o corpo de um record com chave diz sobre a linha de `batches`.
struct ExpectedBatch {
    fingerprint: String,
    first_seq: i64,
    count: i64,
}

/// O record anterior: o `seq` e o `hash` armazenados, ou a âncora recalculada antes do primeiro.
struct Previous {
    seq: i64,
    hash: String,
}

struct Scan {
    breaks: BreakLog,
    batches: HashMap<String, ExpectedBatch>,
    total_records: u64,
    last_recomputed: Option<Hash>,
    expected_found: bool,
}

/// Quebra é resultado, não erro: só tenant desconhecido e falha do banco saem como `Err`.
pub(crate) fn verify_chain(
    reads: &mut dyn VerifyReads,
    tenant: &TenantId,
    expected: Option<&Hash>,
) -> Result<ChainReport, HexlogError> {
    let stored = reads
        .stored_tenant(tenant)?
        .ok_or_else(HexlogError::tenant_not_found)?;
    let anchor = sha256hex(&stored.anchor_doc);
    let mut breaks = BreakLog::default();
    if anchor.as_str() != stored.anchor
        || stored.anchor_doc != chain::anchor_doc(tenant, &stored.created_at)
    {
        breaks.push(None, BreakReason::AnchorMismatch);
    }
    let scan = scan_records(reads, tenant, &anchor, expected)?;
    breaks.absorb(compare_batches(reads.stored_batches(tenant)?, scan.batches));
    breaks.absorb(scan.breaks);
    if expected.is_some() && !scan.expected_found {
        breaks.push(None, BreakReason::ChainHeadNotFound);
    }
    Ok(ChainReport {
        ok: breaks.total == 0,
        total_records: scan.total_records,
        chain_head: scan.last_recomputed.unwrap_or(anchor),
        breaks: breaks.shown,
        total_breaks: breaks.total,
    })
}

/// Lê os records em páginas; só a página corrente fica na memória, e o corpo empresta da linha.
fn scan_records(
    reads: &mut dyn VerifyReads,
    tenant: &TenantId,
    anchor: &Hash,
    expected: Option<&Hash>,
) -> Result<Scan, HexlogError> {
    let mut scan = Scan {
        breaks: BreakLog::default(),
        batches: HashMap::new(),
        total_records: 0,
        last_recomputed: None,
        expected_found: expected == Some(anchor),
    };
    let mut previous = Previous {
        seq: 0,
        hash: anchor.as_str().to_owned(),
    };
    let mut after_seq = None;
    loop {
        let page = reads.stored_records(tenant, after_seq, PAGE_SIZE)?;
        let Some(last_seq) = page.last().map(|row| row.seq) else {
            return Ok(scan);
        };
        let full_page = u32::try_from(page.len()) == Ok(PAGE_SIZE);
        scan.total_records += u64::try_from(page.len())
            .map_err(|_| HexlogError::internal("page length out of range"))?;
        for row in page {
            scan.inspect_row(tenant, &previous, &row, expected);
            previous = Previous {
                seq: row.seq,
                hash: row.hash,
            };
        }
        if !full_page {
            return Ok(scan);
        }
        after_seq = Some(last_seq);
    }
}

impl Scan {
    fn inspect_row(
        &mut self,
        tenant: &TenantId,
        previous: &Previous,
        row: &StoredRow,
        expected: Option<&Hash>,
    ) {
        let recomputed = hash_record(&row.prev_hash, &row.body);
        let body = read_body(&row.body).ok().filter(|body| {
            body.v == BODY_VERSION && body.canonical().is_ok_and(|text| text == row.body)
        });
        match first_break(tenant, previous, row, &recomputed, body.as_ref()) {
            Some(reason) => self.breaks.push(u64::try_from(row.seq).ok(), reason),
            None => {
                self.expected_found |= expected.is_some_and(|wanted| wanted.as_str() == row.hash)
            }
        }
        // Um corpo ilegível não alimenta o conjunto esperado; um hash ou coluna errados, sim.
        let batch = body.as_ref().and_then(|body| body.batch.as_ref());
        if let Some(batch) = batch
            && let Some(key) = &batch.key
        {
            self.batches.insert(
                key.clone().into_owned(),
                ExpectedBatch {
                    fingerprint: batch.fingerprint.clone().into_owned(),
                    first_seq: row.seq,
                    count: i64::from(batch.count),
                },
            );
        }
        self.last_recomputed = Some(recomputed);
    }
}

/// Primeiro problema do record, na ordem: `seq`, cadeia de hashes, corpo, colunas.
fn first_break(
    tenant: &TenantId,
    previous: &Previous,
    row: &StoredRow,
    recomputed: &Hash,
    body: Option<&RecordBody<'_>>,
) -> Option<BreakReason> {
    if row.seq != previous.seq.saturating_add(1) {
        return Some(BreakReason::DivergingSeq);
    }
    if row.prev_hash != previous.hash || row.hash != recomputed.as_str() {
        return Some(BreakReason::HashMismatch);
    }
    let Some(body) = body else {
        return Some(BreakReason::InvalidBody);
    };
    let agrees = body.tenant == tenant.as_str()
        && u64::try_from(row.seq) == Ok(body.seq)
        && derived_columns_match(row, body);
    (!agrees).then_some(BreakReason::ColumnMismatch)
}

fn derived_columns_match(row: &StoredRow, body: &RecordBody<'_>) -> bool {
    row.id == body.id
        && row.at == body.at
        && row.type_name == body.type_name
        && row.retired == i64::from(body.retirement)
        && row.revises.as_deref() == body.revises.as_deref()
        && row.external_id.as_deref() == body.external_id.as_deref()
        && row.def_hash == body.def_hash
        && snapshot_matches(row, body.snapshot.as_ref())
        && lineage_matches(row, &body.lineage)
}

fn snapshot_matches(row: &StoredRow, snapshot: Option<&BodySnapshot<'_>>) -> bool {
    row.snapshot_kind.as_deref() == snapshot.map(|found| &*found.kind)
        && row.snapshot_id.as_deref() == snapshot.map(|found| &*found.id)
}

fn lineage_matches(row: &StoredRow, lineage: &BodyLineage<'_>) -> bool {
    match lineage {
        BodyLineage::Node { id } => {
            row.lineage_kind == LineageKind::Node.as_str()
                && row.lineage_key == *id
                && [
                    &row.relation_type,
                    &row.from_id,
                    &row.to_id,
                    &row.from_origin,
                    &row.to_origin,
                ]
                .iter()
                .all(|column| column.is_none())
        }
        BodyLineage::Relation {
            relation_type,
            from,
            to,
            from_origin,
            to_origin,
        } => {
            row.lineage_kind == LineageKind::Relation.as_str()
                && relation_key_matches(&row.lineage_key, relation_type, from, to)
                && row.relation_type.as_deref() == Some(&**relation_type)
                && row.from_id.as_deref() == Some(&**from)
                && row.to_id.as_deref() == Some(&**to)
                && row.from_origin.as_deref() == Some(&**from_origin)
                && row.to_origin.as_deref() == Some(&**to_origin)
        }
    }
}

/// `<relation_type>|<from>|<to>`, sem montar o texto.
fn relation_key_matches(key: &str, relation_type: &str, from: &str, to: &str) -> bool {
    key.strip_prefix(relation_type)
        .and_then(|rest| rest.strip_prefix('|'))
        .and_then(|rest| rest.strip_prefix(from))
        .and_then(|rest| rest.strip_prefix('|'))
        .is_some_and(|rest| rest == to)
}

/// Uma quebra por linha sem lote esperado, por lote esperado sem linha e por lote que diverge.
fn compare_batches(
    stored: Vec<StoredBatch>,
    mut expected: HashMap<String, ExpectedBatch>,
) -> BreakLog {
    let mut log = BreakLog::default();
    for row in stored {
        let agrees = expected.remove(&row.key).is_some_and(|wanted| {
            wanted.fingerprint == row.fingerprint
                && wanted.first_seq == row.first_seq
                && wanted.count == row.item_count
        });
        if !agrees {
            log.push(None, BreakReason::BatchIndexMismatch);
        }
    }
    for _ in expected.values() {
        log.push(None, BreakReason::BatchIndexMismatch);
    }
    log
}
