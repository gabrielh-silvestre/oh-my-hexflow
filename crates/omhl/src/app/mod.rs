mod attest;
mod byproduct_tail;
mod check_retire;
mod check_run;
mod context;
mod engine;
mod gate_worktree;
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
pub mod output;
pub mod retry;
pub mod runtime;

pub use crate::extensions::model::ExtensionKind;
pub use crate::mechanical::RUNNABLE_EVENTS;
pub use crate::workflow::lock::Verdict;

pub use self::spec::spec_lock;

use std::path::{Path, PathBuf};

use hexlog::{Config, ErrorCode, TenantId};
use serde_json::Value;

use self::action::{Action, Caller};
use self::engine::Engine;
use self::error::AppError;
use self::retry::{RetryPolicy, with_retry};
use crate::byproduct::ByproductStore;
use crate::error::OmhlError;
use crate::facade::{Omhl, OmhlDeps};
use crate::probe::DevelopMark;
use crate::runner::CommandRunner;

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
