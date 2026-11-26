use std::path::Path;

use serde_json::Value;

use crate::ports::files::{read_followed_regular_utf8, read_regular_utf8};

const LANGUAGE_MAX_CHARS: usize = 64;
const SETTINGS_MAX_BYTES: u64 = 64 * 1024;

/// Os cinco valores já lidos; a ordem dos campos é a ordem de precedência.
#[derive(Default)]
pub struct LanguageRungs<'a> {
    pub plugin_option: Option<&'a str>,
    pub managed: Option<&'a str>,
    pub local: Option<&'a str>,
    pub project: Option<&'a str>,
    pub user: Option<&'a str>,
}

/// O primeiro valor válido; um valor inválido conta como ausente e cai no degrau seguinte.
pub fn resolve_language(rungs: &LanguageRungs<'_>) -> Option<String> {
    [
        rungs.plugin_option,
        rungs.managed,
        rungs.local,
        rungs.project,
        rungs.user,
    ]
    .into_iter()
    .flatten()
    .find_map(valid_language)
    .map(str::to_owned)
}

/// Valor aparado, não vazio, de até 64 caracteres e sem caractere de controle (a quebra de linha inclusa).
pub fn valid_language(raw: &str) -> Option<&str> {
    let value = raw.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= LANGUAGE_MAX_CHARS
        && !value.chars().any(char::is_control);

    valid.then_some(value)
}

/// Uma linha só, para o valor nunca abrir uma segunda instrução.
pub fn language_line(language: &str) -> String {
    format!(
        "The language of the person's conversation is \"{language}\"; write any text addressed to the person in that language."
    )
}

/// Recusa link simbólico: um repositório clonado pode trazer `/dev/zero` no lugar do arquivo.
pub fn settings_language(path: Option<&Path>) -> Option<String> {
    language_in(&read_regular_utf8(path?, SETTINGS_MAX_BYTES).ok()?)
}

/// O `settings.json` do usuário costuma ser link de gerenciador de dotfiles: segue o link, mas lê só arquivo regular.
pub fn user_settings_language(path: Option<&Path>) -> Option<String> {
    language_in(&read_followed_regular_utf8(path?, SETTINGS_MAX_BYTES).ok()?)
}

fn language_in(text: &str) -> Option<String> {
    let settings: Value = serde_json::from_str(text).ok()?;

    settings["language"].as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests;
