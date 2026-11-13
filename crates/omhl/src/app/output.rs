use hexlog::{AppendOutcome, Author, BreakReason, ChainReport, NodeView, Page};
use serde_json::{Value, json};

use crate::facade::ClosedWorkItem;
use crate::gate::{GateReport, QuestionKind, QuestionReport};

pub fn node(view: &NodeView) -> Value {
    let mut node = json!({
        "id": view.id,
        "type": view.type_name,
        "record_id": view.record_id,
        "seq": view.seq,
        "at": view.at,
        "data": view.data,
        "author": author(&view.author),
        "shadowed": view.shadowed,
    });
    if let Some(snapshot) = &view.snapshot {
        node["snapshot"] = json!(snapshot);
    }
    if let Some(external_id) = &view.external_id {
        node["external_id"] = json!(external_id);
    }

    node
}

pub fn page(page: &Page<NodeView>) -> Value {
    let items: Vec<Value> = page.items.iter().map(node).collect();
    let mut value = json!({ "items": items });
    if let Some(next) = page.next_after_seq {
        value["next_after_seq"] = json!(next);
    }

    value
}

pub fn appended(outcome: &AppendOutcome) -> Value {
    let records: Vec<Value> = outcome
        .records
        .iter()
        .map(|record| json!({ "id": record.id, "seq": record.seq }))
        .collect();

    json!({
        "records": records,
        "replayed": outcome.replayed,
        "chain_head": { "seq": outcome.chain_head.seq, "hash": outcome.chain_head.hash },
    })
}

pub fn gate(report: &GateReport) -> Value {
    let questions: Vec<Value> = report.questions.iter().map(question).collect();

    json!({
        "gate": report.gate,
        "passed": report.passed,
        "questions": questions,
        "chain_head": { "seq": report.chain_head.seq, "hash": report.chain_head.hash },
    })
}

pub fn closed(closed: &ClosedWorkItem) -> Value {
    json!({ "outcome": appended(&closed.outcome), "report": gate(&closed.report) })
}

pub fn verify(report: &ChainReport) -> Value {
    let breaks: Vec<Value> = report
        .breaks
        .iter()
        .map(|found| {
            let mut entry = json!({ "reason": break_reason(found.reason) });
            if let Some(seq) = found.seq {
                entry["seq"] = json!(seq);
            }
            entry
        })
        .collect();

    json!({
        "ok": report.ok,
        "total_records": report.total_records,
        "chain_head": report.chain_head,
        "breaks": breaks,
        "total_breaks": report.total_breaks,
    })
}

pub fn author(author: &Author) -> Value {
    let mut value = json!({ "agent": author.agent, "client": author.client });
    if let Some(model) = &author.model {
        value["model"] = json!(model);
    }

    value
}

fn question(report: &QuestionReport) -> Value {
    json!({
        "label": report.label,
        "kind": question_kind(report.kind),
        "passed": report.passed,
        "matched": report.matched,
        "failing": report.failing,
        "omitted": report.omitted,
    })
}

fn question_kind(kind: QuestionKind) -> &'static str {
    match kind {
        QuestionKind::Approved => "approved",
        QuestionKind::Occurred => "occurred",
        QuestionKind::NoPending => "no-pending",
        QuestionKind::NoOpenContradiction => "no-open-contradiction",
    }
}

/// `BreakReason::as_str` não é público.
fn break_reason(reason: BreakReason) -> &'static str {
    match reason {
        BreakReason::DivergingSeq => "diverging-seq",
        BreakReason::HashMismatch => "hash-mismatch",
        BreakReason::InvalidBody => "invalid-body",
        BreakReason::ColumnMismatch => "column-mismatch",
        BreakReason::BatchIndexMismatch => "batch-index-mismatch",
        BreakReason::AnchorMismatch => "anchor-mismatch",
        BreakReason::ChainHeadNotFound => "chain-head-not-found",
    }
}
