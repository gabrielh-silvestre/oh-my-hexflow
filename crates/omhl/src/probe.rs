use hexlog::Snapshot;

pub use crate::mechanical::branch::HeadFacts;
use crate::project::ProjectError;
use crate::workflow::config::NativeConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeSource {
    Index,
    Head,
}

pub trait SnapshotProbe: Send + Sync {
    /// Não tem efeito colateral.
    fn probe(&self) -> Result<ProbeOutcome, ProbeError>;
    fn probe_tree(&self, source: TreeSource) -> Result<ProbeOutcome, ProbeError>;
    fn tree_dirty(&self) -> Result<bool, ProbeError>;
    fn head_facts(&self) -> Result<HeadFacts, ProbeError>;
}

/// Porta própria da marca de `develop`: a sonda de leitura não escreve.
pub trait DevelopMark: Send + Sync {
    fn present(&self) -> Result<bool, ProbeError>;
    /// Marca ausente também é `Ok`.
    fn consume(&self) -> Result<(), ProbeError>;
}

/// Lê a ativação dos checks nativos do `.omhl/workflow.toml` da árvore de trabalho do diretório principal, sem fallback.
pub trait NativeResolver: Send + Sync {
    fn resolve(&self) -> Result<NativeConfig, NativeRefusal>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeRefusal {
    MainDirUnknown,
    InvalidWorkflow(ProjectError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Reproducible(Snapshot),
    Dirty,
}

/// O que o `git --version` deixa ler; só o `NotFound` do spawn é falta de `git`, o resto que falha é `Unreadable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitVersion {
    NotFound,
    Reported(String),
    Unreadable,
}

/// A mensagem do estouro de prazo do git: quem decide por ela compara com esta constante.
pub const GIT_TIMED_OUT: &str = "git timed out";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeError {
    pub message: String,
}
