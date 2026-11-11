use hexlog::{NativeNodes, SemanticId};
use serde_json::Value;

use super::append::{Guard, NodeItem, status_of};
use super::attestation_id;
use crate::error::{OmhlError, RuleCode};
use crate::native::NativeBase;
use crate::probe::{ProbeError, ProbeOutcome};
use hexlog::{Snapshot, SnapshotKind};

impl Guard<'_, '_> {
    /// Regras 6 a 10 da attestation (work item, check, id derivado, sonda e desfecho).
    pub(super) fn check_attestation(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let work_item = self.find_open_work_item(node)?;
        let check = self.find_check(node)?;
        check_derived_id(node, check.as_ref(), work_item.as_ref())?;
        match node.base {
            None => self.check_head(node),
            Some(_) => self.check_outcome_unchanged(node),
        }
    }

    fn find_open_work_item(
        &mut self,
        node: &NodeItem<'_>,
    ) -> Result<Option<SemanticId>, OmhlError> {
        let Some(view) = self.find_work_item(node)? else {
            return Ok(None);
        };
        if status_of(&view.data) == Some("closed") {
            return Err(node.error(RuleCode::WorkItemClosed, "body/data/work_item"));
        }

        Ok(Some(view.id))
    }

    /// O check é nativo ou um node vigente, e nos dois casos do tipo `check`.
    fn find_check(&mut self, node: &NodeItem<'_>) -> Result<Option<SemanticId>, OmhlError> {
        let Some(raw) = node.field("check") else {
            return Ok(None);
        };
        let id = SemanticId::parse(raw)
            .map_err(|_| node.error(RuleCode::CheckNotFound, "body/data/check"))?;
        let type_name = match NativeBase.lookup(&id) {
            Some(native) => Some(native.type_name),
            None => self
                .hexlog
                .get(self.tenant, &id)?
                .map(|view| view.type_name),
        };

        match type_name {
            Some(found) if found.as_str() == "check" => Ok(Some(id)),
            _ => Err(node.error(RuleCode::CheckNotFound, "body/data/check")),
        }
    }

    /// Attestation criada: o snapshot é o `HEAD` de uma árvore limpa (`git-commit`) ou a árvore do índice (`git-tree`).
    fn check_head(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let Some(snapshot) = node.snapshot else {
            return Ok(());
        };

        let refusal = match snapshot.kind() {
            SnapshotKind::GitTree => tree_probe_refusal(self.probe_index_tree_once(), snapshot),
            // `SnapshotKind` é non_exhaustive: tipo futuro cai na regra de `HEAD`, que o recusa
            _ => probe_refusal(self.probe_once(), snapshot),
        };

        match refusal {
            Some(code) => Err(node.error(code, "snapshot")),
            None => Ok(()),
        }
    }

    /// Attestation revisada: o desfecho não muda.
    fn check_outcome_unchanged(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let Some(outcome) = node.field("outcome") else {
            return Ok(());
        };
        let current = self.hexlog.get(self.tenant, node.id)?;
        let changed = current
            .is_some_and(|view| view.data.get("outcome").and_then(Value::as_str) != Some(outcome));

        if changed {
            Err(node.error(RuleCode::OutcomeImmutable, "body/data/outcome"))
        } else {
            Ok(())
        }
    }
}

/// O id da attestation é derivado do snapshot, do check e do work item.
fn check_derived_id(
    node: &NodeItem<'_>,
    check: Option<&SemanticId>,
    work_item: Option<&SemanticId>,
) -> Result<(), OmhlError> {
    let (Some(snapshot), Some(check), Some(work_item)) = (node.snapshot, check, work_item) else {
        return Ok(());
    };

    match attestation_id(snapshot, check, work_item) {
        Ok(derived) if &derived == node.id => Ok(()),
        Ok(_) => Err(node.error(RuleCode::AttestationIdMismatch, "body/id")),
        Err(OmhlError::Rule { code, .. }) => Err(node.error(code, "body/id")),
        Err(other) => Err(other),
    }
}

/// Por que a resposta da sonda não aceita o snapshot como `HEAD` de uma árvore limpa.
pub(crate) fn probe_refusal(
    probed: &Result<ProbeOutcome, ProbeError>,
    snapshot: &Snapshot,
) -> Option<RuleCode> {
    match probed {
        Err(_) => Some(RuleCode::ProbeFailed),
        Ok(ProbeOutcome::Dirty) => Some(RuleCode::SnapshotDirty),
        Ok(ProbeOutcome::Reproducible(head)) if head != snapshot => Some(RuleCode::SnapshotNotHead),
        Ok(ProbeOutcome::Reproducible(_)) => None,
    }
}

/// Por que a resposta da sonda de árvore não aceita o snapshot como a árvore do índice.
pub(crate) fn tree_probe_refusal(
    probed: &Result<ProbeOutcome, ProbeError>,
    snapshot: &Snapshot,
) -> Option<RuleCode> {
    match probed {
        Err(_) => Some(RuleCode::ProbeFailed),
        Ok(ProbeOutcome::Dirty) => Some(RuleCode::SnapshotDirty),
        Ok(ProbeOutcome::Reproducible(tree)) if tree != snapshot => {
            Some(RuleCode::SnapshotTreeMismatch)
        }
        Ok(ProbeOutcome::Reproducible(_)) => None,
    }
}
