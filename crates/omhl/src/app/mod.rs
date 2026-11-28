mod agent_role;
mod attest;
mod byproduct_prune;
mod byproduct_tail;
mod check_retire;
mod check_run;
mod context;
mod edge_hooks;
mod engine;
mod gate_worktree;
mod git_version;
mod guideline_retire;
mod hooks;
mod language;
mod load;
mod merge;
mod read;
mod report;
mod report_sessions;
mod spec;
mod spec_facts;
mod tidy;
mod wire;
mod work_item;
mod worktree_exclude;

pub mod action;
pub mod error;
pub mod input;
pub mod list_filter;
pub mod output;
pub mod retry;
pub mod runtime;

pub use crate::byproduct::read_command as byproduct_read_command;
pub use crate::extensions::model::ExtensionKind;
pub use crate::mechanical::{EVENTS, INSTRUCTION_EVENT, RUNNABLE_EVENTS};
pub use crate::workflow::config::INVALID_WORKFLOW_CONFIG;
pub use crate::workflow::lock::{EditInput, Replacement, Tool, Verdict};
pub use crate::workflow::omhl_leaf::Group;

pub use self::agent_role::{AgentRoleEnv, RoleVerdict, agent_role};
pub use self::edge_hooks::{
    BashEvent, GuardOutcome, McpMergeEvent, McpWriteEvent, PUSH_WARNING_TIMEOUT, ProviderAccess,
    branch_birth, command_guard, mcp_merge_guard, mcp_write_warning,
};
pub use self::language::{
    LanguageRungs, language_line, resolve_language, settings_language, user_settings_language,
};
pub use self::spec::{
    SpecLockEvent, SpecLockOutcome, branch_created, heal_branch_marks, spec_lock,
};

use std::path::{Path, PathBuf};
use std::time::Duration;

use hexlog::{Config, ErrorCode, SemanticId, TenantId};
use serde_json::Value;

use self::action::{Action, Caller};
use self::engine::{Engine, Exec};
use self::error::AppError;
use self::retry::{RetryPolicy, with_retry};
use crate::byproduct::{ByproductRetention, ByproductStore};
use crate::error::{OmhlError, RuleCode};
use crate::facade::{Omhl, OmhlDeps};
use crate::probe::{CommitMark, NativeResolver};
use crate::provider::Provider;
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
                native: None,
                retry: retry.clone(),
                retry_after_run: RetryPolicy::after_run(),
                hooks_dir: None,
                provider_runner: None,
                provider: None,
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

    pub fn with_commit_mark(mut self, mark: &'d dyn CommitMark) -> App<'d> {
        self.engine.mark = Some(mark);
        self
    }

    pub fn with_native_resolver(mut self, native: &'d dyn NativeResolver) -> App<'d> {
        self.engine.native = Some(native);
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

    /// O `gh` só é montado depois de o `[provider]` ser lido, dentro do `omhl merge`.
    pub fn with_provider_runner(
        mut self,
        runner: &'d dyn CommandRunner,
        timeout: Duration,
    ) -> App<'d> {
        self.engine.provider_runner = Some((runner, timeout));
        self
    }

    /// Para os testes da lógica: vale só depois de o `[provider]` ser lido.
    pub fn with_provider(mut self, provider: &'d dyn Provider) -> App<'d> {
        self.engine.provider = Some(provider);
        self
    }

    pub fn work_item_refusal(&mut self, id: &SemanticId) -> Result<Option<RuleCode>, AppError> {
        work_item::work_item_refusal(&mut self.engine, id)
    }

    pub fn git_hooks_notice(&self) -> Option<String> {
        self.engine.git_hooks_notice()
    }

    pub fn work_item_type(&mut self, id: &SemanticId) -> Result<Option<String>, AppError> {
        work_item::work_item_type(&mut self.engine, id)
    }

    pub fn premise_facts(&mut self, work_item: &SemanticId) -> Result<Vec<PremiseFact>, AppError> {
        let App { engine, retry } = self;

        with_retry(retry, || engine.premise_facts(work_item))
    }

    pub fn spec_is_valid(&mut self, work_item: &SemanticId) -> Result<bool, AppError> {
        let App { engine, retry } = self;

        with_retry(retry, || engine.spec_is_valid(work_item))
    }

    /// O `check run` e o `merge` cuidam das próprias tentativas: repeti-los inteiros rodaria os comandos de novo.
    pub fn dispatch(&mut self, caller: &Caller, action: &Action) -> Result<Value, AppError> {
        let App { engine, retry } = self;

        match action {
            Action::CheckRun { .. } | Action::Merge { .. } => engine.run(caller, action),
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
