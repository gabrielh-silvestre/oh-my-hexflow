//! Porta do provedor de hospedagem: o método fala só por ela, nunca pelo programa do provedor.

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

pub trait Provider {
    fn default_branch(&self, repo: &RepoRef) -> Result<String, ProviderError>;
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
