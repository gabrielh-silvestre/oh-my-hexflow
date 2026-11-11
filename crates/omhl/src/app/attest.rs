use hexlog::{AppendItem, AppendRequest, ItemBody, RecordId, SemanticId, Snapshot, TypeName};
use serde_json::{Value, json};

use super::action::Caller;
use super::engine::{Engine, parse_at};
use super::error::AppError;
use super::output;
use crate::error::OmhlError;
use crate::guard::attestation_id;

const OUTCOMES: [&str; 3] = ["passed", "passed_with_warning", "failed"];
const FAILED: &str = "failed";
const ATTESTATION: &str = "attestation";

impl Engine<'_> {
    /// Grava o atestado de `check` para `work_item` no commit da sonda, com o id derivado.
    pub(super) fn attest(
        &mut self,
        caller: &Caller,
        check: &str,
        work_item: &str,
        outcome: &str,
        note: Option<&str>,
    ) -> Result<Value, AppError> {
        if !OUTCOMES.contains(&outcome) {
            return Err(AppError::invalid_arguments("/outcome", "unknown value"));
        }
        let check = parse_at(SemanticId::parse, check, "/check")?;
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshot = self.probe_snapshot()?;
        let id = derived_id(&snapshot, &check, &work_item)?;
        let data = attestation_data(&check, &work_item, outcome, note, None);
        let base = self.current_base(&id)?;
        let attestation = json!({ "id": id, "snapshot": snapshot });
        let item = AppendItem {
            body: ItemBody::Node {
                id,
                type_name: TypeName::parse(ATTESTATION)?,
                data: data.clone(),
            },
            base,
            snapshot: Some(snapshot),
            external_id: None,
        };
        let request = AppendRequest {
            tenant: self.tenant.clone(),
            author: caller.author(),
            key: None,
            items: vec![item],
        };
        let appended = self.omhl.append(&request).map_err(attest_pointers)?;

        Ok(response(attestation, data, output::appended(&appended)))
    }

    /// Sem `base` o motor responderia `FORK_REJECTED`: repetir o atestado é uma revisão dele.
    pub(super) fn current_base(&mut self, id: &SemanticId) -> Result<Option<RecordId>, AppError> {
        Ok(self
            .omhl
            .get(&self.tenant, id)?
            .map(|existing| existing.record_id))
    }
}

pub(super) fn derived_id(
    snapshot: &Snapshot,
    check: &SemanticId,
    work_item: &SemanticId,
) -> Result<SemanticId, OmhlError> {
    attestation_id(snapshot, check, work_item).map_err(|error| match error {
        OmhlError::Rule { code, .. } => OmhlError::rule(code, "/check"),
        other => other,
    })
}

pub(super) fn attestation_data(
    check: &SemanticId,
    work_item: &SemanticId,
    outcome: &str,
    note: Option<&str>,
    byproduct: Option<&str>,
) -> Value {
    let mut data = json!({
        "check": check,
        "work_item": work_item,
        "outcome": outcome,
        "ok": outcome != FAILED,
    });
    if let Some(note) = note {
        data["note"] = json!(note);
    }
    if let Some(byproduct) = byproduct {
        data["byproduct"] = json!(byproduct);
    }

    data
}

/// `{"attestation": {...}}` mais os campos do `APPEND`.
fn response(mut attestation: Value, data: Value, appended: Value) -> Value {
    if let (Some(target), Value::Object(fields)) = (attestation.as_object_mut(), data) {
        target.extend(fields);
    }
    let mut value = json!({ "attestation": attestation });
    if let (Some(target), Value::Object(fields)) = (value.as_object_mut(), appended) {
        target.extend(fields);
    }

    value
}

/// Os ponteiros do pedido sintético viram os dos argumentos da ação.
fn attest_pointers(error: OmhlError) -> OmhlError {
    match error {
        OmhlError::Rule { code, pointer } => OmhlError::Rule {
            code,
            pointer: attest_pointer(&pointer),
        },
        OmhlError::Hexlog(mut error) => {
            for detail in &mut error.details {
                detail.path = attest_pointer(&detail.path);
            }
            OmhlError::Hexlog(error)
        }
    }
}

fn attest_pointer(pointer: &str) -> String {
    if let Some(field) = pointer.strip_prefix("/items/0/body/data/") {
        return format!("/{field}");
    }
    match pointer {
        "/items/0/snapshot" => "/snapshot".to_owned(),
        "/items/0/body/id" => "/check".to_owned(),
        other => other.to_owned(),
    }
}
