use std::borrow::Cow;

/// Linha e coluna (base 1, em caracteres) do erro de TOML. O texto do erro repete trechos do
/// arquivo, por isso só a posição sai.
pub(crate) fn toml_error_position(text: &str, error: &toml::de::Error) -> (usize, usize) {
    position_at(text, error.span().map_or(0, |span| span.start))
}

/// Linha e coluna (base 1, em caracteres) de um deslocamento em bytes.
pub(crate) fn position_at(text: &str, offset: usize) -> (usize, usize) {
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;

    (line, column)
}

/// Caracteres de formato e separadores invisíveis que escondem texto de quem lê.
pub(crate) fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{feff}'
            | '\u{e0000}'..='\u{e007f}'
    )
}

pub(crate) fn normalize_line_breaks(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(replace_line_breaks(text))
    } else {
        Cow::Borrowed(text)
    }
}

pub(crate) fn normalize_line_breaks_owned(text: String) -> String {
    if text.contains('\r') {
        replace_line_breaks(&text)
    } else {
        text
    }
}

fn replace_line_breaks(text: &str) -> String {
    let mut segments = text.split('\r');
    let mut normalized = String::with_capacity(text.len());
    normalized.push_str(segments.next().unwrap_or_default());
    for segment in segments {
        normalized.push('\n');
        normalized.push_str(segment.strip_prefix('\n').unwrap_or(segment));
    }

    normalized
}
