mod attest;
mod byproduct_tail;
mod check_retire;
mod check_run;
mod context;
mod engine;
mod gate_worktree;
mod guideline_retire;
mod hooks;
mod load;
mod read;
mod report;
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

pub use crate::extensions::model::ExtensionKind;
pub use crate::mechanical::RUNNABLE_EVENTS;
pub use crate::workflow::config::INVALID_WORKFLOW_CONFIG;
pub use crate::workflow::lock::Verdict;

pub use self::spec::spec_lock;

use std::path::{Path, PathBuf};

use hexlog::{Config, ErrorCode, SemanticId, TenantId};
use serde_json::Value;

use self::action::{Action, Caller};
use self::engine::Engine;
use self::error::AppError;
use self::retry::{RetryPolicy, with_retry};
use crate::byproduct::ByproductStore;
use crate::error::{OmhlError, RuleCode};
use crate::facade::{Omhl, OmhlDeps};
use crate::probe::DevelopMark;
use crate::runner::CommandRunner;
use crate::workflow::matrix::PremiseFact;

/// Ações do `omhl` sobre um tenant, com as novas tentativas de `Busy`.
pub struct App<'d> {
    engine: Engine<'d>,
    retry: RetryPolicy,
}

impl<'d> App<'d> {
    /// Não escreve nada enquanto o tenant existe; só cria o tenant quando falta.
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
                runner: None,
                byproducts: None,
                mark: None,
                retry: retry.clone(),
                retry_after_run: RetryPolicy::after_run(),
                hooks_dir: None,
            },
            retry: retry.clone(),
        })
    }

    /// Sem raiz, `load` e `project` respondem `no-project-root`.
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> App<'d> {
        self.engine.project_root = Some(root.into());
        self
    }

    pub fn with_exec(
        mut self,
        runner: &'d dyn CommandRunner,
        byproducts: &'d dyn ByproductStore,
    ) -> App<'d> {
        self.engine.runner = Some(runner);
        self.engine.byproducts = Some(byproducts);
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

    /// `None` quando o id é de um work item aberto.
    pub fn work_item_refusal(&mut self, id: &SemanticId) -> Result<Option<RuleCode>, AppError> {
        work_item::work_item_refusal(&mut self.engine, id)
    }

    /// As premissas vigentes do work item, como a matriz do gate de worktree as consulta.
    pub fn premise_facts(&mut self, work_item: &SemanticId) -> Result<Vec<PremiseFact>, AppError> {
        let App { engine, retry } = self;

        with_retry(retry, || engine.premise_facts(work_item))
    }

    /// Mesma validação do `spec validate`: pasta de spec ausente e spec reprovada valem `false`.
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
