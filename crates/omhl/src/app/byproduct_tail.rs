use hexlog::{NodeView, Scalar, SemanticId};
use serde_json::{Value, json};

use super::engine::{Engine, parse_at};
use super::error::AppError;
use super::read::nodes_matching;
use crate::byproduct::{self, ByproductHash};
use crate::error::{OmhlError, RuleCode};
use crate::pr_gate::pair;

const TAIL_MAX: usize = 500;
const LINE_MAX_CHARS: usize = 2_000;

impl Engine<'_> {
    /// Somente leitura: as últimas linhas da saída guardada da attestation mais recente.
    pub(super) fn byproduct(
        &mut self,
        check: &str,
        work_item: &str,
        tail: u32,
    ) -> Result<Value, AppError> {
        if tail == 0 {
            return Err(AppError::invalid_arguments("/tail", "unknown value"));
        }
        let check_id = parse_at(SemanticId::parse, check, "/check")?;
        if self.omhl.get(&self.tenant, &check_id)?.is_none() {
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
        let store = self.byproducts.ok_or_else(|| {
            AppError::plain("byproducts-unavailable", "no byproduct store is configured")
        })?;
        let blob = store
            .get(&hash)
            .map_err(|_| AppError::byproduct_corrupt("/work_item"))?
            .ok_or_else(missing)?;
        let lines = tail_lines(&blob, tail).ok_or_else(missing)?;

        Ok(json!({
            "check": check,
            "work_item": work_item,
            "outcome": attestation.data["outcome"],
            "byproduct": hash.as_str(),
            "lines": lines,
        }))
    }

    /// A de maior `seq` entre as attestations do par; o id não ordena nada.
    fn latest_attestation(
        &mut self,
        check: &str,
        work_item: &str,
    ) -> Result<Option<NodeView>, AppError> {
        let where_eq = vec![
            pair("check", Scalar::Str(check.to_owned())),
            pair("work_item", Scalar::Str(work_item.to_owned())),
        ];

        Ok(
            nodes_matching(&mut self.omhl, &self.tenant, "attestation", where_eq)?
                .into_iter()
                .max_by_key(|node| node.seq),
        )
    }
}

fn tail_lines(blob: &[u8], tail: u32) -> Option<Vec<String>> {
    let (stdout, stderr) = byproduct::decode(blob)?;
    let lines: Vec<String> = [stdout, stderr]
        .into_iter()
        .flat_map(|stream| {
            String::from_utf8_lossy(stream)
                .split_terminator('\n')
                .map(|line| line.chars().take(LINE_MAX_CHARS).collect::<String>())
                .collect::<Vec<_>>()
        })
        .collect();
    let wanted = usize::try_from(tail).map_or(TAIL_MAX, |tail| tail.min(TAIL_MAX));

    Some(lines[lines.len().saturating_sub(wanted)..].to_vec())
}
