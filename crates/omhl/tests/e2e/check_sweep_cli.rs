use crate::support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::child::{Cmd, omhl};
use support::ext::ExtTree;
use support::git::git_in;
use support::project::Project;
use tempfile::TempDir;

const WORK_ITEM_APPEND: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.w1","type":"work-item","data":{"title":"t","status":"open"}}}]}"#;

// O `while [ ! -e "$1" ]` segura o líder até o neto gravar o pid, já em sessão própria: sem ele o
// líder sai antes do `setsid()` e o `killpg` do guardião mata o neto ainda no grupo.
const STRAY_SCRIPT: &str = "echo \"$PPID\" > \"$1.owner\"; setsid sh -c 'echo $$ > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"; exec sleep 61' sh \"$1\" >/dev/null 2>&1 </dev/null & while [ ! -e \"$1\" ]; do sleep 0.01; done; exit 0";
const STRAY_WAIT_SCRIPT: &str = "echo \"$PPID\" > \"$1.owner\"; setsid sh -c 'echo $$ > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"; exec sleep 61' sh \"$1\" >/dev/null 2>&1 </dev/null & while [ ! -e \"$1\" ]; do sleep 0.01; done; i=0; while [ ! -e \"$2\" ] && [ \"$i\" -lt 300 ]; do sleep 0.1; i=$((i+1)); done; exit 0";

// Apoio

/// TOML de check `before-pr`, bloqueante, com o `expect` do `GREEN`.
fn check_toml(run: &[&str]) -> String {
    // Uma string JSON é também uma string básica TOML válida.
    let run = serde_json::to_string(run).expect("the argv serializes");

    format!(
        "blocking = true\nhow = \"Always passes.\"\nrun = {run}\ntimeout_secs = 60\nexpect = [{{ exit = 0, outcome = \"passed\" }}]\n\n[when]\nevent = \"before-pr\"\n"
    )
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("the path is UTF-8")
}

/// `STRAY(pidfile)`: solta um `sleep 61` em sessão própria e grava o pid dele em `pidfile`.
fn stray(pidfile: &Path) -> String {
    check_toml(&["sh", "-c", STRAY_SCRIPT, "sh", utf8(pidfile)])
}

/// `STRAY_WAIT(pidfile, go)`: o `STRAY` com o líder esperando `go` (até 30 s) antes de sair.
fn stray_wait(pidfile: &Path, go: &Path) -> String {
    check_toml(&["sh", "-c", STRAY_WAIT_SCRIPT, "sh", utf8(pidfile), utf8(go)])
}

fn git(p: &Project, args: &[&str]) {
    let output = git_in(p.dir(), args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn omhl_in(p: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(p.dir()).xdg(p.xdg())
}

/// Repositório git limpo na branch `feat/wi-w1__x`, com o work item `w1` e os checks carregados.
fn cli_repo(checks: &[(&str, &str)]) -> Project {
    let p = Project::new("acme");
    git(&p, &["init", "-q", "-b", "main"]);
    git(&p, &["config", "user.name", "Test Author"]);
    git(&p, &["config", "user.email", "author@example.com"]);
    let tree = ExtTree::at(p.dir());
    for (slug, toml) in checks {
        tree.check(slug, toml);
    }
    git(&p, &["add", "-A"]);
    git(&p, &["commit", "-q", "-m", "base"]);
    git(&p, &["switch", "-q", "-c", "feat/wi-w1__x"]);

    let appended = omhl_in(&p, &["append"]).stdin(WORK_ITEM_APPEND).run();
    assert_eq!(appended.code, Some(0), "append stderr: {}", appended.stderr);
    let loaded = omhl_in(&p, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);

    p
}

/// O `omhl` com o ambiente do `Cmd::run`, stdin nulo e saídas em cano, sem esperar.
fn spawn_omhl(p: &Project, args: &[&str]) -> Child {
    let home = p.xdg().join("spawn-home");
    fs::create_dir_all(&home).expect("the spawn home is created");
    let mut command = Command::new(env!("CARGO_BIN_EXE_omhl"));
    command
        .args(args)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_DATA_HOME", p.xdg())
        .current_dir(p.dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }

    command.spawn().expect("the omhl binary starts")
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// O JSON da linha com cada `results[i].duration_ms` conferido como inteiro e trocado por `0`.
fn normalized(line: &str) -> Value {
    let mut value = json_line(line);
    if let Some(results) = value["results"].as_array_mut() {
        for result in results {
            assert!(
                result["duration_ms"].is_u64(),
                "duration_ms is an integer: {result}"
            );
            result["duration_ms"] = json!(0);
        }
    }
    value
}

fn sweep_log(p: &Project) -> PathBuf {
    p.xdg().join("omhl").join("sweep.jsonl")
}

fn sweep_log_lines(p: &Project) -> Vec<String> {
    fs::read_to_string(sweep_log(p))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn swept_pids(report: &Value) -> Vec<u64> {
    report["swept"]
        .as_array()
        .expect("swept is an array")
        .iter()
        .map(|item| item["pid"].as_u64().expect("the pid is an integer"))
        .collect()
}

/// `(comm, estado)` de `/proc/<pid>/stat`: o `comm` entre o primeiro `(` e o último `)`.
fn stat(pid: u32) -> Option<(String, char)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (before, after) = text.rsplit_once(')')?;
    let (_, comm) = before.split_once('(')?;
    let state = after.trim_start().chars().next()?;
    Some((comm.to_owned(), state))
}

fn dead(pid: u32) -> bool {
    stat(pid).is_none_or(|(_, state)| state == 'Z')
}

fn wait_dead(pid: u32, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while !dead(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    true
}

/// Mata o fugido só se ele ainda vive e ainda é um `sleep` (o pid pode ter sido reusado).
fn kill_stray(pid: u32) {
    if !dead(pid) && stat(pid).is_some_and(|(comm, _)| comm == "sleep") {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
}

fn read_pid(path: &Path, within: Duration) -> u32 {
    let deadline = Instant::now() + within;
    loop {
        if let Some(pid) = fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "{} did not appear within {within:?}",
            path.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_exit(child: &mut Child, within: Duration) -> ExitStatus {
    let deadline = Instant::now() + within;
    loop {
        if let Some(status) = child.try_wait().expect("the child is polled") {
            return status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("the stuck child is killed");
            child.wait().expect("the killed child is reaped");
            panic!("omhl did not exit within {within:?}");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

// Testes

#[test]
fn it_should_sweep_a_stray_of_a_finished_run_when_check_run_starts() {
    let _sweep = support::sweep_lock::serialize_sweeps();
    let tmp = TempDir::new().expect("temp dir");
    let pidfile = tmp.path().join("stray.pid");
    let owner = tmp.path().join("stray.pid.owner");
    let p = cli_repo(&[("stray", &stray(&pidfile))]);
    let first = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    assert_eq!(first.code, Some(0), "first stderr: {}", first.stderr);
    let s1 = read_pid(&pidfile, Duration::from_secs(5));
    assert!(!dead(s1), "the stray {s1} is alive");
    let o1 = read_pid(&owner, Duration::from_secs(5));
    assert!(dead(o1), "the owner {o1} already exited");
    let n0 = sweep_log_lines(&p).len();
    fs::remove_file(&pidfile).expect("the pid file is removed");
    fs::remove_file(&owner).expect("the owner file is removed");

    let second = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(second.code, Some(0), "second stderr: {}", second.stderr);
    assert_eq!(second.stderr, "");
    assert_eq!(
        second.stdout.lines().count(),
        1,
        "stdout: {}",
        second.stdout
    );
    assert_eq!(normalized(&second.stdout), normalized(&first.stdout));
    assert!(wait_dead(s1, Duration::from_secs(4)), "the stray {s1} died");
    let lines = sweep_log_lines(&p);
    assert_eq!(lines.len(), n0 + 1, "sweep log: {lines:?}");
    let last = json_line(lines.last().expect("the sweep log has a line"));
    let repo = fs::canonicalize(p.dir()).expect("the repository path resolves");
    assert_eq!(last["cwd"], json!(utf8(&repo)));
    assert!(
        swept_pids(&last).contains(&u64::from(s1)),
        "the line sweeps {s1}: {last}"
    );
    let s2 = read_pid(&pidfile, Duration::from_secs(5));
    assert_ne!(s2, s1);
    kill_stray(s2);
}

#[test]
fn it_should_not_sweep_the_stray_of_a_check_run_still_in_progress() {
    let _sweep = support::sweep_lock::serialize_sweeps();
    let tmp = TempDir::new().expect("temp dir");
    let pidfile = tmp.path().join("stray.pid");
    let go = tmp.path().join("go");
    let p = cli_repo(&[("stray", &stray_wait(&pidfile, &go))]);
    let mut background = spawn_omhl(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    );
    let s1 = read_pid(&pidfile, Duration::from_secs(5));

    let sweep = omhl_in(&p, &["check", "sweep"]).run();

    assert_eq!(sweep.code, Some(0), "stderr: {}", sweep.stderr);
    assert!(
        !swept_pids(&json_line(&sweep.stdout)).contains(&u64::from(s1)),
        "the live run's stray {s1} is kept: {}",
        sweep.stdout
    );
    assert!(!dead(s1), "the stray {s1} is alive");
    fs::write(&go, "").expect("the go file is written");
    let status = wait_exit(&mut background, Duration::from_secs(30));
    assert_eq!(status.code(), Some(0), "the background check run passes");
    kill_stray(s1);
}

#[test]
fn it_should_print_the_swept_processes_as_json_in_check_sweep() {
    let _sweep = support::sweep_lock::serialize_sweeps();
    let p = cli_repo(&[]);

    let output = omhl_in(&p, &["check", "sweep"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout.lines().count(),
        1,
        "stdout: {}",
        output.stdout
    );
    let report = json_line(&output.stdout);
    let mut keys: Vec<&str> = report
        .as_object()
        .expect("the report is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["scan_timed_out", "swept", "unreadable"]);
    assert!(report["swept"].is_array(), "swept is a list: {report}");
    assert!(
        report["unreadable"].is_u64(),
        "unreadable is an integer >= 0: {report}"
    );
    assert_eq!(report["scan_timed_out"], json!(false));
}

#[test]
fn it_should_run_the_checks_when_the_sweep_log_cannot_be_written() {
    let _sweep = support::sweep_lock::serialize_sweeps();
    let tmp = TempDir::new().expect("temp dir");
    let pidfile = tmp.path().join("stray.pid");
    let owner = tmp.path().join("stray.pid.owner");
    let p = cli_repo(&[("stray", &stray(&pidfile))]);
    let first = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    assert_eq!(first.code, Some(0), "first stderr: {}", first.stderr);
    let s1 = read_pid(&pidfile, Duration::from_secs(5));
    fs::remove_file(&pidfile).expect("the pid file is removed");
    fs::remove_file(&owner).expect("the owner file is removed");
    // O 1º `check run` pode ter varrido sobra de outro binário e criado o arquivo.
    let _ = fs::remove_file(sweep_log(&p));
    fs::create_dir_all(sweep_log(&p)).expect("the sweep log becomes a directory");

    let second = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(second.code, Some(0), "second stderr: {}", second.stderr);
    assert_eq!(second.stderr, "");
    assert_eq!(normalized(&second.stdout), normalized(&first.stdout));
    assert!(wait_dead(s1, Duration::from_secs(4)), "the stray {s1} died");
    assert!(sweep_log(&p).is_dir(), "the sweep log is still a directory");
    let s2 = read_pid(&pidfile, Duration::from_secs(5));
    assert_ne!(s2, s1);
    kill_stray(s2);
}

#[test]
fn it_should_append_the_swept_stray_to_the_sweep_log_through_check_sweep() {
    let _sweep = support::sweep_lock::serialize_sweeps();
    let tmp = TempDir::new().expect("temp dir");
    let pidfile = tmp.path().join("stray.pid");
    let p = cli_repo(&[("stray", &stray(&pidfile))]);
    let first = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    assert_eq!(first.code, Some(0), "first stderr: {}", first.stderr);
    let s1 = read_pid(&pidfile, Duration::from_secs(5));
    let o1 = read_pid(&tmp.path().join("stray.pid.owner"), Duration::from_secs(5));
    let n0 = sweep_log_lines(&p).len();

    let output = omhl_in(&p, &["check", "sweep"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let report = json_line(&output.stdout);
    let expected = json!({ "comm": "sleep", "owner_pid": o1, "pid": s1 });
    assert!(
        report["swept"]
            .as_array()
            .expect("swept is an array")
            .contains(&expected),
        "swept has {expected}: {report}"
    );
    assert!(wait_dead(s1, Duration::from_secs(4)), "the stray {s1} died");
    let lines = sweep_log_lines(&p);
    assert_eq!(lines.len(), n0 + 1, "sweep log: {lines:?}");
    let last = json_line(lines.last().expect("the sweep log has a line"));
    assert!(
        swept_pids(&last).contains(&u64::from(s1)),
        "the line sweeps {s1}: {last}"
    );
    kill_stray(s1);
}
