use std::path::{Path, PathBuf};

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::App;
use super::error::AppError;
use crate::error::OmhlError;
use crate::mechanical::branch::{BranchKind, branch_prefix, classify};
use crate::ports::git::GitProbe;
use crate::ports::workflow::read_workflow;
use crate::project::marker::find_marker;
use crate::workflow::config::{RequireAtom, WorkflowConfig};
use crate::workflow::matrix::missing_atoms;

const BRANCH_POINTER: &str = "/branch";

/// O que a primeira fase decidiu sem abrir o banco.
pub(super) enum Decision {
    Final(Value),
    NeedsDatabase(PendingLookup),
}

/// O que a segunda fase precisa para consultar o work item, as premissas e a spec.
pub(super) struct PendingLookup {
    branch: String,
    prefix: String,
    work_item: SemanticId,
    without_facts: Vec<RequireAtom>,
    main_dir: PathBuf,
    config: WorkflowConfig,
}

/// Ordem da decisão (d): marcador, `develop` e `main`, branch com worktree, formato e prefixo, matriz.
/// O banco só é aberto (pela segunda fase) quando o `require` tem algum átomo de premissa ou `spec`.
pub(super) fn decide(start: &Path, branch: &str) -> Result<Decision, AppError> {
    let located = find_marker(start)?.ok_or_else(AppError::no_marker)?;
    if matches!(branch, "develop" | "main") {
        return Ok(Decision::Final(allowed(branch, "exempt-branch")));
    }
    let probe = GitProbe::new(&located.root);
    let with_worktree = probe
        .worktree_branches()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    if with_worktree.iter().any(|name| name == branch) {
        return Ok(Decision::Final(allowed(branch, "worktree-exists")));
    }

    let main_dir = probe.main_dir().map_err(|_| AppError::main_dir_unknown())?;
    let config = read_workflow(&main_dir)?;
    let (Some(prefix), BranchKind::WorkItem(work_item)) = (branch_prefix(branch), classify(branch))
    else {
        return Err(AppError::prefix_not_allowed(BRANCH_POINTER));
    };
    let without_facts = missing_atoms(prefix, &config, &[], false)
        .ok_or_else(|| AppError::prefix_not_allowed(BRANCH_POINTER))?;
    if without_facts.is_empty() {
        return Ok(Decision::Final(allowed(branch, "matrix-satisfied")));
    }

    Ok(Decision::NeedsDatabase(PendingLookup {
        branch: branch.to_owned(),
        prefix: prefix.to_owned(),
        work_item,
        without_facts,
        main_dir,
        config,
    }))
}

/// Work item aberto e existente, depois a matriz sobre as premissas e a spec lidas do banco.
pub(super) fn finish(app: &mut App<'_>, pending: PendingLookup) -> Result<Value, AppError> {
    let PendingLookup {
        branch,
        prefix,
        work_item,
        without_facts,
        main_dir,
        config,
    } = pending;
    if let Some(code) = app.work_item_refusal(&work_item)? {
        return Err(OmhlError::rule(code, BRANCH_POINTER).into());
    }
    let premises = app.premise_facts(&work_item)?;
    let spec_valid =
        !without_facts.contains(&RequireAtom::Spec) || app.spec_is_valid(&work_item)?;
    let missing = missing_atoms(&prefix, &config, &premises, spec_valid)
        .ok_or_else(|| AppError::prefix_not_allowed(BRANCH_POINTER))?;
    if missing.is_empty() {
        return Ok(allowed(&branch, "matrix-satisfied"));
    }

    Err(AppError::worktree_gate_refused(
        BRANCH_POINTER,
        json!({
            "kind": "worktree-gate-refused",
            "branch": branch,
            "prefix": prefix,
            "work_item": work_item,
            "missing": missing.iter().map(|atom| atom.as_str()).collect::<Vec<_>>(),
            "main_dir": main_dir.to_string_lossy(),
        }),
    ))
}

fn allowed(branch: &str, reason: &str) -> Value {
    json!({ "allowed": true, "branch": branch, "reason": reason })
}
