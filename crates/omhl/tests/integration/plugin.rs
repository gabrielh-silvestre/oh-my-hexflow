use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

fn plugin_file(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugin")
        .join(relative);
    let text = fs::read_to_string(&path).expect("the plugin file is readable");

    serde_json::from_str(&text).expect("the plugin file is valid JSON")
}

#[test]
fn it_should_declare_the_plugin_name_and_the_cargo_version() {
    let manifest = plugin_file(".claude-plugin/plugin.json");

    assert_eq!(manifest["name"], json!("oh-my-hexflow"));
    assert_eq!(manifest["version"], json!(env!("CARGO_PKG_VERSION")));
}

fn marketplace_entry() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude-plugin/marketplace.json");
    let text = fs::read_to_string(&path).expect("the marketplace file is readable");
    let marketplace: Value = serde_json::from_str(&text).expect("the marketplace is valid JSON");

    marketplace["plugins"][0].clone()
}

#[test]
fn it_should_name_the_marketplace_entry_after_the_plugin_manifest() {
    let manifest = plugin_file(".claude-plugin/plugin.json");

    assert_eq!(marketplace_entry()["name"], manifest["name"]);
}

#[test]
fn it_should_point_the_marketplace_entry_source_to_the_plugin_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = marketplace_entry()["source"]
        .as_str()
        .expect("the source is a path string")
        .to_owned();

    assert_eq!(
        root.join(source).canonicalize().unwrap(),
        root.join("plugin").canonicalize().unwrap()
    );
}

#[test]
fn it_should_omit_the_marketplace_entry_version_or_repeat_the_plugin_manifest_one() {
    let manifest = plugin_file(".claude-plugin/plugin.json");
    let entry = marketplace_entry();

    assert!(entry.get("version").is_none() || entry["version"] == manifest["version"]);
}

const PLUGIN_ROOT_ARG: &str = "${CLAUDE_PLUGIN_ROOT}/hooks/omhl-hook.sh";
const TARBALL: &str = "omhl-x86_64-unknown-linux-musl.tar.gz";
const TARBALL_SHA256: &str = "omhl-x86_64-unknown-linux-musl.tar.gz.sha256";
const NO_PROJECT_DIR: (&str, &str) = ("CLAUDE_PROJECT_DIR", "");

/// A entrada de hook (de qualquer evento) cujo segundo argumento é o nome do hook.
fn entry_for<'a>(hooks: &'a Value, name: &str) -> &'a Value {
    hooks["hooks"]
        .as_object()
        .expect("hooks is an object")
        .values()
        .flat_map(|groups| groups.as_array().expect("an event is a list"))
        .flat_map(|group| group["hooks"].as_array().expect("a group has hooks"))
        .find(|entry| entry["args"][1] == json!(name))
        .unwrap_or_else(|| panic!("no hook entry for {name}"))
}

fn event_group<'a>(hooks: &'a Value, event: &str, matcher: &str) -> &'a Value {
    hooks["hooks"][event]
        .as_array()
        .and_then(|groups| {
            groups
                .iter()
                .find(|group| group["matcher"] == json!(matcher))
        })
        .unwrap_or_else(|| panic!("{event} has no group for {matcher}"))
}

#[test]
fn it_should_wire_the_four_hooks_in_exec_form_through_the_hook_script() {
    let hooks = plugin_file("hooks/hooks.json");

    for name in ["spec-lock", "agent-role", "session-start", "version-check"] {
        let entry = entry_for(&hooks, name);

        assert_eq!(entry["command"], json!("sh"), "{name}");
        assert_eq!(entry["args"], json!([PLUGIN_ROOT_ARG, name]), "{name}");
    }
}

#[test]
fn it_should_not_chain_any_hook_command_with_a_shell_operator() {
    let hooks = plugin_file("hooks/hooks.json");

    for name in ["spec-lock", "agent-role", "session-start", "version-check"] {
        let command = entry_for(&hooks, name)["command"].as_str().unwrap();

        assert!(!command.contains("||"), "{name}: {command}");
    }
}

#[test]
fn it_should_run_the_version_check_beside_the_session_start_hook_on_every_session_event() {
    let hooks = plugin_file("hooks/hooks.json");

    let group = event_group(&hooks, "SessionStart", "startup|resume|clear|compact|fork");

    assert_eq!(group["hooks"][0]["args"][1], json!("session-start"));
    assert_eq!(group["hooks"][0]["timeout"], json!(15));
    assert_eq!(group["hooks"][1]["args"][1], json!("version-check"));
    assert_eq!(group["hooks"][1]["timeout"], json!(5));
}

#[test]
fn it_should_keep_the_pre_tool_use_matchers_and_timeouts() {
    let hooks = plugin_file("hooks/hooks.json");

    let spec_lock = event_group(&hooks, "PreToolUse", "Edit|Write|MultiEdit");
    let agent_role = event_group(&hooks, "PreToolUse", "Bash|Monitor|Agent|Task");

    assert_eq!(spec_lock["hooks"][0]["timeout"], json!(10));
    assert_eq!(agent_role["hooks"][0]["timeout"], json!(10));
}

#[test]
fn it_should_ship_the_hook_script_the_hooks_point_to() {
    assert!(hook_script().is_file());
}

#[test]
fn it_should_wire_the_agent_role_hook_on_four_tool_matchers_beside_the_spec_lock_hook() {
    let hooks = plugin_file("hooks/hooks.json");

    let matchers: Vec<&Value> = hooks["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse is a list")
        .iter()
        .map(|entry| &entry["matcher"])
        .collect();

    assert_eq!(
        matchers,
        [
            &json!("Edit|Write|MultiEdit"),
            &json!("Bash|Monitor|Agent|Task")
        ]
    );
}

fn hook_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/hooks/omhl-hook.sh")
}

struct HookRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Inicia `/bin/sh omhl-hook.sh <nome>` com o ambiente limpo e só o `PATH` dado.
fn spawn_hook(name: &str, path_dir: &Path, cwd: &Path, envs: &[(&str, &str)]) -> Child {
    spawn_script(&hook_script(), name, path_dir, cwd, envs)
}

fn spawn_script(
    script: &Path,
    name: &str,
    path_dir: &Path,
    cwd: &Path,
    envs: &[(&str, &str)],
) -> Child {
    Command::new("/bin/sh")
        .arg(script)
        .arg(name)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", path_dir)
        .envs(envs.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the hook script runs")
}

fn collect(child: Child) -> HookRun {
    let output = child.wait_with_output().expect("the hook script finishes");

    HookRun {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_hook(
    name: &str,
    path_dir: &Path,
    cwd: &Path,
    envs: &[(&str, &str)],
    stdin: &str,
) -> HookRun {
    let mut child = spawn_hook(name, path_dir, cwd, envs);
    // O script pode sair sem ler o stdin; o erro de pipe quebrado não importa.
    let _ = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes());

    collect(child)
}

/// Pasta com um `omhl` falso executável como único programa do `PATH`.
fn fake_omhl(body: &str) -> TempDir {
    let bin = TempDir::new().expect("bin directory is created");
    let fake = bin.path().join("omhl");
    fs::write(&fake, format!("#!/bin/sh\n{body}")).expect("fake omhl is written");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("fake omhl is executable");

    bin
}

/// O falso devolve os argumentos e o stdin que recebeu.
const ECHO_INPUT: &str =
    "printf 'args:%s\\n' \"$*\"\nwhile IFS= read -r line; do printf 'in:%s\\n' \"$line\"; done\n";

fn run_with_fake_omhl(name: &str, body: &str, stdin: &str) -> HookRun {
    let bin = fake_omhl(body);

    run_hook(name, bin.path(), bin.path(), &[], stdin)
}

#[test]
fn it_should_pass_stdin_and_stdout_through_to_omhl_for_each_binary_hook() {
    for name in ["spec-lock", "agent-role", "session-start"] {
        let run = run_with_fake_omhl(
            name,
            &format!("{ECHO_INPUT}exit 0\n"),
            "{\"a\":1}\nsecond line\n",
        );

        assert_eq!(run.code, Some(0), "{name}");
        assert_eq!(
            run.stdout,
            format!("args:hook {name}\nin:{{\"a\":1}}\nin:second line\n"),
            "{name}"
        );
    }
}

#[test]
fn it_should_not_block_bash_when_omhl_does_not_know_the_hook_subcommand() {
    let run = run_with_fake_omhl("agent-role", "exit 2\n", "");

    assert_eq!(run.code, Some(1));
}

#[test]
fn it_should_block_the_edit_when_omhl_exits_with_two() {
    let run = run_with_fake_omhl("spec-lock", "exit 2\n", "");

    assert_eq!(run.code, Some(2));
}

#[test]
fn it_should_block_the_edit_when_omhl_exits_with_one() {
    let run = run_with_fake_omhl("spec-lock", "exit 1\n", "");

    assert_eq!(run.code, Some(2));
}

#[test]
fn it_should_return_the_exit_code_of_omhl_in_session_start() {
    let run = run_with_fake_omhl("session-start", "exit 7\n", "");

    assert_eq!(run.code, Some(7));
}

#[test]
fn it_should_let_the_json_denial_of_omhl_through_with_exit_zero() {
    let denial = r#"{"hookSpecificOutput":{"permissionDecision":"deny"}}"#;

    let run = run_with_fake_omhl("spec-lock", &format!("echo '{denial}'\nexit 0\n"), "");

    assert_eq!(run.code, Some(0));
    assert_eq!(run.stdout.trim_end(), denial);
}

/// Pasta de projeto; com `marked` ela traz o `.omhl.toml`.
fn project(marked: bool) -> TempDir {
    let dir = TempDir::new().expect("project directory is created");
    if marked {
        fs::write(dir.path().join(".omhl.toml"), "").expect("the marker is written");
    }

    dir
}

/// Roda o hook sem `omhl` no `PATH`: o `PATH` aponta para uma pasta vazia.
fn run_without_omhl(name: &str, cwd: &Path, envs: &[(&str, &str)], stdin: &str) -> HookRun {
    let empty = TempDir::new().expect("empty path directory is created");

    run_hook(name, empty.path(), cwd, envs, stdin)
}

fn str_path(path: &Path) -> &str {
    path.to_str().expect("the path is UTF-8")
}

fn assert_denies_with_install_command(run: &HookRun) {
    let denial: Value = serde_json::from_str(&run.stdout).expect("stdout is one JSON object");
    let output = &denial["hookSpecificOutput"];

    assert_eq!(run.code, Some(0));
    assert_eq!(output["permissionDecision"], json!("deny"));
    assert!(
        output["permissionDecisionReason"]
            .as_str()
            .expect("the reason is a string")
            .contains(&format!("releases/latest/download/{TARBALL}"))
    );
}

fn assert_silent_exit_zero(run: &HookRun) {
    assert_eq!(run.code, Some(0));
    assert_eq!(run.stdout, "");
}

#[test]
fn it_should_deny_the_edit_in_a_marked_project_given_by_claude_project_dir() {
    let marked = project(true);
    let elsewhere = project(false);
    let envs = [("CLAUDE_PROJECT_DIR", str_path(marked.path()))];

    let run = run_without_omhl("spec-lock", elsewhere.path(), &envs, "{}\n");

    assert_denies_with_install_command(&run);
}

#[test]
fn it_should_deny_the_edit_in_a_marked_project_given_by_the_stdin_cwd() {
    let marked = project(true);
    let elsewhere = project(false);
    let stdin = format!(
        "{{\"hook_event_name\":\"PreToolUse\",\"cwd\":\"{}\"}}\n",
        marked.path().display()
    );

    let run = run_without_omhl("spec-lock", elsewhere.path(), &[NO_PROJECT_DIR], &stdin);

    assert_denies_with_install_command(&run);
}

#[test]
fn it_should_accept_a_last_stdin_line_without_a_line_break() {
    let marked = project(true);
    let elsewhere = project(false);
    let stdin = format!("{{\"cwd\":\"{}\"}}", marked.path().display());

    let run = run_without_omhl("spec-lock", elsewhere.path(), &[NO_PROJECT_DIR], &stdin);

    assert_denies_with_install_command(&run);
}

#[test]
fn it_should_deny_the_edit_in_a_marked_project_given_by_pwd() {
    let marked = project(true);

    let run = run_without_omhl("spec-lock", marked.path(), &[NO_PROJECT_DIR], "{}\n");

    assert_denies_with_install_command(&run);
}

#[test]
fn it_should_find_the_marker_in_an_ancestor_of_the_project_directory() {
    let marked = project(true);
    let nested = marked.path().join("crates/inner");
    fs::create_dir_all(&nested).expect("nested directory is created");
    let envs = [("CLAUDE_PROJECT_DIR", str_path(&nested))];

    let run = run_without_omhl("spec-lock", &nested, &envs, "{}\n");

    assert_denies_with_install_command(&run);
}

#[test]
fn it_should_let_the_edit_through_in_a_project_without_a_marker() {
    let unmarked = project(false);
    let envs = [("CLAUDE_PROJECT_DIR", str_path(unmarked.path()))];

    let run = run_without_omhl("spec-lock", unmarked.path(), &envs, "{}\n");

    assert_silent_exit_zero(&run);
}

#[test]
fn it_should_ignore_a_marker_found_only_under_an_omhl_directory() {
    let unmarked = project(false);
    let omhl_dir = unmarked.path().join(".omhl");
    fs::create_dir_all(&omhl_dir).expect(".omhl is created");
    fs::write(omhl_dir.join(".omhl.toml"), "").expect("the nested marker is written");

    for dir in [unmarked.path(), omhl_dir.as_path()] {
        let envs = [("CLAUDE_PROJECT_DIR", str_path(dir))];

        let run = run_without_omhl("spec-lock", dir, &envs, "{}\n");

        assert_silent_exit_zero(&run);
    }
}

#[test]
fn it_should_fall_back_to_pwd_when_the_stdin_cwd_is_not_a_plain_absolute_path() {
    let marked = project(true);
    let unmarked = project(false);
    let with_backslash = format!("{{\"cwd\":\"{}/sub\\\\x\"}}\n", marked.path().display());

    for stdin in [
        with_backslash.as_str(),
        "{\"cwd\": \"rel/path\"}\n",
        "{\"tool_name\":\"Edit\"}\n",
    ] {
        let run = run_without_omhl("spec-lock", unmarked.path(), &[NO_PROJECT_DIR], stdin);

        assert_silent_exit_zero(&run);
    }
}

#[test]
fn it_should_not_read_the_stdin_when_claude_project_dir_is_set() {
    let unmarked = project(false);
    let empty = TempDir::new().expect("empty path directory is created");
    let envs = [("CLAUDE_PROJECT_DIR", str_path(unmarked.path()))];
    let mut child = spawn_hook("spec-lock", empty.path(), unmarked.path(), &envs);
    let open_stdin = child.stdin.take().expect("stdin is piped");
    let deadline = Instant::now() + Duration::from_secs(5);

    while child.try_wait().expect("the child is polled").is_none() {
        if Instant::now() > deadline {
            child.kill().expect("the stuck child is killed");
            panic!("the hook script waits for the stdin");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(open_stdin);

    assert_silent_exit_zero(&collect(child));
}

#[test]
fn it_should_stay_silent_in_agent_role_and_session_start_without_omhl() {
    for name in ["agent-role", "session-start"] {
        for marked in [true, false] {
            let dir = project(marked);
            let envs = [("CLAUDE_PROJECT_DIR", str_path(dir.path()))];

            let run = run_without_omhl(name, dir.path(), &envs, "{}\n");

            assert_eq!(run.code, Some(0), "{name} marked={marked}");
            assert_eq!(run.stdout, "", "{name} marked={marked}");
            assert_eq!(run.stderr, "", "{name} marked={marked}");
        }
    }
}

fn dir_entries(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path)
        .expect("the directory is readable")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    names
}

#[test]
fn it_should_warn_that_omhl_is_missing_on_every_session_in_any_project() {
    for marked in [true, false] {
        let dir = project(marked);
        let home = TempDir::new().expect("home directory is created");
        let tmp = TempDir::new().expect("tmp directory is created");
        let envs = [
            ("CLAUDE_PROJECT_DIR", str_path(dir.path())),
            ("HOME", str_path(home.path())),
            ("TMPDIR", str_path(tmp.path())),
        ];
        let project_before = dir_entries(dir.path());

        let first = run_without_omhl("version-check", dir.path(), &envs, "{}\n");
        let second = run_without_omhl("version-check", dir.path(), &envs, "{}\n");

        let warning: Value =
            serde_json::from_str(&first.stdout).expect("stdout is one JSON object");
        let message = warning["systemMessage"]
            .as_str()
            .expect("systemMessage is a string");
        let context = warning["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("additionalContext is a string");
        assert_eq!(first.code, Some(0), "marked={marked}");
        assert_eq!(message, context, "marked={marked}");
        for expected in [
            TARBALL,
            TARBALL_SHA256,
            "~/.local/bin",
            "Linux x86_64",
            "[ \"$(uname -sm)\" = \"Linux x86_64\" ]",
            "cargo build --release",
        ] {
            assert!(message.contains(expected), "marked={marked}: {expected}");
        }
        assert_eq!(second.stdout, first.stdout, "marked={marked}");
        assert!(dir_entries(home.path()).is_empty(), "marked={marked}");
        assert!(dir_entries(tmp.path()).is_empty(), "marked={marked}");
        assert_eq!(dir_entries(dir.path()), project_before, "marked={marked}");
    }
}

/// Roda `version-check` numa cópia do script com o mínimo dado, para não tocar o arquivo do repositório.
fn run_version_check(min: Option<&str>, fake_body: &str) -> HookRun {
    let plugin_dir = TempDir::new().expect("plugin directory is created");
    let script = plugin_dir.path().join("omhl-hook.sh");
    fs::copy(hook_script(), &script).expect("the hook script is copied");
    if let Some(min) = min {
        fs::write(plugin_dir.path().join("min-omhl-version"), min).expect("the minimum is written");
    }
    let bin = fake_omhl(fake_body);

    let mut child = spawn_script(&script, "version-check", bin.path(), bin.path(), &[]);
    drop(child.stdin.take());

    collect(child)
}

fn says(version: &str) -> String {
    format!("echo 'omhl {version}'\n")
}

fn assert_update_warning(run: &HookRun, found: &str, min: &str) {
    let warning: Value = serde_json::from_str(&run.stdout).expect("stdout is one JSON object");
    let message = warning["systemMessage"]
        .as_str()
        .expect("systemMessage is a string");
    let context = warning["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string");
    assert_eq!(run.code, Some(0), "{found} < {min}");
    assert_eq!(message, context, "{found} < {min}");
    assert_eq!(
        warning["hookSpecificOutput"]["hookEventName"],
        json!("SessionStart")
    );
    for expected in [found, min, TARBALL, TARBALL_SHA256, "~/.local/bin"] {
        assert!(message.contains(expected), "{found} < {min}: {expected}");
    }
}

#[test]
fn it_should_warn_when_omhl_is_older_than_the_minimum() {
    let cases = [
        ("0.2.0", "0.1.0"),
        ("0.2.0", "0.1.9"),
        ("0.2.0", "0.1.10-dev"),
        ("0.2.1", "0.2.0"),
        ("1.0.0", "0.9.9"),
    ];

    for (min, found) in cases {
        let run = run_version_check(Some(min), &says(found));

        assert_update_warning(&run, found, min);
    }
}

#[test]
fn it_should_stay_silent_when_omhl_is_as_new_as_the_minimum() {
    let cases = [
        ("0.2.0", "0.2.0"),
        ("0.2.0", "0.2.0-rc.1"),
        ("0.2.0", "0.10.0"),
        ("0.2.0", "1.0.0"),
        ("0.2.1", "0.2.1"),
    ];

    for (min, found) in cases {
        let run = run_version_check(Some(min), &says(found));

        assert_eq!(run.code, Some(0), "{found} >= {min}");
        assert_eq!(run.stdout, "", "{found} >= {min}");
    }
}

#[test]
fn it_should_stay_silent_when_the_version_or_the_minimum_cannot_be_read() {
    let cases = [
        (Some("0.2.0\n"), "exit 1\n"),
        (Some("0.2.0\n"), "echo banana\n"),
        (Some("0.2.0\n"), "echo 'omhl 0.1'\n"),
        (Some("0.2.0\n"), "echo 'omhl 0.x.0'\n"),
        (None, "echo 'omhl 0.1.0'\n"),
        (Some("abc\n"), "echo 'omhl 0.1.0'\n"),
    ];

    for (min, fake_body) in cases {
        let run = run_version_check(min, fake_body);

        assert_eq!(run.code, Some(0), "{min:?} {fake_body}");
        assert_eq!(run.stdout, "", "{min:?} {fake_body}");
    }
}

/// Aceita só uma linha `X.Y.Z` que não passe da versão do Cargo.
fn min_version_is_valid(content: &str, cargo_version: &str) -> bool {
    let parse = |text: &str| -> Option<[u64; 3]> {
        let parts: Vec<&str> = text.split('.').collect();
        let [major, minor, patch] = parts[..] else {
            return None;
        };
        let digits = |part: &str| {
            part.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| part.parse().ok())
                .flatten()
        };
        Some([digits(major)?, digits(minor)?, digits(patch)?])
    };
    let line = content.strip_suffix('\n').unwrap_or(content);

    match (parse(line), parse(cargo_version)) {
        (Some(min), Some(cargo)) => min <= cargo,
        _ => false,
    }
}

#[test]
fn it_should_declare_the_minimum_as_one_line_equal_to_zero_two_zero_within_the_cargo_version() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/hooks/min-omhl-version");
    let content = fs::read_to_string(path).expect("the minimum is readable");

    assert_eq!(content, "0.2.0\n");
    assert!(min_version_is_valid(&content, env!("CARGO_PKG_VERSION")));
}

#[test]
fn it_should_refuse_a_minimum_above_the_cargo_version_and_malformed_files() {
    assert!(!min_version_is_valid("0.1.1\n", "0.1.0"));
    assert!(min_version_is_valid("0.1.0\n", "0.1.0"));
    assert!(min_version_is_valid("0.0.9\n", "0.1.0"));
    assert!(!min_version_is_valid("0.2\n", "0.2.0"));
    assert!(!min_version_is_valid("0.1.0\n0.1.0\n", "0.2.0"));
}

#[test]
fn it_should_keep_the_git_budget_of_the_spec_lock_plus_two_seconds_under_the_hook_timeout() {
    let hooks = plugin_file("hooks/hooks.json");
    let timeout = entry_for(&hooks, "spec-lock")["timeout"]
        .as_u64()
        .expect("the timeout is a number");

    let budget = omhl::ports::git::SPEC_LOCK_GIT_BUDGET + Duration::from_secs(2);

    assert!(budget < Duration::from_secs(timeout), "budget {budget:?}");
}

#[test]
fn it_should_not_start_the_mcp_server() {
    let mcp_json = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/.mcp.json");

    let manifest = plugin_file(".claude-plugin/plugin.json");

    assert!(!mcp_json.exists());
    assert!(manifest.get("mcpServers").is_none());
}

fn repo_text(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);

    fs::read_to_string(&path).unwrap_or_else(|_| panic!("{relative} is readable"))
}

fn position(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is missing"))
}

#[test]
fn it_should_name_both_release_files_in_the_workflow_the_readme_and_the_hook() {
    let sources = [
        repo_text(".github/workflows/release.yml"),
        repo_text("README.md"),
        repo_text("plugin/hooks/omhl-hook.sh"),
    ];

    for text in &sources {
        assert!(text.contains(TARBALL_SHA256), "missing {TARBALL_SHA256}");
        assert!(text.contains(TARBALL), "missing {TARBALL}");
    }
}

#[test]
fn it_should_build_the_release_as_a_cold_locked_musl_binary_within_thirty_minutes() {
    let workflow = repo_text(".github/workflows/release.yml");
    let timeout: u32 = workflow
        .lines()
        .find_map(|line| line.trim().strip_prefix("timeout-minutes:"))
        .expect("the job has a timeout")
        .trim()
        .parse()
        .expect("the timeout is a number");

    assert!(workflow.contains("x86_64-unknown-linux-musl"));
    assert!(workflow.contains("musl-tools"));
    assert!(workflow.contains("--locked"));
    assert!(timeout >= 30, "timeout {timeout}");
    assert!(!workflow.contains("rust-cache"));
}

#[test]
fn it_should_check_the_tag_before_building() {
    let workflow = repo_text(".github/workflows/release.yml");

    assert!(position(&workflow, "Tag matches Cargo.toml") < position(&workflow, "cargo build"));
}

#[test]
fn it_should_create_the_release_once_after_the_checks_with_both_files_and_no_upload() {
    let workflow = repo_text(".github/workflows/release.yml");
    let create = position(&workflow, "gh release create");
    let create_line = workflow[create..].lines().next().expect("a create line");

    assert_eq!(workflow.matches("gh release create").count(), 1);
    assert!(!workflow.contains("gh release upload"));
    assert!(position(&workflow, "statically linked") < create);
    assert!(position(&workflow, "--version") < create);
    assert!(position(&workflow, "tar -czf") < create);
    assert!(position(&workflow, "sha256sum") < create);
    assert!(create_line.contains(TARBALL));
    assert!(create_line.contains(TARBALL_SHA256));
}

/// O texto de `heading` até o próximo título de qualquer nível.
fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = position(text, heading) + heading.len();
    let rest = &text[start..];
    let end = rest.find("\n#").unwrap_or(rest.len());

    &rest[..end]
}

#[test]
fn it_should_list_each_requirement_under_its_own_readme_heading() {
    let readme = repo_text("README.md");
    let expected: [(&str, &[&str]); 3] = [
        ("### Obrigatório", &["2.31", "POSIX", "XDG_DATA_HOME"]),
        ("### Só para compilar do fonte", &["rust-toolchain.toml"]),
        ("### Opcional", &["Worktrunk", "GitHub"]),
    ];

    for (heading, terms) in expected {
        let list = section(&readme, heading);

        for term in terms {
            assert!(list.contains(term), "`{term}` is missing under {heading}");
        }
    }
}

#[test]
fn it_should_show_the_release_files_before_the_source_build_in_the_readme_install_section() {
    let readme = repo_text("README.md");
    let install = section(&readme, "## Instalação");
    let build = position(install, "cargo build --release");

    assert!(position(install, TARBALL) < build);
    assert!(position(install, TARBALL_SHA256) < build);
}

#[test]
fn it_should_not_say_that_a_missing_omhl_blocks_every_write() {
    let texts = [
        (
            "docs/directives-flow.md",
            "o `omhl` ausente do `PATH`, o pânico e a saída `1` bloqueiam toda escrita",
        ),
        (
            "crates/omhl/src/adapters/hook.rs",
            "bloqueia com o `omhl` ausente ou falho",
        ),
        (
            "crates/omhl/src/adapters/cli.rs",
            "bloqueia com o `omhl` ausente ou falho",
        ),
    ];

    for (file, old_phrase) in texts {
        let text = repo_text(file);

        assert!(!text.contains(old_phrase), "{file} keeps the old phrase");
        // Ancorado na descrição do script, porque `exit 0` pode já existir em outro ponto do arquivo.
        assert!(
            text.lines()
                .any(|line| line.contains("omhl-hook.sh") && line.contains("exit 0")),
            "{file} lacks exit 0 in the description of the branch without the binary"
        );
    }
}

#[test]
fn it_should_not_track_any_omc_path_under_the_plugin_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("git")
        .args(["ls-files", "plugin"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs");
    let listing = String::from_utf8(output.stdout).expect("the listing is UTF-8");

    let found = listing
        .lines()
        .find(|path| path.split('/').any(|part| part == ".omc"));

    assert_eq!(found, None, "tracked .omc path under plugin/");
}
