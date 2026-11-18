//! Os dois serializadores do `Doc`: `agent` (compacto, sem linha em branco) e `human` (blocos separados e rótulos alinhados).

use super::doc::{Closing, Doc, Join, Line, Marker};

pub fn agent(doc: &Doc) -> String {
    let mut lines: Vec<String> = doc.lines.iter().map(|line| render_line(line, 0)).collect();
    lines.extend(closing_line(&doc.closing));

    lines.join("\n")
}

/// Um bloco por cabeçalho, nota, item com filhos e fecho, separados por uma linha em branco.
pub fn human(doc: &Doc) -> String {
    let mut blocks: Vec<String> = blocks(&doc.lines)
        .into_iter()
        .map(|block| {
            let width = label_width(block);
            let lines: Vec<String> = block.iter().map(|line| render_line(line, width)).collect();
            lines.join("\n")
        })
        .collect();
    blocks.extend(closing_line(&doc.closing));

    blocks.join("\n\n")
}

fn blocks(lines: &[Line]) -> Vec<&[Line]> {
    let mut blocks = Vec::new();
    let mut start = 0;
    for index in 1..lines.len() {
        if breaks_before(&lines[index - 1], &lines[index], lines.get(index + 1)) {
            blocks.push(&lines[start..index]);
            start = index;
        }
    }
    if !lines.is_empty() {
        blocks.push(&lines[start..]);
    }

    blocks
}

fn is_child(line: &Line) -> bool {
    match line {
        Line::Field { .. } => true,
        Line::Raw { depth, .. } => *depth > 0,
        Line::Header { .. } | Line::Item { .. } | Line::Note { .. } => false,
    }
}

/// Itens seguidos e sem filhos formam uma lista só; todo o resto abre bloco, menos os filhos.
fn breaks_before(prev: &Line, line: &Line, next: Option<&Line>) -> bool {
    if is_child(line) {
        return false;
    }
    let plain_list = matches!((prev, line), (Line::Item { .. }, Line::Item { .. }))
        && !next.is_some_and(is_child);

    !plain_list
}

fn label_width(block: &[Line]) -> usize {
    block
        .iter()
        .filter_map(|line| match line {
            Line::Field { label, .. } => Some(label.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or_default()
}

/// `label_width` alinha os valores dos campos; o `agent` passa 0 e escreve `rótulo: valor`.
fn render_line(line: &Line, label_width: usize) -> String {
    match line {
        Line::Header { symbol, parts } => {
            format!("{symbol} {}", parts.join(Join::Dot.separator()))
        }
        Line::Item {
            depth,
            marker,
            parts,
            join,
        } => {
            let marker = match marker {
                Marker::Dash => "- ".to_owned(),
                Marker::Symbol(symbol) => format!("{symbol} "),
                Marker::None => String::new(),
            };
            format!("{}{marker}{}", indent(*depth), parts.join(join.separator()))
        }
        Line::Field {
            depth,
            label,
            value,
        } => {
            let padding = " ".repeat(label_width.saturating_sub(label.chars().count()));
            format!("{}{label}:{padding} {value}", indent(*depth))
        }
        Line::Note { symbol, text } => format!("{symbol} {text}"),
        Line::Raw { depth, text } => format!("{}{text}", indent(*depth)),
    }
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn closing_line(closing: &Closing) -> Option<String> {
    match closing {
        Closing::None => None,
        Closing::Next(step) => Some(format!("Next: {step}.")),
        Closing::NothingBlocked(step) => Some(format!("Nothing blocked. Next: {step}.")),
    }
}
