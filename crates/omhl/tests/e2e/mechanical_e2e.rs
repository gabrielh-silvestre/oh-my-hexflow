use crate::support;

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::assert_resolved_command;
use support::child::{Cmd, Output as OmhlOutput, omhl_json};
use support::git::{
    expect_git_success, git_command_with_env, git_ok, git_with_env, write_executable,
};
use tempfile::TempDir;

const BEFORE_COMMIT: &str = "before-commit";
const BEFORE_PR: &str = "before-push";
const BEFORE_MERGE: &str = "before-merge";

const RED_RUN: &str = r#"["sh", "-c", "exit 1"]"#;
const GREEN_RUN: &str = r#"["true"]"#;
const SLOW_RUN: &str = r#"["sh", "-c", "sleep 1"]"#;
const CI_BEHIND_SH_RUN: &str = r#"["sh", "-c", "gh pr checks"]"#;
const CI_DIRECT_RUN: &str = r#"["gh", "pr", "checks"]"#;

const WORK_ITEM: &str = "work-items.w1";
const WORK_ITEM_BRANCH: &str = "feat/wi-w1__x";
const OPEN_WORK_ITEM: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.w1","type":"work-item","data":{"title":"t","status":"open","type":"chore"}}}]}"#;

const GATE: &str = "checks.gate";
const PR: &str = "checks.pr";
const MERGE: &str = "checks.merge";
const CI: &str = "checks.ci";
const SLOW: &str = "checks.slow";

const FAKE_GH: &str = "#!/bin/sh\necho 'all checks passed'\nexit 0\n";

const BRANCH_PROTECTED_HINT: &str =
    "main receives no direct changes; create a branch type/wi-<id>__<slug>";
const PUSH_TO_MAIN_HINT: &str = "main receives no direct pushes; open a pull request";
const PUSH_REF_NOT_CHECKED_OUT_HINT: &str =
    "push refs/heads/feat/wi-w2__y from its own worktree; the pre-push hook attests only HEAD";
const SPAWN_NOT_FOUND_HINT: &str = "`gh` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again";

const POLL: Duration = Duration::from_millis(20);
const HOOK_START_LIMIT: Duration = Duration::from_secs(10);
const SIGNAL_LIMIT: Duration = Duration::from_secs(4);

// Apoio

/// O TOML de check da 6.1-A; os checks diferem só em `run`, `timeout_secs` e `when`.
fn check_toml(run: &str, timeout_secs: u32, event: &str) -> String {
    format!(
        "concern = \"style\"\n\
         variant = \"format\"\n\
         blocking = true\n\
         how = \"Fails on purpose until the owner fixes it.\"\n\
         run = {run}\n\
         timeout_secs = {timeout_secs}\n\
         expect = [{{ exit = 0, outcome = \"passed\" }}]\n\
         when = {{ event = \"{event}\" }}\n"
    )
}

fn red_gate() -> (&'static str, String) {
    ("gate", check_toml(RED_RUN, 60, BEFORE_COMMIT))
}

fn green_gate() -> (&'static str, String) {
    ("gate", check_toml(GREEN_RUN, 60, BEFORE_COMMIT))
}

fn slow_gate() -> (&'static str, String) {
    ("gate", check_toml(SLOW_RUN, 60, BEFORE_COMMIT))
}

fn pr_check() -> (&'static str, String) {
    ("pr", check_toml(GREEN_RUN, 60, BEFORE_PR))
}

fn merge_check() -> (&'static str, String) {
    ("merge", check_toml(GREEN_RUN, 60, BEFORE_MERGE))
}

fn ci_behind_sh() -> (&'static str, String) {
    ("ci", check_toml(CI_BEHIND_SH_RUN, 60, BEFORE_MERGE))
}

fn ci_direct() -> (&'static str, String) {
    ("ci", check_toml(CI_DIRECT_RUN, 60, BEFORE_MERGE))
}

fn slow_signal(seconds: &str) -> (&'static str, String) {
    let run = format!(r#"["sleep", "{seconds}"]"#);
    ("slow", check_toml(&run, 600, BEFORE_COMMIT))
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A linha JSON `{"error"...}` do stderr de um `omhl check run --format json` direto.
fn json_error(stderr: &str) -> Value {
    let line = stderr
        .lines()
        .find(|line| line.starts_with("{\"error\""))
        .unwrap_or_else(|| panic!("no error line in stderr: {stderr}"));
    serde_json::from_str(line).expect("the error line is JSON")
}

/// O `code` do erro no texto do hook: a linha `✘ <code> · ...` ou `✘ BLOCKED · <code> · ...`.
fn hook_error_code(stderr: &str) -> String {
    let header = stderr
        .lines()
        .find_map(|line| line.strip_prefix("✘ "))
        .unwrap_or_else(|| panic!("no error line in stderr: {stderr}"));
    let header = header.strip_prefix("BLOCKED · ").unwrap_or(header);
    header.split(" · ").next().unwrap_or_default().to_owned()
}

fn is_lower_hex_64(value: &Value) -> bool {
    value.as_str().is_some_and(|hash| {
        hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn find_on_path(name: &str) -> PathBuf {
    let path = std::env::var_os("PATH").expect("the test process has a PATH");
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("{name} must be on the PATH to run these tests"))
}

/// Repete `done` a cada 20 ms até ele dar `true` ou o prazo vencer; diz se deu `true`.
fn poll_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(POLL);
    }
}

/// Os pids cujo `cmdline` é `argv` e cujo ambiente traz `XDG_DATA_HOME=<xdg>`: o filtro do
/// ambiente separa este teste de um `mechanical_e2e` que rode ao mesmo tempo em outra worktree.
fn matching_processes(argv: &[u8], xdg: &Path) -> Vec<u32> {
    let wanted = [b"XDG_DATA_HOME=".as_slice(), xdg.as_os_str().as_bytes()].concat();
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|cmdline| cmdline == argv)
                && fs::read(format!("/proc/{pid}/environ"))
                    .is_ok_and(|environ| environ.split(|b| *b == 0).any(|kv| kv == wanted))
        })
        .collect()
}

fn send_signal(args: &[&str]) {
    let status = Command::new("kill")
        .args(args)
        .status()
        .expect("the kill binary runs");
    assert!(status.success(), "kill {args:?} failed");
}

/// Dono do `git` filho: quando o teste termina, com falha ou não, mata e colhe o `git` se ele
/// ainda vive e manda `KILL` aos pids que casam naquele momento (relidos, nunca lembrados: um pid
/// lembrado pode já ser de outro processo).
struct Reaper<'a> {
    argv: &'a [u8],
    xdg: &'a Path,
    git: Child,
}

impl Drop for Reaper<'_> {
    fn drop(&mut self) {
        if let Ok(None) = self.git.try_wait() {
            let _ = self.git.kill();
            let _ = self.git.wait();
        }
        for pid in matching_processes(self.argv, self.xdg) {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// Espera o `git` sair (a cada 20 ms, até `limit`); se não sair, mata e colhe, e devolve `None`.
fn wait_or_kill(git: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let mut status = None;
    poll_until(limit, || {
        status = git.try_wait().expect("the git child is polled");
        status.is_some()
    });
    if status.is_none() {
        let _ = git.kill();
        let _ = git.wait();
    }
    status
}

// Fixture

/// `T` temporário com `R` (repositório), `X` (`XDG_DATA_HOME`), `H` (`HOME`) e, quando pedido,
/// `REMOTE` (repositório nu ligado como `origin`); já com o work item aberto, os checks
/// carregados e os hooks instalados.
struct E2e {
    _t: TempDir,
    t: PathBuf,
    r: PathBuf,
    x: PathBuf,
    h: PathBuf,
    remote: PathBuf,
}

impl E2e {
    fn new(checks: &[(&str, String)]) -> E2e {
        E2e::build(checks, false)
    }

    fn with_remote(checks: &[(&str, String)]) -> E2e {
        E2e::build(checks, true)
    }

    fn build(checks: &[(&str, String)], with_remote: bool) -> E2e {
        let t_dir = TempDir::new().expect("temp dir");
        let t = t_dir.path().to_path_buf();
        let e = E2e {
            r: t.join("repo"),
            x: t.join("xdg"),
            h: t.join("home"),
            remote: t.join("remote.git"),
            t,
            _t: t_dir,
        };
        for dir in [&e.r, &e.x, &e.h] {
            fs::create_dir_all(dir).expect("create fixture directory");
        }

        e.setup_git(&e.r, &["init", "-b", "main"]);
        let toplevel = e.setup_git(&e.r, &["rev-parse", "--show-toplevel"]);
        assert_eq!(
            PathBuf::from(toplevel.trim()),
            e.r.canonicalize().expect("canonical repo path"),
            "the test repository must be the throwaway one, not the working tree"
        );
        e.setup_git(&e.r, &["config", "user.name", "Test Author"]);
        e.setup_git(&e.r, &["config", "user.email", "author@example.com"]);
        e.write(&e.r, ".omhl.toml", "version = 1\ntenant = \"acme\"\n");
        for (name, toml) in checks {
            e.write(&e.r, &format!(".omhl/checks/{name}.toml"), toml);
        }
        e.write(&e.r, "f", "x\n");
        e.write(&e.r, "g", "x\n");
        e.setup_git(&e.r, &["add", "-A"]);
        e.setup_git(&e.r, &["commit", "-m", "init"]);
        e.setup_git(&e.r, &["switch", "-c", WORK_ITEM_BRANCH]);
        if with_remote {
            e.setup_git(&e.t, &["init", "--bare", "remote.git"]);
            let url = format!("file://{}", e.remote.display());
            e.setup_git(&e.r, &["remote", "add", "origin", &url]);
        }

        let appended = e.omhl(&["append"]).stdin(OPEN_WORK_ITEM).run();
        assert_eq!(appended.code, Some(0), "append: {}", appended.stderr);
        let loaded = e.omhl(&["load"]).run();
        assert_eq!(loaded.code, Some(0), "load: {}", loaded.stderr);
        let installed = e.omhl(&["hooks", "install", "--format", "json"]).run();
        assert_eq!(
            installed.code,
            Some(0),
            "hooks install: {}",
            installed.stderr
        );
        assert_eq!(
            json_line(&installed.stdout)["warnings"][0]["code"],
            json!("binary-in-build-dir")
        );
        e
    }

    /// Só a montagem anterior ao `hooks install`; depois dele, todo `git` vai por `git`/`git_ok`.
    fn setup_git(&self, dir: &Path, args: &[&str]) -> String {
        git_ok(dir, args)
    }

    fn git(&self, dir: &Path, args: &[&str]) -> Output {
        git_with_env(dir, &self.x, &self.h, args)
    }

    fn git_ok(&self, dir: &Path, args: &[&str]) -> String {
        expect_git_success(&self.git(dir, args), args)
            .trim()
            .to_owned()
    }

    fn omhl(&self, args: &[&str]) -> Cmd {
        omhl_json(args)
            .cwd(&self.r)
            .xdg(&self.x)
            .env("HOME", &self.h)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
    }

    fn write(&self, dir: &Path, rel: &str, content: &str) {
        let target = dir.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(target, content).expect("write file");
    }

    fn attestations(&self, check: &str) -> Vec<Value> {
        let listed = self.omhl(&["list", "--type", "attestation"]).run();
        assert_eq!(listed.code, Some(0), "list: {}", listed.stderr);
        json_line(&listed.stdout)["items"]
            .as_array()
            .expect("items is an array")
            .iter()
            .filter(|item| item["data"]["check"] == json!(check))
            .cloned()
            .collect()
    }

    fn tree_of_head(&self, dir: &Path) -> String {
        self.git_ok(dir, &["rev-parse", "HEAD^{tree}"])
    }

    fn head(&self, dir: &Path) -> String {
        self.git_ok(dir, &["rev-parse", "HEAD"])
    }

    fn count(&self, dir: &Path, rev: &str) -> String {
        self.git_ok(dir, &["rev-list", "--count", rev])
    }

    fn remote_ref(&self, reference: &str) -> Output {
        let remote = self.remote.to_str().expect("utf-8 remote path");
        self.git(
            &self.r,
            &[
                "--git-dir",
                remote,
                "rev-parse",
                "--verify",
                "-q",
                reference,
            ],
        )
    }

    /// `T/fakebin` (com o `gh` falso) seguido do `PATH` do processo de teste.
    fn path_with_fake_gh(&self) -> OsString {
        let fakebin = self.t.join("fakebin");
        fs::create_dir_all(&fakebin).expect("create fakebin");
        write_executable(&fakebin.join("gh"), FAKE_GH);
        let inherited = std::env::var_os("PATH").expect("the test process has a PATH");
        let dirs = std::iter::once(fakebin).chain(std::env::split_paths(&inherited));
        std::env::join_paths(dirs).expect("join PATH")
    }

    /// `T/onlysh`, só com `sh` e `git`: `gh` não existe nesse `PATH`.
    fn path_without_gh(&self) -> PathBuf {
        let onlysh = self.t.join("onlysh");
        fs::create_dir_all(&onlysh).expect("create onlysh");
        for name in ["sh", "git"] {
            symlink(find_on_path(name), onlysh.join(name)).expect("link binary");
        }
        onlysh
    }

    /// `echo <content> > <rel>` e `git add <rel>` em `dir`.
    fn stage(&self, dir: &Path, rel: &str, content: &str) {
        self.write(dir, rel, content);
        self.git_ok(dir, &["add", rel]);
    }
}

struct RedThenGreen {
    t5: String,
    blocked: Output,
    count_after_blocked: String,
    load: OmhlOutput,
    passed: Output,
}

/// Passos 5 a 7 da 6.1-A: commit barrado pelo `gate` vermelho, `gate` trocado para `["true"]` e
/// recarregado, commit que passa.
fn red_then_green(e: &E2e) -> RedThenGreen {
    e.stage(&e.r, "f2", "x\n");
    let blocked = e.git(&e.r, &["commit", "-m", "x"]);
    let t5 = e.git_ok(&e.r, &["write-tree"]);
    let count_after_blocked = e.count(&e.r, "HEAD");

    let gate_path = e.r.join(".omhl/checks/gate.toml");
    let red = fs::read_to_string(&gate_path).expect("read gate.toml");
    fs::write(&gate_path, red.replace(RED_RUN, GREEN_RUN)).expect("write gate.toml");
    let load = e.omhl(&["load"]).run();

    e.git_ok(&e.r, &["add", "-A"]);
    let passed = e.git(&e.r, &["commit", "-m", "x"]);
    RedThenGreen {
        t5,
        blocked,
        count_after_blocked,
        load,
        passed,
    }
}

struct NoVerifyFlow {
    commit: Output,
    push: Output,
    merge_run: OmhlOutput,
    gate_delivery: OmhlOutput,
}

/// O Act do cenário de ponta a ponta: commit com `--no-verify`, push pelo hook, `before-merge` e `gate-delivery`.
fn no_verify_flow(e: &E2e) -> NoVerifyFlow {
    e.stage(&e.r, "n", "n\n");
    let commit = e.git(&e.r, &["commit", "--no-verify", "-m", "n"]);
    let push = e.git(&e.r, &["push", "origin", WORK_ITEM_BRANCH]);
    let merge_run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_MERGE])
        .run();
    let gate_delivery = e.omhl(&["gate-delivery", WORK_ITEM]).run();
    NoVerifyFlow {
        commit,
        push,
        merge_run,
        gate_delivery,
    }
}

fn snapshot(kind: &str, id: &str) -> Value {
    json!({ "kind": kind, "id": id })
}

fn has_passed_at(attestations: &[Value], snap: &Value) -> bool {
    attestations
        .iter()
        .any(|item| item["data"]["outcome"] == json!("passed") && item["snapshot"] == *snap)
}

// Testes

#[test]
fn it_should_block_a_commit_when_a_check_fails_and_allow_it_when_it_passes() {
    let e = E2e::new(&[red_gate()]);

    let flow = red_then_green(&e);

    assert_ne!(flow.blocked.status.code(), Some(0));
    assert!(
        text(&flow.blocked.stderr)
            .lines()
            .any(|line| line.starts_with("✘ BLOCKED · before-commit · ")),
        "stderr: {}",
        text(&flow.blocked.stderr)
    );
    assert_eq!(flow.count_after_blocked, "1");
    assert_eq!(flow.load.code, Some(0), "load: {}", flow.load.stderr);
    assert_eq!(json_line(&flow.load.stdout)["revised"], json!([GATE]));
    assert_eq!(
        flow.passed.status.code(),
        Some(0),
        "stderr: {}",
        text(&flow.passed.stderr)
    );
    assert_eq!(e.count(&e.r, "HEAD"), "2");
}

#[test]
fn it_should_leave_the_attestation_tied_to_the_tree_of_the_new_commit() {
    let e = E2e::new(&[red_gate()]);
    let flow = red_then_green(&e);

    let attestations = e.attestations(GATE);

    let tree = e.tree_of_head(&e.r);
    let failed: Vec<&Value> = attestations
        .iter()
        .filter(|item| item["data"]["outcome"] == json!("failed"))
        .collect();
    let passed: Vec<&Value> = attestations
        .iter()
        .filter(|item| item["data"]["outcome"] == json!("passed"))
        .collect();
    assert_eq!(attestations.len(), 2, "attestations: {attestations:?}");
    assert_eq!(failed.len(), 1);
    assert_eq!(passed.len(), 1);
    assert_eq!(failed[0]["snapshot"], snapshot("git-tree", &flow.t5));
    assert_eq!(passed[0]["data"]["exit_code"], json!(0));
    assert_resolved_command(&passed[0]["data"]["command"], "true", &[]);
    assert_eq!(passed[0]["snapshot"], snapshot("git-tree", &tree));
    assert_ne!(flow.t5, tree);
    let executor = json!({ "agent": "executor", "client": "executor" });
    assert_eq!(failed[0]["author"], executor);
    assert_eq!(passed[0]["author"], executor);
}

#[test]
fn it_should_work_for_a_plain_commit() {
    let e = E2e::new(&[green_gate()]);
    e.stage(&e.r, "h", "y\n");

    let commit = e.git(&e.r, &["commit", "-m", "h"]);

    assert_eq!(
        commit.status.code(),
        Some(0),
        "stderr: {}",
        text(&commit.stderr)
    );
    let tree = snapshot("git-tree", &e.tree_of_head(&e.r));
    assert!(has_passed_at(&e.attestations(GATE), &tree));
}

#[test]
fn it_should_work_for_commit_dash_a() {
    let e = E2e::new(&[green_gate()]);
    e.write(&e.r, "f", "y\n");

    let commit = e.git(&e.r, &["commit", "-a", "-m", "a"]);

    assert_eq!(
        commit.status.code(),
        Some(0),
        "stderr: {}",
        text(&commit.stderr)
    );
    let tree = snapshot("git-tree", &e.tree_of_head(&e.r));
    assert!(has_passed_at(&e.attestations(GATE), &tree));
}

#[test]
fn it_should_work_for_commit_with_a_path() {
    let e = E2e::new(&[green_gate()]);
    e.write(&e.r, "f", "z\n");

    let commit = e.git(&e.r, &["commit", "-m", "p", "f"]);

    assert_eq!(
        commit.status.code(),
        Some(0),
        "stderr: {}",
        text(&commit.stderr)
    );
    let tree = snapshot("git-tree", &e.tree_of_head(&e.r));
    assert!(has_passed_at(&e.attestations(GATE), &tree));
}

#[test]
fn it_should_work_for_a_commit_made_from_a_linked_worktree() {
    let e = E2e::new(&[green_gate()]);
    let wt = e.t.join("wt");
    let wt_arg = wt.to_str().expect("utf-8 worktree path");
    e.git_ok(&e.r, &["worktree", "add", "-b", "feat/wi-w1__wt", wt_arg]);
    e.stage(&wt, "h", "w\n");

    let commit = e.git(&wt, &["commit", "-m", "w"]);

    assert_eq!(
        commit.status.code(),
        Some(0),
        "stderr: {}",
        text(&commit.stderr)
    );
    let tree = snapshot("git-tree", &e.tree_of_head(&wt));
    assert!(has_passed_at(&e.attestations(GATE), &tree));
}

#[test]
fn it_should_block_a_commit_with_a_path_while_another_tracked_file_is_modified() {
    let e = E2e::new(&[green_gate()]);
    e.write(&e.r, "f", "1\n");
    e.write(&e.r, "g", "2\n");

    let commit = e.git(&e.r, &["commit", "-m", "p", "f"]);

    assert_ne!(commit.status.code(), Some(0));
    assert_eq!(
        hook_error_code(&text(&commit.stderr)),
        "worktree-differs-from-index"
    );
    assert_eq!(e.count(&e.r, "HEAD"), "1");
    assert_eq!(e.attestations(GATE), Vec::<Value>::new());
}

#[test]
fn it_should_fire_before_pr_on_push_to_a_local_remote() {
    let e = E2e::with_remote(&[green_gate(), pr_check()]);
    e.stage(&e.r, "h", "y\n");
    e.git_ok(&e.r, &["commit", "-m", "h"]);

    let push = e.git(&e.r, &["push", "origin", WORK_ITEM_BRANCH]);

    let head = e.head(&e.r);
    assert_eq!(
        push.status.code(),
        Some(0),
        "stderr: {}",
        text(&push.stderr)
    );
    assert!(has_passed_at(
        &e.attestations(PR),
        &snapshot("git-commit", &head)
    ));
    let remote = e.remote_ref("refs/heads/feat/wi-w1__x");
    assert_eq!(text(&remote.stdout).trim(), head);
}

#[test]
fn it_should_fail_the_pull_request_gate_for_a_commit_made_with_no_verify() {
    let e = E2e::with_remote(&[green_gate(), pr_check(), merge_check()]);

    let flow = no_verify_flow(&e);

    assert_eq!(
        flow.commit.status.code(),
        Some(0),
        "stderr: {}",
        text(&flow.commit.stderr)
    );
    assert_eq!(
        flow.push.status.code(),
        Some(0),
        "stderr: {}",
        text(&flow.push.stderr)
    );
    assert_eq!(
        flow.merge_run.code,
        Some(0),
        "stderr: {}",
        flow.merge_run.stderr
    );
    assert_eq!(
        flow.gate_delivery.code,
        Some(0),
        "stderr: {}",
        flow.gate_delivery.stderr
    );
    let report = json_line(&flow.gate_delivery.stdout);
    assert_eq!(report["gate"], json!("delivery"));
    assert_eq!(report["passed"], json!(false));
    let tree = e.tree_of_head(&e.r);
    assert!(
        e.attestations(GATE)
            .iter()
            .all(|item| item["snapshot"]["id"] != json!(tree))
    );
}

#[test]
fn it_should_recover_the_missing_attestation_by_running_the_event_manually_on_a_clean_tree() {
    let e = E2e::with_remote(&[green_gate(), pr_check(), merge_check()]);
    no_verify_flow(&e);
    let status = e.git_ok(&e.r, &["status", "--porcelain"]);

    let commit_run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_COMMIT])
        .run();
    let gate_delivery = e.omhl(&["gate-delivery", WORK_ITEM]).run();

    assert_eq!(status, "");
    assert_eq!(commit_run.code, Some(0), "stderr: {}", commit_run.stderr);
    let tree = snapshot("git-tree", &e.tree_of_head(&e.r));
    assert!(has_passed_at(&e.attestations(GATE), &tree));
    assert_eq!(
        gate_delivery.code,
        Some(0),
        "gate-delivery: {}",
        gate_delivery.stderr
    );
    assert_eq!(json_line(&gate_delivery.stdout)["passed"], json!(true));
}

#[test]
fn it_should_pass_the_pull_request_gate_after_the_hooks_attested_the_head_tree_and_commit() {
    let e = E2e::with_remote(&[green_gate(), pr_check(), merge_check()]);
    e.stage(&e.r, "h", "y\n");

    let commit = e.git(&e.r, &["commit", "-m", "h"]);
    let push = e.git(&e.r, &["push", "origin", WORK_ITEM_BRANCH]);
    let merge_run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_MERGE])
        .run();
    let gate_delivery = e.omhl(&["gate-delivery", WORK_ITEM]).run();

    assert_eq!(
        commit.status.code(),
        Some(0),
        "stderr: {}",
        text(&commit.stderr)
    );
    assert_eq!(
        push.status.code(),
        Some(0),
        "stderr: {}",
        text(&push.stderr)
    );
    assert_eq!(merge_run.code, Some(0), "stderr: {}", merge_run.stderr);
    assert_eq!(
        gate_delivery.code,
        Some(0),
        "stderr: {}",
        gate_delivery.stderr
    );
    assert_eq!(json_line(&gate_delivery.stdout)["passed"], json!(true));
    let tree = snapshot("git-tree", &e.tree_of_head(&e.r));
    let commit_snap = snapshot("git-commit", &e.head(&e.r));
    assert!(has_passed_at(&e.attestations(GATE), &tree));
    assert!(has_passed_at(&e.attestations(PR), &commit_snap));
    assert!(has_passed_at(&e.attestations(MERGE), &commit_snap));
}

#[test]
fn it_should_attest_before_merge_through_a_fake_gh() {
    let e = E2e::new(&[ci_behind_sh()]);
    let path = e.path_with_fake_gh();

    let run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_MERGE])
        .env("PATH", path)
        .run();

    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert_eq!(run.stderr, "");
    let attestations = e.attestations(CI);
    assert_eq!(attestations.len(), 1, "attestations: {attestations:?}");
    let data = &attestations[0]["data"];
    assert_eq!(data["outcome"], json!("passed"));
    assert_eq!(data["ok"], json!(true));
    assert_eq!(data["exit_code"], json!(0));
    assert_resolved_command(&data["command"], "sh", &["-c", "gh pr checks"]);
    assert!(
        is_lower_hex_64(&data["byproduct"]),
        "byproduct: {}",
        data["byproduct"]
    );
    assert_eq!(
        attestations[0]["snapshot"],
        snapshot("git-commit", &e.head(&e.r))
    );
}

#[test]
fn it_should_record_a_failed_attestation_with_exit_code_127_when_gh_is_missing_behind_sh() {
    let e = E2e::new(&[ci_behind_sh()]);
    let path = e.path_without_gh();

    let run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_MERGE])
        .env("PATH", &path)
        .run();

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    assert_eq!(run.stdout, "");
    assert_eq!(
        json_error(&run.stderr)["error"]["code"],
        json!("checks-failed")
    );
    let attestations = e.attestations(CI);
    assert_eq!(attestations.len(), 1, "attestations: {attestations:?}");
    let data = &attestations[0]["data"];
    assert_eq!(data["outcome"], json!("failed"));
    assert_eq!(data["ok"], json!(false));
    assert_eq!(data["exit_code"], json!(127));
    assert_resolved_command(&data["command"], "sh", &["-c", "gh pr checks"]);
    assert_eq!(data["timed_out"], json!(false));
}

#[test]
fn it_should_report_a_missing_binary_as_a_stopped_run_without_attestation() {
    let e = E2e::new(&[ci_direct()]);
    let path = e.path_without_gh();

    let run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_MERGE])
        .env("PATH", &path)
        .run();

    assert_eq!(run.code, Some(1), "stderr: {}", run.stderr);
    assert_eq!(run.stdout, "");
    let error = &json_error(&run.stderr)["error"];
    assert_eq!(error["code"], json!("check-not-run"));
    assert_eq!(error["details"][0]["code"], json!("spawn-failed"));
    assert_eq!(
        error["details"][0]["context"]["kind"],
        json!("spawn-not-found")
    );
    assert_eq!(
        error["details"][0]["context"]["argv"],
        json!(["gh", "pr", "checks"])
    );
    assert!(
        run.stderr.contains(SPAWN_NOT_FOUND_HINT),
        "stderr: {}",
        run.stderr
    );
    assert_eq!(e.attestations(CI), Vec::<Value>::new());
}

#[test]
fn it_should_leave_omhl_verify_passing_after_the_full_flow() {
    let e = E2e::with_remote(&[red_gate(), pr_check(), merge_check()]);
    let flow = red_then_green(&e);
    assert_eq!(
        flow.passed.status.code(),
        Some(0),
        "green commit: {}",
        text(&flow.passed.stderr)
    );
    e.git_ok(&e.r, &["push", "origin", WORK_ITEM_BRANCH]);
    let merge_run = e
        .omhl(&["check", "run", "--format", "json", "--event", BEFORE_MERGE])
        .run();
    assert_eq!(
        merge_run.code,
        Some(0),
        "before-merge: {}",
        merge_run.stderr
    );

    let verify = e.omhl(&["verify"]).run();

    assert_eq!(verify.code, Some(0), "stderr: {}", verify.stderr);
    let report = json_line(&verify.stdout);
    assert_eq!(report["ok"], json!(true));
    assert_eq!(report["breaks"], json!([]));
    assert_eq!(report["total_breaks"], json!(0));
}

#[test]
fn it_should_serialize_two_commits_in_two_linked_worktrees_against_one_database() {
    let e = E2e::new(&[slow_gate()]);
    let wt = e.t.join("wt");
    let wt_arg = wt.to_str().expect("utf-8 worktree path");
    e.git_ok(&e.r, &["worktree", "add", "-b", "feat/wi-w1__b", wt_arg]);
    e.stage(&e.r, "a", "a\n");
    e.stage(&wt, "b", "b\n");

    let (first, second) = thread::scope(|scope| {
        let in_repo = scope.spawn(|| e.git(&e.r, &["commit", "-m", "c"]));
        let in_worktree = scope.spawn(|| e.git(&wt, &["commit", "-m", "c"]));
        (
            in_repo.join().expect("the first commit thread finishes"),
            in_worktree
                .join()
                .expect("the second commit thread finishes"),
        )
    });

    assert_eq!(
        first.status.code(),
        Some(0),
        "stderr: {}",
        text(&first.stderr)
    );
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr: {}",
        text(&second.stderr)
    );
    assert!(!text(&first.stderr).contains("database is busy"));
    assert!(!text(&second.stderr).contains("database is busy"));
    let mut attested: Vec<Value> = e
        .attestations(GATE)
        .iter()
        .filter(|item| item["data"]["outcome"] == json!("passed"))
        .map(|item| item["snapshot"]["id"].clone())
        .collect();
    attested.sort_by_key(Value::to_string);
    let mut expected = vec![json!(e.tree_of_head(&e.r)), json!(e.tree_of_head(&wt))];
    expected.sort_by_key(Value::to_string);
    assert_eq!(attested, expected);
}

#[test]
fn it_should_block_a_commit_on_main_with_branch_protected() {
    let e = E2e::new(&[green_gate()]);
    e.git_ok(&e.r, &["switch", "main"]);

    let commit = e.git(&e.r, &["commit", "--allow-empty", "-m", "y"]);

    let stderr = text(&commit.stderr);
    assert_ne!(commit.status.code(), Some(0));
    assert_eq!(hook_error_code(&stderr), "branch-protected");
    assert!(stderr.contains(BRANCH_PROTECTED_HINT), "stderr: {stderr}");
    assert_eq!(e.count(&e.r, "main"), "1");
}

#[test]
fn it_should_block_a_push_to_main_and_allow_a_push_of_the_work_item_branch() {
    let e = E2e::with_remote(&[green_gate(), pr_check()]);
    e.stage(&e.r, "h", "y\n");
    e.git_ok(&e.r, &["commit", "-m", "h"]);

    let to_main = e.git(&e.r, &["push", "origin", "feat/wi-w1__x:main"]);
    let remote_main = e.remote_ref("refs/heads/main");
    let to_branch = e.git(&e.r, &["push", "origin", WORK_ITEM_BRANCH]);

    let stderr = text(&to_main.stderr);
    assert_ne!(to_main.status.code(), Some(0));
    assert_eq!(hook_error_code(&stderr), "branch-protected");
    assert!(stderr.contains("kind: push-to-main"), "stderr: {stderr}");
    assert!(stderr.contains(PUSH_TO_MAIN_HINT), "stderr: {stderr}");
    assert_eq!(remote_main.status.code(), Some(1));
    assert_eq!(
        to_branch.status.code(),
        Some(0),
        "stderr: {}",
        text(&to_branch.stderr)
    );
    let remote_branch = e.remote_ref("refs/heads/feat/wi-w1__x");
    assert_eq!(text(&remote_branch.stdout).trim(), e.head(&e.r));
}

#[test]
fn it_should_leave_no_orphan_when_the_hook_is_interrupted() {
    let _sweep = support::sweep_lock::share_sweeps();
    let e = E2e::new(&[slow_signal("307")]);
    e.stage(&e.r, "s", "s\n");
    let argv = b"sleep\x00307\x00";

    let mut run = Reaper {
        argv,
        xdg: &e.x,
        git: git_command_with_env(&e.r, &e.x, &e.h, &["commit", "-m", "s"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("git commit starts"),
    };
    let started = poll_until(HOOK_START_LIMIT, || {
        !matching_processes(argv, &e.x).is_empty()
    });
    send_signal(&["-INT", "--", &format!("-{}", run.git.id())]);
    let signaled = Instant::now();
    let status = wait_or_kill(&mut run.git, SIGNAL_LIMIT);
    let no_orphan = poll_until(SIGNAL_LIMIT.saturating_sub(signaled.elapsed()), || {
        matching_processes(argv, &e.x).is_empty()
    });

    assert!(started, "the hook never started sleep 307");
    assert!(
        status.is_some_and(|status| !status.success()),
        "git status: {status:?}"
    );
    assert!(no_orphan, "sleep 307 outlived the interrupted hook");
    assert_eq!(e.count(&e.r, "HEAD"), "1");
    assert_eq!(e.attestations(SLOW), Vec::<Value>::new());
}

#[test]
fn it_should_leave_no_orphan_when_the_git_process_is_killed_during_the_hook() {
    let _sweep = support::sweep_lock::share_sweeps();
    let e = E2e::new(&[slow_signal("308")]);
    e.stage(&e.r, "s", "s\n");
    let argv = b"sleep\x00308\x00";

    let mut run = Reaper {
        argv,
        xdg: &e.x,
        git: git_command_with_env(&e.r, &e.x, &e.h, &["commit", "-m", "s"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("git commit starts"),
    };
    let started = poll_until(HOOK_START_LIMIT, || {
        !matching_processes(argv, &e.x).is_empty()
    });
    send_signal(&["-TERM", &run.git.id().to_string()]);
    let no_orphan = poll_until(SIGNAL_LIMIT, || matching_processes(argv, &e.x).is_empty());
    wait_or_kill(&mut run.git, SIGNAL_LIMIT);

    assert!(started, "the hook never started sleep 308");
    assert!(no_orphan, "sleep 308 outlived the killed git process");
    assert_eq!(e.count(&e.r, "HEAD"), "1");
    assert_eq!(e.attestations(SLOW), Vec::<Value>::new());
}

#[test]
fn it_should_delete_the_develop_mark_only_after_the_first_commit_on_develop() {
    let e = E2e::new(&[green_gate()]);
    e.git_ok(&e.r, &["switch", "-c", "develop"]);
    let git_dir = PathBuf::from(e.git_ok(&e.r, &["rev-parse", "--absolute-git-dir"]));
    let mark = git_dir.join("omhl-develop-mark");
    fs::write(&mark, "omhl-develop-mark v1\n").expect("write the develop mark");
    e.stage(&e.r, "d", "d\n");

    let aborted = e.git(&e.r, &["commit", "-m", ""]);
    let mark_after_abort = mark.exists();
    let committed = e.git(&e.r, &["commit", "-m", "d"]);
    let mark_after_commit = mark.exists();

    assert_eq!(
        aborted.status.code(),
        Some(1),
        "stderr: {}",
        text(&aborted.stderr)
    );
    assert!(mark_after_abort);
    assert_eq!(
        committed.status.code(),
        Some(0),
        "stderr: {}",
        text(&committed.stderr)
    );
    assert!(!mark_after_commit);
    assert_eq!(e.attestations(GATE), Vec::<Value>::new());
}

#[test]
fn it_should_refuse_a_push_of_a_branch_that_is_not_checked_out() {
    let e = E2e::with_remote(&[green_gate(), pr_check()]);
    e.git_ok(&e.r, &["branch", "feat/wi-w2__y"]);

    let push = e.git(&e.r, &["push", "origin", "feat/wi-w2__y"]);

    let stderr = text(&push.stderr);
    assert_ne!(push.status.code(), Some(0));
    assert_eq!(hook_error_code(&stderr), "push-ref-not-checked-out");
    assert!(
        stderr.contains(PUSH_REF_NOT_CHECKED_OUT_HINT),
        "stderr: {stderr}"
    );
    assert_eq!(
        e.remote_ref("refs/heads/feat/wi-w2__y").status.code(),
        Some(1)
    );
}
