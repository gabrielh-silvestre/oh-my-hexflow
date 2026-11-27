use ::omhl::error::OmhlError;
use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::graphs;
use ::omhl::native::NativeBase;
use hexlog::{
    AppendItem, AppendOutcome, AppendRequest, Author, ItemBody, Lineage, RecordId, Snapshot,
    TenantId,
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
            session: &crate::support::NO_SESSION,
        };
        let mut omhl = Omhl::open(&self.raw.config(), deps).expect("omhl opens");
        omhl.ensure_tenant(&acme()).expect("the tenant is created");

        omhl
    }

    /// Grava direto no `hexlog`, sem a guarda do `omhl`: fixture de um record que a guarda impede de gravar
    /// (autor fora da matriz do papel). O banco é o mesmo do `open`.
    pub fn forge(&self, req: &AppendRequest) -> AppendOutcome {
        let native = NativeBase;
        let catalog = graphs::catalog().expect("the embedded catalogs build");

        self.raw
            .hexlog(catalog, &native)
            .append(req)
            .expect("the forged append is accepted")
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

/// Com tipo sem átomo: o `close` não procura o branch, e a fixture não tem um que dê o tipo.
pub fn typed_work_item_data(title: &str) -> Value {
    json!({ "title": title, "status": "open", "type": "chore" })
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
    json!({ "when": { "vocabulary": "closed", "event": event }, "how": how, "blocking": blocking })
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

/// As três divergências da consulta por campo: `(id, work_item, kind, resolution)`.
/// Cada filtro separa um subconjunto próprio: `w2` pega d2 e d3, `missing` d1 e d2, `open` d1 e d3.
pub const DIVERGENCE_ROWS: [(&str, &str, &str, &str); 3] = [
    ("divergences.d1", "work-items.w1", "missing", "open"),
    ("divergences.d2", "work-items.w2", "missing", "code-fixed"),
    ("divergences.d3", "work-items.w2", "partial", "open"),
];

/// Itens (`{"body": ...}`) dos work items `w1` e `w2`, abertos; entram antes das divergências.
pub fn divergence_work_item_wires() -> Vec<Value> {
    ["work-items.w1", "work-items.w2"]
        .iter()
        .map(|id| {
            json!({ "body": {
                "kind": "node", "id": id, "type": "work-item", "data": work_item_data("t"),
            } })
        })
        .collect()
}

/// Itens (`{"body": ...}`) das divergências de `DIVERGENCE_ROWS`.
pub fn divergence_wires() -> Vec<Value> {
    DIVERGENCE_ROWS
        .iter()
        .map(|(id, work_item, kind, resolution)| {
            let data = json!({
                "source_ref": "spec#ac-3",
                "kind": kind,
                "text": DIVERGENCE_TEXT,
                "evidence": [{ "source": "docs", "summary": "the docs say so" }],
                "resolution": resolution,
                "work_item": work_item,
            });

            json!({ "body": { "kind": "node", "id": id, "type": "divergence", "data": data } })
        })
        .collect()
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

/// Como `append_ok`, com o autor dado; devolve o erro em vez de falhar o teste.
pub fn append_as(
    omhl: &mut Omhl<'_>,
    author: Author,
    items: Vec<AppendItem>,
) -> Result<AppendOutcome, OmhlError> {
    omhl.append(&AppendRequest {
        author,
        ..request(items)
    })
}

/// Como `append_as`, falhando o teste se a guarda recusar.
pub fn append_ok_as(omhl: &mut Omhl<'_>, author: Author, items: Vec<AppendItem>) -> AppendOutcome {
    append_as(omhl, author, items).expect("the append is accepted")
}

/// String kebab do código de uma recusa de regra; falha o teste se o erro vier do `hexlog`.
/// Compara-se por texto para os testes compilarem antes de o código existir no `RuleCode`.
pub fn rule_code(error: &OmhlError) -> &'static str {
    match error {
        OmhlError::Rule { code, .. } => code.as_str(),
        OmhlError::Hexlog(other) => panic!("expected a rule refusal, got a hexlog error: {other}"),
    }
}

/// Record corrente do node, a `base` das revisões e retiradas.
pub fn record_id_of(omhl: &mut Omhl<'_>, id: &str) -> RecordId {
    omhl.get(&acme(), &sid(id))
        .expect("the node is read")
        .expect("the node exists")
        .record_id
}

/// Grava o pull request `work-items.pr-7` aberto e devolve o id do record.
pub fn seed_pull_request(omhl: &mut Omhl<'_>) -> RecordId {
    let item = node_item(PR_7, "pull-request", pull_request_data("Add gate"));

    append_ok(omhl, vec![item]).records[0].id.clone()
}

// Mensagens das recusas do `list` (a regra, nunca o valor recebido)

pub const ID_PREFIX_RULE: &str = "id prefix must be dot-separated names of 1 to 63 characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to 200 characters in total";
pub const PREFIX_NAMESPACE_ANY: &str = "id prefix must start with the namespace of a type";
pub const PREFIX_NAMESPACE_TYPE: &str = "id prefix is outside the namespace of the type";
pub const TYPE_NOT_DEFINED: &str = "type is not defined in the catalog";
pub const FIELD_UNDECLARED: &str = "field is not declared by the type";
pub const FIELD_NOT_TEXT: &str = "field is not a text field; where compares text only";
pub const FIELD_NOT_TEXT_ANY: &str = "field is not a text field of any type the filter allows";
pub const NO_TYPE_DECLARES_ALL: &str = "no type declares every where field";
pub const VALUE_OUTSIDE_ENUM: &str = "value is not one of the values the field allows";
pub const FIELD_REPEATED: &str = "field repeated in where with different values";
pub const LIMIT_RULE: &str = "limit must be between 1 and 200";
pub const WHERE_RULE: &str = "where must be FIELD=VALUE with a non-empty FIELD";
pub const OUTCOME_RULE: &str = "outcome must be passed, passed_with_warning or failed";
pub const PROJECT_DIR_MISSING: &str = "the project directory does not exist";
pub const DIVERGENCE_TEXT: &str = "the spec asks for a report the code does not have";

/// O erro de uma opção global recusada: sem ponteiro, logo sem detalhes.
pub fn project_dir_refusal() -> Value {
    json!({
        "error": {
            "code": "invalid-project-dir",
            "message": PROJECT_DIR_MISSING,
            "details": [],
        }
    })
}

/// O erro que o `list` devolve para uma opção recusada: a regra na `message` do topo e em `details[0]`.
pub fn list_refusal(pointer: &str, message: &str) -> Value {
    json!({
        "error": {
            "code": "invalid-arguments",
            "message": message,
            "details": [{
                "path": pointer,
                "code": "invalid-arguments",
                "message": message,
            }],
        }
    })
}
