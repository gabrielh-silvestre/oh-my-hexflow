use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

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

#[test]
fn it_should_wire_the_session_start_hook_in_exec_form_and_the_pre_tool_use_hooks_in_shell_form() {
    let hooks = plugin_file("hooks/hooks.json");

    assert_eq!(
        hooks,
        json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup|resume|clear|compact",
                    "hooks": [{
                        "type": "command",
                        "command": "omhl",
                        "args": ["hook", "session-start"],
                        "timeout": 15,
                    }],
                }],
                "PreToolUse": [
                    {
                        "matcher": "Edit|Write|MultiEdit",
                        "hooks": [{
                            "type": "command",
                            "command": "omhl hook spec-lock || exit 1",
                            "timeout": 10,
                        }],
                    },
                    {
                        "matcher": "Bash|Monitor|Agent|Task",
                        "hooks": [{
                            "type": "command",
                            "command": "omhl hook agent-role || exit 1",
                            "timeout": 10,
                        }],
                    },
                ],
            }
        })
    );
}

/// A entrada de `PreToolUse` do matcher `Bash|Monitor|Agent|Task`, a do `agent-role`.
fn agent_role_hook_entry(hooks: &Value) -> &Value {
    hooks["hooks"]["PreToolUse"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["matcher"] == json!("Bash|Monitor|Agent|Task"))
        })
        .expect("PreToolUse has a Bash entry")
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

#[test]
fn it_should_wrap_the_agent_role_command_with_exit_one() {
    let hooks = plugin_file("hooks/hooks.json");

    let command = &agent_role_hook_entry(&hooks)["hooks"][0]["command"];

    assert_eq!(command, &json!("omhl hook agent-role || exit 1"));
}

#[test]
fn it_should_not_block_bash_when_omhl_does_not_know_the_hook_subcommand() {
    let hooks = plugin_file("hooks/hooks.json");
    let command = agent_role_hook_entry(&hooks)["hooks"][0]["command"]
        .as_str()
        .expect("the command is a string");
    let bin = TempDir::new().expect("bin directory is created");
    let fake_omhl = bin.path().join("omhl");
    fs::write(&fake_omhl, "#!/bin/sh\nexit 2\n").expect("fake omhl is written");
    fs::set_permissions(&fake_omhl, fs::Permissions::from_mode(0o755))
        .expect("fake omhl is executable");

    let status = Command::new("/bin/sh")
        .args(["-c", command])
        .env("PATH", bin.path())
        .stdin(Stdio::null())
        .status()
        .expect("the hook command runs");

    assert_eq!(status.code(), Some(1));
}

#[test]
fn it_should_not_start_the_mcp_server() {
    let mcp_json = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/.mcp.json");

    let manifest = plugin_file(".claude-plugin/plugin.json");

    assert!(!mcp_json.exists());
    assert!(manifest.get("mcpServers").is_none());
}
