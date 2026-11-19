//! Modelo intermediário do texto: o renderizador monta o `Doc` e `layout` o serializa em `agent` ou `human`.

use serde_json::Value;

#[cfg(test)]
use super::vocab::NEXT_LABEL;
use super::vocab::State;

/// Como o item abre a linha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    Dash,
    Symbol(State),
    None,
}

/// Separador entre as partes de um cabeçalho ou de um item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Dot,
    Gap,
}

/// Um fato do nó: o `agent` mostra só `agent` (já com a palavra que o identifica) e o `human` mostra `label: human`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    pub agent: String,
    pub human: String,
}

impl Fact {
    /// O mesmo texto nos dois layouts.
    pub fn same(label: &'static str, value: String) -> Fact {
        Fact {
            label,
            agent: value.clone(),
            human: value,
        }
    }
}

/// Uma linha tipada; `depth` é o nível de recuo (2 espaços por nível no `agent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Header {
        state: State,
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
        state: State,
        text: String,
    },
    /// Texto verbatim, nos dois layouts (o `render`).
    Raw {
        depth: usize,
        text: String,
    },
    /// Linha de saída de comando: o `agent` só apara o fim da linha.
    Output {
        depth: usize,
        text: String,
    },
    /// Um nó: o `agent` escreve o id e os fatos numa linha só; o `human` abre um bloco com o id e um rótulo por fato.
    Node {
        depth: usize,
        id: String,
        facts: Vec<Fact>,
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

/// Texto de uma chave do `Value`; chave ausente ou não textual vira vazio, nunca erro.
pub fn text(value: &Value, key: &str) -> String {
    one_line(value[key].as_str().unwrap_or_default())
}

pub fn opt_text(value: &Value, key: &str) -> Option<String> {
    value[key].as_str().map(one_line)
}

/// Item de lista simples (`- a · b`) na raiz.
pub fn dash(parts: Vec<String>) -> Line {
    Line::Item {
        depth: 0,
        marker: Marker::Dash,
        parts,
        join: Join::Dot,
    }
}

/// Campo `rótulo: valor` no nível `depth`.
pub fn field(depth: usize, label: &str, value: String) -> Line {
    Line::Field {
        depth,
        label: label.to_owned(),
        value,
    }
}

/// Os textos de uma lista de strings, inteiros e separados por vírgula.
pub fn joined(value: &Value, key: &str) -> String {
    items(value, key)
        .filter_map(Value::as_str)
        .map(one_line)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn items<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value[key].as_array().into_iter().flatten()
}

pub fn has_items(value: &Value, key: &str) -> bool {
    items(value, key).next().is_some()
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(one_line(text)),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Todo escalar de `value` como par rótulo/valor: objeto aninhado vira `pai.filho`, array de escalares vira uma lista
/// separada por vírgula, `null` não é folha e some.
pub fn scalar_fields(label: &str, value: &Value) -> Vec<(String, String)> {
    match value {
        Value::Object(map) => map
            .iter()
            .flat_map(|(key, child)| {
                let child_label = match label.is_empty() {
                    true => key.clone(),
                    false => format!("{label}.{key}"),
                };
                scalar_fields(&child_label, child)
            })
            .collect(),
        Value::Array(elements) => {
            let scalars: Vec<String> = elements.iter().filter_map(scalar_text).collect();
            if scalars.len() == elements.len() {
                return match scalars.is_empty() {
                    true => Vec::new(),
                    false => vec![(label.to_owned(), scalars.join(", "))],
                };
            }
            elements
                .iter()
                .enumerate()
                .flat_map(|(index, element)| scalar_fields(&format!("{label}[{index}]"), element))
                .collect()
        }
        scalar => scalar_text(scalar)
            .map(|text| vec![(label.to_owned(), text)])
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_turn_line_breaks_into_spaces_and_drop_controls_and_bidi_marks() {
        let cleaned = one_line(&format!("a\n{NEXT_LABEL} rm -rf\u{1b}[31m"));

        assert_eq!(cleaned, format!("a {NEXT_LABEL} rm -rf[31m"));
    }
}
