use hexlog::SemanticId;
use serde_json::{Value, json};

use super::engine::Engine;
use super::error::AppError;
use super::retry::with_retry;
use crate::error::{OmhlError, RuleCode};
use crate::graphs;
use crate::mechanical::branch::{BranchKind, HeadFacts, classify, current_branch};
use crate::probe::{DevelopMark, ProbeError};

const MARK_EVENT: &str = "before-commit";
const BRANCH_NAME_HINT: &str = "rename the branch to type/wi-<id>__<slug> with git branch -m";
const HEAD_AMBIGUOUS_HINT: &str = "HEAD is detached and zero or several local branches contain it; switch to your branch with git switch";
const MAIN_PR_HINT: &str =
    "main is not attested; push a branch type/wi-<id>__<slug> from its own worktree";
const MAIN_CHANGE_HINT: &str =
    "main receives no direct changes; create a branch type/wi-<id>__<slug>";
const DEVELOP_MARK_HINT: &str = "create the mark with the /oh-my-hexflow:omhl-develop-mark command";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemRoute {
    Recorded(SemanticId),
    DryRun,
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
) -> Result<WorkItemRoute, AppError> {
    match classify(branch) {
        BranchKind::Main => {
            let hint = if event == "before-pr" {
                MAIN_PR_HINT
            } else {
                MAIN_CHANGE_HINT
            };
            Err(AppError::branch_protected("").with_context(branch_context(
                "branch-protected",
                event,
                branch,
                hint,
            )))
        }
        BranchKind::Other => Err(
            AppError::branch_name_invalid("").with_context(branch_context(
                "branch-name-invalid",
                event,
                branch,
                BRANCH_NAME_HINT,
            )),
        ),
        BranchKind::Develop if event == MARK_EVENT && !mark_present(engine)? => Err(
            AppError::develop_mark_missing("").with_context(branch_context(
                "develop-mark-missing",
                event,
                branch,
                DEVELOP_MARK_HINT,
            )),
        ),
        BranchKind::Develop => Ok(WorkItemRoute::DryRun),
        BranchKind::WorkItem(id) => open_work_item(engine, event, branch, id),
    }
}

pub(super) fn consume_develop_mark(engine: &Engine<'_>) -> Result<Value, AppError> {
    let mark = develop_mark(engine)?;
    if !mark.present().map_err(|error| probe_failed(&error))? {
        return Ok(json!({ "consumed": false }));
    }
    let facts = engine
        .probe
        .head_facts()
        .map_err(|error| probe_failed(&error))?;
    let on_develop =
        current_branch(&facts).is_ok_and(|branch| classify(&branch) == BranchKind::Develop);
    if on_develop {
        mark.consume().map_err(|error| probe_failed(&error))?;
    }

    Ok(json!({ "consumed": on_develop }))
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
    develop_mark(engine)?
        .present()
        .map_err(|error| probe_failed(&error))
}

/// O `App` da CLI sempre liga a marca; sem ela é erro de montagem, nunca de uso.
fn develop_mark<'d>(engine: &Engine<'d>) -> Result<&'d dyn DevelopMark, AppError> {
    engine.mark.ok_or_else(AppError::develop_mark_unavailable)
}

fn probe_failed(error: &ProbeError) -> AppError {
    AppError::probe_failed(&error.message)
}
