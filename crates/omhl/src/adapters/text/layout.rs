//! Os dois serializadores do `Doc`: `agent` (compacto, sem linha em branco) e `human` (blocos separados e rótulos alinhados).

use super::doc::{Closing, Doc, Join, Line, Marker};

pub fn agent(doc: &Doc) -> String {
    let mut lines: Vec<String> = doc
        .lines
        .iter()
        .map(|line| render_line(line, None))
        .collect();
    lines.extend(closing_line(&doc.closing));

    lines.join("\n")
}

/// Um bloco por cabeçalho, nota, item com filhos e fecho, separados por uma linha em branco.
pub fn human(doc: &Doc) -> String {
    let mut blocks: Vec<String> = blocks(&doc.lines)
        .into_iter()
        .map(|block| {
            let width = label_width(block);
            let lines: Vec<String> = block
                .iter()
                .map(|line| render_line(line, Some(width)))
                .collect();
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
        Line::Header { .. } | Line::Item { .. } | Line::Note { .. } | Line::Node { .. } => false,
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
            Line::Node { facts, .. } => facts.iter().map(|fact| fact.label.chars().count()).max(),
            _ => None,
        })
        .max()
        .unwrap_or_default()
}

/// `label_width` alinha os valores dos campos no `human`; o `agent` passa `None` e escreve `rótulo: valor`.
fn render_line(line: &Line, label_width: Option<usize>) -> String {
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
            format!("{}{}", indent(*depth), labeled(label, value, label_width))
        }
        Line::Node { depth, id, facts } => match label_width {
            None => {
                let agent: Vec<&str> = facts.iter().map(|fact| fact.agent.as_str()).collect();
                let separator = Join::Dot.separator();
                format!("{}{id}{separator}{}", indent(*depth), agent.join(separator))
            }
            Some(_) => {
                let rows = facts.iter().map(|fact| {
                    let row = labeled(fact.label, &fact.human, label_width);
                    format!("{}{row}", indent(depth + 1))
                });
                std::iter::once(format!("{}{id}", indent(*depth)))
                    .chain(rows)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        },
        Line::Note { symbol, text } => format!("{symbol} {text}"),
        Line::Raw { depth, text } => format!("{}{text}", indent(*depth)),
    }
}

fn labeled(label: &str, value: &str, label_width: Option<usize>) -> String {
    let padding = " ".repeat(
        label_width
            .unwrap_or_default()
            .saturating_sub(label.chars().count()),
    );
    format!("{label}:{padding} {value}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn header(text: &str) -> Line {
        Line::Header {
            symbol: "✔",
            parts: vec![text.to_owned()],
        }
    }

    fn item(depth: usize, text: &str) -> Line {
        Line::Item {
            depth,
            marker: Marker::Dash,
            parts: vec![text.to_owned()],
            join: Join::Dot,
        }
    }

    fn field(depth: usize, label: &str, value: &str) -> Line {
        Line::Field {
            depth,
            label: label.to_owned(),
            value: value.to_owned(),
        }
    }

    fn note(text: &str) -> Line {
        Line::Note {
            symbol: "⚠",
            text: text.to_owned(),
        }
    }

    fn doc(lines: Vec<Line>, closing: Closing) -> Doc {
        Doc { lines, closing }
    }

    fn joined_item(parts: &[&str], join: Join) -> Line {
        Line::Item {
            depth: 0,
            marker: Marker::None,
            parts: parts.iter().map(|part| (*part).to_owned()).collect(),
            join,
        }
    }

    #[test]
    fn it_should_indent_agent_lines_by_two_spaces_per_depth() {
        let doc = doc(
            vec![item(0, "a"), item(1, "b"), item(2, "c")],
            Closing::None,
        );

        let text = agent(&doc);

        assert_eq!(text, "- a\n  - b\n    - c");
    }

    #[test]
    fn it_should_not_insert_blank_lines_in_the_agent_layout() {
        let doc = doc(
            vec![
                header("run"),
                note("careful"),
                item(0, "a"),
                field(1, "k", "v"),
                item(0, "b"),
            ],
            Closing::Next("go".to_owned()),
        );

        let text = agent(&doc);

        assert_eq!(text, "✔ run\n⚠ careful\n- a\n  k: v\n- b\nNext: go.");
    }

    #[test]
    fn it_should_not_align_field_labels_in_the_agent_layout() {
        let doc = doc(
            vec![header("run"), field(1, "a", "1"), field(1, "longer", "2")],
            Closing::None,
        );

        let text = agent(&doc);

        assert_eq!(text, "✔ run\n  a: 1\n  longer: 2");
    }

    #[test]
    fn it_should_align_field_labels_within_a_human_block() {
        let doc = doc(
            vec![header("run"), field(0, "a", "1"), field(0, "longer", "2")],
            Closing::None,
        );

        let text = human(&doc);

        assert_eq!(text, "✔ run\na:      1\nlonger: 2");
    }

    #[test]
    fn it_should_align_field_labels_independently_per_human_block() {
        let doc = doc(
            vec![
                header("one"),
                field(0, "a", "1"),
                header("two"),
                field(0, "longer", "2"),
            ],
            Closing::None,
        );

        let text = human(&doc);

        assert_eq!(text, "✔ one\na: 1\n\n✔ two\nlonger: 2");
    }

    #[test]
    fn it_should_separate_human_blocks_with_exactly_one_blank_line() {
        let doc = doc(vec![header("run"), note("careful")], Closing::None);

        let text = human(&doc);

        assert_eq!(text, "✔ run\n\n⚠ careful");
    }

    #[test]
    fn it_should_keep_consecutive_childless_items_in_one_human_block() {
        let doc = doc(
            vec![header("run"), item(0, "a"), item(0, "b")],
            Closing::None,
        );

        let text = human(&doc);

        assert_eq!(text, "✔ run\n\n- a\n- b");
    }

    #[test]
    fn it_should_open_a_human_block_for_an_item_that_has_children() {
        let doc = doc(
            vec![item(0, "a"), item(0, "b"), field(1, "k", "v")],
            Closing::None,
        );

        let text = human(&doc);

        assert_eq!(text, "- a\n\n- b\n  k: v");
    }

    #[test]
    fn it_should_put_the_next_step_in_its_own_human_block() {
        let doc = doc(
            vec![header("run"), field(0, "k", "v")],
            Closing::Next("go".to_owned()),
        );

        let text = human(&doc);

        assert_eq!(text, "✔ run\nk: v\n\nNext: go.");
    }

    #[test]
    fn it_should_put_the_nothing_blocked_step_in_its_own_human_block() {
        let doc = doc(
            vec![header("run"), field(0, "k", "v")],
            Closing::NothingBlocked("go".to_owned()),
        );

        let text = human(&doc);

        assert_eq!(text, "✔ run\nk: v\n\nNothing blocked. Next: go.");
    }

    #[test]
    fn it_should_end_the_agent_text_at_the_last_line_when_there_is_no_closing() {
        let doc = doc(vec![header("run"), item(0, "a")], Closing::None);

        let text = agent(&doc);

        assert_eq!(text, "✔ run\n- a");
    }

    #[test]
    fn it_should_end_the_human_text_at_the_last_block_when_there_is_no_closing() {
        let doc = doc(vec![header("run"), item(0, "a")], Closing::None);

        let text = human(&doc);

        assert_eq!(text, "✔ run\n\n- a");
    }

    #[test]
    fn it_should_join_item_parts_with_a_dot_separator() {
        let doc = doc(vec![joined_item(&["a", "b"], Join::Dot)], Closing::None);

        let text = agent(&doc);

        assert_eq!(text, "a · b");
    }

    #[test]
    fn it_should_join_item_parts_with_a_two_space_gap() {
        let doc = doc(vec![joined_item(&["a", "b"], Join::Gap)], Closing::None);

        let text = agent(&doc);

        assert_eq!(text, "a  b");
    }

    #[test]
    fn it_should_not_end_the_agent_text_with_a_line_feed() {
        let doc = doc(vec![header("run")], Closing::Next("go".to_owned()));

        let text = agent(&doc);

        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn it_should_not_end_the_human_text_with_a_line_feed() {
        let doc = doc(vec![header("run")], Closing::Next("go".to_owned()));

        let text = human(&doc);

        assert!(!text.ends_with('\n'));
    }
}
