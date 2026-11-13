use std::path::PathBuf;

use hexlog::Snapshot;

pub use crate::mechanical::branch::HeadFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeSource {
    Index,
    Head,
}

pub trait SnapshotProbe: Send + Sync {
    /// Lê o git e responde sobre o estado atual. Não tem efeito colateral.
    fn probe(&self) -> Result<ProbeOutcome, ProbeError>;
    /// `Dirty` também com arquivo não rastreado e não ignorado.
    fn probe_tree(&self, source: TreeSource) -> Result<ProbeOutcome, ProbeError>;
    /// Working tree contra o índice, rastreados e não rastreados; ignorados não contam.
    fn tree_dirty(&self) -> Result<bool, ProbeError>;
    fn head_facts(&self) -> Result<HeadFacts, ProbeError>;
}

/// Topologia de branches e worktrees do repositório. Nenhuma operação escreve.
pub trait GitTopology: Send + Sync {
    /// Diretório de trabalho principal, o mesmo de qualquer worktree ligada.
    fn main_dir(&self) -> Result<PathBuf, ProbeError>;
    /// Nome inválido ou inexistente é `Ok(false)`.
    fn branch_exists(&self, name: &str) -> Result<bool, ProbeError>;
    /// Branches com worktree (a principal inclusive), em ordem crescente.
    fn worktree_branches(&self) -> Result<Vec<String>, ProbeError>;
    /// Branches locais, em ordem crescente.
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
