use std::path::Path;

use crate::runner::{CommandRunner, RunError, RunReport, RunSpec};

pub struct ProcessRunner {
    /// `RunMarker` deste processo, lida uma vez; `None` se o `/proc` não se leu.
    marker: Option<String>,
    parent_pid: Option<u32>,
}

impl ProcessRunner {
    /// Sem amarração ao processo pai; para os testes.
    pub fn new() -> ProcessRunner {
        ProcessRunner {
            marker: platform::own_marker(),
            parent_pid: None,
        }
    }

    /// `parent_pid` é o `parent_id()` que o `main` leu na primeira linha.
    pub fn bound_to_parent(parent_pid: u32) -> ProcessRunner {
        ProcessRunner {
            marker: platform::own_marker(),
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
        platform::run(self.marker.as_deref(), self.parent_pid, spec)
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

pub fn sweep_strays() -> Result<SweepReport, RunError> {
    sweep_strays_in(Path::new("/proc"))
}

pub fn sweep_strays_in(proc_root: &Path) -> Result<SweepReport, RunError> {
    platform::sweep_in(proc_root)
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
    use nix::sys::wait::{Id, WaitPidFlag, waitid};
    use nix::unistd::{Pid, getpgid};

    use super::{SweepReport, SweptProcess};
    use crate::mechanical::sweep::{
        Candidate, RUN_MARKER_VAR, RunMarker, select_strays, start_from_stat,
    };
    use crate::ports::pipes::{self, PIPE_GRACE};
    use crate::runner::{RunError, RunErrorKind, RunReport, RunSpec};

    const TERM_GRACE: Duration = Duration::from_secs(2);
    const SWEEP_SCAN_BUDGET: Duration = Duration::from_secs(1);
    const POLL: Duration = Duration::from_millis(10);

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

    pub fn own_marker() -> Option<String> {
        let stat = fs::read_to_string("/proc/self/stat").ok()?;
        let marker = RunMarker {
            pid: std::process::id(),
            start: start_from_stat(&stat)?,
            pidns: own_pidns()?,
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

    pub fn run(
        marker: Option<&str>,
        parent_pid: Option<u32>,
        spec: &RunSpec,
    ) -> Result<RunReport, RunError> {
        let Some(marker) = marker else {
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

        if !matches!(guardian.try_wait(), Ok(None)) {
            // sem guardião, o comando ficaria sem quem o limpe se este processo morrer
            let _ = killpg(group, Signal::SIGKILL);
            let _ = leader.wait();
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

    pub fn sweep_in(root: &Path) -> Result<SweepReport, RunError> {
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
                .and_then(|name| name.parse().ok())
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

#[cfg(not(target_os = "linux"))]
mod platform {
    use std::path::Path;

    use super::SweepReport;
    use crate::runner::{RunError, RunErrorKind, RunReport, RunSpec};

    fn unsupported() -> RunError {
        RunError {
            kind: RunErrorKind::UnsupportedPlatform,
            message: "running commands is supported only on Linux".to_owned(),
        }
    }

    pub fn own_marker() -> Option<String> {
        None
    }

    pub fn run(
        _marker: Option<&str>,
        _parent_pid: Option<u32>,
        _spec: &RunSpec,
    ) -> Result<RunReport, RunError> {
        Err(unsupported())
    }

    pub fn sweep_in(_root: &Path) -> Result<SweepReport, RunError> {
        Err(unsupported())
    }
}
