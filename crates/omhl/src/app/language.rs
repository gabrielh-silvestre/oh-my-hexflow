use std::path::Path;

use serde_json::Value;

use crate::ports::files::{read_followed_regular_utf8, read_regular_utf8};

const LANGUAGE_MAX_CHARS: usize = 64;
const SETTINGS_MAX_BYTES: u64 = 64 * 1024;

#[derive(Default)]
pub struct LanguageRungs<'a> {
    pub plugin_option: Option<&'a str>,
    pub managed: Option<&'a str>,
    pub local: Option<&'a str>,
    pub project: Option<&'a str>,
    pub user: Option<&'a str>,
}

/// Segue a ordem do próprio Claude Code, com só a opção do plugin acima das configurações; um valor inválido conta como ausente e cai no degrau seguinte.
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

/// Só um valor seguro de colar em uma linha de prompt: limitado e sem quebra de linha nem caractere de controle.
pub fn valid_language(raw: &str) -> Option<&str> {
    let value = raw.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= LANGUAGE_MAX_CHARS
        && !value.chars().any(char::is_control);

    valid.then_some(value)
}

/// `language` precisa vir de `valid_language`: a linha só é segura se o valor não tiver quebra de linha.
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
