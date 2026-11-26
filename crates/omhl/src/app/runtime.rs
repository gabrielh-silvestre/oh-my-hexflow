use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use hexlog::{Clock, TenantId};
use serde_json::{Value, json};

use super::App;
use super::action::{Action, Caller};
use super::error::AppError;
use super::gate_worktree::{self, Decision};
use super::git_version;
use super::retry::RetryPolicy;
use crate::facade::{OmhlDeps, SessionSource};
use crate::ports::blob::BlobStore;
use crate::ports::clock::SystemClock;
use crate::ports::git::GitProbe;
use crate::ports::ids::V7Ids;
use crate::ports::paths::resolve_physical;
use crate::ports::process::{
    ProcessRunner, SweepReport, SweptProcess, sweep_project_strays, sweep_strays,
};
use crate::project::marker::find_marker;
use crate::project::paths::{data_root, default_db_path, prepare_db_file};
use crate::project::{Env, ProjectError};

const SWEEP_LOG: &str = "sweep.jsonl";
const SWEEP_LOG_ROTATED: &str = "sweep.jsonl.1";
/// Acima disto o log gira para `sweep.jsonl.1` antes da próxima linha.
const SWEEP_LOG_MAX_BYTES: u64 = 1024 * 1024;

pub struct Runtime {
    tenant: TenantId,
    db_path: PathBuf,
    clock: SystemClock,
    ids: V7Ids,
    probe: GitProbe,
    session: SessionSource,
    root: PathBuf,
    retry: RetryPolicy,
    runner: ProcessRunner,
    byproducts: BlobStore,
    /// Diretório do processo, não o `--project-dir`: é contra ele que o git resolve um `GIT_INDEX_FILE` relativo.
    cwd: PathBuf,
    data_root: Option<PathBuf>,
}

impl Runtime {
    pub fn discover(start: &Path, db: Option<&Path>, env: &Env<'_>) -> Result<Runtime, AppError> {
        let cwd = std::env::current_dir().map_err(|_| AppError::invalid_project_dir())?;
        let located = find_marker(start)?.ok_or_else(AppError::no_marker)?;
        let db_path = match db {
            Some(path) => path.to_path_buf(),
            None => default_db_path(env)?,
        };
        if is_inside(&db_path, &located.root)? {
            return Err(AppError::db_inside_project());
        }
        prepare_db_file(&db_path)?;

        let byproducts = BlobStore::new(&db_path, &located.marker.tenant);

        Ok(Runtime {
            tenant: located.marker.tenant,
            byproducts,
            db_path,
            clock: SystemClock,
            ids: V7Ids,
            probe: GitProbe::new(&located.root)
                .with_index_file(env.git_index_file.map(Path::new), &cwd),
            session: env.session.clone().into(),
            root: located.root,
            retry: RetryPolicy::default(),
            runner: ProcessRunner::bound_to_parent(env.parent_pid),
            cwd,
            data_root: data_root(env),
        })
    }

    pub fn app(&self) -> Result<App<'_>, AppError> {
        let deps = OmhlDeps {
            clock: &self.clock,
            ids: &self.ids,
            probe: &self.probe,
            session: &self.session,
        };
        let hooks_dir = self
            .data_root
            .as_ref()
            .map(|dir| dir.join("omhl").join("hooks"));

        Ok(App::open(&self.db_path, &self.tenant, deps, &self.retry)?
            .with_project_root(&self.root)
            .with_exec(&self.runner, &self.byproducts)
            .with_retention(&self.byproducts)
            .with_develop_mark(&self.probe)
            .with_native_resolver(&self.probe)
            .with_hooks_dir(hooks_dir))
    }

    pub fn git_notice(&self) -> Option<String> {
        git_version::notice(&self.probe.version())
    }

    fn check_sweep(&self) -> Result<Value, AppError> {
        let report = sweep_strays().map_err(|_| AppError::process_table_unreadable())?;
        self.append_sweep_log(&report);

        Ok(json!({
            "scan_timed_out": report.scan_timed_out,
            "swept": swept_json(&report.swept),
            "unreadable": report.unreadable,
        }))
    }

    fn sweep_quietly(&self) {
        if let Ok(report) = sweep_project_strays(&self.root) {
            self.append_sweep_log(&report);
        }
    }

    fn append_sweep_log(&self, report: &SweepReport) {
        let Some(data_root) = &self.data_root else {
            return;
        };
        if report.swept.is_empty() {
            return;
        }
        let dir = data_root.join("omhl");
        let line = json!({
            "cwd": self.cwd.to_string_lossy(),
            "pid": std::process::id(),
            "swept": swept_json(&report.swept),
            "ts": self.clock.now(),
        });
        let log_path = dir.join(SWEEP_LOG);
        let _ = std::fs::create_dir_all(&dir).and_then(|()| {
            // Uma só geração antiga: duas rotações simultâneas podem perder uma geração (aceito).
            if std::fs::metadata(&log_path).is_ok_and(|meta| meta.len() > SWEEP_LOG_MAX_BYTES) {
                std::fs::rename(&log_path, dir.join(SWEEP_LOG_ROTATED))?;
            }
            let mut log = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&log_path)?;
            writeln!(log, "{line}")
        });
    }
}

fn unresolved_db_path() -> ProjectError {
    ProjectError {
        code: "database-file",
        message: "the database path cannot be resolved".to_owned(),
    }
}

/// Resolve os links do caminho inteiro do banco e da raiz (o `..` e o link para dentro do projeto também contam).
/// O `absolute` vem antes porque `resolve_physical` perde o `..` inicial de um caminho relativo.
fn is_inside(db_path: &Path, root: &Path) -> Result<bool, ProjectError> {
    let absolute = std::path::absolute(db_path).map_err(|_| unresolved_db_path())?;
    let db = resolve_physical(&absolute).map_err(|_| unresolved_db_path())?;
    let root = resolve_physical(root).map_err(|_| unresolved_db_path())?;

    Ok(db.starts_with(root))
}

fn swept_json(swept: &[SweptProcess]) -> Value {
    swept
        .iter()
        .map(|process| {
            json!({ "comm": process.comm, "owner_pid": process.owner_pid, "pid": process.pid })
        })
        .collect()
}

/// O veredito sem premissa a consultar não pode criar o arquivo do banco, que o `discover` prepara.
fn run_gate_worktree(
    start: &Path,
    db: Option<&Path>,
    env: &Env<'_>,
    branch: &str,
) -> Result<Value, AppError> {
    match gate_worktree::decide(start, branch)? {
        Decision::Final(verdict) => Ok(verdict),
        Decision::NeedsDatabase(pending) => {
            let runtime = Runtime::discover(start, db, env)?;
            gate_worktree::finish(&mut runtime.app()?, pending)
        }
    }
}

pub fn run_action(
    start: &Path,
    db: Option<&Path>,
    env: &Env<'_>,
    caller: &Caller,
    action: &Action,
) -> Result<Value, AppError> {
    if matches!(std::fs::metadata(start), Err(error) if error.kind() == ErrorKind::NotFound) {
        return Err(AppError::project_dir_missing());
    }
    if let Action::GateWorktree { branch } = action {
        return run_gate_worktree(start, db, env, branch);
    }
    let runtime = Runtime::discover(start, db, env)?;

    match action {
        Action::CheckSweep => runtime.check_sweep(),
        Action::CheckRun { .. } => {
            runtime.sweep_quietly();
            runtime.app()?.dispatch(caller, action)
        }
        _ => runtime.app()?.dispatch(caller, action),
    }
}
