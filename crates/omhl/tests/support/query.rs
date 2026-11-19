use hexlog::{AppendItem, Snapshot, SnapshotKind};
use serde_json::{Value, json};

use super::agent_author;
use super::forge::forge;
use super::omhl::{check_data, node_item, pull_request_data, with_snapshot, work_item_data};
use super::project::Project;

// Ids do estado de fixture da consulta: um nó de cada um dos nove tipos (mais um check sombreado e um work item
// fechado). Nenhum id nem texto repete um valor de vocabulário fechado (`lint`, `open`, `missing`...).

pub const WORK_ITEM: &str = "work-items.payroll";
pub const CHECK: &str = "checks.alpha-probe";
pub const PREMISE: &str = "premises.cents";
pub const GUIDELINE: &str = "guidelines.bind-params";
pub const DECISION: &str = "decisions.journal-mode";
pub const GAP: &str = "gaps.rounding";
pub const DIVERGENCE: &str = "divergences.report";
pub const PULL_REQUEST: &str = "work-items.pr-payroll";
/// Id reservado por um check nativo: o nó do banco fica sombreado.
pub const SHADOWED_CHECK: &str = "checks.run-tests";
pub const CLOSED_WORK_ITEM: &str = "work-items.archived";

const ATTESTATION_SHA: &str = "3f9a1c07d25be8446a0b7c91e5d2f48a6b3c0e17";
const GATE_SHA: &str = "c81d0e5b7a4f92336e1d8b05a7f4c29e60b13d58";
const SPEC_DIGEST: &str = "9be4d27a60c1f5883ae2b7d14c05e96f3a8d71b2c4e05f6a193b8d27c40e5f61";

/// O id da attestation como o app o compõe: o SHA partido em 32 e 8 caracteres, o check e o work item.
pub const ATTESTATION: &str =
    "attestations.3f9a1c07d25be8446a0b7c91e5d2f48a.6b3c0e17.checks.alpha-probe.work-items.payroll";

fn confirmation(question: &str, answer: &str) -> Value {
    json!({ "by": "person:ana", "question": question, "answer": answer })
}

fn project_check() -> Value {
    json!({
        "when": { "vocabulary": "closed", "event": "before-pr" },
        "how": "Guards the payroll totals.",
        "blocking": true,
        "concern": "style",
        "variant": "format",
        "run": ["sh", "-c", "exit 0"],
        "timeout_secs": 60,
        "expect": [{ "exit": 0, "outcome": "passed" }],
    })
}

fn attestation() -> AppendItem {
    let snapshot = Snapshot::new(SnapshotKind::GitCommit, ATTESTATION_SHA).expect("valid snapshot");
    let data = json!({
        "check": CHECK,
        "work_item": WORK_ITEM,
        "outcome": "passed",
        "ok": true,
        "note": "Verified by the probe.",
        "command": ["true"],
        "spec_digest": SPEC_DIGEST,
        "exit_code": 0,
        "duration_ms": 31,
        "timed_out": false,
        "truncated": false,
    });

    with_snapshot(node_item(ATTESTATION, "attestation", data), &snapshot)
}

fn closed_work_item() -> Value {
    json!({
        "title": "Archived rollout",
        "status": "closed",
        "gate": {
            "name": "pr-gate",
            "passed": true,
            "chain_head_seq": 11,
            "snapshot": { "kind": "git-commit", "id": GATE_SHA },
        },
    })
}

/// Onze nós em ordem de `seq`: work item, check, premissa, diretiva, decisão, gap, divergência, pull request,
/// attestation, check sombreado e work item fechado.
pub fn query_project() -> Project {
    let project = Project::new("acme");
    let items = vec![
        node_item(WORK_ITEM, "work-item", work_item_data("Payroll rollout")),
        node_item(CHECK, "check", project_check()),
        node_item(
            PREMISE,
            "premise",
            json!({
                "text": "Money lives in cents.",
                "status": "confirmed",
                "kind": "business",
                "work_item": WORK_ITEM,
                "confirmed_by": confirmation("How are amounts stored?", "As whole cents."),
            }),
        ),
        node_item(
            GUIDELINE,
            "guideline",
            json!({
                "text": "Every query binds its parameters.",
                "kind": "architecture",
                "confirmed_by": confirmation("Are queries parameterized?", "Always."),
            }),
        ),
        node_item(
            DECISION,
            "decision",
            json!({
                "text": "Use journaling for writes.",
                "evidence": [{ "source": "docs/storage.md", "summary": "The storage doc recommends it." }],
                "work_item": WORK_ITEM,
            }),
        ),
        node_item(
            GAP,
            "gap",
            json!({
                "question": "Which rounding applies?",
                "context": "Totals differ by one cent.",
                "work_item": WORK_ITEM,
                "kind": "technical",
            }),
        ),
        node_item(
            DIVERGENCE,
            "divergence",
            json!({
                "source_ref": "spec#ac-3",
                "kind": "partial",
                "text": "The export report is absent.",
                "evidence": [{ "source": "spec.md", "summary": "Spec item three asks for it." }],
                "resolution": "accepted",
                "work_item": WORK_ITEM,
                "confirmed_by": confirmation("Is the report dropped?", "Yes, dropped."),
                "amendment": "emenda-01",
            }),
        ),
        node_item(PULL_REQUEST, "pull-request", {
            let mut data = pull_request_data("Add the gate");
            data["repo"] = json!("acme-org/payroll-svc");
            data["number"] = json!(41);
            data["branch"] = json!("feat/wi-payroll__x");
            data
        }),
        attestation(),
        node_item(
            SHADOWED_CHECK,
            "check",
            check_data("before-commit", "Shadows the built-in probe.", false),
        ),
        node_item(CLOSED_WORK_ITEM, "work-item", closed_work_item()),
    ];

    forge(&project, agent_author(), items);

    project
}
