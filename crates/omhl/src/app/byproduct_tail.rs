use std::collections::BTreeMap;

use hexlog::{NodeView, Scalar, SemanticId};
use serde_json::{Value, json};

use super::engine::{Engine, parse_at};
use super::error::AppError;
use super::read::nodes_matching;
use crate::byproduct::{self, ByproductHash};
use crate::delivery_gate::pair;
use crate::error::{OmhlError, RuleCode};
use crate::mechanical::redact::Redactor;
use crate::native::NativeBase;

const TAIL_MAX: usize = 500;
const LINE_MAX_CHARS: usize = 2_000;

impl Engine<'_> {
    pub(super) fn byproduct(
        &mut self,
        check: &str,
        work_item: &str,
        tail: u32,
    ) -> Result<Value, AppError> {
        let wanted = usize::try_from(tail)
            .ok()
            .filter(|wanted| (1..=TAIL_MAX).contains(wanted))
            .ok_or_else(|| {
                AppError::invalid_arguments(
                    "/tail",
                    &format!("tail must be between 1 and {TAIL_MAX}"),
                )
            })?;
        let check_id = parse_at(SemanticId::parse, check, "/check")?;
        if !is_native(check) && self.omhl.get(&self.tenant, &check_id)?.is_none() {
            return Err(OmhlError::rule(RuleCode::CheckNotFound, "/check").into());
        }
        let attestation = self
            .latest_attestation(check, work_item)?
            .ok_or_else(|| AppError::attestation_not_found("/work_item"))?;
        let missing = || AppError::byproduct_missing("/work_item");
        let hash = attestation.data["byproduct"]
            .as_str()
            .and_then(|text| ByproductHash::parse(text).ok())
            .ok_or_else(missing)?;
        let store = self
            .exec
            .ok_or_else(AppError::byproducts_unavailable)?
            .byproducts;
        let blob = store
            .get(&hash)
            .map_err(|_| AppError::byproduct_corrupt("/work_item"))?
            .ok_or_else(missing)?;
        let (lines, truncated_lines) =
            tail_lines(&blob, wanted, &Redactor::new()).ok_or_else(missing)?;
        let mut result = json!({
            "check": check,
            "work_item": work_item,
            "outcome": attestation.data["outcome"],
            "byproduct": hash.as_str(),
            "lines": lines,
        });
        if truncated_lines > 0 {
            result["truncated_lines"] = json!(truncated_lines);
        }

        Ok(result)
    }

    fn latest_attestation(
        &mut self,
        check: &str,
        work_item: &str,
    ) -> Result<Option<NodeView>, AppError> {
        let where_eq = vec![
            pair("check", Scalar::Str(check.to_owned())),
            pair("work_item", Scalar::Str(work_item.to_owned())),
        ];

        let nodes = nodes_matching(&mut self.omhl, &self.tenant, "attestation", where_eq)?;

        if is_native(check) {
            return Ok(select_native(nodes));
        }

        Ok(latest_per_pair(nodes).into_iter().next())
    }
}

fn is_native(check: &str) -> bool {
    NativeBase.checks().iter().any(|native| native.id == check)
}

fn select_native(nodes: Vec<NodeView>) -> Option<NodeView> {
    let mut newest = newest_snapshot(nodes);
    newest.sort_by_key(|node| node.seq);
    let failed = newest
        .iter()
        .rposition(|node| node.data["outcome"].as_str() != Some("passed"));

    match failed {
        Some(index) => Some(newest.swap_remove(index)),
        None => newest.pop(),
    }
}

pub(super) fn selectable_per_pair(nodes: Vec<NodeView>) -> Vec<NodeView> {
    let mut pairs: BTreeMap<(String, Option<String>), Vec<NodeView>> = BTreeMap::new();
    for node in nodes {
        let check = node.data["check"].as_str().unwrap_or_default().to_owned();
        let work_item = node.data["work_item"].as_str().map(str::to_owned);
        pairs.entry((check, work_item)).or_default().push(node);
    }

    pairs.into_values().flat_map(newest_snapshot).collect()
}

fn newest_snapshot(nodes: Vec<NodeView>) -> Vec<NodeView> {
    let Some(snapshot) = nodes
        .iter()
        .max_by_key(|node| node.seq)
        .map(|node| node.snapshot.clone())
    else {
        return Vec::new();
    };

    nodes
        .into_iter()
        .filter(|node| node.snapshot == snapshot)
        .collect()
}

/// A attestation de maior `seq` de cada par (check, work item); sem work item forma o par (check, sem work item).
pub(super) fn latest_per_pair(mut nodes: Vec<NodeView>) -> Vec<NodeView> {
    nodes.sort_by_key(|node| node.seq);
    let mut latest: BTreeMap<(String, Option<String>), NodeView> = BTreeMap::new();
    for node in nodes {
        let check = node.data["check"].as_str().unwrap_or_default().to_owned();
        let work_item = node.data["work_item"].as_str().map(str::to_owned);
        latest.insert((check, work_item), node);
    }

    latest.into_values().collect()
}

/// Redige cada fluxo inteiro antes de cortar linhas: cobre o blob gravado sem redação e o bloco PEM de várias linhas.
fn tail_lines(blob: &[u8], wanted: usize, redactor: &Redactor) -> Option<(Vec<String>, usize)> {
    let (stdout, stderr) = byproduct::decode(blob)?;
    let lines: Vec<String> = [stdout, stderr]
        .into_iter()
        .flat_map(|stream| {
            String::from_utf8_lossy(&redactor.redact(stream))
                .split_terminator('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    let mut truncated = 0;
    let window = lines[lines.len().saturating_sub(wanted)..]
        .iter()
        .map(|line| match line.char_indices().nth(LINE_MAX_CHARS) {
            Some((cut, _)) => {
                truncated += 1;
                line[..cut].to_owned()
            }
            None => line.clone(),
        })
        .collect();

    Some((window, truncated))
}

#[cfg(test)]
mod tests;
