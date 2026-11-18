//! Modelo intermediário do texto: o renderizador monta o `Doc` e `layout` o serializa em `agent` ou `human`.

/// Como o item abre a linha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    Dash,
    Symbol(&'static str),
    None,
}

/// Separador entre as partes de um cabeçalho ou de um item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// ` · `
    Dot,
    /// Dois espaços.
    Gap,
}

impl Join {
    pub(super) fn separator(self) -> &'static str {
        match self {
            Self::Dot => " · ",
            Self::Gap => "  ",
        }
    }
}

/// Uma linha tipada; `depth` é o nível de recuo (2 espaços por nível no `agent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Header {
        symbol: &'static str,
        parts: Vec<String>,
    },
    Item {
        depth: usize,
        marker: Marker,
        parts: Vec<String>,
        join: Join,
    },
    Field {
        depth: usize,
        label: String,
        value: String,
    },
    Note {
        symbol: &'static str,
        text: String,
    },
    Raw {
        depth: usize,
        text: String,
    },
}

/// A última linha: o próximo passo, com ou sem o aviso de que nada bloqueou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closing {
    None,
    Next(String),
    NothingBlocked(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    pub lines: Vec<Line>,
    pub closing: Closing,
}

/// Run, hint, cause e cauda vêm do TOML, do SO ou do comando: cada campo fica numa linha só e sem controle.
/// Quebras de linha viram espaço; os demais controles e os caracteres de formato (zero-width, bidi) somem.
pub fn one_line(raw: &str) -> String {
    raw.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}' => Some(' '),
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}
