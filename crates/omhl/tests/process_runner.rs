#![cfg(target_os = "linux")]

mod proc_roles;

use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ::omhl::ports::process::ProcessRunner;
use ::omhl::runner::{CommandRunner, RunErrorKind};
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;
use proc_roles::{
    assert_dead_since, command_line, guardian_of, is_dead, kind_name, pid_file, proc_state, run_ok,
    spawn_role, spec, stat_field, wait_child_exit, wait_file, wait_guardian_armed, wait_until,
};
use tempfile::TempDir;

const TWENTY_SECS: Duration = Duration::from_secs(20);
const DEATH: Duration = Duration::from_secs(4);

fn pid(raw: u32) -> Pid {
    Pid::from_raw(i32::try_from(raw).expect("a pid fits in i32"))
}

fn flock_exit_code(lock: &Path) -> Option<i32> {
    Command::new("flock")
        .args(["-n", "-x"])
        .arg(lock)
        .args(["-c", "true"])
        .status()
        .expect("flock starts")
        .code()
}

#[test]
fn helper_role() {
    proc_roles::helper_role();
}

// Execução comum (1 a 16)

#[test]
fn it_should_run_an_argv_without_a_shell() {
    let dir = TempDir::new().expect("work directory is created");

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(&["echo", "a;b"], dir.path(), TWENTY_SECS),
    );

    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.stdout, b"a;b\n");
    assert!(report.stderr.is_empty());
    assert!(!report.timed_out);
    assert!(!report.truncated);
}

#[test]
fn it_should_force_no_color_and_the_c_locale() {
    let dir = TempDir::new().expect("work directory is created");
    let mut run_spec = spec(
        &["sh", "-c", "printf '%s|%s' \"$NO_COLOR\" \"$LC_ALL\""],
        dir.path(),
        TWENTY_SECS,
    );
    run_spec.env_set = vec![
        ("NO_COLOR".to_owned(), "0".to_owned()),
        ("LC_ALL".to_owned(), "pt_BR.UTF-8".to_owned()),
    ];

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert_eq!(report.stdout, b"1|C");
}

#[test]
fn it_should_inherit_the_rest_of_the_environment() {
    let dir = TempDir::new().expect("work directory is created");
    let mut run_spec = spec(
        &["sh", "-c", "printf '%s|%s' \"$PATH\" \"$OMHL_T_SET\""],
        dir.path(),
        TWENTY_SECS,
    );
    run_spec.env_set = vec![("OMHL_T_SET".to_owned(), "x".to_owned())];
    let mut expected = std::env::var_os("PATH")
        .expect("the test has a PATH")
        .as_bytes()
        .to_vec();
    expected.extend_from_slice(b"|x");

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert_eq!(report.stdout, expected);
}

#[test]
fn it_should_remove_the_variables_listed_in_env_remove() {
    assert!(
        std::env::var_os("HOME").is_some(),
        "precondition: the test process needs HOME set"
    );
    let dir = TempDir::new().expect("work directory is created");
    let mut run_spec = spec(
        &["sh", "-c", "printf %s \"${HOME-unset}\""],
        dir.path(),
        TWENTY_SECS,
    );
    run_spec.env_remove = vec!["HOME".to_owned(), "OMHL_T_ABSENT".to_owned()];

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.stdout, b"unset");
}

#[test]
fn it_should_kill_the_whole_process_group_on_timeout() {
    let dir = TempDir::new().expect("work directory is created");
    let run_spec = spec(
        &[
            "sh",
            "-c",
            "sleep 61 & echo $! > neto.pid.tmp; mv neto.pid.tmp neto.pid; sleep 61",
        ],
        dir.path(),
        Duration::from_secs(3),
    );
    let started = Instant::now();

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    let returned = Instant::now();
    assert!(report.timed_out);
    assert_eq!(report.exit_code, None);
    assert!(returned - started < Duration::from_secs(10));
    let neto = pid_file(dir.path(), "neto.pid");
    assert_dead_since(&[neto], returned, DEATH);
}

#[test]
fn it_should_kill_a_grandchild_that_outlives_the_leader() {
    let dir = TempDir::new().expect("work directory is created");
    let run_spec = spec(
        &[
            "sh",
            "-c",
            "sleep 61 & echo $! > neto.pid.tmp; mv neto.pid.tmp neto.pid; echo leader-done",
        ],
        dir.path(),
        TWENTY_SECS,
    );
    let started = Instant::now();

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    let returned = Instant::now();
    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.stdout, b"leader-done\n");
    assert!(!report.timed_out);
    assert!(!report.truncated);
    assert!(returned - started < Duration::from_secs(10));
    let neto = pid_file(dir.path(), "neto.pid");
    assert_dead_since(&[neto], returned, DEATH);
}

#[test]
fn it_should_return_when_a_detached_descendant_keeps_the_output_pipe_open() {
    let dir = TempDir::new().expect("work directory is created");
    let run_spec = spec(
        &[
            "sh",
            "-c",
            "setsid sh -c 'echo $$ > neto.pid.tmp; mv neto.pid.tmp neto.pid; exec sleep 20' & while [ ! -e neto.pid ]; do sleep 0.01; done; echo done",
        ],
        dir.path(),
        Duration::from_secs(60),
    );
    let started = Instant::now();

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.stdout, b"done\n");
    assert!(report.stderr.is_empty());
    assert!(!report.timed_out);
    assert!(report.truncated);
}

#[test]
fn it_should_keep_the_stderr_that_arrived_while_stdout_waited_for_the_grace_period() {
    let dir = TempDir::new().expect("work directory is created");
    let run_spec = spec(
        &[
            "sh",
            "-c",
            "setsid sh -c 'echo $$ > neto.pid.tmp; mv neto.pid.tmp neto.pid; exec sleep 5' & while [ ! -e neto.pid ]; do sleep 0.01; done; echo out; echo err >&2",
        ],
        dir.path(),
        Duration::from_secs(60),
    );
    let started = Instant::now();

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.stdout, b"out\n");
    assert_eq!(report.stderr, b"err\n");
    assert!(report.truncated);
}

#[test]
fn it_should_cap_each_stream_and_flag_truncation() {
    let dir = TempDir::new().expect("work directory is created");
    let run_spec = spec(
        &[
            "sh",
            "-c",
            "head -c 6291456 /dev/zero; head -c 6291456 /dev/zero >&2",
        ],
        dir.path(),
        TWENTY_SECS,
    );

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert_eq!(report.stdout.len(), 4_194_304);
    assert!(report.stdout.iter().all(|byte| *byte == 0));
    assert_eq!(report.stderr.len(), 4_194_304);
    assert!(report.truncated);
    assert_eq!(report.exit_code, Some(0));
    assert!(!report.timed_out);
}

#[test]
fn it_should_keep_draining_a_stream_after_the_cap_so_the_command_is_not_killed() {
    let dir = TempDir::new().expect("work directory is created");
    let mut run_spec = spec(
        &["sh", "-c", "head -c 1048576 /dev/zero && echo end >&2"],
        dir.path(),
        TWENTY_SECS,
    );
    run_spec.max_stream_bytes = 16;

    let report = run_ok(&ProcessRunner::new(), &run_spec);

    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.stdout.len(), 16);
    assert_eq!(report.stderr, b"end\n");
    assert!(report.truncated);
    assert!(!report.timed_out);
}

#[test]
fn it_should_report_a_missing_binary_as_a_not_found_error() {
    let dir = TempDir::new().expect("work directory is created");

    let result = ProcessRunner::new().run(&spec(
        &["omhl-test-missing-binary-0"],
        dir.path(),
        TWENTY_SECS,
    ));

    let Err(error) = result else {
        panic!("the run succeeds");
    };
    assert!(
        matches!(error.kind, RunErrorKind::NotFound),
        "got {}",
        kind_name(&error.kind)
    );
}

#[test]
fn it_should_report_a_non_executable_file_as_a_permission_denied_error() {
    let dir = TempDir::new().expect("work directory is created");
    let noexec = dir.path().join("noexec");
    fs::write(&noexec, "#!/bin/sh\necho x\n").expect("the script is written");
    fs::set_permissions(&noexec, fs::Permissions::from_mode(0o644))
        .expect("the script mode is set");
    let argv = noexec.to_str().expect("the temporary path is UTF-8");

    let result = ProcessRunner::new().run(&spec(&[argv], dir.path(), TWENTY_SECS));

    let Err(error) = result else {
        panic!("the run succeeds");
    };
    assert!(
        matches!(error.kind, RunErrorKind::PermissionDenied),
        "got {}",
        kind_name(&error.kind)
    );
}

#[test]
fn it_should_report_the_exit_code_and_the_duration() {
    let dir = TempDir::new().expect("work directory is created");

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(&["sh", "-c", "sleep 0.3; exit 7"], dir.path(), TWENTY_SECS),
    );

    assert_eq!(report.exit_code, Some(7));
    assert!(
        report.duration >= Duration::from_millis(300),
        "duration {:?}",
        report.duration
    );
    assert!(
        report.duration < Duration::from_secs(10),
        "duration {:?}",
        report.duration
    );
    assert!(!report.timed_out);
}

#[test]
fn it_should_report_no_exit_code_when_a_signal_ends_the_process() {
    let dir = TempDir::new().expect("work directory is created");

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(&["sh", "-c", "kill -s KILL $$"], dir.path(), TWENTY_SECS),
    );

    assert_eq!(report.exit_code, None);
    assert!(!report.timed_out);
}

#[test]
fn it_should_run_in_the_given_working_directory() {
    let dir = TempDir::new().expect("work directory is created");
    let mut expected = fs::canonicalize(dir.path())
        .expect("the directory resolves")
        .as_os_str()
        .as_bytes()
        .to_vec();
    expected.push(b'\n');

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(&["sh", "-c", "pwd -P"], dir.path(), TWENTY_SECS),
    );

    assert_eq!(report.stdout, expected);
}

#[test]
fn it_should_close_stdin() {
    let dir = TempDir::new().expect("work directory is created");
    let started = Instant::now();

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(
            &["sh", "-c", "if read -r x; then echo got; else echo eof; fi"],
            dir.path(),
            TWENTY_SECS,
        ),
    );

    assert_eq!(report.stdout, b"eof\n");
    assert!(started.elapsed() < Duration::from_secs(10));
}

// Interrupção (17 a 30)

#[test]
fn it_should_stop_the_command_and_its_grandchild_when_the_runner_group_receives_sigint() {
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "plain", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);

    let signaled = Instant::now();
    killpg(pid(r), Signal::SIGINT).expect("the runner group is signaled");

    assert_eq!(wait_child_exit(&mut runner, DEATH).signal(), Some(2));
    assert_dead_since(&[c, n, g], signaled, DEATH);
}

#[test]
fn it_should_stop_the_command_and_its_grandchild_when_the_runner_receives_sigterm() {
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "plain", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);

    let signaled = Instant::now();
    kill(pid(r), Signal::SIGTERM).expect("the runner is signaled");

    assert_eq!(wait_child_exit(&mut runner, DEATH).signal(), Some(15));
    assert_dead_since(&[c, n, g], signaled, DEATH);
}

#[test]
fn it_should_stop_the_command_and_its_grandchild_when_the_runner_is_killed() {
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "plain", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);

    let signaled = Instant::now();
    kill(pid(r), Signal::SIGKILL).expect("the runner is signaled");

    assert_eq!(wait_child_exit(&mut runner, DEATH).signal(), Some(9));
    assert_dead_since(&[c, n, g], signaled, DEATH);
}

#[test]
fn it_should_release_a_lock_held_by_the_command_after_an_interruption() {
    let dir = TempDir::new().expect("work directory is created");
    let lock = dir.path().join("lock");
    let mut runner = spawn_role("runner", dir.path(), "flock", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);
    assert_eq!(
        flock_exit_code(&lock),
        Some(1),
        "precondition: the command holds the lock"
    );

    let signaled = Instant::now();
    killpg(pid(r), Signal::SIGINT).expect("the runner group is signaled");

    assert_dead_since(&[c, n, g], signaled, DEATH);
    assert_eq!(flock_exit_code(&lock), Some(0));
    wait_child_exit(&mut runner, DEATH);
}

#[test]
fn it_should_leave_no_guardian_process_after_a_normal_run() {
    let dir = TempDir::new().expect("work directory is created");

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(
            &["sh", "-c", "cut -d' ' -f5 /proc/$$/stat"],
            dir.path(),
            TWENTY_SECS,
        ),
    );

    let text = String::from_utf8_lossy(&report.stdout);
    let g: u32 = text.trim_end().parse().expect("the pgrp is a number");
    assert_eq!(report.exit_code, Some(0));
    assert_ne!(g, std::process::id());
    assert_eq!(proc_state(g), None);
}

#[test]
fn it_should_ignore_sigterm_sent_to_the_guardian_alone() {
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "plain", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);
    wait_guardian_armed(g, TWENTY_SECS);

    kill(pid(g), Signal::SIGTERM).expect("the guardian is signaled");
    let window = Instant::now();
    while window.elapsed() < Duration::from_millis(500) {
        assert!(!is_dead(g), "the guardian died from SIGTERM");
        assert!(!is_dead(c), "the command died from SIGTERM to the guardian");
        assert!(
            !is_dead(n),
            "the grandchild died from SIGTERM to the guardian"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let signaled = Instant::now();
    killpg(pid(r), Signal::SIGINT).expect("the runner group is signaled");

    assert_dead_since(&[r, c, n, g], signaled, DEATH);
    wait_child_exit(&mut runner, DEATH);
}

#[test]
fn it_should_let_a_cooperative_command_clean_its_own_process_group_after_an_interruption() {
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "coop", &[], Stdio::null());
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    wait_file(&dir.path().join("armed"), TWENTY_SECS);
    let g = guardian_of(r, c);
    assert_eq!(
        stat_field(n, 5),
        u64::from(n),
        "precondition: the grandchild leads its own group"
    );

    let signaled = Instant::now();
    kill(pid(r), Signal::SIGKILL).expect("the runner is signaled");

    assert_dead_since(&[n, c, g], signaled, DEATH);
    wait_child_exit(&mut runner, DEATH);
}

#[test]
fn it_should_send_sigterm_before_sigkill_on_timeout_so_the_command_can_clean_up() {
    let dir = TempDir::new().expect("work directory is created");
    let started = Instant::now();

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(
            &["sh", "-c", command_line("coop")],
            dir.path(),
            Duration::from_secs(3),
        ),
    );

    let returned = Instant::now();
    assert!(report.timed_out);
    assert_eq!(report.exit_code, Some(0));
    assert!(returned - started < Duration::from_secs(10));
    let neto = pid_file(dir.path(), "neto.pid");
    assert_dead_since(&[neto], returned, DEATH);
}

#[test]
fn it_should_kill_a_command_that_ignores_sigterm_after_the_grace_period() {
    let dir = TempDir::new().expect("work directory is created");
    let started = Instant::now();

    let report = run_ok(
        &ProcessRunner::new(),
        &spec(
            &["sh", "-c", command_line("ignore-term")],
            dir.path(),
            Duration::from_secs(3),
        ),
    );

    let returned = Instant::now();
    assert!(report.timed_out);
    assert_eq!(report.exit_code, None);
    assert!(returned - started >= Duration::from_secs(3));
    assert!(returned - started < Duration::from_secs(10));
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    assert_dead_since(&[c, n], returned, DEATH);
}

#[test]
fn it_should_not_hold_the_output_pipe_of_the_caller_in_the_guardian() {
    let dir = TempDir::new().expect("work directory is created");
    let mut runner = spawn_role("runner", dir.path(), "plain", &[], Stdio::piped());
    let mut stderr = runner.stderr.take().expect("the runner stderr is piped");
    let (eof_tx, eof) = mpsc::channel();
    thread::spawn(move || {
        let mut sink = Vec::new();
        // só o EOF importa; um erro de leitura também encerra a espera
        let _ = stderr.read_to_end(&mut sink);
        let _ = eof_tx.send(());
    });
    let r = runner.id();
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);

    let signaled = Instant::now();
    kill(pid(r), Signal::SIGKILL).expect("the runner is signaled");

    eof.recv_timeout(TWENTY_SECS)
        .expect("the runner stderr reaches EOF");
    let guardian_alive_at_eof = !is_dead(g);
    let elapsed = signaled.elapsed();
    assert!(
        guardian_alive_at_eof,
        "the EOF came only after the guardian died, {elapsed:?} after SIGKILL"
    );
    assert_dead_since(&[c, n, g], signaled, DEATH);
    wait_child_exit(&mut runner, DEATH);
}

#[test]
fn it_should_stop_everything_when_the_parent_of_the_runner_dies() {
    let dir = TempDir::new().expect("work directory is created");
    let mut parent = spawn_role(
        "parent",
        dir.path(),
        "plain",
        &[("OMHL_TEST_BOUND", "1")],
        Stdio::null(),
    );
    let p = parent.id();
    let r = pid_file(dir.path(), "runner.pid");
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    let g = guardian_of(r, c);
    assert_eq!(
        stat_field(r, 4),
        u64::from(p),
        "precondition: the runner is a child of the parent role"
    );

    let signaled = Instant::now();
    kill(pid(p), Signal::SIGKILL).expect("the parent is signaled");

    assert_eq!(wait_child_exit(&mut parent, DEATH).signal(), Some(9));
    assert_dead_since(&[r, c, n, g], signaled, DEATH);
}

#[test]
fn it_should_refuse_to_run_when_the_parent_exited_before_the_runner_bound_to_it() {
    let dir = TempDir::new().expect("work directory is created");
    let mut parent = spawn_role(
        "parent",
        dir.path(),
        "plain",
        &[("OMHL_TEST_BOUND", "1"), ("OMHL_TEST_GO_BEFORE_RUN", "1")],
        Stdio::null(),
    );
    let p = parent.id();
    let r = pid_file(dir.path(), "runner.pid");
    wait_file(&dir.path().join("ready"), TWENTY_SECS);
    kill(pid(p), Signal::SIGKILL).expect("the parent is signaled");
    wait_child_exit(&mut parent, DEATH);
    wait_until(TWENTY_SECS, "the adoption of the runner", || {
        stat_field(r, 4) != u64::from(p)
    });

    fs::write(dir.path().join("go"), "").expect("the go file is written");

    assert_eq!(
        wait_file(&dir.path().join("result"), TWENTY_SECS),
        "err parent-gone"
    );
    let cmd_pid = dir.path().join("cmd.pid");
    assert!(!cmd_pid.exists(), "the command started");
    thread::sleep(Duration::from_secs(1));
    assert!(!cmd_pid.exists(), "the command started after the refusal");
}

#[test]
fn it_should_continue_a_stopped_command_on_timeout_so_its_grandchild_is_cleaned_up() {
    let dir = TempDir::new().expect("work directory is created");
    let cwd = dir.path().to_path_buf();
    let (report_tx, report_rx) = mpsc::channel();
    let started = Instant::now();
    thread::spawn(move || {
        let result = ProcessRunner::new().run(&spec(
            &["sh", "-c", command_line("coop")],
            &cwd,
            Duration::from_secs(5),
        ));
        let _ = report_tx.send(result);
    });
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    wait_file(&dir.path().join("armed"), TWENTY_SECS);
    kill(pid(c), Signal::SIGSTOP).expect("the command is stopped");
    wait_until(TWENTY_SECS, "the stop of the command", || {
        proc_state(c) == Some('T')
    });
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "precondition: the command stopped before the timeout, {:?} after the start",
        started.elapsed()
    );

    let result = report_rx
        .recv_timeout(TWENTY_SECS)
        .expect("the run returns");

    let returned = Instant::now();
    let report = match result {
        Ok(report) => report,
        Err(e) => panic!("run failed: {}", kind_name(&e.kind)),
    };
    assert!(report.timed_out);
    assert_eq!(report.exit_code, Some(0));
    assert_dead_since(&[c, n], returned, DEATH);
}

#[test]
fn it_should_stop_the_guardian_group_after_sigttin_when_the_runner_dies_under_a_subreaper() {
    let dir = TempDir::new().expect("work directory is created");
    let mut reaper = spawn_role("reaper", dir.path(), "coop", &[], Stdio::null());
    let r = pid_file(dir.path(), "runner.pid");
    let c = pid_file(dir.path(), "cmd.pid");
    let n = pid_file(dir.path(), "neto.pid");
    wait_file(&dir.path().join("armed"), TWENTY_SECS);
    assert_eq!(
        stat_field(n, 5),
        u64::from(n),
        "precondition: the grandchild leads its own group"
    );
    let g = guardian_of(r, c);
    wait_guardian_armed(g, TWENTY_SECS);
    killpg(pid(g), Signal::SIGTTIN).expect("the guardian group is signaled");
    wait_until(TWENTY_SECS, "the stop of the command", || {
        proc_state(c) == Some('T')
    });
    assert_ne!(
        proc_state(g),
        Some('T'),
        "precondition: the guardian ignores SIGTTIN"
    );

    let signaled = Instant::now();
    kill(pid(r), Signal::SIGKILL).expect("the runner is signaled");

    assert_dead_since(&[g, c, n], signaled, DEATH);
    fs::write(dir.path().join("done"), "").expect("the done file is written");
    assert_eq!(wait_child_exit(&mut reaper, TWENTY_SECS).code(), Some(0));
}

/// Filhos diretos deste processo de teste com o `comm` dado e a pasta de trabalho `dir`.
fn own_children(comm: &str, dir: &Path) -> Vec<u32> {
    let own = std::process::id().to_string();
    let dir = fs::canonicalize(dir).expect("the directory resolves");
    let entries = fs::read_dir("/proc").expect("/proc is readable");
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            // o processo pode sumir entre a listagem e a leitura: ilegível conta como outro
            let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
                return false;
            };
            let ppid = stat[close + 1..].split_whitespace().nth(1);
            &stat[open + 1..close] == comm
                && ppid == Some(own.as_str())
                && fs::read_link(format!("/proc/{pid}/cwd")).is_ok_and(|cwd| cwd == dir)
        })
        .collect()
}

#[test]
fn it_should_kill_a_command_that_left_its_process_group_on_timeout() {
    let dir = TempDir::new().expect("work directory is created");
    let cwd = dir.path().to_path_buf();
    let (report_tx, report_rx) = mpsc::channel();
    let started = Instant::now();
    thread::spawn(move || {
        let result = ProcessRunner::new().run(&spec(
            &["sh", "-c", "trap '' TERM; exec setsid sleep 61"],
            &cwd,
            Duration::from_secs(3),
        ));
        let _ = report_tx.send(result);
    });

    let result = report_rx.recv_timeout(TWENTY_SECS);

    let elapsed = started.elapsed();
    // um run preso deixa o sleep 61 vivo: mata antes das asserções para não sobrar nada
    for stray in own_children("sleep", dir.path()) {
        let _ = kill(pid(stray), Signal::SIGKILL);
    }
    let report = match result {
        Ok(Ok(report)) => report,
        Ok(Err(e)) => panic!("run failed: {}", kind_name(&e.kind)),
        Err(_) => panic!("the run did not return within {TWENTY_SECS:?}"),
    };
    assert!(report.timed_out);
    assert_eq!(report.exit_code, None);
    assert!(
        elapsed >= Duration::from_secs(3),
        "the run took {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the run took {elapsed:?}"
    );
}
