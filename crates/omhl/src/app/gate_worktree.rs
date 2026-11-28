use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::App;
use super::error::AppError;
use crate::error::{OmhlError, RuleCode};
use crate::mechanical::branch::{BranchKind, classify};
use crate::ports::git::GitProbe;
use crate::ports::workflow::read_workflow;
use crate::project::marker::find_marker;
use crate::workflow::branch_policy::BranchFormat;
use crate::workflow::config::RequireAtom;
use crate::workflow::matrix::unsatisfied_atoms;
use crate::workflow::work_type::effective_type;

const BRANCH_POINTER: &str = "/branch";

/// Códigos em que o portão decidiu; qualquer outro erro é falha de infraestrutura.
pub(super) const REFUSAL_CODES: [&str; 5] = [
    "worktree-gate-refused",
    "prefix-not-allowed",
    "work-item-type-unknown",
    "work-item-not-found",
    "work-item-closed",
];

pub(super) enum Decision {
    Final(Value),
    NeedsDatabase(PendingLookup),
}

pub(super) struct PendingLookup {
    branch: String,
    prefix: Option<String>,
    work_item: SemanticId,
    prefixes: BTreeMap<String, Vec<RequireAtom>>,
    format: BranchFormat,
    main_dir: PathBuf,
}

pub(super) fn decide(start: &Path, branch: &str) -> Result<Decision, AppError> {
    let located = find_marker(start)?.ok_or_else(AppError::no_marker)?;
    let probe = GitProbe::new(&located.root);
    let main_dir = probe.main_dir().map_err(|_| AppError::main_dir_unknown())?;
    let config = read_workflow(&main_dir)?;
    let (work_item, prefix) = match classify(&config.branch, branch) {
        BranchKind::Main | BranchKind::Exception(_) => {
            return Ok(Decision::Final(allowed(branch, "exempt-branch")));
        }
        BranchKind::WorkItem { id, kind } => (id, kind),
        BranchKind::Other => {
            return Err(AppError::prefix_not_allowed(
                BRANCH_POINTER,
                &config.branch.format,
            ));
        }
    };
    let with_worktree = probe
        .worktree_branches()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    if with_worktree.iter().any(|name| name == branch) {
        return Ok(Decision::Final(allowed(branch, "worktree-exists")));
    }

    Ok(Decision::NeedsDatabase(PendingLookup {
        branch: branch.to_owned(),
        prefix,
        work_item,
        prefixes: config.prefixes,
        format: config.branch.format,
        main_dir,
    }))
}

pub(super) fn finish(app: &mut App<'_>, pending: PendingLookup) -> Result<Value, AppError> {
    let PendingLookup {
        branch,
        prefix,
        work_item,
        prefixes,
        format,
        main_dir,
    } = pending;
    if let Some(code) = app.work_item_refusal(&work_item)? {
        return Err(OmhlError::rule(code, BRANCH_POINTER).into());
    }
    let recorded = app.work_item_type(&work_item)?;
    let branch_prefixes: Vec<&str> = prefix.as_deref().into_iter().collect();
    let Some(name) = effective_type(recorded.as_deref(), &branch_prefixes, &prefixes) else {
        if recorded.is_none() && prefix.is_some() {
            return Err(AppError::prefix_not_allowed(BRANCH_POINTER, &format));
        }
        return Err(OmhlError::rule(RuleCode::WorkItemTypeUnknown, BRANCH_POINTER).into());
    };
    let required = &prefixes[&name];
    let premises = app.premise_facts(&work_item)?;
    let spec_valid = !required.contains(&RequireAtom::Spec) || app.spec_is_valid(&work_item)?;
    let missing = unsatisfied_atoms(required, &premises, spec_valid);
    if missing.is_empty() {
        return Ok(allowed(&branch, "matrix-satisfied"));
    }

    Err(AppError::worktree_gate_refused(
        BRANCH_POINTER,
        json!({
            "kind": "worktree-gate-refused",
            "branch": branch,
            "prefix": name,
            "work_item": work_item,
            "missing": missing.iter().map(|atom| atom.as_str()).collect::<Vec<_>>(),
            "main_dir": main_dir.to_string_lossy(),
        }),
    ))
}

fn allowed(branch: &str, reason: &str) -> Value {
    json!({ "allowed": true, "branch": branch, "reason": reason })
}
