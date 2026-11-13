use std::path::Path;

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::App;
use super::action::{Action, Caller};
use super::error::AppError;
use super::runtime::Runtime;
use crate::mechanical::branch::{BranchKind, branch_prefix, classify};
use crate::ports::git::GitProbe;
use crate::ports::workflow::read_workflow;
use crate::probe::GitTopology;
use crate::project::Env;
use crate::project::marker::find_marker;
use crate::workflow::config::RequireAtom;
use crate::workflow::matrix::{PremiseFact, PremiseKind, missing_atoms};

const BRANCH_POINTER: &str = "/branch";
const PREMISE_TYPE: &str = "premise";

/// Ordem da decisão (d): marcador, `develop` e `main`, branch com worktree, formato e prefixo, matriz.
/// O banco só é aberto quando o `require` tem algum átomo de premissa ou `spec`.
pub(super) fn run(
    start: &Path,
    db: Option<&Path>,
    env: &Env<'_>,
    caller: &Caller,
    branch: &str,
) -> Result<Value, AppError> {
    let located = find_marker(start)?.ok_or_else(AppError::no_marker)?;
    if matches!(branch, "develop" | "main") {
        return Ok(allowed(branch, "exempt-branch"));
    }
    let probe = GitProbe::new(&located.root);
    let with_worktree = probe
        .worktree_branches()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    if with_worktree.iter().any(|name| name == branch) {
        return Ok(allowed(branch, "worktree-exists"));
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
        return Ok(allowed(branch, "matrix-satisfied"));
    }

    let runtime = Runtime::discover(start, db, env)?;
    let mut app = runtime.app()?;
    let premises = work_item_premises(&mut app, caller, &work_item)?;
    let spec_valid =
        !without_facts.contains(&RequireAtom::Spec) || spec_is_valid(&mut app, caller, &work_item)?;
    let missing = missing_atoms(prefix, &config, &premises, spec_valid)
        .ok_or_else(|| AppError::prefix_not_allowed(BRANCH_POINTER))?;
    if missing.is_empty() {
        return Ok(allowed(branch, "matrix-satisfied"));
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

/// Roda o mesmo validador do `spec validate`; pasta de spec ausente e spec reprovada valem `false`, e a violação nunca sai daqui.
fn spec_is_valid(
    app: &mut App<'_>,
    caller: &Caller,
    work_item: &SemanticId,
) -> Result<bool, AppError> {
    let validation = app.dispatch(
        caller,
        &Action::SpecValidate {
            work_item: work_item.as_str().to_owned(),
        },
    );

    match validation {
        Ok(_) => Ok(true),
        Err(error) if matches!(error.code(), "spec-not-found" | "spec-invalid") => Ok(false),
        Err(error) => Err(error),
    }
}

/// As premissas do work item, lidas do banco página a página.
fn work_item_premises(
    app: &mut App<'_>,
    caller: &Caller,
    work_item: &SemanticId,
) -> Result<Vec<PremiseFact>, AppError> {
    let mut premises = Vec::new();
    let mut after_seq = None;
    loop {
        let page = app.dispatch(
            caller,
            &Action::List {
                type_name: Some(PREMISE_TYPE.to_owned()),
                id_prefix: None,
                limit: None,
                after_seq,
                where_eq: vec![("work_item".to_owned(), work_item.as_str().to_owned())],
            },
        )?;
        let items = page["items"].as_array().map_or(&[][..], Vec::as_slice);
        premises.extend(items.iter().map(|item| premise_fact(&item["data"])));
        match page["next_after_seq"].as_u64() {
            Some(next) => after_seq = Some(next),
            None => return Ok(premises),
        }
    }
}

/// Confirmada é `status` confirmado com `confirmed_by`, a mesma regra do grafo da spec.
fn premise_fact(data: &Value) -> PremiseFact {
    let kind = match data["kind"].as_str() {
        Some("business") => Some(PremiseKind::Business),
        Some("technical") => Some(PremiseKind::Technical),
        _ => None,
    };

    PremiseFact {
        kind,
        confirmed: data["status"] == "confirmed" && data.get("confirmed_by").is_some(),
    }
}
