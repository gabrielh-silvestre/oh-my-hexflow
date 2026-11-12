use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSpec {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    /// Raiz do projeto de quem chama: a marca de varredura leva o hash dela, nunca o do `cwd` do check.
    pub project_root: PathBuf,
    pub timeout: Duration,
    pub env_set: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub max_stream_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub truncated: bool,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunErrorKind {
    NotFound,
    PermissionDenied,
    Spawn,
    Io,
    UnsupportedPlatform,
    ParentGone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunError {
    pub kind: RunErrorKind,
    pub message: String,
}

/// O adapter sempre força `NO_COLOR=1` e `LC_ALL=C` depois de aplicar `env_set` e `env_remove`.
pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError>;

    /// O arquivo que `run` executaria para `argv[0]`; `None` quando o runner não sabe ou o programa não existe.
    fn resolve_program(&self, _spec: &RunSpec) -> Option<PathBuf> {
        None
    }
}
