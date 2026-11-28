use hexlog::SemanticId;
use serde_json::{Value, json};

use super::engine::Engine;
use super::error::AppError;
use super::retry::with_retry;
use crate::error::{OmhlError, RuleCode};
use crate::graphs;
use crate::mechanical::branch::{BranchKind, HeadFacts, classify, current_branch};
use crate::probe::{CommitMark, ProbeError};
use crate::workflow::branch_policy::{BranchConfig, ExceptionMode};

const MARK_EVENT: &str = "before-commit";
const HEAD_AMBIGUOUS_HINT: &str = "HEAD is detached and zero or several local branches contain it; switch to your branch with git switch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemRoute {
    Recorded(SemanticId),
    DryRun,
    Skip,
}

pub(super) fn head_branch(facts: &HeadFacts, event: &str) -> Result<String, AppError> {
    current_branch(facts).map_err(|_| {
        AppError::head_ambiguous("").with_context(json!({
            "kind": "head-ambiguous",
            "event": event,
            "hint": HEAD_AMBIGUOUS_HINT,
        }))
    })
}

pub(super) fn resolve_work_item(
    engine: &mut Engine<'_>,
    event: &str,
    branch: &str,
    policy: &BranchConfig,
) -> Result<WorkItemRoute, AppError> {
    let format = policy.format.template();
    match classify(policy, branch) {
        BranchKind::Main => {
            let hint = if event == "before-push" {
                format!("main is not attested; push a branch {format} from its own worktree")
            } else {
                format!("main receives no direct changes; create a branch {format}")
            };
            Err(AppError::branch_protected("").with_context(branch_context(
                "branch-protected",
                event,
                branch,
                &hint,
            )))
        }
        BranchKind::Other => Err(
            AppError::branch_name_invalid("").with_context(branch_context(
                "branch-name-invalid",
                event,
                branch,
                &format!("rename the branch to {format} with git branch -m"),
            )),
        ),
        BranchKind::Exception(ExceptionMode::DryRunWithMark)
            if event == MARK_EVENT && !mark_present(engine)? =>
        {
            Err(
                AppError::commit_mark_missing("").with_context(branch_context(
                    "commit-mark-missing",
                    event,
                    branch,
                    &mark_hint(engine)?,
                )),
            )
        }
        BranchKind::Exception(ExceptionMode::Skip) => Ok(WorkItemRoute::Skip),
        BranchKind::Exception(_) => Ok(WorkItemRoute::DryRun),
        BranchKind::WorkItem { id, .. } => open_work_item(engine, event, branch, id),
    }
}

fn mark_hint(engine: &Engine<'_>) -> Result<String, AppError> {
    let location = commit_mark(engine)?
        .location()
        .map_err(|_| AppError::commit_mark_unavailable())?;
    Ok(format!(
        "create the mark with the /oh-my-hexflow:omhl-commit-mark command, or write {} with the first line `omhl-commit-mark v1`",
        location.display()
    ))
}

pub(super) fn consume_commit_mark(engine: &Engine<'_>) -> Result<Value, AppError> {
    let mark = commit_mark(engine)?;
    if !mark.present().map_err(|error| probe_failed(&error))? {
        return Ok(json!({ "consumed": false }));
    }
    let facts = engine
        .probe
        .head_facts()
        .map_err(|error| probe_failed(&error))?;
    let policy = engine.lenient_branch_policy();
    let on_mark_branch = current_branch(&facts).is_ok_and(|branch| {
        classify(&policy, &branch) == BranchKind::Exception(ExceptionMode::DryRunWithMark)
    });
    if on_mark_branch {
        mark.consume().map_err(|error| probe_failed(&error))?;
    }

    Ok(json!({ "consumed": on_mark_branch }))
}

fn open_work_item(
    engine: &mut Engine<'_>,
    event: &str,
    branch: &str,
    id: SemanticId,
) -> Result<WorkItemRoute, AppError> {
    let Some(code) = work_item_refusal(engine, &id)? else {
        return Ok(WorkItemRoute::Recorded(id));
    };

    Err(
        AppError::from(OmhlError::rule(code, "/work_item")).with_context(json!({
            "kind": code.as_str(),
            "event": event,
            "branch": branch,
            "work_item": id,
        })),
    )
}

pub(super) fn work_item_refusal(
    engine: &mut Engine<'_>,
    id: &SemanticId,
) -> Result<Option<RuleCode>, AppError> {
    let view = with_retry(&engine.retry, || Ok(engine.omhl.get(&engine.tenant, id)?))?;

    Ok(match view {
        Some(view) if view.type_name.as_str() == graphs::WORK_ITEM_TYPE => {
            (view.data["status"] == "closed").then_some(RuleCode::WorkItemClosed)
        }
        _ => Some(RuleCode::WorkItemNotFound),
    })
}

fn branch_context(kind: &str, event: &str, branch: &str, hint: &str) -> Value {
    json!({ "kind": kind, "event": event, "branch": branch, "hint": hint })
}

fn mark_present(engine: &Engine<'_>) -> Result<bool, AppError> {
    commit_mark(engine)?
        .present()
        .map_err(|_| AppError::commit_mark_unavailable())
}

/// O `App` da CLI sempre liga a marca; sem ela é erro de montagem, nunca de uso.
fn commit_mark<'d>(engine: &Engine<'d>) -> Result<&'d dyn CommitMark, AppError> {
    engine.mark.ok_or_else(AppError::commit_mark_unavailable)
}

fn probe_failed(error: &ProbeError) -> AppError {
    AppError::probe_failed(&error.message)
}
