//! Porta do provedor de hospedagem: o método fala só por ela, nunca pelo programa do provedor.

use hexlog::SemanticId;

use crate::workflow::config::ProviderName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub host: String,
    /// No GitHub, exatamente dois segmentos: dono e repositório.
    pub path: Vec<String>,
}

impl RepoRef {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.host, self.path.join("/"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    CliMissing,
    NotAuthenticated,
    CliTooOld,
    Timeout,
    Failed { stderr: String },
    Malformed,
    HeadMoved { expected: String, seen: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub state: PrState,
    pub head_commit: String,
    pub base_branch: String,
    pub cross_repository: bool,
    pub auto_merge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Green,
    NotGreen,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckContext {
    pub name: String,
    pub state: CheckState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRead {
    pub head_commit: String,
    pub contexts: Vec<CheckContext>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Required {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedCheck {
    pub name: String,
    pub required: Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Merge,
    Squash,
    Rebase,
}

impl Strategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Strategy::Merge => "merge",
            Strategy::Squash => "squash",
            Strategy::Rebase => "rebase",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRead {
    pub state: PrState,
    /// Fila de merge ou merge automático ligado.
    pub pending: bool,
    pub base_branch: String,
    pub head_commit: String,
    pub merge_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeCall {
    pub success: bool,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkParent {
    pub is_fork: bool,
    pub parent: Option<String>,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::CliMissing => f.write_str("the provider CLI is not on the PATH"),
            ProviderError::NotAuthenticated => f.write_str("the provider CLI is not logged in"),
            ProviderError::CliTooOld => f.write_str("the provider CLI is too old"),
            ProviderError::Timeout => f.write_str("the provider CLI timed out"),
            ProviderError::Failed { stderr } => {
                write!(f, "the provider CLI failed: {}", stderr.trim())
            }
            ProviderError::Malformed => {
                f.write_str("the provider answered with an unexpected shape")
            }
            ProviderError::HeadMoved { expected, seen } => {
                write!(f, "the head commit moved from {expected} to {seen}")
            }
        }
    }
}

pub trait Provider {
    fn preflight(&self) -> Result<(), ProviderError>;
    fn default_branch(&self, repo: &RepoRef) -> Result<String, ProviderError>;
    fn pull_requests(
        &self,
        repo: &RepoRef,
        head_branch: &str,
    ) -> Result<Vec<PullRequest>, ProviderError>;
    fn checks(&self, repo: &RepoRef, number: u64) -> Result<CheckRead, ProviderError>;
    /// `HeadMoved` quando o commit lido não é o esperado.
    fn classify_red(
        &self,
        repo: &RepoRef,
        number: u64,
        expected_commit: &str,
    ) -> Result<Vec<RedCheck>, ProviderError>;
    /// Só chama: quem decide pelo resultado é a releitura do estado.
    fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        commit: &str,
        strategy: Strategy,
    ) -> Result<MergeCall, ProviderError>;
    fn state(&self, repo: &RepoRef, number: u64) -> Result<StateRead, ProviderError>;
    fn fork_parent(&self, repo: &RepoRef) -> Result<ForkParent, ProviderError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiVerdict {
    Green,
    None,
    Running,
    Red(Vec<String>),
}

pub fn ci_verdict(read: &CheckRead) -> CiVerdict {
    let red: Vec<String> = read
        .contexts
        .iter()
        .filter(|context| context.state == CheckState::NotGreen)
        .map(|context| context.name.clone())
        .collect();
    if !red.is_empty() {
        return CiVerdict::Red(red);
    }
    if read.contexts.is_empty() {
        return CiVerdict::None;
    }
    match read
        .contexts
        .iter()
        .any(|context| context.state == CheckState::Running)
    {
        true => CiVerdict::Running,
        false => CiVerdict::Green,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttestResult {
    Passed,
    PassedWithWarning,
}

impl AttestResult {
    pub fn as_str(self) -> &'static str {
        match self {
            AttestResult::Passed => "passed",
            AttestResult::PassedWithWarning => "passed_with_warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiDecision {
    Attest { result: AttestResult, note: String },
    Refuse(&'static str),
}

pub const CI_REQUIRED_MISSING: &str = "ci-required-missing";
pub const CI_RUNNING: &str = "ci-running";
pub const CI_NOT_GREEN: &str = "ci-not-green";
pub const NO_CI_NOTE: &str = "no CI seen at that SHA";

/// Um vermelho que a leitura não classifica conta como exigido: nenhum `--force` o passa.
pub fn ci_decision(
    verdict: &CiVerdict,
    red: &[RedCheck],
    require_ci: bool,
    force_note: Option<&str>,
) -> CiDecision {
    match verdict {
        CiVerdict::Green => CiDecision::Attest {
            result: AttestResult::Passed,
            note: String::new(),
        },
        CiVerdict::None if require_ci => CiDecision::Refuse(CI_REQUIRED_MISSING),
        CiVerdict::None => CiDecision::Attest {
            result: AttestResult::PassedWithWarning,
            note: NO_CI_NOTE.to_owned(),
        },
        CiVerdict::Running => CiDecision::Refuse(CI_RUNNING),
        CiVerdict::Red(names) => {
            let required = |name: &String| {
                red.iter()
                    .find(|check| &check.name == name)
                    .is_none_or(|check| check.required != Required::No)
            };
            match (names.iter().any(required), force_note) {
                (false, Some(note)) => CiDecision::Attest {
                    result: AttestResult::PassedWithWarning,
                    note: format!(
                        "red CI checks not required by branch protection: {}; owner authorization: {note}",
                        names.join(", ")
                    ),
                },
                _ => CiDecision::Refuse(CI_NOT_GREEN),
            }
        }
    }
}

/// Segundo segmento sob o id do work item, que nenhuma branch nomeia.
pub fn pull_request_id(work_item: &SemanticId, provider: ProviderName, number: u64) -> SemanticId {
    SemanticId::parse(&format!(
        "{}.{}-{number}",
        work_item.as_str(),
        provider.as_str()
    ))
    .expect("a work item id with a provider segment is a valid id")
}

/// Aceita `https://HOST/OWNER/REPO[.git]`, `ssh://git@HOST/OWNER/REPO[.git]` e `git@HOST:OWNER/REPO[.git]`.
pub fn repo_from_origin_url(url: &str, host: &str) -> Option<RepoRef> {
    let url = url.trim();
    let (url_host, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("ssh://"))
    {
        let (authority, path) = rest.split_once('/')?;
        let authority = authority.rsplit('@').next()?;
        (authority.split(':').next()?, path)
    } else {
        let (authority, path) = url.split_once(':')?;
        (authority.rsplit('@').next()?, path)
    };
    if url_host != host {
        return None;
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<String> = path.split('/').map(str::to_owned).collect();
    if segments.len() != 2 || segments.iter().any(String::is_empty) {
        return None;
    }
    Some(RepoRef {
        host: host.to_owned(),
        path: segments,
    })
}

#[cfg(test)]
mod tests;
