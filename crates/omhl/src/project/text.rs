use std::borrow::Cow;

/// Linha e coluna (base 1, em caracteres) do erro de TOML. O texto do erro repete trechos do
/// arquivo, por isso só a posição sai.
pub(crate) fn toml_error_position(text: &str, error: &toml::de::Error) -> (usize, usize) {
    let offset = error.span().map_or(0, |span| span.start);
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;

    (line, column)
}

/// `\r\n` e `\r` viram `\n`; sem `\r` o texto volta emprestado, sem alocar.
pub(crate) fn normalize_line_breaks(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(replace_line_breaks(text))
    } else {
        Cow::Borrowed(text)
    }
}

/// Como `normalize_line_breaks`, mas sem `\r` o próprio buffer volta, sem copiar.
pub(crate) fn normalize_line_breaks_owned(text: String) -> String {
    if text.contains('\r') {
        replace_line_breaks(&text)
    } else {
        text
    }
}

/// Um passo só: cada `\r` vira `\n` e o `\n` que o segue some.
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
