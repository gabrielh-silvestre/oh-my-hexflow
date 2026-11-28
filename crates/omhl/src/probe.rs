use hexlog::Snapshot;

pub use crate::mechanical::branch::HeadFacts;
use crate::project::ProjectError;
use crate::workflow::branch_policy::BranchConfig;
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

/// Porta própria da marca de commit: a sonda de leitura não escreve.
pub trait CommitMark: Send + Sync {
    fn present(&self) -> Result<bool, ProbeError>;
    /// Caminho absoluto do arquivo da marca, para a dica de quem a cria à mão.
    fn location(&self) -> Result<std::path::PathBuf, ProbeError>;
    /// Marca ausente também é `Ok`.
    fn consume(&self) -> Result<(), ProbeError>;
}

/// Resolve quais checks nativos o projeto ligou; recusa em vez de assumir um padrão.
pub trait NativeResolver: Send + Sync {
    fn resolve(&self) -> Result<NativeConfig, NativeRefusal>;
    /// Política do blob `HEAD` do diretório principal, para os leitores que não recusam um principal ilegível.
    fn stable_branch(&self) -> BranchConfig {
        BranchConfig::default()
    }
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

/// O resultado cru de uma chamada do `git`, para quem decide pela recusa dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Um `git` rodado num diretório dado; o argv chega sem `-C`.
pub trait TidyGit {
    fn git(&self, dir: &std::path::Path, args: &[&str]) -> Result<GitOutput, ProbeError>;
}
