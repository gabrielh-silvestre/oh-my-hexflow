use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use hexlog::{Snapshot, SnapshotKind};

use crate::probe::{ProbeError, ProbeOutcome, SnapshotProbe};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const OID_PREFIX: &[u8] = b"# branch.oid ";
const DIRTY_PREFIXES: [&[u8]; 4] = [b"1 ", b"2 ", b"u ", b"? "];

/// Sonda que pergunta ao executável `git` se a árvore do repositório que envolve `dir` está limpa.
pub struct GitProbe {
    dir: PathBuf,
    program: OsString,
    timeout: Duration,
}

struct GitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl GitProbe {
    pub fn new(dir: &Path) -> GitProbe {
        GitProbe {
            dir: dir.to_path_buf(),
            program: OsString::from("git"),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_program(self, program: impl Into<OsString>) -> GitProbe {
        GitProbe {
            program: program.into(),
            ..self
        }
    }

    pub fn with_timeout(self, timeout: Duration) -> GitProbe {
        GitProbe { timeout, ..self }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        // Variáveis GIT_* herdadas (GIT_DIR, GIT_INDEX_FILE, GIT_CONFIG_COUNT...) desviariam a resposta.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(&key);
            }
        }
        command
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C")
            .arg("-C")
            .arg(&self.dir)
            .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
            .args([
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--no-renames",
                "--no-ahead-behind",
                "--untracked-files=normal",
                "--ignore-submodules=none",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self) -> Result<GitOutput, ProbeError> {
        let mut child = self.command().spawn().map_err(|error| ProbeError {
            message: error.to_string(),
        })?;
        let (stdout, stderr) = match (child.stdout.take(), child.stderr.take()) {
            (Some(stdout), Some(stderr)) => (stdout, stderr),
            _ => return Err(error("git pipes are unavailable")),
        };
        // Duas threads leem os canos para o filho não travar com saída maior que o buffer.
        thread::scope(|scope| {
            let stdout = scope.spawn(move || read_all(stdout));
            let stderr = scope.spawn(move || read_all(stderr));
            let status = wait_within(&mut child, self.timeout);
            let stdout = stdout.join().unwrap_or_default();
            let stderr = stderr.join().unwrap_or_default();
            status.map(|status| GitOutput {
                status,
                stdout,
                stderr,
            })
        })
    }
}

impl SnapshotProbe for GitProbe {
    fn probe(&self) -> Result<ProbeOutcome, ProbeError> {
        let output = self.run()?;
        if !output.status.success() {
            return Err(error(String::from_utf8_lossy(&output.stderr).trim()));
        }
        parse_status(&output.stdout)
    }
}

fn error(message: impl Into<String>) -> ProbeError {
    ProbeError {
        message: message.into(),
    }
}

fn read_all(mut pipe: impl Read) -> Vec<u8> {
    let mut buffer = Vec::new();
    // Erro de leitura deixa o que já chegou; o parser recusa saída incompleta.
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

/// Espera o filho até o prazo; passado o prazo, mata e recolhe o processo.
fn wait_within(child: &mut Child, timeout: Duration) -> Result<ExitStatus, ProbeError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("git timed out"));
            }
            Err(io_error) => return Err(error(io_error.to_string())),
        }
    }
}

fn parse_status(stdout: &[u8]) -> Result<ProbeOutcome, ProbeError> {
    let mut records = stdout.split(|byte| *byte == 0);
    let Some(oid) = records.find_map(|record| record.strip_prefix(OID_PREFIX)) else {
        return Err(error("unexpected git output"));
    };
    if oid == b"(initial)" {
        return Err(error("the repository has no commits"));
    }
    let snapshot = std::str::from_utf8(oid)
        .ok()
        .and_then(|oid| Snapshot::new(SnapshotKind::GitCommit, oid).ok())
        .ok_or_else(|| error("unexpected git output"))?;
    let dirty = stdout.split(|byte| *byte == 0).any(|record| {
        DIRTY_PREFIXES
            .iter()
            .any(|prefix| record.starts_with(prefix))
    });
    Ok(match dirty {
        true => ProbeOutcome::Dirty,
        false => ProbeOutcome::Reproducible(snapshot),
    })
}
