use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::error::AppError;

pub const INPUT_MAX_BYTES: usize = 1_048_576;

/// Texto do `serde_json` quando passa do limite de profundidade de contêiner (versão fixada no `Cargo.toml`).
const DEPTH_LIMIT_MESSAGE: &str = "recursion limit exceeded";

pub fn parse_json_text(bytes: &[u8]) -> Result<Value, AppError> {
    if bytes.len() > INPUT_MAX_BYTES {
        return Err(AppError::input_too_large(INPUT_MAX_BYTES));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| AppError::input_not_utf8())?;

    serde_json::from_str(text).map_err(|error| match error.to_string() {
        message if message.starts_with(DEPTH_LIMIT_MESSAGE) => AppError::input_too_deep(),
        _ => AppError::input_invalid_json(error.line(), error.column()),
    })
}

/// Lê um byte além do limite: a entrada grande demais é detectada sem ler o resto.
pub fn read_json_input(input: &mut impl Read) -> Result<Value, AppError> {
    let mut bytes = Vec::new();
    input
        .take(INPUT_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::input_unreadable())?;

    parse_json_text(&bytes)
}

/// Campo de caminho de um evento de hook: tem de ser um texto com caminho absoluto.
pub fn absolute_path(value: &Value) -> Result<PathBuf, AppError> {
    match value.as_str().map(Path::new) {
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        _ => Err(AppError::invalid_hook_input()),
    }
}

#[cfg(test)]
mod tests;
