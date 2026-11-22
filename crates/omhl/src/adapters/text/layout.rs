use super::doc::{Closing, Doc, Join, Line, Marker};
use super::vocab::Vocabulary;
#[cfg(test)]
use super::vocab::{
    AgentVocabulary, HumanVocabulary, NEXT_LABEL, NOTHING_BLOCKED, SYMBOL_OK, SYMBOL_WARNING, State,
};

pub fn agent(doc: &Doc, vocabulary: &impl Vocabulary) -> String {
    let mut lines: Vec<String> = doc
        .lines
        .iter()
        .map(|line| render_line(line, None, vocabulary))
        .collect();
    lines.extend(closing_line(&doc.closing, vocabulary));

    lines.join("\n")
}

pub fn human(doc: &Doc, vocabulary: &impl Vocabulary) -> String {
    let mut blocks: Vec<String> = blocks(&doc.lines)
        .into_iter()
        .map(|block| {
            let width = label_width(block);
            let lines: Vec<String> = block
                .iter()
                .map(|line| render_line(line, Some(width), vocabulary))
                .collect();
            lines.join("\n")
        })
        .collect();
    blocks.extend(closing_line(&doc.closing, vocabulary));

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
        Line::Raw { depth, .. } | Line::Output { depth, .. } => *depth > 0,
        Line::Header { .. } | Line::Item { .. } | Line::Note { .. } | Line::Node { .. } => false,
    }
}

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
            Line::Node { facts, .. } => facts.iter().map(|fact| fact.label.chars().count()).max(),
            _ => None,
        })
        .max()
        .unwrap_or_default()
}

fn render_line(line: &Line, label_width: Option<usize>, vocabulary: &impl Vocabulary) -> String {
    match line {
        Line::Header { state, parts } => {
            format!(
                "{} {}",
                vocabulary.symbol(*state),
                joined(parts, Join::Dot, vocabulary)
            )
        }
        Line::Item {
            depth,
            marker,
            parts,
            join,
        } => {
            let marker = match marker {
                Marker::Dash => vocabulary.dash().to_owned(),
                Marker::Symbol(state) => format!("{} ", vocabulary.symbol(*state)),
                Marker::None => String::new(),
            };
            format!(
                "{}{marker}{}",
                vocabulary.indent(*depth),
                joined(parts, *join, vocabulary)
            )
        }
        Line::Field {
            depth,
            label,
            value,
        } => {
            format!(
                "{}{}",
                vocabulary.indent(*depth),
                labeled(label, &vocabulary.clean(value), label_width)
            )
        }
        Line::Node { depth, id, facts } => match label_width {
            None => {
                let agent: Vec<String> = facts
                    .iter()
                    .map(|fact| vocabulary.clean(&fact.agent))
                    .collect();
                let separator = vocabulary.separator(Join::Dot);
                format!(
                    "{}{}{separator}{}",
                    vocabulary.indent(*depth),
                    vocabulary.clean(id),
                    agent.join(separator)
                )
            }
            Some(_) => {
                let rows = facts.iter().map(|fact| {
                    let row = labeled(fact.label, &vocabulary.clean(&fact.human), label_width);
                    format!("{}{row}", vocabulary.indent(depth + 1))
                });
                std::iter::once(format!(
                    "{}{}",
                    vocabulary.indent(*depth),
                    vocabulary.clean(id)
                ))
                .chain(rows)
                .collect::<Vec<_>>()
                .join("\n")
            }
        },
        Line::Note { state, text } => {
            format!("{} {}", vocabulary.symbol(*state), vocabulary.clean(text))
        }
        Line::Raw { depth, text } => format!("{}{text}", vocabulary.indent(*depth)),
        Line::Output { depth, text } => {
            format!(
                "{}{}",
                vocabulary.indent(*depth),
                vocabulary.clean_output(text)
            )
        }
    }
}

fn joined(parts: &[String], join: Join, vocabulary: &impl Vocabulary) -> String {
    let cleaned: Vec<String> = parts.iter().map(|part| vocabulary.clean(part)).collect();

    cleaned.join(vocabulary.separator(join))
}

fn labeled(label: &str, value: &str, label_width: Option<usize>) -> String {
    let padding = " ".repeat(
        label_width
            .unwrap_or_default()
            .saturating_sub(label.chars().count()),
    );
    format!("{label}:{padding} {value}")
}

fn closing_line(closing: &Closing, vocabulary: &impl Vocabulary) -> Option<String> {
    match closing {
        Closing::None => None,
        Closing::Next(step) => Some(vocabulary.next(&vocabulary.clean(step))),
        Closing::NothingBlocked(step) => Some(vocabulary.nothing_blocked(&vocabulary.clean(step))),
    }
}

#[cfg(test)]
mod tests;
