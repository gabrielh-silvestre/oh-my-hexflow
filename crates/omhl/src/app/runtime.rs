use std::path::{Path, PathBuf};

use hexlog::TenantId;

use super::App;
use super::error::AppError;
use super::retry::RetryPolicy;
use crate::facade::OmhlDeps;
use crate::ports::clock::SystemClock;
use crate::ports::git::GitProbe;
use crate::ports::ids::V7Ids;
use crate::project::Env;
use crate::project::marker::find_marker;
use crate::project::paths::{default_db_path, prepare_db_file};

/// Dono do projeto e das portas de produção; cada `app()` as empresta.
pub struct Runtime {
    tenant: TenantId,
    db_path: PathBuf,
    clock: SystemClock,
    ids: V7Ids,
    probe: GitProbe,
    retry: RetryPolicy,
}

impl Runtime {
    /// O banco só é preparado depois que o marcador é achado.
    pub fn discover(start: &Path, db: Option<&Path>, env: &Env<'_>) -> Result<Runtime, AppError> {
        let located = find_marker(start)?.ok_or_else(|| {
            AppError::plain(
                "no-marker",
                "no .omhl.toml found in the project directory or any parent",
            )
        })?;
        let db_path = match db {
            Some(path) => path.to_path_buf(),
            None => default_db_path(env)?,
        };
        prepare_db_file(&db_path)?;

        Ok(Runtime {
            tenant: located.marker.tenant,
            db_path,
            clock: SystemClock,
            ids: V7Ids,
            probe: GitProbe::new(&located.root),
            retry: RetryPolicy::default(),
        })
    }

    pub fn app(&self) -> Result<App<'_>, AppError> {
        let deps = OmhlDeps {
            clock: &self.clock,
            ids: &self.ids,
            probe: &self.probe,
        };

        App::open(&self.db_path, &self.tenant, deps, &self.retry)
    }
}
