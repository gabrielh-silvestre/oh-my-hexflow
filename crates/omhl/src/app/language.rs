const LANGUAGE_MAX_CHARS: usize = 64;

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

#[cfg(test)]
mod tests;
