//! `omhl merge`: do PR achado pela branch até o fechamento com o nó do PR, sempre pelo `gh` da porta.

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::action::{Caller, CheckTarget};
use super::engine::{Engine, require_project_root};
use super::error::AppError;
use super::tidy::{StageResult, tidy};
use crate::delivery_gate::{Forcing, PullRequestRecord, close_verdict};
use crate::facade::Closing;
use crate::mechanical::branch::{BranchKind, classify, integration};
use crate::native::CI_GREEN_ID;
use crate::ports::git::GitProbe;
use crate::ports::provider::build;
use crate::ports::workflow::read_workflow;
use crate::probe::ProbeError;
use crate::provider::{
    AttestResult, CiDecision, CiVerdict, PrState, Provider, ProviderError, PullRequest, RepoRef,
    Strategy, ci_decision, ci_verdict, pull_request_id, repo_from_origin_url,
};

const BEFORE_MERGE: &str = "before-merge";
const DEFAULT_HOST: &str = "github.com";
const STAYS_ON_GITHUB: &str = "the merge stays on GitHub: omhl merge merges nothing here";
const REPEAT: &str = "run omhl merge again";

/// Os passos feitos até aqui; a recusa os leva com o passo que recusou no fim.
struct Progress {
    done: Vec<Value>,
    current: &'static str,
    resume: bool,
    /// Fatos do passo que recusou, como os checks vermelhos do `ci`.
    refusing: Value,
}

impl Progress {
    fn finish(&mut self, step: Value) {
        self.done.push(step);
    }

    fn refusal(&self, error: AppError) -> AppError {
        let next = match (self.resume, self.current) {
            (true, "gate") => format!(
                "the pull request is already merged: record the missing attestation and {REPEAT}, or close the work item with omhl close, as the owner authorizes"
            ),
            _ => error
                .next_step()
                .map_or_else(|| format!("fix the cause and {REPEAT}"), str::to_owned),
        };
        let mut steps = self.done.clone();
        let mut step = json!({ "step": self.current });
        if let (Some(step), Some(facts)) = (step.as_object_mut(), self.refusing.as_object()) {
            step.extend(facts.clone());
        }
        steps.push(step);

        error.with_error_context(json!({ "steps": steps, "next": next }))
    }
}

fn refusal(code: &str, message: impl Into<String>, next_step: impl Into<String>) -> AppError {
    AppError::merge_refusal(code, message.into(), next_step.into())
}

fn probe_failed(error: &ProbeError) -> AppError {
    AppError::probe_failed(&error.message)
}

fn provider_failure(error: ProviderError, host: &str) -> AppError {
    match error {
        ProviderError::CliMissing => refusal(
            "provider-cli-missing",
            "the GitHub CLI gh is not on the PATH",
            format!("install the GitHub CLI (gh) and {REPEAT}"),
        ),
        ProviderError::NotAuthenticated => refusal(
            "provider-not-authenticated",
            format!("gh is not logged in to {host}"),
            format!("run gh auth login --hostname {host}, then {REPEAT}"),
        ),
        ProviderError::CliTooOld => refusal(
            "provider-cli-too-old",
            "gh pr merge does not list --match-head-commit, so the merge cannot be pinned to the evaluated commit",
            format!("update the GitHub CLI (gh) and {REPEAT}"),
        ),
        ProviderError::HeadMoved { .. } => head_not_pushed(),
        ProviderError::Timeout => AppError::probe_failed("gh did not answer within the time limit"),
        ProviderError::Failed { stderr } => {
            AppError::probe_failed(&format!("gh failed: {}", stderr.trim()))
        }
        ProviderError::Malformed => AppError::probe_failed("gh answered with an unexpected shape"),
    }
}

fn head_not_pushed() -> AppError {
    refusal(
        "head-not-pushed",
        "the local HEAD is not the head commit of the pull request, so an unpushed or rewritten commit would be merged without evaluation",
        format!("push the branch (or check out the pushed tip) and {REPEAT}"),
    )
}

fn base_mismatch(state: PrState, pr_base: &str, expected: &str) -> AppError {
    let next = match state {
        PrState::Merged => format!(
            "stop and ask the owner: the pull request is already merged into {pr_base}, and the work item stays open"
        ),
        _ => format!("change the base of the pull request to {expected} on GitHub and {REPEAT}"),
    };

    refusal(
        "pull-request-base-mismatch",
        format!("the pull request targets {pr_base}, but the integration branch is {expected}"),
        next,
    )
    .with_error_context(json!({ "pr_base": pr_base, "expected_base": expected }))
}

fn numbers(pull_requests: &[&PullRequest]) -> String {
    pull_requests
        .iter()
        .map(|pr| format!("#{}", pr.number))
        .collect::<Vec<_>>()
        .join(", ")
}

fn not_found() -> AppError {
    refusal(
        "pull-request-not-found",
        "GitHub has no open pull request from this branch, and no merged one at HEAD",
        format!("open the pull request of this branch on GitHub and {REPEAT}"),
    )
}

fn fork_unsupported(what: &str, context: Value) -> AppError {
    refusal(
        "fork-unsupported",
        format!("{what}; forks are not supported, and {STAYS_ON_GITHUB}"),
        "merge the pull request on GitHub and close the work item with omhl close",
    )
    .with_error_context(context)
}

fn ci_refusal(code: &'static str, verdict: &CiVerdict) -> AppError {
    let (message, next) = match code {
        "ci-running" => (
            "the CI of the evaluated commit is still running".to_owned(),
            format!("wait for the CI to finish and {REPEAT}"),
        ),
        "ci-required-missing" => (
            "provider.require_ci is on and no CI was seen at the evaluated commit".to_owned(),
            format!("wait for the CI to register on the pull request and {REPEAT}"),
        ),
        _ => (
            match verdict {
                CiVerdict::Red(names) => format!(
                    "CI checks of the evaluated commit are not green: {}",
                    names.join(", ")
                ),
                _ => "a CI check of the evaluated commit is not green".to_owned(),
            },
            format!(
                "fix the red check and push; only a check the branch protection does not require passes with --force --note and the owner's authorization, then {REPEAT}"
            ),
        ),
    };

    refusal(code, message, next)
}

fn merge_refused(reason: &str) -> AppError {
    refusal(
        "merge-refused",
        format!("GitHub did not merge the pull request: {}", reason.trim()),
        format!("read the gh message, fix its cause and {REPEAT}"),
    )
}

/// `owner/repo`, com o host na frente só fora do `github.com`.
fn recorded_repo(repo: &RepoRef) -> String {
    match repo.host == DEFAULT_HOST {
        true => repo.path.join("/"),
        false => repo.slug(),
    }
}

impl Engine<'_> {
    pub(super) fn merge(
        &mut self,
        caller: &Caller,
        work_item: &SemanticId,
        strategy: Strategy,
        force_note: Option<&str>,
        base: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut progress = Progress {
            done: Vec::new(),
            current: "lookup",
            resume: false,
            refusing: Value::Null,
        };
        self.merge_steps(caller, work_item, strategy, force_note, base, &mut progress)
            .map_err(|error| progress.refusal(error))
    }

    fn merge_steps(
        &mut self,
        caller: &Caller,
        work_item: &SemanticId,
        strategy: Strategy,
        force_note: Option<&str>,
        base: Option<&str>,
        progress: &mut Progress,
    ) -> Result<Value, AppError> {
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let git = GitProbe::new(&root);
        let main_dir = git.main_dir().map_err(|error| probe_failed(&error))?;
        let config = read_workflow(&main_dir).map_err(AppError::invalid_workflow)?;
        let Some(provider_config) = config.provider.clone() else {
            return Err(refusal(
                "integration-off",
                "the main-directory workflow.toml declares no [provider], so omhl merge has no provider to merge through",
                "merge the pull request on the provider, then close the work item with omhl close",
            ));
        };
        let host = provider_config.host.clone();
        self.omhl.ensure_open(&self.tenant, work_item)?;
        let branch = git
            .current_branch()
            .map_err(|error| probe_failed(&error))?
            .ok_or_else(|| {
                refusal(
                    "head-detached",
                    "HEAD is detached, so no branch names the work item",
                    format!("check out the branch of the work item and {REPEAT}"),
                )
            })?;
        if !matches!(classify(&config.branch, &branch), BranchKind::WorkItem { id, .. } if id == *work_item)
        {
            return Err(refusal(
                "branch-not-work-item",
                format!("the branch {branch} is not a branch of this work item"),
                format!("check out the branch of the work item and {REPEAT}"),
            ));
        }
        let snapshots = self.probe_gate_snapshots()?;
        let head = snapshots.head_commit.id().to_owned();
        let url = git
            .origin_url()
            .map_err(|error| probe_failed(&error))?
            .ok_or_else(|| {
                refusal(
                    "origin-missing",
                    "the repository has no origin remote, so the pull request cannot be found",
                    format!("add the origin remote of the repository and {REPEAT}"),
                )
            })?;
        let repo = repo_from_origin_url(&url, &host).ok_or_else(|| {
            refusal(
                "origin-host-mismatch",
                format!(
                    "the origin remote is not a repository on {host}, the declared provider host"
                ),
                format!(
                    "point origin to the repository on {host}, or fix provider.host, and {REPEAT}"
                ),
            )
        })?;
        let built;
        let provider: &dyn Provider = match self.provider {
            Some(provider) => provider,
            None => {
                let (runner, timeout) = self
                    .provider_runner
                    .ok_or_else(AppError::runner_unavailable)?;
                built = build(&provider_config, runner, timeout);
                built.as_ref()
            }
        };
        let failure = |error: ProviderError| provider_failure(error, &host);
        provider.preflight().map_err(failure)?;

        let listed = provider.pull_requests(&repo, &branch).map_err(failure)?;
        let pr = self.pick_pull_request(provider, &repo, &listed, &head, &host)?;
        progress.finish(json!({ "step": "lookup", "result": "done" }));

        progress.current = "head";
        if pr.head_commit != head {
            return Err(head_not_pushed());
        }
        progress.finish(json!({ "step": "head", "result": "done" }));

        progress.current = "base";
        let expected = match integration(&config.branch) {
            Some(name) => name.to_owned(),
            None => provider.default_branch(&repo).map_err(failure)?,
        };
        if pr.base_branch != expected {
            return Err(base_mismatch(pr.state, &pr.base_branch, &expected));
        }
        progress.finish(json!({
            "step": "base", "result": "done", "expected": expected, "actual": pr.base_branch,
        }));

        let resume = pr.state == PrState::Merged;
        progress.resume = resume;
        let mut warnings = Vec::new();
        progress.current = "checks";
        if resume {
            progress.finish(json!({ "step": "checks", "result": "skipped" }));
        } else {
            self.check_run(&CheckTarget::Event(BEFORE_MERGE.to_owned()), None)?;
            progress.finish(json!({ "step": "checks", "result": "done" }));
        }

        progress.current = "ci";
        if resume {
            progress.finish(json!({ "step": "ci", "result": "skipped" }));
        } else {
            let read = provider.checks(&repo, pr.number).map_err(failure)?;
            if read.head_commit != head {
                return Err(head_not_pushed());
            }
            let verdict = ci_verdict(&read);
            let red = match &verdict {
                CiVerdict::Red(_) => provider
                    .classify_red(&repo, pr.number, &head)
                    .map_err(failure)?,
                _ => Vec::new(),
            };
            let (result, note) =
                match ci_decision(&verdict, &red, provider_config.require_ci, force_note) {
                    CiDecision::Refuse(code) => {
                        if let CiVerdict::Red(names) = &verdict {
                            progress.refusing = json!({ "red": names });
                        }
                        return Err(ci_refusal(code, &verdict));
                    }
                    CiDecision::Attest { result, note } => (result, note),
                };
            let note = Some(note).filter(|note| !note.is_empty());
            self.attest(
                caller,
                CI_GREEN_ID,
                work_item.as_str(),
                result.as_str(),
                note.as_deref(),
            )?;
            let forced = match (&verdict, result) {
                (CiVerdict::Red(names), AttestResult::PassedWithWarning) => names.clone(),
                _ => Vec::new(),
            };
            warnings.extend(note);
            progress.finish(json!({ "step": "ci", "result": result.as_str(), "forced": forced }));
        }

        progress.current = "gate";
        let (enabled, _) = self.resolve_native_for_pr(base)?;
        let requirements = self.work_requirements(work_item, &enabled)?;
        let (review_base, review_required) = self.review_base(base, &enabled)?;
        let report = self.omhl.evaluate_delivery_gate(
            &self.tenant,
            work_item,
            &snapshots,
            &enabled,
            &requirements,
            review_required,
        )?;
        // Só uma pergunta `warn` reprovada leva o `--force` ao fechamento do `dlv1`.
        let forcing = close_verdict(&report, force_note)
            .map_err(|refused| AppError::close_refused(refused).with_review_base(&review_base))?;
        let (gate_note, overridden) = match &forcing {
            Some(Forcing::Forced(gate_override)) => (force_note, gate_override.overridden.clone()),
            _ => (None, Vec::new()),
        };
        progress.finish(json!({
            "step": "gate",
            "result": if gate_note.is_some() { "forced" } else { "passed" },
            "overridden": overridden,
        }));

        progress.current = "merge";
        if pr.auto_merge && !resume {
            return Err(merge_pending());
        }
        let call = (!resume).then(|| provider.merge(&repo, pr.number, &head, strategy));
        let read = provider.state(&repo, pr.number).map_err(failure)?;
        let call_failure = match call {
            Some(Ok(call)) if !call.success => Some(call.stderr),
            Some(Err(error)) => Some(format!("{error:?}")),
            _ => None,
        };
        match (read.state, call_failure) {
            (PrState::Merged, _) if read.base_branch != expected => {
                return Err(base_mismatch(PrState::Merged, &read.base_branch, &expected));
            }
            (PrState::Merged, _) if read.head_commit == head => {}
            (PrState::Merged, _) => {
                return Err(merge_refused(
                    "the pull request was merged at another commit",
                ));
            }
            (PrState::Open, None) => return Err(merge_pending()),
            (PrState::Open, Some(stderr)) => return Err(merge_refused(&stderr)),
            (PrState::Closed, stderr) => {
                return Err(merge_refused(&stderr.unwrap_or_else(|| {
                    "the pull request is closed without a merge".to_owned()
                })));
            }
        }
        progress.finish(json!({
            "step": "merge", "result": if resume { "skipped" } else { "done" },
        }));

        progress.current = "close";
        let id = pull_request_id(work_item, provider_config.name, pr.number);
        // shortcut: lido antes da leitura do fechamento, não dentro dela; mover para a facade se houver gravação concorrente.
        if self.omhl.get(&self.tenant, &id)?.is_some() {
            return Err(refusal(
                "pull-request-record-exists",
                format!(
                    "a node already has the id {}, so the pull request record would overwrite it",
                    id.as_str()
                ),
                "ask the owner: omhl merge never overwrites a node",
            ));
        }
        let record = PullRequestRecord {
            id: id.clone(),
            provider: provider_config.name.as_str(),
            repo: recorded_repo(&repo),
            number: pr.number,
            branch,
            strategy: strategy.as_str(),
            merge_commit: read.merge_commit.clone(),
        };
        let author = caller.author();
        let closing = Closing {
            author: &author,
            note: gate_note,
        };
        self.omhl
            .close_work_item_with(
                &self.tenant,
                work_item,
                &snapshots,
                &enabled,
                &requirements,
                review_required,
                &closing,
                Some(&record),
            )
            .map_err(AppError::from)?;
        progress.finish(json!({ "step": "close", "result": "done" }));

        let main_dir = std::fs::canonicalize(&main_dir).unwrap_or(main_dir);
        let declared = integration(&config.branch).is_some();
        let stages: Vec<Value> = tidy(&git, &main_dir, &record.branch, &expected, declared)
            .into_iter()
            .map(|(stage, result)| {
                let (result, reason) = match result {
                    StageResult::Done => ("done", None),
                    StageResult::Skipped(reason) => ("skipped", Some(reason)),
                    StageResult::NotNeeded => ("not-needed", None),
                };
                json!({ "stage": stage.as_str(), "result": result, "reason": reason })
            })
            .collect();
        progress.finish(json!({ "step": "tidy", "result": "done", "stages": stages }));

        Ok(json!({
            "work_item": work_item,
            "pull_request": {
                "id": id,
                "number": pr.number,
                "repo": record.repo,
                "strategy": strategy.as_str(),
                "merge_commit": read.merge_commit,
            },
            "steps": progress.done,
            "warnings": warnings,
            "next": format!("cd {}", main_dir.display()),
        }))
    }

    /// Entre os PRs do próprio repositório, o aberto ou, na retomada, o mergeado no `HEAD`.
    fn pick_pull_request(
        &self,
        provider: &dyn Provider,
        repo: &RepoRef,
        listed: &[PullRequest],
        head: &str,
        host: &str,
    ) -> Result<PullRequest, AppError> {
        let own: Vec<&PullRequest> = listed.iter().filter(|pr| !pr.cross_repository).collect();
        let candidates: Vec<&PullRequest> = own
            .iter()
            .copied()
            .filter(|pr| {
                pr.state == PrState::Open || (pr.state == PrState::Merged && pr.head_commit == head)
            })
            .collect();
        match candidates.as_slice() {
            [only] => Ok((*only).clone()),
            [] if own.is_empty() && !listed.is_empty() => {
                let forks: Vec<&PullRequest> = listed.iter().collect();
                let named: Vec<u64> = forks.iter().map(|pr| pr.number).collect();
                Err(fork_unsupported(
                    &format!(
                        "GitHub lists only fork pull requests for this branch: {}",
                        numbers(&forks)
                    ),
                    json!({ "fork_pull_requests": named }),
                ))
            }
            [] if listed.is_empty() => {
                let fork = provider
                    .fork_parent(repo)
                    .map_err(|error| provider_failure(error, host))?;
                match (fork.is_fork, fork.parent) {
                    (true, parent) => {
                        let parent = parent.unwrap_or_else(|| "its parent".to_owned());
                        Err(fork_unsupported(
                            &format!("origin is a fork of {parent} on GitHub"),
                            json!({ "parent": parent }),
                        ))
                    }
                    (false, _) => Err(not_found()),
                }
            }
            [] => Err(not_found()),
            many => Err(refusal(
                "pull-request-ambiguous",
                format!(
                    "GitHub lists more than one pull request for this branch: {}",
                    numbers(many)
                ),
                format!("close the extra pull requests on GitHub and {REPEAT}"),
            )
            .with_error_context(json!({
                "pull_requests": many.iter().map(|pr| pr.number).collect::<Vec<_>>(),
            }))),
        }
    }
}

fn merge_pending() -> AppError {
    refusal(
        "merge-pending",
        "the merge is pending on GitHub (merge queue or auto-merge), so the work item stays open",
        format!("wait for the merge and {REPEAT} to resume and close the work item"),
    )
    .with_error_context(json!({ "state": "open" }))
}
