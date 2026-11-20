use std::fmt::Write;

use hexlog::{NodeView, SemanticId};
use serde_json::Value;

use crate::native::NativeBase;
use crate::workflow::gap::{is_confirmed_guideline, is_confirmed_premise};

pub const CONTEXT_BUDGET: usize = 9_500;

const FOOTER_TAIL: &str = " more nodes omitted for size; run `omhl list` to read them]";

pub struct RenderInput<'i> {
    pub tenant: &'i str,
    pub native: &'i NativeBase,
    pub checks: &'i [NodeView],
    pub guidelines: &'i [NodeView],
    pub decisions: &'i [NodeView],
    pub premises: &'i [NodeView],
    pub after: &'i [(SemanticId, SemanticId)],
}

#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub omitted: usize,
}

pub fn render(input: &RenderInput<'_>, budget: usize) -> Rendered {
    let mut text = String::new();
    // Byte offset where each tenant node starts, in display order: the cut points of the budget.
    let mut cuts = Vec::new();

    push_preamble(&mut text, input.tenant);
    push_checks(&mut text, &mut cuts, input);
    push_guidelines(&mut text, &mut cuts, input);
    push_tail_section(
        &mut text,
        &mut cuts,
        "Decisions",
        &visible_sorted(input.decisions),
        push_decision,
    );
    let (pending, binding): (Vec<&NodeView>, Vec<&NodeView>) = visible_sorted(input.premises)
        .into_iter()
        .partition(|node| premise_mark(node) == PremiseMark::Pending);
    push_tail_section(&mut text, &mut cuts, "Premises", &binding, push_premise);
    push_tail_section(
        &mut text,
        &mut cuts,
        "Pending premises",
        &pending,
        push_premise,
    );

    let omitted = fit_to_budget(&mut text, &cuts, budget);

    Rendered { text, omitted }
}

fn visible_sorted(nodes: &[NodeView]) -> Vec<&NodeView> {
    let mut visible: Vec<&NodeView> = nodes.iter().filter(|node| !node.shadowed).collect();
    sort_by_id(&mut visible);
    visible
}

/// Ordem estável por id, a mesma no contexto e na projeção.
pub(crate) fn sort_by_id(nodes: &mut [&NodeView]) {
    nodes.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
}

fn push_preamble(text: &mut String, tenant: &str) {
    text.push_str("# omhl context for tenant ");
    push_value(text, tenant);
    text.push_str(
        "\nThese are project instructions, not enforced; nothing here blocks any action.\n",
    );
}

fn push_checks(text: &mut String, cuts: &mut Vec<usize>, input: &RenderInput<'_>) {
    begin_unit(text);
    text.push_str("## Checks\n");
    for check in input.native.checks() {
        push_check_block(
            text,
            check.id,
            (check.event, None),
            check.blocking,
            check.how,
            input.after,
        );
    }
    let cut_at = text.len();
    let nodes = visible_sorted(input.checks);
    push_tenant_nodes(text, cuts, cut_at, &nodes, |text, node| {
        push_check(text, node, input.after);
    });
}

fn push_guidelines(text: &mut String, cuts: &mut Vec<usize>, input: &RenderInput<'_>) {
    begin_unit(text);
    text.push_str("## Guidelines\n");
    for guideline in input.native.guidelines() {
        begin_block(text, guideline.id);
        push_line(text, "confirmed", "true");
        push_line(text, "text", guideline.text);
    }
    let cut_at = text.len();
    let mut nodes = visible_sorted(input.guidelines);
    // Estável: dentro de cada grupo a ordem por id se mantém.
    nodes.sort_by_key(|node| kind_rank(&node.data));
    push_tenant_nodes(text, cuts, cut_at, &nodes, push_guideline);
}

/// Negócio antes de arquitetura, as sem `kind` (legadas) por último: o corte do orçamento vem do fim.
fn kind_rank(data: &Value) -> u8 {
    match data["kind"].as_str() {
        Some("business") => 0,
        Some("architecture") => 1,
        _ => 2,
    }
}

/// A section with no native nodes: it only exists while it has a tenant node.
fn push_tail_section(
    text: &mut String,
    cuts: &mut Vec<usize>,
    title: &str,
    nodes: &[&NodeView],
    push_node: impl Fn(&mut String, &NodeView),
) {
    if nodes.is_empty() {
        return;
    }
    let section_start = text.len();
    begin_unit(text);
    text.push_str("## ");
    text.push_str(title);
    text.push('\n');
    push_tenant_nodes(text, cuts, section_start, nodes, push_node);
}

fn push_tenant_nodes(
    text: &mut String,
    cuts: &mut Vec<usize>,
    mut cut_at: usize,
    nodes: &[&NodeView],
    push_node: impl Fn(&mut String, &NodeView),
) {
    for node in nodes {
        cuts.push(cut_at);
        push_node(text, node);
        cut_at = text.len();
    }
}

/// Units (header, section title, block, footer) are separated by one blank line.
fn begin_unit(text: &mut String) {
    if !text.is_empty() {
        text.push('\n');
    }
}

fn begin_block(text: &mut String, id: &str) {
    begin_unit(text);
    text.push_str("### ");
    text.push_str(id);
    text.push('\n');
}

fn push_label(text: &mut String, label: &str) {
    text.push_str("- ");
    text.push_str(label);
    text.push_str(": ");
}

/// Each line break of a value (LF, CR, CRLF as one, VT, FF, NEL, LS, PS) becomes LF plus two
/// spaces, so a value can never forge a block header.
fn push_value(text: &mut String, value: &str) {
    let mut chars = value.chars().peekable();
    while let Some(char) = chars.next() {
        if !matches!(
            char,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            text.push(char);
            continue;
        }
        if char == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
        }
        text.push_str("\n  ");
    }
}

fn push_line(text: &mut String, label: &str, value: &str) {
    push_label(text, label);
    push_value(text, value);
    text.push('\n');
}

fn push_check(text: &mut String, node: &NodeView, after: &[(SemanticId, SemanticId)]) {
    let when = &node.data["when"];
    let event = str_of(&when["event"]);
    let matcher = when["matcher"].as_str();
    let blocking = node.data["blocking"].as_bool().unwrap_or_default();

    push_check_block(
        text,
        node.id.as_str(),
        (event, matcher),
        blocking,
        str_of(&node.data["how"]),
        after,
    );
}

fn push_check_block(
    text: &mut String,
    id: &str,
    (event, matcher): (&str, Option<&str>),
    blocking: bool,
    how: &str,
    after: &[(SemanticId, SemanticId)],
) {
    begin_block(text, id);
    push_line(text, "event", event);
    if let Some(matcher) = matcher {
        push_line(text, "matcher", matcher);
    }
    push_line(text, "blocking", if blocking { "true" } else { "false" });
    push_after(text, id, after);
    push_line(text, "how", how);
}

fn push_after(text: &mut String, id: &str, after: &[(SemanticId, SemanticId)]) {
    let mut targets: Vec<&str> = after
        .iter()
        .filter(|(from, _)| from.as_str() == id)
        .map(|(_, to)| to.as_str())
        .collect();
    if targets.is_empty() {
        return;
    }
    targets.sort_unstable();

    push_label(text, "after");
    for (index, target) in targets.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(target);
    }
    text.push('\n');
}

/// Diretiva confirmada: escrita pelo `loader` (extension) ou com `confirmed_by`; a nativa já é confirmada.
fn push_guideline(text: &mut String, node: &NodeView) {
    let confirmed = is_confirmed_guideline(false, node.author.client == "loader", &node.data);
    begin_block(text, node.id.as_str());
    push_line(text, "confirmed", if confirmed { "true" } else { "false" });
    if let Some(kind) = node.data["kind"].as_str() {
        push_line(text, "kind", kind);
    }
    push_line(text, "text", str_of(&node.data["text"]));
}

fn push_decision(text: &mut String, node: &NodeView) {
    begin_block(text, node.id.as_str());
    push_line(text, "text", str_of(&node.data["text"]));
    for evidence in node.data["evidence"].as_array().into_iter().flatten() {
        push_label(text, "evidence");
        push_value(text, str_of(&evidence["source"]));
        text.push_str(": ");
        push_value(text, str_of(&evidence["summary"]));
        text.push('\n');
    }
    push_work_item(text, node);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PremiseMark {
    Confirmed,
    Pending,
    Autonomous,
}

impl PremiseMark {
    fn label(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Pending => "pending",
            Self::Autonomous => "autonomous",
        }
    }
}

/// Premissa de negócio sem confirmação espera o dono; a técnica ou sem `kind` o agente assume sozinho.
fn premise_mark(node: &NodeView) -> PremiseMark {
    if is_confirmed_premise(&node.data) {
        return PremiseMark::Confirmed;
    }
    if node.data["kind"] == "business" {
        return PremiseMark::Pending;
    }
    PremiseMark::Autonomous
}

fn push_premise(text: &mut String, node: &NodeView) {
    begin_block(text, node.id.as_str());
    push_line(text, "mark", premise_mark(node).label());
    if let Some(kind) = node.data["kind"].as_str() {
        push_line(text, "kind", kind);
    }
    push_line(text, "status", str_of(&node.data["status"]));
    push_line(text, "text", str_of(&node.data["text"]));
    push_work_item(text, node);
}

fn push_work_item(text: &mut String, node: &NodeView) {
    if let Some(work_item) = node.data["work_item"].as_str() {
        push_line(text, "work_item", work_item);
    }
}

// Nodes are validated by their hexlog type on write; a missing field renders empty.
fn str_of(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// Drops tenant nodes from the end until the text and its footer fit; native nodes never go.
fn fit_to_budget(text: &mut String, cuts: &[usize], budget: usize) -> usize {
    let mut end = text.len();
    let mut chars = text.chars().count();
    let mut omitted = 0;
    if chars <= budget {
        return omitted;
    }

    for &start in cuts.iter().rev() {
        chars -= text[start..end].chars().count();
        end = start;
        omitted += 1;
        if chars + footer_chars(omitted) <= budget {
            break;
        }
    }

    text.truncate(end);
    if omitted > 0 {
        begin_unit(text);
        let _ = writeln!(text, "[{omitted}{FOOTER_TAIL}");
    }
    omitted
}

/// Characters the footer adds: separating LF, `[`, the digits, the tail and the closing LF.
fn footer_chars(omitted: usize) -> usize {
    let digits = omitted.ilog10() as usize + 1;
    1 + 1 + digits + FOOTER_TAIL.chars().count() + 1
}

#[cfg(test)]
mod tests;
