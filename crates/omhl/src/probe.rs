use hexlog::Snapshot;

pub trait SnapshotProbe: Send + Sync {
    /// Lê o git e responde sobre o estado atual. Não tem efeito colateral.
    fn probe(&self) -> Result<ProbeOutcome, ProbeError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Reproducible(Snapshot),
    Dirty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeError {
    pub message: String,
}
