use hexlog::{NativeNodes, SemanticId};
use serde_json::Value;

use super::append::{Guard, NodeItem, status_of};
use super::attestation_id;
use crate::error::{OmhlError, RuleCode};
use crate::mechanical::is_mechanical;
use crate::native::NativeBase;
use crate::probe::{ProbeError, ProbeOutcome};
use hexlog::{Snapshot, SnapshotKind};

const ATTESTER: &str = "executor";

struct FoundCheck {
    id: SemanticId,
    mechanical: bool,
}

impl Guard<'_, '_> {
    pub(super) fn check_attestation(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let work_item = self.find_open_work_item(node)?;
        let check = self.find_check(node)?;
        check_derived_id(
            node,
            check.as_ref().map(|found| &found.id),
            work_item.as_ref(),
        )?;
        // `spec_digest` é o selo do `check run`: quem o declara também é o `executor`
        if check.is_some_and(|found| found.mechanical) || node.has_field("spec_digest") {
            self.check_mechanical_authorship(node)?;
        }
        match node.base {
            None => self.check_head(node),
            Some(_) => self.check_outcome_unchanged(node),
        }
    }

    pub(super) fn find_open_work_item(
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

    fn find_check(&mut self, node: &NodeItem<'_>) -> Result<Option<FoundCheck>, OmhlError> {
        let Some(raw) = node.field("check") else {
            return Ok(None);
        };
        let id = SemanticId::parse(raw)
            .map_err(|_| node.error(RuleCode::CheckNotFound, "body/data/check"))?;
        // check nativo nunca tem `run`
        let (type_name, mechanical) = match NativeBase.lookup(&id) {
            Some(native) => (Some(native.type_name), false),
            None => match self.hexlog.get(self.tenant, &id)? {
                Some(view) => (Some(view.type_name), is_mechanical(&view.data)),
                None => (None, false),
            },
        };

        match type_name {
            Some(found) if found.as_str() == "check" => Ok(Some(FoundCheck { id, mechanical })),
            _ => Err(node.error(RuleCode::CheckNotFound, "body/data/check")),
        }
    }

    fn check_mechanical_authorship(&self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        if self.author.client != ATTESTER {
            return Err(node.error(RuleCode::CheckRunAuthor, "body/data/check"));
        }

        match first_missing_field(node) {
            Some(field) => Err(node.error(
                RuleCode::AttestationIncomplete,
                &format!("body/data/{field}"),
            )),
            None => Ok(()),
        }
    }

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

/// Um `failed` sem `exit_code` é o timeout ou a morte por sinal.
fn first_missing_field(node: &NodeItem<'_>) -> Option<&'static str> {
    ["byproduct", "command", "spec_digest"]
        .into_iter()
        .find(|key| !node.has_field(key))
        .or_else(|| {
            let exited = matches!(
                node.field("outcome"),
                Some("passed" | "passed_with_warning")
            );
            (exited && !node.has_field("exit_code")).then_some("exit_code")
        })
}

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
