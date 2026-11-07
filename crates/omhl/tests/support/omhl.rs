use ::omhl::error::OmhlError;
use ::omhl::facade::{Omhl, OmhlDeps};
use hexlog::{
    AppendItem, AppendOutcome, AppendRequest, ItemBody, Lineage, RecordId, Snapshot, TenantId,
};
use serde_json::{Value, json};

use super::raw::RawHarness;
use super::{FakeProbe, author, sid, tenant, tname};

/// Work item de apoio dos testes da camada de invariantes (aberto, ver `seed_pull_request`).
pub const PR_7: &str = "work-items.pr-7";
pub const RUN_TESTS: &str = "checks.run-tests";
pub const LINT_DOCS: &str = "checks.lint-docs";

pub fn acme() -> TenantId {
    tenant("acme")
}

/// Donos do que um `Omhl` empresta: banco temporário, relógio, ids e a sonda falsa.
/// Para mudar o resultado da sonda, larga-se o `Omhl`, chama-se `set_probe` e reabre-se.
pub struct OmhlHarness {
    raw: RawHarness,
    probe: FakeProbe,
}

impl OmhlHarness {
    pub fn new(probe: FakeProbe) -> OmhlHarness {
        OmhlHarness {
            raw: RawHarness::new(),
            probe,
        }
    }

    pub fn set_probe(&mut self, probe: FakeProbe) {
        self.probe = probe;
    }

    /// Abre o `Omhl` real sobre o arquivo temporário e garante o tenant `acme`.
    pub fn open(&self) -> Omhl<'_> {
        let deps = OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe: &self.probe,
        };
        let mut omhl = Omhl::open(&self.raw.config(), deps).expect("omhl opens");
        omhl.ensure_tenant(&acme()).expect("the tenant is created");

        omhl
    }
}

// Montagem de pedidos

pub fn request(items: Vec<AppendItem>) -> AppendRequest {
    AppendRequest {
        tenant: acme(),
        author: author(),
        key: None,
        items,
    }
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

/// Revision: o mesmo item, com `base` apontando para o record corrente.
pub fn revision_item(id: &str, type_name: &str, data: Value, base: &RecordId) -> AppendItem {
    AppendItem {
        base: Some(base.clone()),
        ..node_item(id, type_name, data)
    }
}

pub fn retirement_item(id: &str, base: &RecordId) -> AppendItem {
    AppendItem {
        body: ItemBody::Retirement {
            lineage: Lineage::Node(sid(id)),
        },
        base: Some(base.clone()),
        snapshot: None,
        external_id: None,
    }
}

pub fn with_snapshot(item: AppendItem, snapshot: &Snapshot) -> AppendItem {
    AppendItem {
        snapshot: Some(snapshot.clone()),
        ..item
    }
}

/// Id de attestation pela fórmula do contrato, escrita aqui de forma independente da implementação:
/// `attestations.<sha[..32]>.<sha[32..]>.<check>.<work_item>`.
pub fn attestation_id_for(snapshot: &Snapshot, check: &str, work_item: &str) -> String {
    let sha = snapshot.id();

    format!(
        "attestations.{}.{}.{check}.{work_item}",
        &sha[..32],
        &sha[32..]
    )
}

/// Attestation criada em `snapshot`: id derivado, desfecho `passed`, `ok: true`.
pub fn attestation_item(snapshot: &Snapshot, check: &str, work_item: &str) -> AppendItem {
    let id = attestation_id_for(snapshot, check, work_item);

    with_snapshot(
        node_item(
            &id,
            "attestation",
            attestation_data(check, work_item, "passed", true),
        ),
        snapshot,
    )
}

// Dados dos tipos do catalog

pub fn pull_request_data(title: &str) -> Value {
    json!({
        "title": title,
        "status": "open",
        "provider": "github",
        "repo": "gabrielh-silvestre/oh-my-hexflow",
        "number": 7,
    })
}

pub fn work_item_data(title: &str) -> Value {
    json!({ "title": title, "status": "open" })
}

/// Troca o `status` por `closed` e acrescenta o objeto `gate` que o schema exige nesse estado.
pub fn closed_with_gate(mut data: Value) -> Value {
    data["status"] = json!("closed");
    data["gate"] = json!({
        "name": "pr-gate",
        "passed": true,
        "chain_head_seq": 3,
        "snapshot": { "kind": "git-commit", "id": "a".repeat(40) },
    });

    data
}

pub fn check_data(event: &str, how: &str, blocking: bool) -> Value {
    json!({ "when": { "vocabulary": "free", "event": event }, "how": how, "blocking": blocking })
}

pub fn attestation_data(check: &str, work_item: &str, outcome: &str, ok: bool) -> Value {
    json!({ "check": check, "work_item": work_item, "outcome": outcome, "ok": ok })
}

pub fn premise_data(text: &str, work_item: Option<&str>) -> Value {
    let mut data = json!({ "text": text, "status": "assumed" });
    if let Some(id) = work_item {
        data["work_item"] = json!(id);
    }

    data
}

pub fn decision_data(text: &str, work_item: Option<&str>) -> Value {
    let mut data = json!({
        "text": text,
        "evidence": [{ "source": "docs", "summary": "the docs say so" }],
    });
    if let Some(id) = work_item {
        data["work_item"] = json!(id);
    }

    data
}

// Atalhos sobre o `Omhl`

pub fn append_ok(omhl: &mut Omhl<'_>, items: Vec<AppendItem>) -> AppendOutcome {
    omhl.append(&request(items))
        .expect("the append is accepted")
}

/// Erro de regra (ou do `hexlog`) com que o `append` recusa; falha o teste se for aceito.
pub fn refusal(omhl: &mut Omhl<'_>, items: Vec<AppendItem>) -> OmhlError {
    omhl.append(&request(items))
        .expect_err("the append is refused")
}

/// Grava o pull request `work-items.pr-7` aberto e devolve o id do record.
pub fn seed_pull_request(omhl: &mut Omhl<'_>) -> RecordId {
    let item = node_item(PR_7, "pull-request", pull_request_data("Add gate"));

    append_ok(omhl, vec![item]).records[0].id.clone()
}
