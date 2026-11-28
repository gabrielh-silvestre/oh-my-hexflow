//! Único lugar que inicia o `gh` e monta o argv dele.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::provider::{Provider, ProviderError, RepoRef};
use crate::runner::{CommandRunner, RunErrorKind, RunSpec};

pub const GH_PROGRAM: &str = "gh";

const MAX_STREAM_BYTES: u64 = 1024 * 1024;

/// Variáveis que redirecionam ou colorem o `gh`; o `GH_TOKEN` fica, porque é a credencial da pessoa.
const ISOLATED_ENV: [&str; 4] = ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"];

pub struct GhProvider<'r> {
    runner: &'r dyn CommandRunner,
    host: String,
    timeout: Duration,
}

impl<'r> GhProvider<'r> {
    pub fn new(runner: &'r dyn CommandRunner, host: &str, timeout: Duration) -> GhProvider<'r> {
        GhProvider {
            runner,
            host: host.to_owned(),
            timeout,
        }
    }

    fn run_json(&self, args: &[&str]) -> Result<Value, ProviderError> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let spec = RunSpec {
            argv: std::iter::once(GH_PROGRAM)
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
            project_root: cwd.clone(),
            cwd,
            timeout: self.timeout,
            env_set: vec![("GH_PROMPT_DISABLED".to_owned(), "1".to_owned())],
            env_remove: ISOLATED_ENV.map(str::to_owned).to_vec(),
            max_stream_bytes: MAX_STREAM_BYTES,
        };
        let report = self.runner.run(&spec).map_err(|error| match error.kind {
            RunErrorKind::NotFound => ProviderError::CliMissing,
            _ => ProviderError::Failed {
                stderr: error.message,
            },
        })?;
        if report.timed_out {
            return Err(ProviderError::Timeout);
        }
        // O JSON do stdout decide; o código de saída só explica uma resposta que não veio.
        if let Ok(value) = serde_json::from_slice::<Value>(&report.stdout) {
            return Ok(value);
        }
        let stderr = String::from_utf8_lossy(&report.stderr).into_owned();
        Err(classify_failure(stderr, report.stdout.is_empty()))
    }
}

fn classify_failure(stderr: String, no_stdout: bool) -> ProviderError {
    if stderr.contains("gh auth login") || stderr.contains("not logged in") {
        return ProviderError::NotAuthenticated;
    }
    if stderr.contains("unknown flag") || stderr.contains("Unknown JSON field") {
        return ProviderError::CliTooOld;
    }
    if no_stdout {
        return ProviderError::Failed { stderr };
    }
    ProviderError::Malformed
}

impl Provider for GhProvider<'_> {
    fn default_branch(&self, repo: &RepoRef) -> Result<String, ProviderError> {
        let slug = format!("{}/{}", self.host, repo.path.join("/"));
        let value = self.run_json(&["repo", "view", &slug, "--json", "defaultBranchRef"])?;
        value
            .pointer("/defaultBranchRef/name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .ok_or(ProviderError::Malformed)
    }
}

#[cfg(test)]
mod tests;
