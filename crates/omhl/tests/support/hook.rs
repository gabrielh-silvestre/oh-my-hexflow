use std::path::Path;

use serde_json::{Value, json};

use super::child::{Output as Child, omhl};

/// O hook liberou a chamada: sai com 0, stdout vazio e uma única linha de erro no stderr com o código dado.
pub fn assert_released_with_code(output: &Child, code: &str) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(output.stderr.matches('\n').count(), 1, "one error line");
    let line: Value = serde_json::from_str(output.stderr.trim_end()).expect("the line is JSON");
    assert_eq!(line["error"]["code"], json!(code));
}

pub fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// Evento PreToolUse do Bash; sem `agent_type` é a sessão principal.
pub fn bash_call(command: &str, agent_type: Option<&str>) -> String {
    bash_call_with(&json!({ "command": command }), agent_type)
}

/// Como `bash_call`, com o `tool_input` inteiro à escolha (`description`, `timeout`...).
pub fn bash_call_with(tool_input: &Value, agent_type: Option<&str>) -> String {
    let mut event = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": "/tmp",
        "tool_name": "Bash",
        "tool_input": tool_input,
    });
    if let Some(agent_type) = agent_type {
        event["agent_type"] = json!(agent_type);
    }

    event.to_string()
}

pub fn run_agent_role_in(plugin_root: &Path, stdin: &str) -> Child {
    omhl(&["hook", "agent-role"])
        .env("CLAUDE_PLUGIN_ROOT", plugin_root)
        .stdin(stdin)
        .run()
}

/// O `hookSpecificOutput` da resposta; falha se o hook ficou calado.
pub fn decision_of(output: &Child) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        !output.stdout.is_empty(),
        "expected a decision, stdout is empty"
    );

    json_line(&output.stdout)["hookSpecificOutput"].clone()
}
