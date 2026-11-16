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
        let store = self
            .byproducts
            .ok_or_else(AppError::byproducts_unavailable)?;
        let blob = store
            .get(&hash)
            .map_err(|_| AppError::byproduct_corrupt("/work_item"))?
            .ok_or_else(missing)?;
        let (lines, truncated_lines) = tail_lines(&blob, wanted).ok_or_else(missing)?;
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

/// As últimas `wanted` linhas e quantas delas passaram de `LINE_MAX_CHARS` e foram cortadas.
fn tail_lines(blob: &[u8], wanted: usize) -> Option<(Vec<String>, usize)> {
    let (stdout, stderr) = byproduct::decode(blob)?;
    let lines: Vec<String> = [stdout, stderr]
        .into_iter()
        .flat_map(|stream| {
            String::from_utf8_lossy(stream)
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
