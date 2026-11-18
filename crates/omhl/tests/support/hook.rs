use serde_json::{Value, json};

use super::child::Output;

/// O hook liberou a chamada: sai com 0, stdout vazio e uma única linha de erro no stderr com o código dado.
pub fn assert_released_with_code(output: &Output, code: &str) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(output.stderr.matches('\n').count(), 1, "one error line");
    let line: Value = serde_json::from_str(output.stderr.trim_end()).expect("the line is JSON");
    assert_eq!(line["error"]["code"], json!(code));
}
