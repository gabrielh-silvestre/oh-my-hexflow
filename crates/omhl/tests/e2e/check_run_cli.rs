use crate::support;

use std::fs;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::assert_resolved_command;
use support::child::{Cmd, omhl, omhl_json};
use support::ext::ExtTree;
use support::git::{git_in, git_ok};
use support::project::Project;
use tempfile::TempDir;

const WORK_ITEM_APPEND: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.w1","type":"work-item","data":{"title":"t","status":"open"}}}]}"#;
const ZEROS: &str = "0000000000000000000000000000000000000000";

const GREEN: &str = r#"blocking = true
how = "Always passes."
run = ["true"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const RED: &str = r#"blocking = true
how = "Always fails."
run = ["sh", "-c", "exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const NOISY_RED: &str = r#"blocking = true
how = "Always fails."
run = ["sh", "-c", "printf 'SECRET-OUT-%s\\n' 7a1; printf 'SECRET-ERR-%s\\n' 7a1 >&2; exit 1"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const GATE_V1: &str = r#"blocking = true
how = "Always passes."
run = ["sh", "-c", "exit 0"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const GATE_V2: &str = r#"blocking = true
how = "Always passes."
run = ["sh", "-c", "exit 3"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;

const TEXT_ONLY: &str = r#"blocking = true
how = "Check it by hand."

[when]
event = "before-pr"
"#;

// Apoio

/// TOML de check `before-pr`, bloqueante, com o `expect` do `GREEN`.
fn check_toml(how: &str, run: &[&str], timeout_secs: u64) -> String {
    // Uma string JSON é também uma string básica TOML válida.
    let how = serde_json::to_string(how).expect("the how serializes");
    let run = serde_json::to_string(run).expect("the argv serializes");

    format!(
        "blocking = true\nhow = {how}\nrun = {run}\ntimeout_secs = {timeout_secs}\nexpect = [{{ exit = 0, outcome = \"passed\" }}]\n\n[when]\nevent = \"before-pr\"\n"
    )
}

/// `TOUCH(path)`: grava `ran` em `path`, prova de que o comando rodou.
fn touch(path: &Path) -> String {
    let path = path.to_str().expect("the path is UTF-8");

    check_toml(
        "Always passes.",
        &["sh", "-c", "printf ran > \"$1\"", "sh", path],
        60,
    )
}

fn head(p: &Project) -> String {
    git_ok(p.dir(), &["rev-parse", "HEAD"]).trim().to_owned()
}

fn omhl_in(p: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(p.dir()).xdg(p.xdg())
}

/// Como `omhl_in`, pedindo a saída em JSON: para as chamadas que leem a saída e não são `check run`.
fn omhl_json_in(p: &Project, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(p.dir()).xdg(p.xdg())
}

/// Repositório git limpo na branch `feat/wi-w1__x`, com o work item `w1` e os checks carregados.
fn cli_repo(checks: &[(&str, &str)]) -> Project {
    let p = Project::new("acme");
    git_ok(p.dir(), &["init", "-q", "-b", "main"]);
    git_ok(p.dir(), &["config", "user.name", "Test Author"]);
    git_ok(p.dir(), &["config", "user.email", "author@example.com"]);
    let tree = ExtTree::at(p.dir());
    for (slug, toml) in checks {
        tree.check(slug, toml);
    }
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "base"]);
    git_ok(p.dir(), &["switch", "-q", "-c", "feat/wi-w1__x"]);

    let appended = omhl_in(&p, &["append"]).stdin(WORK_ITEM_APPEND).run();
    assert_eq!(appended.code, Some(0), "append stderr: {}", appended.stderr);
    let loaded = omhl_json_in(&p, &["load"]).run();
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

fn error_of(stderr: &str) -> Value {
    json_line(stderr)["error"].clone()
}

fn attestations(p: &Project) -> Vec<Value> {
    let listed = omhl_json_in(p, &["list", "--type", "attestation"]).run();
    assert_eq!(listed.code, Some(0), "list stderr: {}", listed.stderr);
    json_line(&listed.stdout)["items"]
        .as_array()
        .expect("items is an array")
        .clone()
}

fn attestation(p: &Project, check: &str) -> Value {
    attestations(p)
        .into_iter()
        .find(|item| item["data"]["check"] == check)
        .unwrap_or_else(|| panic!("an attestation of {check} exists"))
}

fn dead(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit_once(')')
            .is_none_or(|(_, rest)| rest.trim_start().starts_with('Z')),
    }
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

fn pre_push_line(local: &str, sha: &str, remote: &str) -> String {
    format!("{local} {sha} {remote} {ZEROS}\n")
}

// Testes

#[test]
fn it_should_attest_a_before_pr_check_through_the_binary_in_a_real_repository() {
    let p = cli_repo(&[("green", GREEN)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    let head = head(&p);
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");
    assert_eq!(
        normalized(&output.stdout),
        json!({
            "event": "before-pr",
            "work_item": "work-items.w1",
            "snapshot": { "kind": "git-commit", "id": head },
            "recorded": true,
            "results": [{
                "check": "checks.green",
                "outcome": "passed",
                "ok": true,
                "blocking": true,
                "exit_code": 0,
                "duration_ms": 0,
                "timed_out": false,
                "truncated": false,
            }],
            "blocked": [],
        })
    );
    let items = attestations(&p);
    assert_eq!(items.len(), 1, "one attestation: {items:?}");
    let item = &items[0];
    assert_eq!(item["data"]["outcome"], json!("passed"));
    assert_eq!(item["data"]["ok"], json!(true));
    assert_eq!(item["data"]["exit_code"], json!(0));
    let command = item["data"]["command"]
        .as_array()
        .expect("command is a list");
    assert_eq!(command.len(), 1, "command: {command:?}");
    let program = Path::new(command[0].as_str().expect("command[0] is text"));
    assert!(program.is_absolute(), "not resolved: {program:?}");
    assert_eq!(program.file_name().and_then(|n| n.to_str()), Some("true"));
    assert_eq!(
        item["data"]["spec_digest"],
        json!("529de2e8426939e64c4e6dbc60c6d0fd954d899cee83e3c495194deae41564f3")
    );
    assert_eq!(item["data"]["check"], json!("checks.green"));
    assert_eq!(item["data"]["work_item"], json!("work-items.w1"));
    assert_eq!(
        item["author"],
        json!({ "client": "executor", "agent": "executor" })
    );
    assert_eq!(
        item["snapshot"],
        json!({ "kind": "git-commit", "id": head })
    );
}

#[test]
fn it_should_print_one_json_line_on_stdout_and_exit_zero_when_every_check_passes() {
    let p = cli_repo(&[("a", GREEN), ("b", GREEN)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.ends_with('\n'), "stdout ends in a newline");
    assert_eq!(
        output.stdout.lines().count(),
        1,
        "stdout: {}",
        output.stdout
    );
    assert_eq!(output.stderr, "");
    let report = normalized(&output.stdout);
    let passed = |check: &str| {
        json!({
            "check": check,
            "outcome": "passed",
            "ok": true,
            "blocking": true,
            "exit_code": 0,
            "duration_ms": 0,
            "timed_out": false,
            "truncated": false,
        })
    };
    assert_eq!(
        report["results"],
        json!([passed("checks.a"), passed("checks.b")])
    );
    assert_eq!(report["blocked"], json!([]));
}

#[test]
fn it_should_exit_one_and_print_the_failing_check_on_stderr_when_a_blocking_check_fails() {
    let p = cli_repo(&[("red", RED)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(
        output.stderr.lines().count(),
        1,
        "stderr: {}",
        output.stderr
    );
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("checks-failed"));
    let mut details = error["details"].clone();
    details[0]
        .as_object_mut()
        .expect("a detail is an object")
        .remove("context");
    assert_eq!(
        details,
        json!([{
            "path": "checks.red",
            "code": "failed",
            "message": "omhl byproduct checks.red work-items.w1 --tail 40",
        }])
    );
    let item = attestation(&p, "checks.red");
    assert_eq!(item["data"]["outcome"], json!("failed"));
    assert_eq!(item["data"]["exit_code"], json!(1));
}

#[test]
fn it_should_print_nothing_on_stdout_when_a_blocking_check_fails() {
    let p = cli_repo(&[("red", RED)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    // Pré-condição: sem ela, um pânico (stdout vazio, exit 101) passaria por este teste.
    assert_eq!(
        error_of(&output.stderr)["code"],
        json!("checks-failed"),
        "stderr: {}",
        output.stderr
    );
    assert_eq!(output.stdout, "");
}

#[test]
fn it_should_not_echo_the_command_output_in_the_error_json() {
    let p = cli_repo(&[("red", NOISY_RED)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("checks-failed"));
    for secret in ["SECRET-OUT-7a1", "SECRET-ERR-7a1"] {
        assert!(!output.stdout.contains(secret), "stdout leaks {secret}");
        assert!(!output.stderr.contains(secret), "stderr leaks {secret}");
    }
    assert!(
        error["details"][0]["context"].get("output_tail").is_none(),
        "the detail has no output tail: {}",
        error["details"][0]
    );
}

#[test]
fn it_should_exit_two_for_an_unknown_event() {
    let dir = TempDir::new().expect("temp dir");

    let output = omhl(&["check", "run", "--event", "before-deploy"])
        .cwd(dir.path())
        .run();

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("invalid value 'before-deploy' for '--event <EVENT>'"),
        "stderr: {}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("[possible values: before-commit, before-pr, before-merge]"),
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_exit_two_when_neither_event_nor_check_is_given() {
    let dir = TempDir::new().expect("temp dir");

    let output = omhl(&["check", "run"]).cwd(dir.path()).run();

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("the following required arguments were not provided"),
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_exit_two_when_both_event_and_check_are_given() {
    let dir = TempDir::new().expect("temp dir");

    let output = omhl(&[
        "check",
        "run",
        "--event",
        "before-pr",
        "--check",
        "checks.x",
    ])
    .cwd(dir.path())
    .run();

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("the argument '--event <EVENT>' cannot be used with '--check <CHECK>'"),
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_refuse_a_before_pr_run_when_the_committed_file_differs_from_the_stored_definition() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    ExtTree::at(p.dir()).check("gate", GATE_V2);
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "edit gate"]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("check-not-run"));
    assert_eq!(error["details"][0]["code"], json!("spec-stale"));
    assert!(
        error["details"][0]["context"]["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("omhl load")),
        "error: {error}"
    );
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_ignore_and_warn_an_orphaned_check_when_its_file_was_deleted() {
    let out = TempDir::new().expect("the marker directory is created");
    let marker = out.path().join("ran");
    let p = cli_repo(&[("gate", &touch(&marker))]);
    fs::remove_file(p.dir().join(".omhl/checks/gate.toml")).expect("the check file is deleted");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "drop gate"]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let result = json_line(&output.stdout);
    assert_eq!(result["results"], json!([]));
    assert_eq!(result["warnings"][0]["code"], json!("check-orphaned"));
    assert_eq!(result["warnings"][0]["check"], json!("checks.gate"));
    let hint = result["warnings"][0]["hint"]
        .as_str()
        .expect("the hint is text");
    assert!(
        hint.contains("rebase it onto main") && hint.contains("permanent"),
        "hint: {hint}"
    );
    assert!(!marker.exists(), "the orphaned command must not run");
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_run_the_live_checks_and_warn_the_orphaned_one_in_the_same_run() {
    let p = cli_repo(&[("gone", RED), ("live", GREEN)]);
    fs::remove_file(p.dir().join(".omhl/checks/gone.toml")).expect("the check file is deleted");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "drop gone"]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let result = json_line(&output.stdout);
    assert_eq!(result["results"][0]["check"], json!("checks.live"));
    assert_eq!(result["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(result["warnings"][0]["check"], json!("checks.gone"));
    assert_eq!(attestations(&p).len(), 1);
}

#[test]
fn it_should_refuse_with_spec_stale_when_the_check_file_no_longer_parses() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    ExtTree::at(p.dir()).check("gate", "run = [");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "break gate"]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("check-not-run"));
    assert_eq!(error["details"][0]["code"], json!("spec-stale"));
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_refuse_a_before_commit_run_when_the_file_differs_from_the_stored_definition() {
    let v1 = GATE_V1.replace("before-pr", "before-commit");
    let v2 = GATE_V2.replace("before-pr", "before-commit");
    let p = cli_repo(&[("gate", &v1)]);
    ExtTree::at(p.dir()).check("gate", &v2);
    git_ok(p.dir(), &["add", "-A"]);

    let output = omhl_in(
        &p,
        &[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-commit",
        ],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("check-not-run"));
    assert_eq!(error["details"][0]["code"], json!("spec-stale"));
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_refuse_a_before_commit_run_when_a_submodule_has_an_untracked_file() {
    let gate = GATE_V1.replace("before-pr", "before-commit");
    let p = cli_repo(&[("gate", &gate)]);
    let upstream = TempDir::new().expect("the upstream dir is created");
    git_in(upstream.path(), &["init", "-q", "-b", "main"]);
    fs::write(upstream.path().join("lib.txt"), "lib\n").expect("the upstream file is written");
    git_in(upstream.path(), &["add", "-A"]);
    git_in(
        upstream.path(),
        &[
            "-c",
            "user.name=Test Author",
            "-c",
            "user.email=author@example.com",
            "commit",
            "-q",
            "-m",
            "lib",
        ],
    );
    let upstream_path = upstream.path().to_str().expect("the path is UTF-8");
    git_ok(
        p.dir(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            upstream_path,
            "vendor",
        ],
    );
    git_ok(p.dir(), &["commit", "-q", "-m", "add submodule"]);
    fs::write(p.dir().join("vendor/untracked.txt"), "new\n").expect("the file is written");

    let output = omhl_in(
        &p,
        &[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-commit",
        ],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(
        error_of(&output.stderr)["code"],
        json!("worktree-differs-from-index")
    );
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_run_the_new_command_after_load() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    ExtTree::at(p.dir()).check("gate", GATE_V2);
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "edit gate"]);
    let loaded = omhl_json_in(&p, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);
    assert_eq!(json_line(&loaded.stdout)["revised"], json!(["checks.gate"]));

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(error_of(&output.stderr)["code"], json!("checks-failed"));
    let data = &attestation(&p, "checks.gate")["data"];
    assert_resolved_command(&data["command"], "sh", &["-c", "exit 3"]);
    assert_eq!(data["outcome"], json!("failed"));
    assert_eq!(data["exit_code"], json!(3));
}

#[test]
fn it_should_fail_a_check_that_exceeds_its_timeout_through_the_binary() {
    let slow = check_toml("Always passes.", &["sleep", "5"], 1);
    let p = cli_repo(&[("slow", &slow)]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(error_of(&output.stderr)["code"], json!("checks-failed"));
    let data = &attestation(&p, "checks.slow")["data"];
    assert_eq!(data["outcome"], json!("failed"));
    assert_eq!(data["timed_out"], json!(true));
    assert!(data.get("exit_code").is_none(), "no exit_code: {data}");
    let duration = data["duration_ms"]
        .as_u64()
        .expect("duration_ms is an integer");
    assert!(
        (1000..4000).contains(&duration),
        "duration_ms {duration} is in [1000, 4000)"
    );
}

#[test]
fn it_should_leave_omhl_verify_passing_after_a_check_run() {
    let p = cli_repo(&[("green", GREEN)]);
    let run = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    assert_eq!(run.code, Some(0), "check run stderr: {}", run.stderr);

    let verified = omhl_json_in(&p, &["verify"]).run();

    assert_eq!(verified.code, Some(0), "stderr: {}", verified.stderr);
    assert_eq!(json_line(&verified.stdout)["ok"], json!(true));
}

#[test]
fn it_should_not_let_a_git_index_file_in_the_environment_change_a_before_pr_snapshot() {
    let p = cli_repo(&[("green", GREEN)]);
    let tmp = TempDir::new().expect("temp dir");
    let alt_index = tmp.path().join("alt-index");
    fs::write(p.dir().join("extra.txt"), "e\n").expect("extra.txt is written");
    git_ok(p.dir(), &["add", "extra.txt"]);
    fs::copy(p.dir().join(".git").join("index"), &alt_index).expect("the index is copied");
    git_ok(p.dir(), &["rm", "-q", "--cached", "extra.txt"]);
    fs::remove_file(p.dir().join("extra.txt")).expect("extra.txt is removed");

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .env("GIT_INDEX_FILE", &alt_index)
    .run();

    let snapshot = json!({ "kind": "git-commit", "id": head(&p) });
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(normalized(&output.stdout)["snapshot"], snapshot);
    assert_eq!(attestation(&p, "checks.green")["snapshot"], snapshot);
}

#[test]
fn it_should_reject_the_removed_work_item_flag() {
    let dir = TempDir::new().expect("temp dir");

    let output = omhl(&["check", "run", "--event", "before-pr", "--work-item", "w1"])
        .cwd(dir.path())
        .run();

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("unexpected argument '--work-item' found"),
        "stderr: {}",
        output.stderr
    );
}

#[test]
fn it_should_answer_branch_name_invalid_for_a_branch_outside_the_pattern_through_the_binary() {
    let tmp = TempDir::new().expect("temp dir");
    let ran = tmp.path().join("ran");
    let p = cli_repo(&[("touch", &touch(&ran))]);
    git_ok(p.dir(), &["switch", "-q", "-c", "topic"]);

    let output = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("branch-name-invalid"));
    let context = &error["details"][0]["context"];
    assert_eq!(context["kind"], json!("branch-name-invalid"));
    assert_eq!(context["branch"], json!("topic"));
    assert_eq!(
        context["hint"],
        json!("rename the branch to type/wi-<id>__<slug> with git branch -m")
    );
    assert!(!ran.exists(), "the check command did not run");
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

#[test]
fn it_should_leave_no_orphan_and_no_attestation_when_the_binary_is_interrupted() {
    let _sweep = support::sweep_lock::share_sweeps();
    let tmp = TempDir::new().expect("temp dir");
    let pids = tmp.path().join("pids");
    let pids_arg = pids.to_str().expect("the path is UTF-8");
    let interrupted = check_toml(
        "Always passes.",
        &[
            "sh",
            "-c",
            "sleep 30 & echo $! > \"$1.grandchild.tmp\"; mv \"$1.grandchild.tmp\" \"$1.grandchild\"; echo $$ > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"; wait",
            "sh",
            pids_arg,
        ],
        120,
    );
    let p = cli_repo(&[("interrupted", &interrupted)]);
    let mut child = spawn_omhl(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    );
    let shell = read_pid(&pids, Duration::from_secs(10));
    let sleeper = read_pid(&tmp.path().join("pids.grandchild"), Duration::from_secs(10));

    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    let status = wait_exit(&mut child, Duration::from_secs(10));

    assert_eq!(status.code(), None, "omhl ends by a signal: {status:?}");
    assert!(
        wait_dead(shell, Duration::from_secs(4)),
        "the sh {shell} died"
    );
    assert!(
        wait_dead(sleeper, Duration::from_secs(4)),
        "the sleep {sleeper} died"
    );
    let listed = omhl_json_in(&p, &["list", "--type", "attestation"]).run();
    assert_eq!(json_line(&listed.stdout), json!({ "items": [] }));
}

#[test]
fn it_should_read_the_pushed_refs_from_stdin_when_the_pre_push_flag_is_given() {
    let feature = "refs/heads/feat/wi-w1__x";
    let pushed = cli_repo(&[("green", GREEN)]);
    let tmp = TempDir::new().expect("temp dir");
    let ran = tmp.path().join("ran");
    let to_main = cli_repo(&[("touch", &touch(&ran))]);

    let feature_push = omhl_in(
        &pushed,
        &[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-pr",
            "--pre-push",
        ],
    )
    .stdin(pre_push_line(feature, &head(&pushed), feature))
    .run();
    let main_push = omhl_in(
        &to_main,
        &[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-pr",
            "--pre-push",
        ],
    )
    .stdin(pre_push_line(feature, &head(&to_main), "refs/heads/main"))
    .run();

    assert_eq!(
        feature_push.code,
        Some(0),
        "stderr: {}",
        feature_push.stderr
    );
    let results = &normalized(&feature_push.stdout)["results"];
    assert_eq!(
        results.as_array().map(Vec::len),
        Some(1),
        "results: {results}"
    );
    assert_eq!(results[0]["check"], json!("checks.green"));
    assert_eq!(results[0]["outcome"], json!("passed"));
    assert_eq!(main_push.code, Some(1), "stderr: {}", main_push.stderr);
    assert_eq!(main_push.stdout, "");
    let error = error_of(&main_push.stderr);
    assert_eq!(error["code"], json!("branch-protected"));
    assert_eq!(
        error["details"][0]["context"]["kind"],
        json!("push-to-main")
    );
    assert!(!ran.exists(), "the check command did not run");
    assert_eq!(attestations(&to_main), Vec::<Value>::new());
}

#[test]
fn it_should_exit_two_when_the_pre_push_flag_comes_without_the_before_pr_event() {
    let dir = TempDir::new().expect("temp dir");

    let with_commit = omhl(&["check", "run", "--event", "before-commit", "--pre-push"])
        .cwd(dir.path())
        .run();
    let with_check = omhl(&["check", "run", "--check", "checks.x", "--pre-push"])
        .cwd(dir.path())
        .run();

    for output in [with_commit, with_check] {
        assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
        assert_eq!(output.stdout, "");
        assert!(
            output
                .stderr
                .contains("--pre-push is only valid with --event before-pr"),
            "stderr: {}",
            output.stderr
        );
    }
}

#[test]
fn it_should_consume_the_develop_mark_through_the_post_commit_command() {
    let p = cli_repo(&[]);
    git_ok(p.dir(), &["switch", "-q", "-c", "develop"]);
    let mark = p.dir().join(".git").join("omhl-develop-mark");
    fs::write(&mark, "omhl-develop-mark v1\n").expect("the mark is written");

    let first = omhl_in(&p, &["hooks", "post-commit"]).run();
    let mark_left = mark.exists();
    let second = omhl_in(&p, &["hooks", "post-commit"]).run();

    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);
    assert_eq!(first.stdout, "{\"consumed\":true}\n");
    assert_eq!(first.stderr, "");
    assert!(!mark_left, "the first post-commit consumed the mark");
    assert_eq!(second.code, Some(0), "stderr: {}", second.stderr);
    assert_eq!(second.stdout, "{\"consumed\":false}\n");
}

#[test]
fn it_should_refuse_a_pre_push_input_larger_than_the_input_limit() {
    let tmp = TempDir::new().expect("temp dir");
    let ran = tmp.path().join("ran");
    let p = cli_repo(&[("touch", &touch(&ran))]);
    let head = head(&p);
    let feature = "refs/heads/feat/wi-w1__x";
    let mut stdin = pre_push_line(feature, &head, feature).repeat(7_942);
    stdin.push_str(&pre_push_line(
        &format!("refs/tags/{}", "t".repeat(64)),
        &head,
        &format!("refs/tags/{}", "t".repeat(65)),
    ));
    assert_eq!(stdin.len(), 1_048_577);

    let output = omhl_in(
        &p,
        &[
            "check",
            "run",
            "--format",
            "json",
            "--event",
            "before-pr",
            "--pre-push",
        ],
    )
    .stdin(stdin)
    .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(output.stdout, "");
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("push-input-invalid"));
    assert_eq!(
        error["details"][0]["context"]["kind"],
        json!("input-too-large")
    );
    assert!(!ran.exists(), "the check command did not run");
    assert_eq!(attestations(&p), Vec::<Value>::new());
}

/// A pergunta do `gate-pr` com o `label` dado.
fn gate_question(p: &Project, label: &str) -> Value {
    let gate = omhl_json_in(p, &["gate-pr", "work-items.w1"]).run();
    json_line(&gate.stdout)["questions"]
        .as_array()
        .expect("the gate has questions")
        .iter()
        .find(|question| question["label"] == label)
        .unwrap_or_else(|| panic!("the gate asks {label}: {}", gate.stdout))
        .clone()
}

/// Troca o TOML do `gate` no banco por `toml` e volta a árvore para o `HEAD`, sem mudar o commit.
fn load_then_restore_tree(p: &Project, toml: &str) {
    ExtTree::at(p.dir()).check("gate", toml);
    let loaded = omhl_json_in(p, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);
    git_ok(p.dir(), &["checkout", "--", ".omhl/checks"]);
}

#[test]
fn it_should_accept_in_the_gate_an_attestation_sealed_with_the_current_definition() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    let run = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);

    let question = gate_question(&p, "spec-digest-match:checks.gate");

    assert_eq!(question["passed"], json!(true));
}

#[test]
fn it_should_refuse_in_the_gate_an_attestation_sealed_with_an_old_definition() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    let run = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    load_then_restore_tree(&p, GATE_V2);
    assert_eq!(gate_question(&p, "checks.gate")["passed"], json!(true));

    let question = gate_question(&p, "spec-digest-match:checks.gate");

    assert_eq!(question["passed"], json!(false));
}

#[test]
fn it_should_refuse_in_the_gate_a_hand_made_attestation_after_a_load_gives_the_run_back() {
    let p = cli_repo(&[("gate", TEXT_ONLY)]);
    let attested = omhl_json_in(
        &p,
        &[
            "attest",
            "checks.gate",
            "work-items.w1",
            "--outcome",
            "passed",
        ],
    )
    .run();
    assert_eq!(attested.code, Some(0), "attest stderr: {}", attested.stderr);
    load_then_restore_tree(&p, GATE_V1);
    assert_eq!(gate_question(&p, "checks.gate")["passed"], json!(true));

    let sealed = gate_question(&p, "spec-digest-match:checks.gate");

    assert_eq!(sealed["passed"], json!(false));
}

// `check retire`

/// Apaga o arquivo do check da árvore e commita: o node segue vigente no banco, órfão.
fn drop_check_file(p: &Project, slug: &str) {
    fs::remove_file(p.dir().join(format!(".omhl/checks/{slug}.toml")))
        .expect("the check file is deleted");
    git_ok(p.dir(), &["add", "-A"]);
    git_ok(p.dir(), &["commit", "-q", "-m", "drop check"]);
}

#[test]
fn it_should_retire_an_orphaned_check_and_stop_warning_about_it() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    drop_check_file(&p, "gate");

    let retired = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();
    let run = omhl_in(
        &p,
        &["check", "run", "--format", "json", "--event", "before-pr"],
    )
    .run();
    let node = omhl_json_in(&p, &["get", "checks.gate"]).run();

    assert_eq!(retired.code, Some(0), "stderr: {}", retired.stderr);
    assert_eq!(json_line(&retired.stdout)["retired"], json!("checks.gate"));
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let result = json_line(&run.stdout);
    assert_eq!(
        result["warnings"],
        json!([{ "code": "no-checks-for-event" }])
    );
    assert_eq!(json_line(&node.stdout)["node"], Value::Null);
}

#[test]
fn it_should_keep_the_chain_valid_after_retiring_a_check() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    drop_check_file(&p, "gate");
    let before = json_line(&omhl_json_in(&p, &["verify"]).run().stdout)["total_records"]
        .as_u64()
        .expect("verify counts the records");

    let retired = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();
    let verified = omhl_json_in(&p, &["verify"]).run();

    assert_eq!(retired.code, Some(0), "stderr: {}", retired.stderr);
    let report = json_line(&verified.stdout);
    assert_eq!(report["ok"], json!(true));
    assert_eq!(report["total_records"], json!(before + 1));
}

#[test]
fn it_should_stop_the_gate_from_asking_for_a_retired_check() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    drop_check_file(&p, "gate");
    let asked = |p: &Project| -> Vec<String> {
        let gate = omhl_json_in(p, &["gate-pr", "work-items.w1"]).run();
        json_line(&gate.stdout)["questions"]
            .as_array()
            .expect("the gate has questions")
            .iter()
            .filter_map(|question| question["label"].as_str().map(str::to_owned))
            .collect()
    };
    assert!(asked(&p).contains(&"checks.gate".to_owned()));

    let retired = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();

    assert_eq!(retired.code, Some(0), "stderr: {}", retired.stderr);
    let labels = asked(&p);
    assert!(
        labels.iter().all(|label| !label.contains("checks.gate")),
        "labels: {labels:?}"
    );
}

#[test]
fn it_should_refuse_to_retire_a_check_whose_file_still_exists() {
    let p = cli_repo(&[("gate", GATE_V1)]);

    let output = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    let error = error_of(&output.stderr);
    assert_eq!(error["code"], json!("check-not-orphaned"));
    assert_eq!(error["details"][0]["path"], json!("/check"));
    let node = omhl_json_in(&p, &["get", "checks.gate"]).run();
    assert_ne!(json_line(&node.stdout)["node"], Value::Null);
}

#[test]
fn it_should_refuse_to_retire_an_unknown_or_already_retired_check() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    drop_check_file(&p, "gate");
    let first = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();

    let again = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();
    let unknown = omhl_json_in(&p, &["check", "retire", "checks.nope"]).run();

    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);
    assert_eq!(again.code, Some(1), "stderr: {}", again.stderr);
    assert_eq!(error_of(&again.stderr)["code"], json!("check-not-found"));
    assert_eq!(unknown.code, Some(1), "stderr: {}", unknown.stderr);
    assert_eq!(error_of(&unknown.stderr)["code"], json!("check-not-found"));
}

#[test]
fn it_should_report_a_retired_check_as_skipped_when_its_file_comes_back() {
    let p = cli_repo(&[("gate", GATE_V1)]);
    drop_check_file(&p, "gate");
    let retired = omhl_json_in(&p, &["check", "retire", "checks.gate"]).run();
    ExtTree::at(p.dir()).check("gate", GATE_V1);

    let loaded = omhl_json_in(&p, &["load"]).run();

    assert_eq!(retired.code, Some(0), "stderr: {}", retired.stderr);
    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    let report = json_line(&loaded.stdout);
    assert_eq!(
        report["skipped"],
        json!([{ "id": "checks.gate", "code": "extension-retired" }])
    );
    assert_eq!(report["created"], json!([]));
}
