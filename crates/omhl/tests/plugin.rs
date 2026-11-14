use std::fs;
use std::path::Path;

use serde_json::{Value, json};

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

#[test]
fn it_should_wire_the_session_start_hook_in_exec_form_and_the_spec_lock_hook_in_shell_form() {
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
                "PreToolUse": [{
                    "matcher": "Edit|Write|MultiEdit",
                    "hooks": [{
                        "type": "command",
                        "command": "omhl hook spec-lock || exit 1",
                        "timeout": 10,
                    }],
                }],
            }
        })
    );
}

#[test]
fn it_should_not_start_the_mcp_server() {
    let mcp_json = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/.mcp.json");

    let manifest = plugin_file(".claude-plugin/plugin.json");

    assert!(!mcp_json.exists());
    assert!(manifest.get("mcpServers").is_none());
}
