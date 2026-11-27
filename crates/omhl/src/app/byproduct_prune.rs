use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::{Value, json};

use super::byproduct_tail::selectable_per_pair;
use super::engine::Engine;
use super::error::AppError;
use super::read::nodes_matching;
use crate::byproduct::ByproductRetention;

const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MIN_PRUNE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

impl Engine<'_> {
    /// O `check run` poda sem canal de erro (como `sweep_quietly`): o `omhl byproduct prune` é quem mostra a falha.
    pub(super) fn prune_quietly(&mut self) {
        if let Some(retention) = self.retention {
            let _ = self.prune(retention, false);
        }
    }

    pub(super) fn byproduct_prune(&mut self) -> Result<Value, AppError> {
        let retention = self.retention.ok_or_else(AppError::retention_unavailable)?;

        self.prune(retention, true)
    }

    /// Sem `force`, no máximo uma poda a cada 24 horas; com `force`, renova o carimbo sem olhá-lo.
    fn prune(
        &mut self,
        retention: &dyn ByproductRetention,
        force: bool,
    ) -> Result<Value, AppError> {
        let interval = if force {
            Duration::ZERO
        } else {
            MIN_PRUNE_INTERVAL
        };
        // Com a pasta do (banco, tenant) ainda ausente a reivindicação falha, e `stale` a acha vazia.
        if !retention.claim_prune_slot(interval) && !force {
            return Ok(summary(0, 0, 0));
        }
        let failed = |_| AppError::byproduct_prune_failed();
        let stale = retention.stale(RETENTION).map_err(failed)?;
        if stale.is_empty() {
            return Ok(summary(0, 0, 0));
        }
        let live = self.live_hashes()?;
        let (mut removed, mut kept_live, mut freed_bytes) = (0, 0, 0);
        let mut failure = None;
        for blob in stale {
            if live.contains(blob.hash.as_str()) {
                kept_live += 1;
                continue;
            }
            match retention.remove(&blob, RETENTION) {
                Ok(true) => {
                    removed += 1;
                    freed_bytes += blob.bytes;
                }
                Ok(false) => {}
                // Um arquivo que não sai não impede os outros de saírem.
                Err(error) => failure = Some(error),
            }
        }
        if let Some(error) = failure {
            return Err(failed(error));
        }

        Ok(summary(removed, kept_live, freed_bytes))
    }

    /// Hashes das attestations vigentes que o leitor de byproduct pode escolher em cada par (check, work item) do tenant.
    fn live_hashes(&mut self) -> Result<BTreeSet<String>, AppError> {
        let attestations = nodes_matching(&mut self.omhl, &self.tenant, "attestation", Vec::new())
            .map_err(|_| AppError::byproduct_prune_failed())?;

        Ok(selectable_per_pair(attestations)
            .iter()
            .filter_map(|node| node.data["byproduct"].as_str().map(str::to_owned))
            .collect())
    }
}

fn summary(removed: u64, kept_live: u64, freed_bytes: u64) -> Value {
    json!({ "removed": removed, "kept_live": kept_live, "freed_bytes": freed_bytes })
}
