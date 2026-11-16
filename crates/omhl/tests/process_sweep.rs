#![cfg(target_os = "linux")]

mod proc_roles;

use std::fs::{self, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ::omhl::mechanical::sweep::root_hash;
use ::omhl::ports::process::{
    ProcessRunner, SweepReport, sweep_project_strays, sweep_strays, sweep_strays_in,
};
use ::omhl::runner::{CommandRunner, RunError, RunSpec};
use nix::errno::Errno;
use nix::fcntl::{Flock, FlockArg};
use nix::sys::signal::{Signal, kill};
use nix::sys::stat::Mode;
use nix::unistd::{Pid, mkfifo};
use proc_roles::{
    GUARDIAN_PREFIX, assert_dead_since, guardian_of, is_dead, kind_name, pid_file, proc_state,
    run_ok, spawn_role, spec, stat_field, wait_child_exit, wait_file, wait_until,
};
use tempfile::TempDir;

const TWENTY_SECS: Duration = Duration::from_secs(20);
const DEATH: Duration = Duration::from_secs(4);

/// `sweep_strays()` varre o `/proc` inteiro: em paralelo, um teste mataria o neto de outro.
fn serialize_sweeps() -> Flock<fs::File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::env::temp_dir().join("omhl-process-sweep.lock"))
        .expect("the sweep lock file opens");
    Flock::lock(file, FlockArg::LockExclusive)
        .unwrap_or_else(|(_, errno)| panic!("the sweep lock is taken: {errno}"))
}

fn pid(raw: u32) -> Pid {
    Pid::from_raw(i32::try_from(raw).expect("a pid fits in i32"))
}

fn sweep_report(result: Result<SweepReport, RunError>) -> SweepReport {
    match result {
        Ok(report) => report,
        Err(e) => panic!("sweep failed: {}", kind_name(&e.kind)),
    }
}

fn environ(pid: u32) -> Vec<Vec<u8>> {
    fs::read(format!("/proc/{pid}/environ"))
        .unwrap_or_else(|e| panic!("/proc/{pid}/environ is readable: {e}"))
        .split(|byte| *byte == 0)
        .map(<[u8]>::to_vec)
        .collect()
}

fn run_marker_of(pid: u32) -> Option<String> {
    environ(pid).into_iter().find_map(|entry| {
        entry
            .strip_prefix(b"OMHL_RUN_ID=")
            .map(|value| String::from_utf8_lossy(value).into_owned())
    })
}

/// Lança o papel `runner` com o comando dado, colhe o runner e devolve o neto, conferido que ele
/// continua vivo e leva a marca do runner.
fn stray_of_a_gone_runner(dir: &Path, cmd: &str) -> u32 {
    let mut runner = spawn_role("runner", dir, cmd, &[], Stdio::null());
    let r = runner.id();
    wait_child_exit(&mut runner, TWENTY_SECS);
    let n = pid_file(dir, "neto.pid");
    assert!(!is_dead(n), "precondition: the grandchild is alive");
    let marker = run_marker_of(n).unwrap_or_default();
    assert!(
        marker.starts_with(&format!("v1:{r}:")),
        "precondition: the grandchild carries the marker of the runner {r}, got {marker:?}"
    );
    n
}

#[test]
fn helper_role() {
    proc_roles::helper_role();
}

#[test]
fn it_should_put_the_run_marker_of_the_caller_in_the_command_environment() {
    let dir = TempDir::new().expect("work directory is created");
    let mut run_spec = spec(
        &["sh", "-c", "printf %s \"$OMHL_RUN_ID\""],
        dir.path(),
        TWENTY_SECS,
    );
    run_spec.env_set = vec![("OMHL_RUN_ID".to_owned(), "v1:1:1:1".to_owned())];
    let start = stat_field(std::process::id(), 22);
    let namespace = fs::read_link("/proc/self/ns/pid").expect("the pid namespace link is readable");
    let namespace = namespace.to_str().expect("the namespace link is UTF-8");
    let inode = namespace
        .strip_prefix("pid:[")
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("unexpected namespace link {namespace:?}"));

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert_eq!(
        String::from_utf8_lossy(&report.stdout),
        format!(
            "v1:{}:{}:{}:{}",
            std::process::id(),
            start,
            inode,
            root_hash(dir.path())
        )
    );
}

fn marker_root_of(run_spec: &RunSpec) -> String {
    let report = run_ok(&ProcessRunner::new(), run_spec);
    let marker = String::from_utf8(report.stdout).expect("the marker is UTF-8");

    marker.rsplit(':').next().unwrap_or_default().to_owned()
}

fn print_marker(cwd: &Path, project_root: &Path) -> RunSpec {
    let mut run_spec = spec(
        &["sh", "-c", "printf %s \"$OMHL_RUN_ID\""],
        cwd,
        TWENTY_SECS,
    );
    run_spec.project_root = project_root.to_path_buf();

    run_spec
}

#[test]
fn it_should_hash_the_project_root_and_not_the_cwd_of_the_check() {
    let project = TempDir::new().expect("project directory is created");
    let own_cwd = project.path().join("sub");
    fs::create_dir(&own_cwd).expect("the check cwd is created");

    let root = marker_root_of(&print_marker(&own_cwd, project.path()));

    let canonical = fs::canonicalize(project.path()).expect("the project is canonical");
    assert_eq!(root, root_hash(&canonical));
}

#[test]
fn it_should_hash_the_same_root_through_a_symlink_or_a_trailing_slash() {
    let project = TempDir::new().expect("project directory is created");
    let links = TempDir::new().expect("links directory is created");
    let link = links.path().join("project-link");
    symlink(project.path(), &link).expect("the symlink is created");
    let trailing = project.path().join("");
    let canonical = fs::canonicalize(project.path()).expect("the project is canonical");

    let through_link = marker_root_of(&print_marker(project.path(), &link));
    let through_slash = marker_root_of(&print_marker(project.path(), &trailing));

    assert_eq!(through_link, root_hash(&canonical));
    assert_eq!(through_slash, root_hash(&canonical));
}

#[test]
fn it_should_sweep_the_stray_when_the_given_root_is_a_symlink_to_the_project() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let links = TempDir::new().expect("links directory is created");
    let link = links.path().join("project-link");
    symlink(dir.path(), &link).expect("the symlink is created");
    let n = stray_of_a_gone_runner(dir.path(), "detached");

    let called = Instant::now();
    let result = sweep_project_strays(&link);

    assert!(result.is_ok(), "the sweep fails");
    assert_dead_since(&[n], called, DEATH);
}

#[test]
fn it_should_leave_the_run_marker_out_of_the_guardian_environment() {
    let dir = TempDir::new().expect("work directory is created");
    let cwd = dir.path().to_path_buf();
    let (report_tx, report_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = ProcessRunner::new().run(&spec(
            &[
                "sh",
                "-c",
                "cut -d' ' -f5 /proc/$$/stat > pgrp.tmp; mv pgrp.tmp pgrp; while [ ! -e go ]; do sleep 0.05; done",
            ],
            &cwd,
            Duration::from_secs(60),
        ));
        let _ = report_tx.send(result);
    });
    let g: u32 = wait_file(&dir.path().join("pgrp"), TWENTY_SECS)
        .parse()
        .expect("the pgrp is a number");

    let cmdline = fs::read(format!("/proc/{g}/cmdline")).expect("the guardian cmdline is readable");
    let entries = environ(g);

    assert!(
        cmdline.starts_with(GUARDIAN_PREFIX),
        "process {g} is not the guardian: {:?}",
        String::from_utf8_lossy(&cmdline)
    );
    assert!(
        !entries
            .iter()
            .any(|entry| entry.starts_with(b"OMHL_RUN_ID=")),
        "the guardian environment carries OMHL_RUN_ID"
    );
    assert!(
        entries.iter().any(|entry| entry.starts_with(b"PATH=")),
        "the guardian environment lost PATH"
    );
    fs::write(dir.path().join("go"), "").expect("the go file is written");
    let report = match report_rx
        .recv_timeout(TWENTY_SECS)
        .expect("the run returns")
    {
        Ok(report) => report,
        Err(e) => panic!("run failed: {}", kind_name(&e.kind)),
    };
    assert_eq!(report.exit_code, Some(0));
}

#[test]
fn it_should_sweep_a_detached_descendant_whose_runner_is_gone() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let n = stray_of_a_gone_runner(dir.path(), "detached");

    let called = Instant::now();
    let result = sweep_strays();

    assert!(result.is_ok(), "the sweep fails");
    assert_dead_since(&[n], called, DEATH);
}

#[test]
fn it_should_sweep_the_stray_of_the_project_whose_root_is_given() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let n = stray_of_a_gone_runner(dir.path(), "detached");

    let called = Instant::now();
    let result = sweep_project_strays(dir.path());

    assert!(result.is_ok(), "the sweep fails");
    assert_dead_since(&[n], called, DEATH);
}

#[test]
fn it_should_not_sweep_the_detached_descendant_of_a_live_runner() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role(
        "runner",
        dir.path(),
        "detached",
        &[("OMHL_TEST_GO_AFTER_RUN", "1")],
        Stdio::null(),
    );
    wait_file(&dir.path().join("result"), TWENTY_SECS);
    let n = pid_file(dir.path(), "neto.pid");

    let result = sweep_strays();

    assert!(result.is_ok(), "the sweep fails");
    assert!(
        !is_dead(n),
        "the sweep killed the grandchild of a live runner"
    );
    thread::sleep(Duration::from_millis(500));
    assert!(
        !is_dead(n),
        "the grandchild of a live runner died after the sweep"
    );
    fs::write(dir.path().join("go"), "").expect("the go file is written");
    assert_eq!(wait_child_exit(&mut runner, TWENTY_SECS).code(), Some(0));
    // outra varredura pode ter pego o neto depois que o runner saiu
    if let Err(e) = kill(pid(n), Signal::SIGKILL) {
        assert_eq!(e, Errno::ESRCH, "the grandchild cleanup fails");
    }
}

#[test]
fn it_should_sweep_a_process_that_ignores_sigterm_by_killing_it_after_the_grace_period() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let n = stray_of_a_gone_runner(dir.path(), "detached-ignore-term");

    let called = Instant::now();
    let result = sweep_strays();

    assert!(result.is_ok(), "the sweep fails");
    assert_dead_since(&[n], called, Duration::from_secs(5));
}

#[test]
fn it_should_not_sweep_the_command_of_a_gone_runner_while_its_guardian_is_alive() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "ignore-term", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);
    let killed = Instant::now();
    kill(pid(r), Signal::SIGKILL).expect("the runner is signaled");
    wait_child_exit(&mut runner, DEATH);

    let result = sweep_strays();

    let returned = Instant::now();
    let report = sweep_report(result);
    let swept: Vec<u32> = report.swept.iter().map(|process| process.pid).collect();
    assert!(
        !swept.contains(&c) && !swept.contains(&n),
        "the sweep took the command {c} or the grandchild {n}: {swept:?}"
    );
    // o KILL do guardião vem 2 s depois do EOF; depois de 1,5 s o grupo pode já estar morrendo
    if returned < killed + Duration::from_millis(1500) {
        assert!(!is_dead(c), "the command died during the sweep");
        assert!(!is_dead(n), "the grandchild died during the sweep");
    }
    assert_dead_since(&[g, c, n], killed, DEATH);
}

#[test]
fn it_should_sweep_a_stray_whose_runner_is_a_zombie() {
    let _serial = serialize_sweeps();
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "detached", &[], Stdio::null());
    let r = runner.id();
    wait_until(TWENTY_SECS, "the runner becoming a zombie", || {
        proc_state(r) == Some('Z')
    });
    let n = pid_file(dir.path(), "neto.pid");
    assert!(!is_dead(n), "precondition: the grandchild is alive");

    let called = Instant::now();
    let result = sweep_strays();

    assert!(result.is_ok(), "the sweep fails");
    assert_dead_since(&[n], called, DEATH);
    wait_child_exit(&mut runner, DEATH);
}

#[test]
fn it_should_give_up_the_sweep_scan_after_its_budget_without_signaling_anything() {
    let root = TempDir::new().expect("proc root is created");
    let process = root.path().join("4242");
    fs::create_dir(&process).expect("the process directory is created");
    fs::write(
        process.join("stat"),
        "4242 (sleep) S 1 4242 4242 0 -1 4194560 100 0 0 0 0 0 0 0 20 0 1 0 987654 1000 10\n",
    )
    .expect("the stat file is written");
    let environ = process.join("environ");
    mkfifo(&environ, Mode::from_bits_truncate(0o600)).expect("the environ FIFO is created");
    let started = Instant::now();

    let result = sweep_strays_in(root.path(), None);

    let elapsed = started.elapsed();
    // abre o FIFO para escrita sem bloquear (0o4000 = O_NONBLOCK) e solta a leitura presa
    if let Err(e) = OpenOptions::new()
        .write(true)
        .custom_flags(0o4000)
        .open(&environ)
    {
        assert_eq!(e.raw_os_error(), Some(6), "the FIFO cleanup fails: {e}");
    }
    assert!(
        elapsed < Duration::from_secs(2),
        "the scan took {elapsed:?}"
    );
    let report = sweep_report(result);
    assert!(report.scan_timed_out);
    assert!(report.swept.is_empty());
}
