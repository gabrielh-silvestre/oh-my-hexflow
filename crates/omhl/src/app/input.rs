use std::io::Read;

use serde_json::Value;

use super::error::AppError;

pub const INPUT_MAX_BYTES: usize = 1_048_576;

/// Texto do `serde_json` quando passa do limite de 128 níveis de contêiner (versão fixada no `Cargo.toml`).
const DEPTH_LIMIT_MESSAGE: &str = "recursion limit exceeded";

/// Tamanho, UTF-8, profundidade e JSON, nessa ordem; o erro não repete o texto recebido.
pub fn parse_json_text(bytes: &[u8]) -> Result<Value, AppError> {
    if bytes.len() > INPUT_MAX_BYTES {
        return Err(AppError::plain(
            "input-too-large",
            &format!("input is larger than {INPUT_MAX_BYTES} bytes"),
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AppError::plain("input-not-utf8", "input is not valid UTF-8"))?;

    serde_json::from_str(text).map_err(|error| match error.to_string() {
        message if message.starts_with(DEPTH_LIMIT_MESSAGE) => {
            AppError::plain("input-too-deep", "input is nested too deeply")
        }
        _ => AppError::plain(
            "input-invalid-json",
            &format!(
                "input is not valid JSON at line {} column {}",
                error.line(),
                error.column()
            ),
        ),
    })
}

/// Lê um byte além do limite: a entrada grande demais é detectada sem ler o resto.
pub fn read_json_input(input: &mut impl Read) -> Result<Value, AppError> {
    let mut bytes = Vec::new();
    input
        .take(INPUT_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::plain("input-unreadable", "input could not be read"))?;

    parse_json_text(&bytes)
}
