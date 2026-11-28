//! Único lugar que inicia o `gh` e monta o argv dele.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::provider::{
    CheckContext, CheckRead, CheckState, ForkParent, MergeCall, PrState, Provider, ProviderError,
    PullRequest, RedCheck, RepoRef, Required, StateRead, Strategy,
};
use crate::runner::{CommandRunner, RunErrorKind, RunReport, RunSpec};
use crate::workflow::branch_policy::is_valid_branch_name;

pub const GH_PROGRAM: &str = "gh";

const MAX_STREAM_BYTES: u64 = 1024 * 1024;

/// Variáveis que redirecionam ou colorem o `gh`; o `GH_TOKEN` fica, porque é a credencial da pessoa.
const ISOLATED_ENV: [&str; 4] = ["GH_REPO", "GH_HOST", "GH_FORCE_TTY", "CLICOLOR_FORCE"];

const ENTERPRISE_TOKENS: [&str; 2] = ["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"];

const PULL_REQUEST_FIELDS: &str =
    "number,state,headRefOid,isCrossRepository,baseRefName,autoMergeRequest";
const MATCH_HEAD_FLAG: &str = "--match-head-commit";

/// O `isRequired` só existe por contexto, e o commit lido vem junto para conferir a ponta.
const REQUIRED_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!, $endCursor: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { commits(last: 1) { nodes { commit { oid statusCheckRollup { contexts(first: 100, after: $endCursor) { pageInfo { hasNextPage endCursor } nodes { __typename ... on CheckRun { name isRequired(pullRequestNumber: $number) } ... on StatusContext { context isRequired(pullRequestNumber: $number) } } } } } } } } } }";

pub struct GhProvider<'r> {
    runner: &'r dyn CommandRunner,
    host: String,
    timeout: Duration,
    extra_remove: Vec<String>,
}

impl<'r> GhProvider<'r> {
    pub fn new(runner: &'r dyn CommandRunner, host: &str, timeout: Duration) -> GhProvider<'r> {
        GhProvider {
            runner,
            host: host.to_owned(),
            timeout,
            extra_remove: Vec::new(),
        }
    }

    /// Leitor sem pedido de ninguém: o host vem do `workflow.toml` versionado e o token Enterprise vale para qualquer host.
    pub fn unattended(mut self) -> GhProvider<'r> {
        if self.host != "github.com" {
            self.extra_remove = ENTERPRISE_TOKENS.map(str::to_owned).to_vec();
        }
        self
    }

    fn slug(&self, repo: &RepoRef) -> String {
        format!("{}/{}", self.host, repo.path.join("/"))
    }

    fn run(&self, args: &[&str]) -> Result<RunReport, ProviderError> {
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
            env_remove: ISOLATED_ENV
                .map(str::to_owned)
                .into_iter()
                .chain(self.extra_remove.iter().cloned())
                .collect(),
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

        Ok(report)
    }

    fn run_json(&self, args: &[&str]) -> Result<Value, ProviderError> {
        let report = self.run(args)?;
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

/// Conclusão de um check run concluído, senão o status; o desconhecido é não verde.
pub(crate) fn map_check_run(conclusion_or_status: &str) -> CheckState {
    match conclusion_or_status {
        "SUCCESS" | "NEUTRAL" | "SKIPPED" => CheckState::Green,
        "QUEUED" | "IN_PROGRESS" | "PENDING" | "EXPECTED" | "WAITING" | "REQUESTED" => {
            CheckState::Running
        }
        _ => CheckState::NotGreen,
    }
}

pub(crate) fn map_status_context(state: &str) -> CheckState {
    match state {
        "SUCCESS" => CheckState::Green,
        "PENDING" | "EXPECTED" => CheckState::Running,
        _ => CheckState::NotGreen,
    }
}

fn text<'v>(value: &'v Value, key: &str) -> Result<&'v str, ProviderError> {
    value[key].as_str().ok_or(ProviderError::Malformed)
}

fn pr_state(value: &Value) -> Result<PrState, ProviderError> {
    match text(value, "state")? {
        "OPEN" => Ok(PrState::Open),
        "MERGED" => Ok(PrState::Merged),
        "CLOSED" => Ok(PrState::Closed),
        _ => Err(ProviderError::Malformed),
    }
}

fn pull_request(value: &Value) -> Result<PullRequest, ProviderError> {
    Ok(PullRequest {
        number: value["number"].as_u64().ok_or(ProviderError::Malformed)?,
        state: pr_state(value)?,
        head_commit: text(value, "headRefOid")?.to_owned(),
        base_branch: text(value, "baseRefName")?.to_owned(),
        cross_repository: value["isCrossRepository"].as_bool().unwrap_or(false),
        auto_merge: !value["autoMergeRequest"].is_null(),
    })
}

fn check_context(value: &Value) -> Result<CheckContext, ProviderError> {
    match text(value, "__typename")? {
        "CheckRun" => {
            let reading = match value["status"].as_str() {
                Some("COMPLETED") => value["conclusion"].as_str(),
                status => status,
            };
            Ok(CheckContext {
                name: text(value, "name")?.to_owned(),
                state: map_check_run(reading.unwrap_or_default()),
            })
        }
        "StatusContext" => Ok(CheckContext {
            name: text(value, "context")?.to_owned(),
            state: map_status_context(value["state"].as_str().unwrap_or_default()),
        }),
        _ => Err(ProviderError::Malformed),
    }
}

fn red_check(node: &Value) -> Result<RedCheck, ProviderError> {
    let name = node["name"]
        .as_str()
        .or_else(|| node["context"].as_str())
        .ok_or(ProviderError::Malformed)?;
    let required = match node["isRequired"].as_bool() {
        Some(true) => Required::Yes,
        Some(false) => Required::No,
        None => Required::Unknown,
    };

    Ok(RedCheck {
        name: name.to_owned(),
        required,
    })
}

impl Provider for GhProvider<'_> {
    fn preflight(&self) -> Result<(), ProviderError> {
        let auth = self.run(&["auth", "status", "--hostname", &self.host])?;
        if auth.exit_code != Some(0) {
            return Err(ProviderError::NotAuthenticated);
        }
        let help = self.run(&["pr", "merge", "--help"])?;
        let lists_flag = [&help.stdout, &help.stderr]
            .iter()
            .any(|stream| String::from_utf8_lossy(stream).contains(MATCH_HEAD_FLAG));
        match lists_flag {
            true => Ok(()),
            false => Err(ProviderError::CliTooOld),
        }
    }

    fn default_branch(&self, repo: &RepoRef) -> Result<String, ProviderError> {
        let slug = self.slug(repo);
        let value = self.run_json(&["repo", "view", &slug, "--json", "defaultBranchRef"])?;
        value
            .pointer("/defaultBranchRef/name")
            .and_then(Value::as_str)
            .filter(|name| is_valid_branch_name(name))
            .map(str::to_owned)
            .ok_or(ProviderError::Malformed)
    }

    fn pull_requests(
        &self,
        repo: &RepoRef,
        head_branch: &str,
    ) -> Result<Vec<PullRequest>, ProviderError> {
        let slug = self.slug(repo);
        let value = self.run_json(&[
            "pr",
            "list",
            "--repo",
            &slug,
            "--head",
            head_branch,
            "--state",
            "all",
            "--json",
            PULL_REQUEST_FIELDS,
        ])?;
        value
            .as_array()
            .ok_or(ProviderError::Malformed)?
            .iter()
            .map(pull_request)
            .collect()
    }

    fn checks(&self, repo: &RepoRef, number: u64) -> Result<CheckRead, ProviderError> {
        let slug = self.slug(repo);
        let number = number.to_string();
        let value = self.run_json(&[
            "pr",
            "view",
            &number,
            "--repo",
            &slug,
            "--json",
            "headRefOid,statusCheckRollup",
        ])?;
        let contexts = match &value["statusCheckRollup"] {
            Value::Null => Vec::new(),
            Value::Array(items) => items.iter().map(check_context).collect::<Result<_, _>>()?,
            _ => return Err(ProviderError::Malformed),
        };

        Ok(CheckRead {
            head_commit: text(&value, "headRefOid")?.to_owned(),
            contexts,
        })
    }

    fn classify_red(
        &self,
        repo: &RepoRef,
        number: u64,
        expected_commit: &str,
    ) -> Result<Vec<RedCheck>, ProviderError> {
        let [owner, name] = repo.path.as_slice() else {
            return Err(ProviderError::Malformed);
        };
        let owner = format!("owner={owner}");
        let name = format!("name={name}");
        let number = format!("number={number}");
        let query = format!("query={REQUIRED_QUERY}");
        let pages = self.run_json(&[
            "api",
            "graphql",
            "--paginate",
            "--slurp",
            "--hostname",
            &self.host,
            "-f",
            &owner,
            "-f",
            &name,
            "-F",
            &number,
            "-f",
            &query,
        ])?;
        let mut red = Vec::new();
        for page in pages.as_array().ok_or(ProviderError::Malformed)? {
            let commit = page
                .pointer("/data/repository/pullRequest/commits/nodes/0/commit")
                .ok_or(ProviderError::Malformed)?;
            let seen = text(commit, "oid")?;
            if seen != expected_commit {
                return Err(ProviderError::HeadMoved {
                    expected: expected_commit.to_owned(),
                    seen: seen.to_owned(),
                });
            }
            let nodes = commit
                .pointer("/statusCheckRollup/contexts/nodes")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            for node in nodes {
                red.push(red_check(node)?);
            }
        }

        Ok(red)
    }

    fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        commit: &str,
        strategy: Strategy,
    ) -> Result<MergeCall, ProviderError> {
        let slug = self.slug(repo);
        let number = number.to_string();
        let flag = format!("--{}", strategy.as_str());
        let report = self.run(&[
            "pr",
            "merge",
            &number,
            "--repo",
            &slug,
            MATCH_HEAD_FLAG,
            commit,
            &flag,
        ])?;

        Ok(MergeCall {
            success: report.exit_code == Some(0),
            stderr: String::from_utf8_lossy(&report.stderr).into_owned(),
        })
    }

    fn state(&self, repo: &RepoRef, number: u64) -> Result<StateRead, ProviderError> {
        let slug = self.slug(repo);
        let number = number.to_string();
        let value = self.run_json(&[
            "pr",
            "view",
            &number,
            "--repo",
            &slug,
            "--json",
            "state,autoMergeRequest,baseRefName,headRefOid,mergeCommit",
        ])?;

        Ok(StateRead {
            state: pr_state(&value)?,
            pending: !value["autoMergeRequest"].is_null(),
            base_branch: text(&value, "baseRefName")?.to_owned(),
            head_commit: text(&value, "headRefOid")?.to_owned(),
            merge_commit: value
                .pointer("/mergeCommit/oid")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    fn fork_parent(&self, repo: &RepoRef) -> Result<ForkParent, ProviderError> {
        let slug = self.slug(repo);
        let value = self.run_json(&["repo", "view", &slug, "--json", "isFork,parent"])?;
        let parent = match (
            value.pointer("/parent/owner/login").and_then(Value::as_str),
            value.pointer("/parent/name").and_then(Value::as_str),
        ) {
            (Some(login), Some(name)) => Some(format!("{login}/{name}")),
            _ => None,
        };

        Ok(ForkParent {
            is_fork: value["isFork"].as_bool().unwrap_or(false),
            parent,
        })
    }
}

#[cfg(test)]
mod tests;
