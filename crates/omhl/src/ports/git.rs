use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use hexlog::{Snapshot, SnapshotKind};

use crate::probe::{
    CommitMark, GIT_TIMED_OUT, GitOutput, GitVersion, HeadFacts, ProbeError, ProbeOutcome,
    SnapshotProbe, TidyGit, TreeSource,
};
use crate::project::layout::EXT_FILE_MAX_BYTES;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// O total das chamadas de git de uma execução do hook `spec-lock`; menor que o `timeout` do `hooks.json`.
pub const SPEC_LOCK_GIT_BUDGET: Duration = Duration::from_secs(6);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ERROR_CHARS: usize = 200;
const OID_PREFIX: &[u8] = b"# branch.oid ";
const DIRTY_PREFIXES: [&[u8]; 4] = [b"1 ", b"2 ", b"u ", b"? "];
const MARK_FILE: &str = "omhl-commit-mark";
const MARK_LINE: &[u8] = b"omhl-commit-mark v1";

#[derive(Clone)]
pub struct GitProbe {
    dir: PathBuf,
    program: OsString,
    timeout: Duration,
    /// Prazo dividido entre todas as chamadas desta sonda.
    deadline: Option<Instant>,
    index_file: Option<PathBuf>,
}

/// `content` só vem lido até `EXT_FILE_MAX_BYTES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBlob {
    pub mode: String,
    pub size: u64,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobReason {
    Link,
    TooLarge,
    Absent,
    NotUtf8,
}

#[derive(Clone, Copy)]
pub enum WorkflowSource<'a> {
    Rev(&'a str),
    Index,
}

const WORKFLOW_PATH: &str = ".omhl/workflow.toml";

/// Lê `.omhl/workflow.toml` da origem; só arquivo regular, UTF-8 e dentro do limite vira texto.
pub fn workflow_text(probe: &GitProbe, source: WorkflowSource<'_>) -> Result<String, BlobReason> {
    let blob = match source {
        WorkflowSource::Rev(rev) => probe.read_blob(rev, WORKFLOW_PATH),
        WorkflowSource::Index => probe.read_index_blob(WORKFLOW_PATH),
    };
    let Ok(Some(blob)) = blob else {
        return Err(BlobReason::Absent);
    };
    if !matches!(blob.mode.as_str(), "100644" | "100755") {
        return Err(BlobReason::Link);
    }
    if blob.size > EXT_FILE_MAX_BYTES {
        return Err(BlobReason::TooLarge);
    }
    String::from_utf8(blob.content).map_err(|_| BlobReason::NotUtf8)
}

fn entry_fields(entry: &str) -> Vec<&str> {
    entry
        .split_once('\t')
        .map_or(entry, |(meta, _)| meta)
        .split(' ')
        .collect()
}

struct RawOutput {
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
            deadline: None,
            index_file: None,
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

    pub fn rooted_at(&self, dir: &Path) -> GitProbe {
        GitProbe {
            dir: dir.to_path_buf(),
            index_file: None,
            ..self.clone()
        }
    }

    pub fn with_deadline(self, deadline: Instant) -> GitProbe {
        GitProbe {
            deadline: Some(deadline),
            ..self
        }
    }

    /// O `git -C <dir>` troca de diretório antes de ler a variável.
    pub fn with_index_file(self, index_file: Option<&Path>, base: &Path) -> GitProbe {
        GitProbe {
            index_file: index_file.map(|path| base.join(path)),
            ..self
        }
    }

    fn base_command(&self) -> Command {
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
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn git(&self, args: &[&str]) -> Command {
        let mut command = self.base_command();
        command.args(args);
        command
    }

    fn tree_git(&self, args: &[&str]) -> Command {
        let mut command = self.git(args);
        if let Some(index_file) = &self.index_file {
            command.env("GIT_INDEX_FILE", index_file);
        }
        command
    }

    /// O menor entre o teto da chamada e o que sobra do prazo da sonda; com o prazo vencido falha sem criar processo.
    fn call_deadline(&self) -> Result<Instant, ProbeError> {
        let own = Instant::now() + self.timeout;
        match self.deadline {
            Some(shared) if Instant::now() >= shared => Err(error(GIT_TIMED_OUT)),
            Some(shared) => Ok(own.min(shared)),
            None => Ok(own),
        }
    }

    fn run(&self, mut command: Command) -> Result<RawOutput, ProbeError> {
        let deadline = self.call_deadline()?;
        let child = command.spawn().map_err(|error| ProbeError {
            message: error.to_string(),
        })?;
        finish(child, deadline)
    }

    pub fn version(&self) -> GitVersion {
        let Ok(deadline) = self.call_deadline() else {
            return GitVersion::Unreadable;
        };
        match self.git(&["--version"]).spawn() {
            Err(error) if error.kind() == ErrorKind::NotFound => GitVersion::NotFound,
            Err(_) => GitVersion::Unreadable,
            Ok(child) => match finish(child, deadline) {
                Ok(output) if output.status.success() => {
                    GitVersion::Reported(String::from_utf8_lossy(&output.stdout).into_owned())
                }
                _ => GitVersion::Unreadable,
            },
        }
    }

    fn run_ok(&self, command: Command) -> Result<Vec<u8>, ProbeError> {
        let output = self.run(command)?;
        if !output.status.success() {
            return Err(git_error(&output));
        }
        Ok(output.stdout)
    }

    fn line(&self, args: &[&str]) -> Result<String, ProbeError> {
        text_line(self.run_ok(self.git(args))?)
    }

    fn index_dirty(&self) -> Result<bool, ProbeError> {
        // Sem esta conferência o `git` trataria o índice ausente como vazio.
        if let Some(index_file) = &self.index_file
            && !index_file.exists()
        {
            return Err(error("the git index file does not exist"));
        }
        let diff = self.run(self.tree_git(&[
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--ignore-submodules=none",
        ]))?;
        match diff.status.code() {
            Some(0) => {}
            Some(1) => return Ok(true),
            _ => return Err(git_error(&diff)),
        }
        let untracked =
            self.run_ok(self.tree_git(&["ls-files", "--others", "--exclude-standard", "-z"]))?;
        if !untracked.is_empty() {
            return Ok(true);
        }
        self.hides_changes(self.tree_git(&["ls-files", "-v", "-z"]))
    }

    /// `status` e `diff` não veem `--skip-worktree` (tag `S`) nem `--assume-unchanged` (minúscula): o
    /// comando rodaria sobre a working tree enquanto a attestation prende a árvore do índice.
    /// O `skip-worktree` de um sparse-checkout também cai aqui; recusar é o lado seguro.
    fn hides_changes(&self, list_files: Command) -> Result<bool, ProbeError> {
        let listing = self.run_ok(list_files)?;
        Ok(listing
            .split(|byte| *byte == 0)
            .filter_map(|entry| entry.first())
            .any(|tag| *tag == b'S' || tag.is_ascii_lowercase()))
    }

    fn tree_snapshot(&self, command: Command) -> Result<ProbeOutcome, ProbeError> {
        let stdout = self.run_ok(command)?;
        let snapshot = std::str::from_utf8(&stdout)
            .ok()
            .and_then(|text| text.strip_suffix('\n'))
            .and_then(|hex| Snapshot::new(SnapshotKind::GitTree, hex).ok())
            .ok_or_else(|| error("unexpected git output"))?;
        Ok(ProbeOutcome::Reproducible(snapshot))
    }

    fn rebase_head(&self) -> Result<Option<String>, ProbeError> {
        for name in ["rebase-merge/head-name", "rebase-apply/head-name"] {
            let path = self.line(&["rev-parse", "--path-format=absolute", "--git-path", name])?;
            match fs::read_to_string(path) {
                Ok(content) => return Ok(Some(content)),
                Err(io_error) if io_error.kind() == ErrorKind::NotFound => {}
                Err(io_error) => return Err(error(io_error.to_string())),
            }
        }
        Ok(None)
    }

    fn head_commit(&self) -> Result<Option<String>, ProbeError> {
        self.ref_commit("HEAD")
    }

    pub fn ref_commit(&self, refname: &str) -> Result<Option<String>, ProbeError> {
        let peeled = format!("{refname}^{{commit}}");
        let output = self.run(self.git(&["rev-parse", "--verify", "-q", &peeled]))?;
        match output.status.code() {
            Some(0) => text_line(output.stdout).map(Some),
            Some(1) if output.stdout.is_empty() => Ok(None),
            _ => Err(git_error(&output)),
        }
    }

    /// `None` quando `base` não resolve a um commit.
    pub fn merge_base(&self, base: &str) -> Result<Option<String>, ProbeError> {
        let Some(commit) = self.ref_commit(base)? else {
            return Ok(None);
        };
        self.line(&["merge-base", &commit, "HEAD"]).map(Some)
    }

    pub fn changed_names(&self, merge_base: &str) -> Result<Vec<String>, ProbeError> {
        let stdout = self.run_ok(self.git(&[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            merge_base,
            "HEAD",
        ]))?;
        let text = utf8_output(stdout)?;
        Ok(text
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// `Ok(None)` quando o caminho não existe no commit; `Err` quando `rev` não resolve.
    pub fn read_blob(&self, rev: &str, path: &str) -> Result<Option<GitBlob>, ProbeError> {
        let listing = utf8_output(self.run_ok(self.git(&[
            "ls-tree",
            "-z",
            "--full-tree",
            rev,
            "--",
            path,
        ]))?)?;
        let Some(entry) = listing.split('\0').find(|entry| !entry.is_empty()) else {
            return Ok(None);
        };
        let [mode, kind, oid] = entry_fields(entry)[..] else {
            return Err(error("unexpected git output"));
        };
        self.load_blob(mode, oid, kind == "blob")
    }

    pub fn read_index_blob(&self, path: &str) -> Result<Option<GitBlob>, ProbeError> {
        let listing = utf8_output(self.run_ok(self.git(&["ls-files", "-s", "-z", "--", path]))?)?;
        let Some(entry) = listing.split('\0').find(|entry| !entry.is_empty()) else {
            return Ok(None);
        };
        let [mode, oid, _stage] = entry_fields(entry)[..] else {
            return Err(error("unexpected git output"));
        };
        self.load_blob(mode, oid, mode != "160000")
    }

    /// Entrada que não é blob (link de submódulo, árvore) volta com tamanho 0 e sem conteúdo.
    fn load_blob(
        &self,
        mode: &str,
        oid: &str,
        is_blob: bool,
    ) -> Result<Option<GitBlob>, ProbeError> {
        if !is_blob {
            return Ok(Some(GitBlob {
                mode: mode.to_owned(),
                size: 0,
                content: Vec::new(),
            }));
        }
        let size: u64 = self
            .line(&["cat-file", "-s", oid])?
            .parse()
            .map_err(|_| error("unexpected git output"))?;
        let content = match size <= EXT_FILE_MAX_BYTES {
            true => self.run_ok(self.git(&["cat-file", "blob", oid]))?,
            false => Vec::new(),
        };

        Ok(Some(GitBlob {
            mode: mode.to_owned(),
            size,
            content,
        }))
    }

    pub fn current_branch(&self) -> Result<Option<String>, ProbeError> {
        let name = self.line(&["branch", "--show-current"])?;
        Ok(Some(name).filter(|name| !name.is_empty()))
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, ProbeError> {
        let output = self.run(self.git(&["merge-base", "--is-ancestor", ancestor, descendant]))?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(git_error(&output)),
        }
    }

    fn containing_branches(&self) -> Result<Vec<String>, ProbeError> {
        let stdout = self.run_ok(self.git(&[
            "for-each-ref",
            "--format=%(refname:short)",
            "--contains",
            "HEAD",
            "refs/heads",
        ]))?;
        let text = utf8_output(stdout)?;
        Ok(text.lines().map(str::to_owned).collect())
    }

    pub fn checkout_dir(&self) -> Result<PathBuf, ProbeError> {
        self.line(&["rev-parse", "--path-format=absolute", "--show-toplevel"])
            .map(PathBuf::from)
    }

    fn mark_path(&self) -> Result<PathBuf, ProbeError> {
        let git_dir = self.line(&["rev-parse", "--path-format=absolute", "--absolute-git-dir"])?;
        Ok(Path::new(&git_dir).join(MARK_FILE))
    }
}

impl SnapshotProbe for GitProbe {
    fn probe(&self) -> Result<ProbeOutcome, ProbeError> {
        let status = self.git(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--no-renames",
            "--no-ahead-behind",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ]);
        match parse_status(&self.run_ok(status)?)? {
            ProbeOutcome::Reproducible(_)
                if self.hides_changes(self.git(&["ls-files", "-v", "-z"]))? =>
            {
                Ok(ProbeOutcome::Dirty)
            }
            outcome => Ok(outcome),
        }
    }

    fn probe_tree(&self, source: TreeSource) -> Result<ProbeOutcome, ProbeError> {
        match source {
            TreeSource::Index if self.index_dirty()? => Ok(ProbeOutcome::Dirty),
            TreeSource::Index => self.tree_snapshot(self.tree_git(&["write-tree"])),
            TreeSource::Head => match self.probe()? {
                ProbeOutcome::Dirty => Ok(ProbeOutcome::Dirty),
                ProbeOutcome::Reproducible(_) => {
                    self.tree_snapshot(self.git(&["rev-parse", "HEAD^{tree}"]))
                }
            },
        }
    }

    fn tree_dirty(&self) -> Result<bool, ProbeError> {
        self.index_dirty()
    }

    fn head_facts(&self) -> Result<HeadFacts, ProbeError> {
        let current =
            Some(self.line(&["branch", "--show-current"])?).filter(|name| !name.is_empty());
        let rebase_head = self.rebase_head()?;
        let head = self.head_commit()?;
        // Sem commit o `for-each-ref --contains HEAD` sai 129.
        let containing = match head {
            Some(_) => self.containing_branches()?,
            None => Vec::new(),
        };
        Ok(HeadFacts {
            current,
            rebase_head,
            containing,
            head,
        })
    }
}

impl TidyGit for GitProbe {
    fn git(&self, dir: &Path, args: &[&str]) -> Result<GitOutput, ProbeError> {
        let rooted = self.rooted_at(dir);
        let output = rooted.run(rooted.git(args))?;
        Ok(GitOutput {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

impl CommitMark for GitProbe {
    fn location(&self) -> Result<PathBuf, ProbeError> {
        self.mark_path()
    }

    fn present(&self) -> Result<bool, ProbeError> {
        match fs::read(self.mark_path()?) {
            Ok(content) => Ok(content.split(|byte| *byte == b'\n').next() == Some(MARK_LINE)),
            Err(io_error) if io_error.kind() == ErrorKind::NotFound => Ok(false),
            Err(io_error) => Err(error(io_error.to_string())),
        }
    }

    fn consume(&self) -> Result<(), ProbeError> {
        match fs::remove_file(self.mark_path()?) {
            Err(io_error) if io_error.kind() != ErrorKind::NotFound => {
                Err(error(io_error.to_string()))
            }
            _ => Ok(()),
        }
    }
}

impl GitProbe {
    /// Diretório de trabalho principal, o mesmo de qualquer worktree ligada.
    pub fn main_dir(&self) -> Result<PathBuf, ProbeError> {
        let common_dir = self.common_dir()?;
        common_dir
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| error("unexpected git output"))
    }

    pub fn common_dir(&self) -> Result<PathBuf, ProbeError> {
        self.line(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .map(PathBuf::from)
    }

    /// Raízes das worktrees ligadas, sem a principal.
    pub fn linked_worktrees(&self) -> Result<Vec<PathBuf>, ProbeError> {
        let stdout = self.run_ok(self.git(&["worktree", "list", "--porcelain"]))?;
        let text = utf8_output(stdout)?;
        Ok(text
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .skip(1)
            .map(PathBuf::from)
            .collect())
    }

    /// Saída 0 é ignorado, 1 não é; qualquer outra é falha de leitura.
    pub fn is_ignored(&self, path: &str) -> Result<bool, ProbeError> {
        let mut command = self.git(&["check-ignore", "-q", "--", path]);
        // O arquivo global de excludes vale aqui: tirar o `GIT_CONFIG_GLOBAL` mudaria a resposta.
        if let Some(global) = std::env::var_os("GIT_CONFIG_GLOBAL") {
            command.env("GIT_CONFIG_GLOBAL", global);
        }
        let output = self.run(command)?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(git_error(&output)),
        }
    }

    pub fn origin_url(&self) -> Result<Option<String>, ProbeError> {
        let output = self.run(self.git(&["config", "--get", "remote.origin.url"]))?;
        match output.status.code() {
            Some(0) => text_line(output.stdout).map(Some),
            Some(1) => Ok(None),
            _ => Err(git_error(&output)),
        }
    }

    /// Branches com worktree (a principal inclusive), em ordem crescente.
    pub fn worktree_branches(&self) -> Result<Vec<String>, ProbeError> {
        let stdout = self.run_ok(self.git(&["worktree", "list", "--porcelain"]))?;
        let text = utf8_output(stdout)?;
        let mut branches: Vec<String> = text
            .lines()
            .filter_map(|line| line.strip_prefix("branch refs/heads/"))
            .map(str::to_owned)
            .collect();
        branches.sort();
        Ok(branches)
    }

    /// Branches locais, em ordem crescente.
    pub fn local_branches(&self) -> Result<Vec<String>, ProbeError> {
        let stdout =
            self.run_ok(self.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads"]))?;
        let text = utf8_output(stdout)?;
        let mut branches: Vec<String> = text.lines().map(str::to_owned).collect();
        branches.sort();
        Ok(branches)
    }
}

fn finish(mut child: Child, deadline: Instant) -> Result<RawOutput, ProbeError> {
    let (stdout, stderr) = match (child.stdout.take(), child.stderr.take()) {
        (Some(stdout), Some(stderr)) => (stdout, stderr),
        _ => return Err(error("git pipes are unavailable")),
    };
    // Duas threads leem os canos para o filho não travar com saída maior que o buffer.
    let (stdout, stderr) = (spawn_reader(stdout), spawn_reader(stderr));
    let status = wait_until(&mut child, deadline)?;

    Ok(RawOutput {
        status,
        stdout: collect(&stdout, deadline)?,
        stderr: collect(&stderr, deadline)?,
    })
}

fn error(message: impl Into<String>) -> ProbeError {
    ProbeError {
        message: message.into(),
    }
}

fn utf8_output(stdout: Vec<u8>) -> Result<String, ProbeError> {
    String::from_utf8(stdout).map_err(|_| error("unexpected git output"))
}

fn git_error(output: &RawOutput) -> ProbeError {
    let message = String::from_utf8_lossy(&output.stderr);
    error(
        message
            .trim()
            .chars()
            .take(MAX_ERROR_CHARS)
            .collect::<String>(),
    )
}

fn text_line(stdout: Vec<u8>) -> Result<String, ProbeError> {
    let mut text = utf8_output(stdout)?;
    if text.ends_with('\n') {
        text.pop();
    }
    Ok(text)
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

fn collect(reader: &Receiver<Vec<u8>>, deadline: Instant) -> Result<Vec<u8>, ProbeError> {
    reader
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| error(GIT_TIMED_OUT))
}

fn wait_until(child: &mut Child, deadline: Instant) -> Result<ExitStatus, ProbeError> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error(GIT_TIMED_OUT));
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
