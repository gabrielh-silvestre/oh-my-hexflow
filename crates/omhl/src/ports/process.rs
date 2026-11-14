use std::path::{Path, PathBuf};

use crate::runner::{CommandRunner, RunError, RunReport, RunSpec};

pub struct ProcessRunner {
    parent_pid: Option<u32>,
}

impl ProcessRunner {
    /// Sem amarração ao processo pai; para os testes.
    pub fn new() -> ProcessRunner {
        ProcessRunner { parent_pid: None }
    }

    /// `parent_pid` é o `parent_id()` que o `main` leu na primeira linha.
    pub fn bound_to_parent(parent_pid: u32) -> ProcessRunner {
        ProcessRunner {
            parent_pid: Some(parent_pid),
        }
    }
}

impl Default for ProcessRunner {
    fn default() -> ProcessRunner {
        ProcessRunner::new()
    }
}

impl CommandRunner for ProcessRunner {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        platform::run(self.parent_pid, spec)
    }

    fn resolve_program(&self, spec: &RunSpec) -> Option<PathBuf> {
        platform::resolve_program(spec)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweptProcess {
    pub pid: u32,
    pub comm: String,
    pub owner_pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepReport {
    pub swept: Vec<SweptProcess>,
    pub unreadable: usize,
    pub scan_timed_out: bool,
}

/// Varre os processos de qualquer projeto (`omhl check sweep`).
pub fn sweep_strays() -> Result<SweepReport, RunError> {
    sweep_strays_in(Path::new("/proc"), None)
}

/// Varre só os processos cuja marca leva a raiz de `project_root` (início do `check run`).
pub fn sweep_project_strays(project_root: &Path) -> Result<SweepReport, RunError> {
    sweep_strays_in(Path::new("/proc"), Some(project_root))
}

pub fn sweep_strays_in(
    proc_root: &Path,
    project_root: Option<&Path>,
) -> Result<SweepReport, RunError> {
    platform::sweep_in(proc_root, project_root)
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs;
    use std::io::{self, ErrorKind};
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::thread;
    use std::time::{Duration, Instant};

    use nix::errno::Errno;
    use nix::sys::prctl::set_pdeathsig;
    use nix::sys::signal::{Signal, kill, killpg};
    use nix::sys::wait::{Id, WaitPidFlag, WaitStatus, waitid};
    use nix::unistd::{Pid, getpgid};

    use super::{SweepReport, SweptProcess};
    use crate::mechanical::sweep::{
        Candidate, RUN_MARKER_VAR, RunMarker, root_hash, select_strays, start_from_stat,
    };
    use crate::ports::pipes::{self, PIPE_GRACE};
    use crate::runner::{RunError, RunErrorKind, RunReport, RunSpec};

    const TERM_GRACE: Duration = Duration::from_secs(2);
    const SWEEP_SCAN_BUDGET: Duration = Duration::from_secs(1);
    const POLL: Duration = Duration::from_millis(10);
    /// Pids que a varredura pode sinalizar: o 0 e o 1 nunca, e acima de `i32::MAX` o `kill` leria um número negativo (-1 = todos).
    const SWEEPABLE_PIDS: std::ops::RangeInclusive<u32> = 2..=0x7fff_ffff;

    /// Lê o stdin até o EOF (o runner morreu ou terminou) e limpa o próprio grupo, onde roda o comando.
    const GUARDIAN_SCRIPT: &str = "trap \"\" INT TERM HUP QUIT TTIN TTOU; read -r _; kill -s TERM -- -$$; kill -s CONT -- -$$; sleep 2; kill -s KILL -- -$$";

    enum Event {
        LeaderExited,
    }

    struct ProcStat {
        comm: String,
        state: char,
        pgrp: u32,
        start: u64,
    }

    fn error(kind: RunErrorKind, message: String) -> RunError {
        RunError { kind, message }
    }

    fn pid_of(child: &Child) -> Pid {
        Pid::from_raw(child.id() as i32)
    }

    /// Barra final e link simbólico dão o mesmo hash que o caminho real; sem caminho real (raiz sumida), vale o caminho dado.
    fn project_root_hash(root: &Path) -> String {
        root_hash(&fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()))
    }

    /// A marca deste processo para um comando do projeto em `root`; `None` se o `/proc` não se leu.
    fn own_marker(root: &Path) -> Option<String> {
        let stat = fs::read_to_string("/proc/self/stat").ok()?;
        let marker = RunMarker {
            pid: std::process::id(),
            start: start_from_stat(&stat)?,
            pidns: own_pidns()?,
            root: project_root_hash(root),
        };
        Some(marker.encode())
    }

    /// Inode do namespace de pid, do link `pid:[<inode>]`.
    fn own_pidns() -> Option<u64> {
        let link = fs::read_link("/proc/self/ns/pid").ok()?;
        link.to_str()?
            .strip_prefix("pid:[")?
            .strip_suffix(']')?
            .parse()
            .ok()
    }

    /// O arquivo que o `run` executaria para `argv[0]`, com o `PATH` que o comando vê (o `env_set` vence o herdado).
    pub fn resolve_program(spec: &RunSpec) -> Option<PathBuf> {
        let program = spec.argv.first()?;
        if program.contains('/') {
            let joined = spec.cwd.join(program);
            // `Path::join` mantém o `.` de `./tool`; sem ele o caminho gravado é o mesmo arquivo, mais curto
            return is_executable_file(&joined).then(|| joined.components().collect());
        }
        let path_var = spec
            .env_set
            .iter()
            .rev()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| std::ffi::OsString::from(value))
            .or_else(|| std::env::var_os("PATH"))?;

        std::env::split_paths(&path_var)
            .map(|dir| spec.cwd.join(dir).join(program))
            .find(|candidate| is_executable_file(candidate))
    }

    fn is_executable_file(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;

        fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }

    pub fn run(parent_pid: Option<u32>, spec: &RunSpec) -> Result<RunReport, RunError> {
        let Some(marker) = own_marker(&spec.project_root) else {
            return Err(error(
                RunErrorKind::Io,
                "the run marker of this process could not be read from /proc".to_owned(),
            ));
        };
        let Some((program, args)) = spec.argv.split_first() else {
            return Err(error(RunErrorKind::Spawn, "the argv is empty".to_owned()));
        };
        if let Some(parent) = parent_pid {
            set_pdeathsig(Signal::SIGKILL)
                .map_err(|e| error(RunErrorKind::Io, format!("set_pdeathsig failed: {e}")))?;
            if std::os::unix::process::parent_id() != parent {
                return Err(error(
                    RunErrorKind::ParentGone,
                    format!("the parent process {parent} exited before the run"),
                ));
            }
        }

        let mut guardian = Command::new("sh")
            .args(["-c", GUARDIAN_SCRIPT])
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_remove(RUN_MARKER_VAR)
            .spawn()
            .map_err(|e| {
                error(
                    RunErrorKind::Spawn,
                    format!("the guardian did not start: {e}"),
                )
            })?;
        let group = pid_of(&guardian);

        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&spec.cwd);
        for (key, value) in &spec.env_set {
            command.env(key, value);
        }
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        command
            .env("NO_COLOR", "1")
            .env("LC_ALL", "C")
            .env(RUN_MARKER_VAR, marker)
            .process_group(group.as_raw());
        // antes do spawn: quando ele volta, o comando já pode estar rodando há um tempo
        let started = Instant::now();
        let mut leader = match command.spawn() {
            Ok(leader) => leader,
            Err(e) => {
                let _ = killpg(group, Signal::SIGKILL);
                let _ = guardian.wait();
                return Err(spawn_error(program, &e));
            }
        };

        // WNOWAIT: um guardião morto fica por colher e segura o número do grupo até o killpg
        let guardian_alive = matches!(
            waitid(
                Id::Pid(group),
                WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT
            ),
            Ok(WaitStatus::StillAlive)
        );
        if !guardian_alive {
            // sem guardião, o comando ficaria sem quem o limpe se este processo morrer
            let _ = killpg(group, Signal::SIGKILL);
            // um líder que já saiu do grupo não recebe o KILL do grupo; sem isto o wait() esperaria para sempre
            let _ = kill(pid_of(&leader), Signal::SIGKILL);
            let _ = leader.wait();
            let _ = guardian.wait();
            return Err(error(
                RunErrorKind::Spawn,
                "the guardian exited before the command started".to_owned(),
            ));
        }

        let stdout = pipes::spawn_reader(
            leader
                .stdout
                .take()
                .expect("the stdout of the command is piped"),
            spec.max_stream_bytes,
        );
        let stderr = pipes::spawn_reader(
            leader
                .stderr
                .take()
                .expect("the stderr of the command is piped"),
            spec.max_stream_bytes,
        );
        let (event_sender, events) = mpsc::channel();
        let leader_pid = pid_of(&leader);
        thread::spawn(move || {
            // WNOWAIT deixa o líder por colher: o pid não é reusado antes do `killpg`
            while matches!(
                waitid(
                    Id::Pid(leader_pid),
                    WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT
                ),
                Err(Errno::EINTR)
            ) {}
            let _ = event_sender.send(Event::LeaderExited);
        });

        let timed_out = matches!(
            events.recv_timeout(spec.timeout),
            Err(RecvTimeoutError::Timeout)
        );
        if timed_out {
            // o líder ainda não foi colhido (WNOWAIT): o pid dele não é reusado.
            // Só o líder que saiu do grupo recebe TERM/CONT direto (TERM duplicado vale como "segundo Ctrl+C").
            let escaped = getpgid(Some(leader_pid)).is_ok_and(|pgid| pgid != group);
            let _ = killpg(group, Signal::SIGTERM);
            if escaped {
                let _ = kill(leader_pid, Signal::SIGTERM);
            }
            let _ = killpg(group, Signal::SIGCONT);
            if escaped {
                let _ = kill(leader_pid, Signal::SIGCONT);
            }
            if events.recv_timeout(TERM_GRACE).is_err() {
                let _ = killpg(group, Signal::SIGKILL);
                // um líder fora do grupo não recebe o KILL do grupo; sem isto o recv() esperaria para sempre
                let _ = kill(leader_pid, Signal::SIGKILL);
                let _ = events.recv();
            }
        }
        // o guardião ignora `TERM`: só o `KILL` o tira, com o que sobrou no grupo
        let _ = killpg(group, Signal::SIGKILL);

        let status = leader.wait();
        let duration = started.elapsed();
        let _ = guardian.wait();
        let status = status
            .map_err(|e| error(RunErrorKind::Io, format!("the command was not reaped: {e}")))?;

        let deadline = Instant::now() + PIPE_GRACE;
        let (stdout, stdout_truncated) = pipes::collect(&stdout, deadline);
        let (stderr, stderr_truncated) = pipes::collect(&stderr, deadline);
        Ok(RunReport {
            exit_code: status.code(),
            stdout,
            stderr,
            timed_out,
            truncated: stdout_truncated || stderr_truncated,
            duration,
        })
    }

    fn spawn_error(program: &str, e: &io::Error) -> RunError {
        let kind = match e.kind() {
            ErrorKind::NotFound => RunErrorKind::NotFound,
            ErrorKind::PermissionDenied => RunErrorKind::PermissionDenied,
            _ => RunErrorKind::Spawn,
        };
        error(kind, format!("{program} did not start: {e}"))
    }

    pub fn sweep_in(root: &Path, project_root: Option<&Path>) -> Result<SweepReport, RunError> {
        let (sender, receiver) = mpsc::channel();
        let scan_root = root.to_path_buf();
        thread::spawn(move || {
            let _ = sender.send(scan(&scan_root));
        });
        let (candidates, unreadable) = match receiver.recv_timeout(SWEEP_SCAN_BUDGET) {
            Ok(scanned) => scanned.map_err(|e| {
                error(
                    RunErrorKind::Io,
                    format!("{} could not be listed: {e}", root.display()),
                )
            })?,
            Err(RecvTimeoutError::Timeout) => {
                return Ok(SweepReport {
                    swept: Vec::new(),
                    unreadable: 0,
                    scan_timed_out: true,
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(error(
                    RunErrorKind::Io,
                    "the sweep scan stopped without an answer".to_owned(),
                ));
            }
        };
        let own_pidns = own_pidns().ok_or_else(|| {
            error(
                RunErrorKind::Io,
                "/proc/self/ns/pid could not be read".to_owned(),
            )
        })?;
        let own_marker = std::env::var(RUN_MARKER_VAR).ok();

        let strays = select_strays(
            std::process::id(),
            own_marker.as_deref(),
            own_pidns,
            project_root.map(project_root_hash).as_deref(),
            &candidates,
            &|pid| live_stat(root, pid).map(|stat| stat.start),
            &|pgrp| {
                live_stat(root, pgrp).is_some()
                    && read_environ(root, pgrp).is_some_and(|env| marker_of(&env).is_none())
            },
        );
        Ok(SweepReport {
            swept: signal_batch(root, &strays),
            unreadable,
            scan_timed_out: false,
        })
    }

    /// Os processos com a marca no ambiente e quantos ambientes não se leram.
    fn scan(root: &Path) -> io::Result<(Vec<Candidate>, usize)> {
        let mut candidates = Vec::new();
        let mut unreadable = 0;
        for entry in fs::read_dir(root)?.flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .filter(|name| name.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|name| name.parse::<u32>().ok())
                .filter(|pid| SWEEPABLE_PIDS.contains(pid))
            else {
                continue;
            };
            let Some(environ) = read_environ(root, pid) else {
                unreadable += 1;
                continue;
            };
            let (Some(marker), Some(stat)) = (marker_of(&environ), read_stat(root, pid)) else {
                continue;
            };
            candidates.push(Candidate {
                pid,
                start: stat.start,
                pgrp: stat.pgrp,
                marker,
            });
        }
        Ok((candidates, unreadable))
    }

    fn proc_file(root: &Path, pid: u32, name: &str) -> PathBuf {
        root.join(pid.to_string()).join(name)
    }

    fn read_environ(root: &Path, pid: u32) -> Option<Vec<u8>> {
        fs::read(proc_file(root, pid, "environ")).ok()
    }

    fn marker_of(environ: &[u8]) -> Option<String> {
        let prefix = format!("{RUN_MARKER_VAR}=");
        environ
            .split(|byte| *byte == 0)
            .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
            .map(|value| String::from_utf8_lossy(value).into_owned())
    }

    fn read_stat(root: &Path, pid: u32) -> Option<ProcStat> {
        let text = fs::read_to_string(proc_file(root, pid, "stat")).ok()?;
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        let mut fields = text.get(close + 1..)?.split_whitespace();
        let state = fields.next()?.chars().next()?;
        let pgrp = fields.nth(1)?.parse().ok()?;
        Some(ProcStat {
            comm: text.get(open + 1..close)?.to_owned(),
            state,
            pgrp,
            start: start_from_stat(&text)?,
        })
    }

    /// O `stat` de um processo vivo; zumbi conta como morto.
    fn live_stat(root: &Path, pid: u32) -> Option<ProcStat> {
        read_stat(root, pid).filter(|stat| stat.state != 'Z')
    }

    /// O alvo ainda vivo e com o início lido na varredura (o pid não foi reusado).
    fn same_target(root: &Path, stray: &Candidate) -> Option<ProcStat> {
        live_stat(root, stray.pid).filter(|stat| stat.start == stray.start)
    }

    fn signal(root: &Path, stray: &Candidate, signal: Signal) -> bool {
        same_target(root, stray).is_some() && kill(Pid::from_raw(stray.pid as i32), signal).is_ok()
    }

    /// `TERM` e `CONT` a todos, uma só carência e `KILL` a quem sobrou: no máximo um `TERM_GRACE`.
    fn signal_batch(root: &Path, strays: &[Candidate]) -> Vec<SweptProcess> {
        let mut swept = Vec::new();
        let mut batch = Vec::new();
        for stray in strays {
            let Some(stat) = same_target(root, stray) else {
                continue;
            };
            if kill(Pid::from_raw(stray.pid as i32), Signal::SIGTERM).is_ok() {
                swept.push(SweptProcess {
                    pid: stray.pid,
                    comm: stat.comm,
                    owner_pid: RunMarker::parse(&stray.marker).map_or(0, |marker| marker.pid),
                });
                batch.push(stray);
            }
        }
        batch.retain(|stray| signal(root, stray, Signal::SIGCONT));

        let deadline = Instant::now() + TERM_GRACE;
        while Instant::now() < deadline
            && batch.iter().any(|stray| same_target(root, stray).is_some())
        {
            thread::sleep(POLL);
        }
        for stray in batch {
            signal(root, stray, Signal::SIGKILL);
        }
        swept
    }
}

#[cfg(windows)]
mod platform {
    //! Port para o Claude desktop no Windows: um Job Object por execução, com
    //! KILL_ON_JOB_CLOSE. Fechar o handle do job (drop) mata o comando e tudo o que
    //! ele abriu, inclusive se o `omhl` morrer. Sem guardião, sem sweep e sem `ParentGone`.
    use std::io::{self, ErrorKind};
    use std::os::windows::io::AsRawHandle;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::thread;
    use std::time::Instant;

    use win32job::{ExtendedLimitInfo, Job};

    use super::SweepReport;
    use crate::ports::pipes::{self, PIPE_GRACE};
    use crate::runner::{RunError, RunErrorKind, RunReport, RunSpec};

    const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

    fn error(kind: RunErrorKind, message: String) -> RunError {
        RunError { kind, message }
    }

    fn spawn_error(program: &str, e: &io::Error) -> RunError {
        let kind = match e.kind() {
            ErrorKind::NotFound => RunErrorKind::NotFound,
            ErrorKind::PermissionDenied => RunErrorKind::PermissionDenied,
            _ => RunErrorKind::Spawn,
        };
        error(kind, format!("{program} did not start: {e}"))
    }

    fn env_value(spec: &RunSpec, key: &str) -> Option<std::ffi::OsString> {
        spec.env_set
            .iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| std::ffi::OsString::from(v))
            .or_else(|| std::env::var_os(key))
    }

    fn with_extensions(base: &Path, exts: &[String]) -> Option<PathBuf> {
        if base.extension().is_some() && base.is_file() {
            return Some(base.to_path_buf());
        }
        exts.iter().find_map(|ext| {
            let mut name = base.as_os_str().to_os_string();
            name.push(ext);
            let candidate = PathBuf::from(name);
            candidate.is_file().then_some(candidate)
        })
    }

    pub fn resolve_program(spec: &RunSpec) -> Option<PathBuf> {
        let program = spec.argv.first()?;
        let exts: Vec<String> = env_value(spec, "PATHEXT")
            .and_then(|v| v.into_string().ok())
            .unwrap_or_else(|| DEFAULT_PATHEXT.to_owned())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect();
        if program.contains('/') || program.contains('\\') {
            return with_extensions(&spec.cwd.join(program), &exts);
        }
        let path_var = env_value(spec, "PATH")?;
        std::env::split_paths(&path_var)
            .find_map(|dir| with_extensions(&spec.cwd.join(dir).join(program), &exts))
    }

    pub fn run(_parent_pid: Option<u32>, spec: &RunSpec) -> Result<RunReport, RunError> {
        let Some((program, args)) = spec.argv.split_first() else {
            return Err(error(RunErrorKind::Spawn, "the argv is empty".to_owned()));
        };
        let resolved = resolve_program(spec).unwrap_or_else(|| PathBuf::from(program));

        let mut info = ExtendedLimitInfo::new();
        info.limit_kill_on_job_close();
        let job = Job::create_with_limit_info(&mut info).map_err(|e| {
            error(
                RunErrorKind::Spawn,
                format!("the job object was not created: {e}"),
            )
        })?;

        let mut command = Command::new(&resolved);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&spec.cwd);
        for (key, value) in &spec.env_set {
            command.env(key, value);
        }
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        command.env("NO_COLOR", "1").env("LC_ALL", "C");

        let started = Instant::now();
        let mut child = command.spawn().map_err(|e| spawn_error(program, &e))?;
        // A janela entre o spawn e o assign é aceita no protótipo.
        let handle = child.as_raw_handle() as isize;
        if let Err(e) = job.assign_process(handle) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(
                RunErrorKind::Spawn,
                format!("the command could not join the job object: {e}"),
            ));
        }

        let stdout = pipes::spawn_reader(
            child.stdout.take().expect("the stdout of the command is piped"),
            spec.max_stream_bytes,
        );
        let stderr = pipes::spawn_reader(
            child.stderr.take().expect("the stderr of the command is piped"),
            spec.max_stream_bytes,
        );
        let (sender, statuses) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(child.wait());
        });

        let first = statuses.recv_timeout(spec.timeout);
        let timed_out = matches!(first, Err(RecvTimeoutError::Timeout));
        // Fecha o job: no timeout mata o líder; nos dois casos mata o que sobrou na árvore.
        drop(job);
        let status = match first {
            Ok(status) => status,
            Err(_) => statuses.recv().map_err(|_| {
                error(
                    RunErrorKind::Io,
                    "the command was not reaped".to_owned(),
                )
            })?,
        };
        let duration = started.elapsed();
        let status = status
            .map_err(|e| error(RunErrorKind::Io, format!("the command was not reaped: {e}")))?;

        let deadline = Instant::now() + PIPE_GRACE;
        let (stdout, stdout_truncated) = pipes::collect(&stdout, deadline);
        let (stderr, stderr_truncated) = pipes::collect(&stderr, deadline);
        Ok(RunReport {
            exit_code: status.code(),
            stdout,
            stderr,
            timed_out,
            truncated: stdout_truncated || stderr_truncated,
            duration,
        })
    }

    pub fn sweep_in(_root: &Path, _project_root: Option<&Path>) -> Result<SweepReport, RunError> {
        Ok(SweepReport {
            swept: Vec::new(),
            unreadable: 0,
            scan_timed_out: false,
        })
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use std::path::{Path, PathBuf};

    use super::SweepReport;
    use crate::runner::{RunError, RunErrorKind, RunReport, RunSpec};

    pub fn resolve_program(_spec: &RunSpec) -> Option<PathBuf> {
        None
    }

    pub fn run(_parent_pid: Option<u32>, _spec: &RunSpec) -> Result<RunReport, RunError> {
        Err(RunError {
            kind: RunErrorKind::UnsupportedPlatform,
            message: "running commands is supported only on Linux".to_owned(),
        })
    }

    pub fn sweep_in(_root: &Path, _project_root: Option<&Path>) -> Result<SweepReport, RunError> {
        Err(RunError {
            kind: RunErrorKind::UnsupportedPlatform,
            message: "sweeping processes is supported only on Linux".to_owned(),
        })
    }
}
