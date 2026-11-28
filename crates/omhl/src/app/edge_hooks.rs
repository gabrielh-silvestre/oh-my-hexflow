//! Os hooks de borda do Claude Code: marca de nascimento, portão de criação e avisos de push.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::error::AppError;
use super::gate_worktree::{self, Decision, REFUSAL_CODES};
use super::heal_branch_marks;
use super::runtime::Runtime;
use crate::ports::git::GitProbe;
use crate::ports::provider::build_unattended;
use crate::ports::workflow::read_workflow;
use crate::project::marker::find_marker;
use crate::project::{ClaudeEnv, Env};
use crate::provider::{RepoRef, repo_from_origin_url};
use crate::runner::CommandRunner;
use crate::workflow::config::{ProviderConfig, WorkflowConfig};
use crate::workflow::shell::git_command::{BranchName, GitCommand, PushTarget, recognize};

/// Abaixo do timeout de 10 s da entrada do `hooks.json`.
pub const PUSH_WARNING_TIMEOUT: Duration = Duration::from_secs(5);

const PROTECTED: &str = "main";
const PUSH_CODE: &str = "protected-branch-push";
const GATE_UNAVAILABLE: &str = "creation-gate-unavailable";
const GATE_REFUSED: &str = "worktree-gate-refused";
const MERGE_DENIAL_CODE: &str = "merge-through-omhl";
const MERGE_DENIAL: &str = "with the provider integration on, the agent merges only through omhl merge <work-item> [--squash|--rebase], which checks the CI and the delivery gate before the merge and closes the work item; the owner can still merge on GitHub";

pub struct BashEvent {
    pub cwd: PathBuf,
    pub command: String,
}

pub struct WriteToolEvent {
    pub cwd: PathBuf,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
}

pub struct MergeToolEvent {
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuardOutcome {
    pub deny: Option<String>,
    pub context: Vec<String>,
}

pub struct ProviderAccess<'a> {
    pub runner: &'a dyn CommandRunner,
    pub timeout: Duration,
}

/// Roda depois do comando: a marca vem da lista das branches, não do nome escrito.
pub fn branch_birth(event: &BashEvent) -> Result<Value, AppError> {
    let creates = recognize(&event.command)
        .iter()
        .any(|command| matches!(command, GitCommand::CreateBranch(_)));
    if creates {
        heal_branch_marks(&event.cwd)?;
    }

    Ok(json!({}))
}

pub fn command_guard(event: &BashEvent, gh: &ProviderAccess<'_>) -> Result<GuardOutcome, AppError> {
    let mut outcome = GuardOutcome::default();
    for command in recognize(&event.command) {
        match command {
            GitCommand::CreateBranch(BranchName::Literal(name)) if outcome.deny.is_none() => {
                creation_gate(&event.cwd, &name, &mut outcome);
            }
            GitCommand::ProviderMerge if outcome.deny.is_none() => {
                outcome.deny = match integration_on(&event.cwd) {
                    Ok(true) => Some(merge_denial()),
                    Ok(false) => None,
                    Err(error) => Some(format!(
                        "{}: {}; gh pr merge stays denied until the workflow file is fixed",
                        error.code(),
                        error.message()
                    )),
                };
            }
            GitCommand::Push(targets) => {
                outcome
                    .context
                    .extend(push_warning(&event.cwd, &targets, gh));
            }
            _ => {}
        }
    }

    Ok(outcome)
}

pub fn write_tool_warning(
    event: &WriteToolEvent,
    gh: &ProviderAccess<'_>,
) -> Result<GuardOutcome, AppError> {
    let Some(branch) = event.branch.as_deref() else {
        return Ok(warning(format!(
            "{PUSH_CODE}: the call names no branch, so it may write to the default branch of the repository; {PROTECTED} and the default branch receive the delivery through a pull request, and only the installed git hooks block a direct push"
        )));
    };
    if branch == PROTECTED {
        return Ok(warning(protected_line(PROTECTED)));
    }
    let Some((provider, origin)) = provider_and_origin(&event.cwd) else {
        return Ok(GuardOutcome::default());
    };
    let same_repo = matches!(
        (&event.owner, &event.repo, &origin),
        (Some(owner), Some(repo), Ok(origin))
            if origin.path[0].eq_ignore_ascii_case(owner) && origin.path[1].eq_ignore_ascii_case(repo)
    );
    if !same_repo {
        return Ok(GuardOutcome::default());
    }

    Ok(match read_default_branch(&provider, origin, gh) {
        Ok(default) if default == branch => warning(protected_line(&default)),
        Ok(_) => GuardOutcome::default(),
        Err(()) => warning(unread_line()),
    })
}

/// Lê só a declaração da integração, nunca o banco: um banco travado não segura a decisão.
pub fn merge_tool_guard(event: &MergeToolEvent) -> Result<GuardOutcome, AppError> {
    Ok(GuardOutcome {
        deny: integration_on(&event.cwd)?.then(merge_denial),
        context: Vec::new(),
    })
}

fn merge_denial() -> String {
    format!("{MERGE_DENIAL_CODE}: {MERGE_DENIAL}")
}

/// Sem marcador ou fora de um repositório git, não há integração a ler.
fn integration_on(cwd: &Path) -> Result<bool, AppError> {
    let Some(located) = find_marker(cwd)? else {
        return Ok(false);
    };
    let Ok(main_dir) = GitProbe::new(&located.root).main_dir() else {
        return Ok(false);
    };

    Ok(read_workflow(&main_dir)
        .map_err(AppError::invalid_workflow)?
        .provider
        .is_some())
}

fn warning(line: String) -> GuardOutcome {
    GuardOutcome {
        deny: None,
        context: vec![line],
    }
}

fn creation_gate(cwd: &Path, branch: &str, outcome: &mut GuardOutcome) {
    match gate_verdict(cwd, branch) {
        Ok(()) => {}
        Err(error) if REFUSAL_CODES.contains(&error.code()) => {
            outcome.deny = Some(refusal_reason(branch, &error));
        }
        Err(error) => outcome.context.push(format!(
            "{GATE_UNAVAILABLE}: the creation gate could not decide on {branch} ({}); the command goes on, and gate-delivery and close still check the work item",
            error.code()
        )),
    }
}

/// O banco só abre aqui, com o portão ligado e um nome literal.
fn gate_verdict(cwd: &Path, branch: &str) -> Result<(), AppError> {
    let Some(located) = find_marker(cwd)? else {
        return Ok(());
    };
    let main_dir = GitProbe::new(&located.root)
        .main_dir()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    if !read_workflow(&main_dir)?.creation_gate.enabled {
        return Ok(());
    }
    let Decision::NeedsDatabase(pending) = gate_worktree::decide(cwd, branch)? else {
        return Ok(());
    };
    // shortcut: o hook não recebe o `Env` da borda e lê as variáveis do banco aqui; passar o `Env` se outro hook precisar.
    let xdg_data_home = std::env::var_os("XDG_DATA_HOME");
    let home = std::env::var_os("HOME");
    let env = Env {
        xdg_data_home: xdg_data_home.as_deref(),
        home: home.as_deref(),
        git_index_file: None,
        parent_pid: std::os::unix::process::parent_id(),
        claude_code: false,
        role: None,
        plugin_root: None,
        session: std::env::var_os("CLAUDE_CODE_SESSION_ID"),
        claude: ClaudeEnv::default(),
    };
    let runtime = Runtime::discover(cwd, None, &env)?;
    gate_worktree::finish(&mut runtime.app()?, pending).map(|_| ())
}

fn refusal_reason(branch: &str, error: &AppError) -> String {
    let refusal = error.to_json();
    let missing: Vec<&str> = refusal["error"]["details"][0]["context"]["missing"]
        .as_array()
        .map(|atoms| atoms.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let cause = if missing.is_empty() {
        format!("{}: {}", error.code(), error.message())
    } else {
        format!("missing {}", missing.join(", "))
    };

    format!(
        "{GATE_REFUSED}: the branch {branch} cannot be created yet ({cause}); record what is missing for the work item (premise or spec), as omhl gate-worktree --branch {branch} lists, and run the command again"
    )
}

fn push_warning(cwd: &Path, targets: &[PushTarget], gh: &ProviderAccess<'_>) -> Option<String> {
    let current = targets
        .contains(&PushTarget::Current)
        .then(|| GitProbe::new(cwd).current_branch().ok().flatten())
        .flatten();
    let reaches = |branch: &str| {
        targets.iter().any(|target| target.hits(branch)) || current.as_deref() == Some(branch)
    };
    if reaches(PROTECTED) {
        return Some(protected_line(PROTECTED));
    }
    let main_dir = GitProbe::new(cwd).main_dir().ok()?;
    let config = match read_workflow(&main_dir) {
        Ok(WorkflowConfig {
            provider: Some(provider),
            ..
        }) => provider,
        Ok(_) => return None,
        Err(_) => return Some(unread_line()),
    };
    let origin = origin_repo(&main_dir, &config);
    match read_default_branch(&config, origin, gh) {
        Ok(default) => reaches(&default).then(|| protected_line(&default)),
        Err(()) => Some(unread_line()),
    }
}

fn provider_and_origin(cwd: &Path) -> Option<(ProviderConfig, Result<RepoRef, ()>)> {
    let main_dir = GitProbe::new(cwd).main_dir().ok()?;
    let provider = read_workflow(&main_dir).ok()?.provider?;
    let origin = origin_repo(&main_dir, &provider);

    Some((provider, origin))
}

fn origin_repo(main_dir: &Path, provider: &ProviderConfig) -> Result<RepoRef, ()> {
    GitProbe::new(main_dir)
        .origin_url()
        .ok()
        .flatten()
        .and_then(|url| repo_from_origin_url(&url, &provider.host))
        .ok_or(())
}

/// Lida a cada chamada, sem estado guardado; qualquer falha vira o aviso de leitura falha.
fn read_default_branch(
    config: &ProviderConfig,
    origin: Result<RepoRef, ()>,
    gh: &ProviderAccess<'_>,
) -> Result<String, ()> {
    let repo = origin?;
    build_unattended(config, gh.runner, gh.timeout)
        .default_branch(&repo)
        .map_err(|_| ())
}

fn protected_line(branch: &str) -> String {
    let also_main = if branch == PROTECTED {
        ""
    } else {
        " (the default branch of the provider)"
    };
    format!(
        "{PUSH_CODE}: this push reaches {branch}{also_main}; {branch} receives the delivery through a pull request, and only the installed git hooks block a direct push"
    )
}

fn unread_line() -> String {
    format!(
        "{PUSH_CODE}: the default branch could not be read, so this warning covers only {PROTECTED}; {PROTECTED} receives the delivery through a pull request, and only the installed git hooks block a direct push"
    )
}

#[cfg(test)]
mod tests;
