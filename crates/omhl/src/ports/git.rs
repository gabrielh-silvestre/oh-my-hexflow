use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use hexlog::{Snapshot, SnapshotKind};

use crate::probe::{ProbeError, ProbeOutcome, SnapshotProbe};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ERROR_CHARS: usize = 200;
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
        let (stdout, stderr) = (spawn_reader(stdout), spawn_reader(stderr));
        let deadline = Instant::now() + self.timeout;
        let status = wait_until(&mut child, deadline)?;

        Ok(GitOutput {
            status,
            stdout: collect(&stdout, deadline)?,
            stderr: collect(&stderr, deadline)?,
        })
    }
}

impl SnapshotProbe for GitProbe {
    fn probe(&self) -> Result<ProbeOutcome, ProbeError> {
        let output = self.run()?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr);

            return Err(error(
                message
                    .trim()
                    .chars()
                    .take(MAX_ERROR_CHARS)
                    .collect::<String>(),
            ));
        }
        parse_status(&output.stdout)
    }
}

fn error(message: impl Into<String>) -> ProbeError {
    ProbeError {
        message: message.into(),
    }
}

/// A thread solta termina quando o cano fecha, mesmo que um descendente do `git` o segure.
fn spawn_reader(pipe: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = Vec::new();
        // Erro de leitura deixa o que já chegou; o parser recusa saída incompleta.
        let _ = pipe.take(MAX_OUTPUT_BYTES).read_to_end(&mut buffer);
        let _ = sender.send(buffer);
    });

    receiver
}

/// Sem `join`: o que não chegou até o prazo é abandonado.
fn collect(reader: &Receiver<Vec<u8>>, deadline: Instant) -> Result<Vec<u8>, ProbeError> {
    reader
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| error("git timed out"))
}

/// Espera o filho até o prazo; passado o prazo, mata e recolhe o processo.
fn wait_until(child: &mut Child, deadline: Instant) -> Result<ExitStatus, ProbeError> {
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
