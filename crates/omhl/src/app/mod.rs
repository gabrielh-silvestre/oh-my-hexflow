mod attest;
mod context;
mod engine;
mod load;
mod read;
mod wire;

pub mod action;
pub mod error;
pub mod input;
pub mod output;
pub mod retry;
pub mod runtime;

use std::path::{Path, PathBuf};

use hexlog::{Config, ErrorCode, TenantId};
use serde_json::Value;

use self::action::{Action, Caller};
use self::engine::Engine;
use self::error::AppError;
use self::retry::{RetryPolicy, with_retry};
use crate::error::OmhlError;
use crate::facade::{Omhl, OmhlDeps};

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
            },
            retry: retry.clone(),
        })
    }

    /// Sem raiz, `load` e `project` respondem `no-project-root`.
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> App<'d> {
        self.engine.project_root = Some(root.into());
        self
    }

    pub fn dispatch(&mut self, caller: &Caller, action: &Action) -> Result<Value, AppError> {
        let App { engine, retry } = self;

        with_retry(retry, || engine.run(caller, action))
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
