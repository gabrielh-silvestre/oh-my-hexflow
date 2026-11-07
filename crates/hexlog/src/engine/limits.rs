use std::fmt::Write;

use serde_json::{Number, Value};

use crate::error::push_pointer_segment;

/// Contêineres (array ou objeto) que `data` aceita: o corpo do record gasta um dos 127 níveis do `from_str`.
pub(crate) const JSON_NESTING_MAX: usize = 126;
/// Maior inteiro que um double representa sem arredondar vizinhos: 2^53 - 1.
const SAFE_INTEGER_MAX: u64 = (1 << 53) - 1;

/// Tetos de entrada configuráveis (seção 4.5 do plano).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub data_max_chars: usize,
    pub batch_items: usize,
    pub details_max: usize,
    pub page_default: u32,
    pub page_max: u32,
    pub author_field_max: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            data_max_chars: 16_000,
            batch_items: 50,
            details_max: 50,
            page_default: 50,
            page_max: 200,
            author_field_max: 100,
        }
    }
}

/// `true` quando `value` tem mais de `max` níveis de contêiner (escalar não conta). Iterativa.
pub(crate) fn json_nesting_exceeds(value: &Value, max: usize) -> bool {
    let mut pending = vec![(value, 0usize)];
    while let Some((node, depth)) = pending.pop() {
        if matches!(node, Value::Array(_) | Value::Object(_)) && depth >= max {
            return true;
        }
        match node {
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
            Value::Object(map) => pending.extend(map.values().map(|item| (item, depth + 1))),
            _ => {}
        }
    }
    false
}

enum Children<'a> {
    Array(std::iter::Enumerate<std::slice::Iter<'a, Value>>),
    Object(serde_json::map::Iter<'a>),
}

struct Level<'a> {
    children: Children<'a>,
    pointer_len: usize,
}

fn is_safe_integer(number: &Number) -> bool {
    number
        .as_i64()
        .map(i64::unsigned_abs)
        .or_else(|| number.as_u64())
        .is_some_and(|magnitude| magnitude <= SAFE_INTEGER_MAX)
}

/// Achados `(ponteiro RFC 6901, código)` em ordem de documento, até `limit`. Iterativa, com um só `String` de ponteiro.
pub(crate) fn json_issues(value: &Value, limit: usize) -> Vec<(String, &'static str)> {
    let mut issues = Vec::new();
    let mut pointer = String::new();
    let mut open: Vec<Level> = Vec::new();
    let mut entering = Some(value);
    while issues.len() < limit {
        if let Some(node) = entering.take() {
            let children = match node {
                Value::Number(number) if !is_safe_integer(number) => {
                    issues.push((pointer.clone(), "number-not-safe-integer"));
                    None
                }
                Value::Array(_) | Value::Object(_) if open.len() >= JSON_NESTING_MAX => {
                    issues.push((pointer.clone(), "max-depth-exceeded"));
                    None
                }
                Value::Array(items) => Some(Children::Array(items.iter().enumerate())),
                Value::Object(map) => Some(Children::Object(map.iter())),
                _ => None,
            };
            if let Some(children) = children {
                open.push(Level {
                    children,
                    pointer_len: pointer.len(),
                });
            }
            continue;
        }
        let Some(level) = open.last_mut() else { break };
        pointer.truncate(level.pointer_len);
        entering = match &mut level.children {
            Children::Array(items) => items.next().map(|(index, item)| {
                let _ = write!(pointer, "/{index}");
                item
            }),
            Children::Object(entries) => entries.next().map(|(key, item)| {
                pointer.push('/');
                push_pointer_segment(&mut pointer, key);
                item
            }),
        };
        if entering.is_none() {
            open.pop();
        }
    }
    issues
}

#[cfg(test)]
mod tests;
