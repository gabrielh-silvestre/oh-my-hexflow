#![allow(dead_code)]

use std::fs;
use std::io::ErrorKind;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use ::omhl::ports::process::ProcessRunner;
use ::omhl::runner::{CommandRunner, RunErrorKind, RunReport, RunSpec};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};

/// Nome do `#[test]` auxiliar de cada binário; o `spawn_role` o relança com `--exact`.
pub const ROLE_TEST: &str = "helper_role";

/// Começo de `/proc/<G>/cmdline` do guardião.
pub const GUARDIAN_PREFIX: &[u8] = b"sh\0-c\0trap \"\" INT TERM HUP QUIT TTIN TTOU;";

/// HUP, INT, QUIT, TERM, TTIN e TTOU no `SigIgn` do guardião armado.
pub const GUARDIAN_IGNORED: u64 = 0x304007;

const POLL: Duration = Duration::from_millis(10);
const FILE_WAIT: Duration = Duration::from_secs(20);

pub fn spec(argv: &[&str], cwd: &Path, timeout: Duration) -> RunSpec {
    RunSpec {
        argv: argv.iter().map(|item| (*item).to_owned()).collect(),
        cwd: cwd.to_path_buf(),
        project_root: cwd.to_path_buf(),
        timeout,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        max_stream_bytes: 4_194_304,
    }
}

pub fn run_ok(runner: &ProcessRunner, spec: &RunSpec) -> RunReport {
    match runner.run(spec) {
        Ok(report) => report,
        Err(e) => panic!("run failed: {}", kind_name(&e.kind)),
    }
}

pub fn kind_name(kind: &RunErrorKind) -> &'static str {
    match kind {
        RunErrorKind::NotFound => "not-found",
        RunErrorKind::PermissionDenied => "permission-denied",
        RunErrorKind::Spawn => "spawn",
        RunErrorKind::Io => "io",
        RunErrorKind::UnsupportedPlatform => "unsupported-platform",
        RunErrorKind::ParentGone => "parent-gone",
    }
}

/// O texto do `sh -c` de cada comando dos testes de interrupção e de varredura.
pub fn command_line(name: &str) -> &'static str {
    match name {
        "plain" => {
            "echo $$ > cmd.pid.tmp; mv cmd.pid.tmp cmd.pid; sleep 61 & echo $! > neto.pid.tmp; mv neto.pid.tmp neto.pid; wait"
        }
        "coop" => {
            "trap 'kill -s TERM -- -$g; exit 0' TERM INT; echo $$ > cmd.pid.tmp; mv cmd.pid.tmp cmd.pid; setsid sh -c 'echo $$ > neto.pid.tmp; mv neto.pid.tmp neto.pid; exec sleep 61' & g=$!; while [ ! -e neto.pid ]; do sleep 0.01; done; : > armed; wait"
        }
        "ignore-term" => {
            "trap '' TERM; echo $$ > cmd.pid.tmp; mv cmd.pid.tmp cmd.pid; sleep 61 & echo $! > neto.pid.tmp; mv neto.pid.tmp neto.pid; wait"
        }
        "flock" => {
            "exec 9>lock; flock -x 9; echo $$ > cmd.pid.tmp; mv cmd.pid.tmp cmd.pid; sleep 61 & echo $! > neto.pid.tmp; mv neto.pid.tmp neto.pid; wait"
        }
        "detached" => {
            "setsid sh -c 'echo $$ > neto.pid.tmp; mv neto.pid.tmp neto.pid; exec sleep 61' & while [ ! -e neto.pid ]; do sleep 0.01; done; echo done"
        }
        "detached-ignore-term" => {
            "trap '' TERM; setsid sh -c 'echo $$ > neto.pid.tmp; mv neto.pid.tmp neto.pid; exec sleep 61' & while [ ! -e neto.pid ]; do sleep 0.01; done; echo done"
        }
        other => panic!("unknown test command: {other}"),
    }
}

/// Relança o binário de teste no papel dado, num grupo de processos próprio.
///
/// Passa por `env --default-signal=INT`: se o processo de teste herdou `SIGINT` ignorado
/// (`cmd &`, `nohup`), o papel herdaria o mesmo e descartaria o `killpg(SIGINT)` dos testes.
/// O `env` faz `exec`, então o pid do filho é o do papel. `pre_exec` exigiria `unsafe`.
pub fn spawn_role(
    role: &str,
    dir: &Path,
    cmd: &str,
    extra: &[(&str, &str)],
    stderr: Stdio,
) -> Child {
    let mut command = Command::new("env");
    command
        .arg("--default-signal=INT")
        .arg(std::env::current_exe().expect("the test binary path is known"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env("OMHL_TEST_ROLE", role)
        .env("OMHL_TEST_DIR", dir)
        .env("OMHL_TEST_CMD", cmd);
    for (key, value) in extra {
        command.env(key, value);
    }
    command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .expect("the role process starts")
}

/// Corpo do `#[test]` auxiliar: sem `OMHL_TEST_ROLE` volta na hora.
pub fn helper_role() {
    let Some(role) = std::env::var_os("OMHL_TEST_ROLE") else {
        return;
    };
    match role.to_str() {
        Some("runner") => runner_role(),
        Some("parent") => parent_role(),
        Some("reaper") => reaper_role(),
        other => panic!("unknown test role: {other:?}"),
    }
}

fn runner_role() {
    // a primeira leitura do papel: o pai que o binário amarrado vê ao nascer
    let parent = std::os::unix::process::parent_id();
    let dir = role_dir();
    write_atomic(&dir, "runner.pid", &std::process::id().to_string());
    let runner = if flag("OMHL_TEST_BOUND") {
        ProcessRunner::bound_to_parent(parent)
    } else {
        ProcessRunner::new()
    };
    if flag("OMHL_TEST_GO_BEFORE_RUN") {
        write_atomic(&dir, "ready", "");
        wait_file(&dir.join("go"), FILE_WAIT);
    }
    let name = std::env::var("OMHL_TEST_CMD").expect("OMHL_TEST_CMD is set");
    let timeout = std::env::var("OMHL_TEST_TIMEOUT_SECS")
        .map(|secs| secs.parse().expect("OMHL_TEST_TIMEOUT_SECS is a number"))
        .unwrap_or(60);

    let result = runner.run(&spec(
        &["sh", "-c", command_line(&name)],
        &dir,
        Duration::from_secs(timeout),
    ));

    let line = match result {
        Ok(report) => format!(
            "ok exit={} timed_out={} truncated={}",
            report
                .exit_code
                .map_or_else(|| "none".to_owned(), |code| code.to_string()),
            report.timed_out,
            report.truncated
        ),
        Err(e) => format!("err {}", kind_name(&e.kind)),
    };
    write_atomic(&dir, "result", &line);
    if flag("OMHL_TEST_GO_AFTER_RUN") {
        wait_file(&dir.join("go"), Duration::from_secs(60));
    }
}

fn parent_role() {
    let mut runner = Command::new(std::env::current_exe().expect("the test binary path is known"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env("OMHL_TEST_ROLE", "runner")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the runner role starts");
    runner.wait().expect("the runner role is reaped");
}

fn reaper_role() {
    nix::sys::prctl::set_child_subreaper(true).expect("the reaper becomes a subreaper");
    let dir = role_dir();
    let mut runner = Command::new(std::env::current_exe().expect("the test binary path is known"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env("OMHL_TEST_ROLE", "runner")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the runner role starts");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !dir.join("done").exists() && Instant::now() < deadline {
        while let Ok(status) = waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            if status == WaitStatus::StillAlive {
                break;
            }
        }
        thread::sleep(POLL);
    }
    // o waitpid acima já colheu o runner com os órfãos adotados; aqui o wait só devolve ECHILD
    let _ = runner.wait();
}

fn role_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("OMHL_TEST_DIR").expect("OMHL_TEST_DIR is set"))
}

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1")
}

fn write_atomic(dir: &Path, name: &str, content: &str) {
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, content).expect("the temporary file is written");
    fs::rename(&tmp, dir.join(name)).expect("the temporary file is moved into place");
}

/// Espera ativa até o arquivo existir; devolve o conteúdo sem o `\n` final.
pub fn wait_file(path: &Path, limit: Duration) -> String {
    let deadline = Instant::now() + limit;
    loop {
        match fs::read_to_string(path) {
            Ok(content) => return content.strip_suffix('\n').unwrap_or(&content).to_owned(),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => panic!("{} could not be read: {e}", path.display()),
        }
        if Instant::now() >= deadline {
            panic!("{} did not appear within {limit:?}", path.display());
        }
        thread::sleep(POLL);
    }
}

/// O pid gravado em `<dir>/<name>`, esperado por até 20 s.
pub fn pid_file(dir: &Path, name: &str) -> u32 {
    let text = wait_file(&dir.join(name), FILE_WAIT);
    text.parse()
        .unwrap_or_else(|_| panic!("{name} holds a pid, got {text:?}"))
}

fn stat_tail(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = stat.rfind(')')?;
    Some(stat[close + 1..].to_owned())
}

/// Estado em `/proc/<pid>/stat`, depois do último `)`; `None` se o processo não existe.
pub fn proc_state(pid: u32) -> Option<char> {
    stat_tail(pid)?.trim_start().chars().next()
}

pub fn is_dead(pid: u32) -> bool {
    matches!(proc_state(pid), None | Some('Z'))
}

/// Espera ativa; devolve os pids ainda vivos ao fim do prazo.
pub fn wait_dead(pids: &[u32], limit: Duration) -> Vec<u32> {
    let deadline = Instant::now() + limit;
    loop {
        let alive: Vec<u32> = pids.iter().copied().filter(|pid| !is_dead(*pid)).collect();
        if alive.is_empty() || Instant::now() >= deadline {
            return alive;
        }
        thread::sleep(POLL);
    }
}

/// Falha com os pids vivos e o estado de cada um se algum não morreu até `since + limit`.
pub fn assert_dead_since(pids: &[u32], since: Instant, limit: Duration) {
    let alive = wait_dead(
        pids,
        (since + limit).saturating_duration_since(Instant::now()),
    );

    let states: Vec<(u32, Option<char>)> =
        alive.iter().map(|pid| (*pid, proc_state(*pid))).collect();
    assert!(
        alive.is_empty(),
        "still alive {limit:?} after the signal (pid, state): {states:?}"
    );
}

/// Campo `n` (`n >= 3`) de `/proc/<pid>/stat`, contado depois do último `)`.
pub fn stat_field(pid: u32, n: usize) -> u64 {
    let tail = stat_tail(pid).unwrap_or_else(|| panic!("/proc/{pid}/stat is readable"));
    let field = tail
        .split_whitespace()
        .nth(n - 3)
        .unwrap_or_else(|| panic!("/proc/{pid}/stat has field {n}"));
    field
        .parse()
        .unwrap_or_else(|_| panic!("field {n} of /proc/{pid}/stat is a number, got {field:?}"))
}

pub fn wait_child_exit(child: &mut Child, limit: Duration) -> ExitStatus {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().expect("the child is polled") {
            return status;
        }
        if Instant::now() >= deadline {
            panic!("child {} did not exit within {limit:?}", child.id());
        }
        thread::sleep(POLL);
    }
}

/// Espera ativa até `ready()`; estourado o prazo, falha com `what`.
pub fn wait_until(limit: Duration, what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !ready() {
        if Instant::now() >= deadline {
            panic!("{what} did not happen within {limit:?}");
        }
        thread::sleep(POLL);
    }
}

fn ignored_signals(pid: u32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix("SigIgn:"))?;
    u64::from_str_radix(line.trim(), 16).ok()
}

/// Espera o `trap ""` do guardião rodar: `SigIgn` contém `GUARDIAN_IGNORED`.
pub fn wait_guardian_armed(g: u32, limit: Duration) {
    wait_until(
        limit,
        &format!("the guardian {g} ignoring its signals"),
        || ignored_signals(g).is_some_and(|mask| mask & GUARDIAN_IGNORED == GUARDIAN_IGNORED),
    );
}

/// O guardião do comando `c` lançado pelo runner `r` (campo 5 do `stat` do comando), conferidas
/// as precondições: fora do grupo do runner, líder do próprio grupo e com o `cmdline` do guardião.
pub fn guardian_of(r: u32, c: u32) -> u32 {
    let g = u32::try_from(stat_field(c, 5)).expect("a pgrp fits in u32");

    assert_ne!(g, r, "the command must not run in the group of the runner");
    assert_ne!(g, c, "the command must not lead its own group");
    assert_eq!(
        stat_field(g, 5),
        u64::from(g),
        "the guardian leads its own group"
    );
    let cmdline = fs::read(format!("/proc/{g}/cmdline")).expect("the guardian cmdline is readable");
    assert!(
        cmdline.starts_with(GUARDIAN_PREFIX),
        "process {g} is not the guardian: {:?}",
        String::from_utf8_lossy(&cmdline)
    );
    g
}
