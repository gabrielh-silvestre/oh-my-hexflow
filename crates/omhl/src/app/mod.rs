mod agent_role;
mod attest;
mod byproduct_prune;
mod byproduct_tail;
mod check_retire;
mod check_run;
mod context;
mod engine;
mod gate_worktree;
mod git_version;
mod guideline_retire;
mod hooks;
mod language;
mod load;
mod read;
mod report;
mod report_sessions;
mod spec;
mod spec_facts;
mod wire;
mod work_item;

pub mod action;
pub mod error;
pub mod input;
pub mod list_filter;
pub mod output;
pub mod retry;
pub mod runtime;

pub use crate::byproduct::read_command as byproduct_read_command;
pub use crate::extensions::model::ExtensionKind;
pub use crate::mechanical::RUNNABLE_EVENTS;
pub use crate::workflow::config::INVALID_WORKFLOW_CONFIG;
pub use crate::workflow::lock::{EditInput, Replacement, Tool, Verdict};
pub use crate::workflow::omhl_leaf::Group;

pub use self::agent_role::{AgentRoleEnv, RoleVerdict, agent_role};
pub use self::language::{
    LanguageRungs, language_line, resolve_language, settings_language, user_settings_language,
    valid_language,
};
pub use self::spec::{SpecLockEvent, SpecLockOutcome, spec_lock, wt_post_start};

use std::path::{Path, PathBuf};

use hexlog::{Config, ErrorCode, SemanticId, TenantId};
use serde_json::Value;

use self::action::{Action, Caller};
use self::engine::{Engine, Exec};
use self::error::AppError;
use self::retry::{RetryPolicy, with_retry};
use crate::byproduct::{ByproductRetention, ByproductStore};
use crate::error::{OmhlError, RuleCode};
use crate::facade::{Omhl, OmhlDeps};
use crate::probe::DevelopMark;
use crate::runner::CommandRunner;
use crate::workflow::matrix::PremiseFact;

pub struct App<'d> {
    engine: Engine<'d>,
    retry: RetryPolicy,
}

impl<'d> App<'d> {
    pub fn open(
        db_path: &Path,
        tenant: &TenantId,
        deps: OmhlDeps<'d>,
        retry: &RetryPolicy,
    ) -> Result<App<'d>, AppError> {
        let config = Config {
            busy_timeout: retry.busy_timeout,
            ..Config::new(db_path.to_path_buf())
        };
        let mut omhl = with_retry(retry, || Ok(Omhl::open(&config, deps)?))?;
        with_retry(retry, || ensure_tenant(&mut omhl, tenant))?;

        Ok(App {
            engine: Engine {
                omhl,
                tenant: tenant.clone(),
                probe: deps.probe,
                project_root: None,
                exec: None,
                retention: None,
                mark: None,
                retry: retry.clone(),
                retry_after_run: RetryPolicy::after_run(),
                hooks_dir: None,
            },
            retry: retry.clone(),
        })
    }

    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> App<'d> {
        self.engine.project_root = Some(root.into());
        self
    }

    pub fn with_exec(
        mut self,
        runner: &'d dyn CommandRunner,
        byproducts: &'d dyn ByproductStore,
    ) -> App<'d> {
        self.engine.exec = Some(Exec { runner, byproducts });
        self
    }

    /// Só o `Runtime` a liga: sem ela o `check run` não poda e não cria o carimbo.
    pub fn with_retention(mut self, retention: &'d dyn ByproductRetention) -> App<'d> {
        self.engine.retention = Some(retention);
        self
    }

    pub fn with_develop_mark(mut self, mark: &'d dyn DevelopMark) -> App<'d> {
        self.engine.mark = Some(mark);
        self
    }

    /// Para o teste injetar pausas curtas na gravação depois do comando.
    pub fn with_retry_after_run(mut self, policy: RetryPolicy) -> App<'d> {
        self.engine.retry_after_run = policy;
        self
    }

    /// Pasta de usuário dos hooks do git.
    pub fn with_hooks_dir(mut self, dir: Option<PathBuf>) -> App<'d> {
        self.engine.hooks_dir = dir;
        self
    }

    pub fn work_item_refusal(&mut self, id: &SemanticId) -> Result<Option<RuleCode>, AppError> {
        work_item::work_item_refusal(&mut self.engine, id)
    }

    pub fn premise_facts(&mut self, work_item: &SemanticId) -> Result<Vec<PremiseFact>, AppError> {
        let App { engine, retry } = self;

        with_retry(retry, || engine.premise_facts(work_item))
    }

    pub fn spec_is_valid(&mut self, work_item: &SemanticId) -> Result<bool, AppError> {
        let App { engine, retry } = self;

        with_retry(retry, || engine.spec_is_valid(work_item))
    }

    /// O `check run` cuida das próprias tentativas: repeti-lo inteiro rodaria os comandos de novo.
    pub fn dispatch(&mut self, caller: &Caller, action: &Action) -> Result<Value, AppError> {
        let App { engine, retry } = self;

        match action {
            Action::CheckRun { .. } => engine.run(caller, action),
            _ => with_retry(retry, || engine.run(caller, action)),
        }
    }
}

fn ensure_tenant(omhl: &mut Omhl<'_>, tenant: &TenantId) -> Result<(), AppError> {
    match omhl.chain_head(tenant) {
        Ok(_) => Ok(()),
        Err(OmhlError::Hexlog(error)) if error.code == ErrorCode::TenantNotFound => {
            omhl.ensure_tenant(tenant)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
