use std::path::PathBuf;

use hexlog::Snapshot;

pub use crate::mechanical::branch::HeadFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeSource {
    Index,
    Head,
}

pub trait SnapshotProbe: Send + Sync {
    ///  Não tem efeito colateral.
    fn probe(&self) -> Result<ProbeOutcome, ProbeError>;
    fn probe_tree(&self, source: TreeSource) -> Result<ProbeOutcome, ProbeError>;
    fn tree_dirty(&self) -> Result<bool, ProbeError>;
    fn head_facts(&self) -> Result<HeadFacts, ProbeError>;
}

///  Nenhuma operação escreve.
pub trait GitTopology: Send + Sync {
    fn main_dir(&self) -> Result<PathBuf, ProbeError>;
    /// Nome inválido ou inexistente é `Ok(false)`.
    fn branch_exists(&self, name: &str) -> Result<bool, ProbeError>;
    fn worktree_branches(&self) -> Result<Vec<String>, ProbeError>;
    fn local_branches(&self) -> Result<Vec<String>, ProbeError>;
}

/// Porta própria da marca de `develop`: a sonda de leitura não escreve.
pub trait DevelopMark: Send + Sync {
    fn present(&self) -> Result<bool, ProbeError>;
    /// Marca ausente também é `Ok`.
    fn consume(&self) -> Result<(), ProbeError>;
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
