use std::borrow::Cow;
use std::collections::BTreeMap;

use hexlog::{EndpointStatus, NodeView, RelationView};
use serde_json::Value;

use super::{ProjectedFile, ProjectionError, ProjectionState};
use crate::project::layout::{
    GENERATED_HEADER, OMHL_DIR, PROJECTION_DIR, PROJECTION_FILE_MAX_BYTES, PROJECTION_FILES_MAX,
};
use crate::project::text::{is_invisible_format, normalize_line_breaks};
use crate::render::{is_late, sort_by_id};

const TRUNCATION_MARKER: &str = "<!-- omhl: truncated -->\n";
const INLINE_STRING_MAX_CHARS: usize = 80;
const FILE_MAX_BYTES: usize = PROJECTION_FILE_MAX_BYTES as usize;
const SECTIONS: [(&str, bool, &str); 6] = [
    ("premise", false, "Premises"),
    ("premise", true, "Late premises"),
    ("decision", false, "Decisions"),
    ("attestation", false, "Attestations"),
    ("gap", false, "Gaps"),
    ("divergence", false, "Divergences"),
];

/// The bytes depend only on the
/// current state, never on record ids, sequence numbers, timestamps or authors.
pub fn build(state: &ProjectionState) -> Result<Vec<ProjectedFile>, ProjectionError> {
    let mut items: Vec<&NodeView> = state
        .nodes
        .iter()
        .filter(|node| !node.shadowed && is_work_item(node))
        .collect();
    if items.len() > PROJECTION_FILES_MAX {
        return Err(ProjectionError {
            path: format!("{OMHL_DIR}/{PROJECTION_DIR}"),
            code: "too-many-work-items",
            message: "there are more work items than the projection allows".to_owned(),
        });
    }
    sort_by_id(&mut items);
    let children = children_by_owner(&state.nodes);
    let owners = owner_by_node_id(&items, &children);
    let relations = relations_by_owner(&state.relations, &owners);
    Ok(items
        .iter()
        .map(|item| {
            let id = item.id.as_str();
            ProjectedFile {
                name: format!("{id}.md"),
                content: render_file(
                    item,
                    children.get(id).map_or(&[], Vec::as_slice),
                    relations.get(id).map_or(&[], Vec::as_slice),
                ),
            }
        })
        .collect())
}

fn is_work_item(node: &NodeView) -> bool {
    matches!(node.type_name.as_str(), "work-item" | "pull-request")
}

fn children_by_owner(nodes: &[NodeView]) -> BTreeMap<&str, Vec<&NodeView>> {
    let mut grouped: BTreeMap<&str, Vec<&NodeView>> = BTreeMap::new();
    for node in nodes.iter().filter(|node| {
        !node.shadowed
            && SECTIONS
                .iter()
                .any(|(kind, _, _)| *kind == node.type_name.as_str())
    }) {
        if let Some(owner) = node.data.get("work_item").and_then(Value::as_str) {
            grouped.entry(owner).or_default().push(node);
        }
    }
    for owned in grouped.values_mut() {
        sort_by_id(owned);
    }
    grouped
}

fn owner_by_node_id<'s>(
    items: &[&'s NodeView],
    children: &BTreeMap<&'s str, Vec<&'s NodeView>>,
) -> BTreeMap<&'s str, &'s str> {
    let mut owners = BTreeMap::new();
    for (owner, nodes) in children {
        for node in nodes {
            owners.insert(node.id.as_str(), *owner);
        }
    }
    for item in items {
        owners.insert(item.id.as_str(), item.id.as_str());
    }
    owners
}

fn relations_by_owner<'s>(
    relations: &'s [RelationView],
    owners: &BTreeMap<&'s str, &'s str>,
) -> BTreeMap<&'s str, Vec<&'s RelationView>> {
    let mut grouped: BTreeMap<&str, Vec<&RelationView>> = BTreeMap::new();
    for relation in relations.iter().filter(|relation| {
        !relation.retired
            && relation.from_status != EndpointStatus::Retired
            && relation.to_status != EndpointStatus::Retired
    }) {
        if let Some(owner) = owners.get(relation.from.as_str()) {
            grouped.entry(*owner).or_default().push(relation);
        }
    }
    for owned in grouped.values_mut() {
        owned.sort_by(|a, b| relation_key(a).cmp(&relation_key(b)));
    }
    grouped
}

fn relation_key(relation: &RelationView) -> (&str, &str, &str) {
    (
        relation.relation_type.as_str(),
        relation.from.as_str(),
        relation.to.as_str(),
    )
}

fn render_file(item: &NodeView, children: &[&NodeView], relations: &[&RelationView]) -> String {
    let title = item
        .data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut blocks = vec![
        format!("{GENERATED_HEADER}\n# {}", escape(title, &[])),
        header_markers(item),
    ];
    for (kind, late, section) in SECTIONS {
        let mut owned = children
            .iter()
            .filter(|node| node.type_name.as_str() == kind && is_late(node) == late)
            .peekable();
        if owned.peek().is_none() {
            continue;
        }
        blocks.push(format!("## {section}"));
        for node in owned {
            push_node_blocks(&mut blocks, node);
        }
    }
    push_relation_blocks(&mut blocks, relations);
    truncate_to_limit(blocks.join("\n\n") + "\n")
}

fn push_relation_blocks(blocks: &mut Vec<String>, relations: &[&RelationView]) {
    if relations.is_empty() {
        return;
    }
    blocks.push("## Relations".to_owned());
    let lines: Vec<String> = relations
        .iter()
        .map(|r| {
            let (name, from, to) = relation_key(r);
            format!("- {name}: {from} -> {to}")
        })
        .collect();
    blocks.push(lines.join("\n"));
}

fn header_markers(item: &NodeView) -> String {
    let status = item
        .data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut lines = vec![
        format!("- id: {}", item.id.as_str()),
        format!("- type: {}", item.type_name.as_str()),
        format!("- status: {}", escape(status, &[])),
    ];
    lines.extend(data_fields(&item.data, &["title", "status", "gate"]));
    lines.extend(gate_line(&item.data));
    lines.join("\n")
}

fn gate_line(data: &Value) -> Option<String> {
    let gate = data.get("gate")?;
    let name = escape(gate.get("name")?.as_str()?, &[]);
    let passed = gate.get("passed")?.as_bool()?;
    let head = gate.get("chain_head_seq")?.as_u64()?;
    let snapshot = snapshot_ref(gate.get("snapshot")?)?;
    Some(format!(
        "- gate: {name} passed={passed} chain_head_seq={head} snapshot={snapshot}"
    ))
}

fn snapshot_ref(snapshot: &Value) -> Option<String> {
    let kind = snapshot.get("kind")?.as_str()?;
    let id = snapshot.get("id")?.as_str()?;
    Some(format!("{}:{}", escape(kind, &[]), escape(id, &[])))
}

fn push_node_blocks(blocks: &mut Vec<String>, node: &NodeView) {
    blocks.push(format!("### {}", node.id.as_str()));
    let mut lines = data_fields(&node.data, &["work_item"]);
    let snapshot = node
        .snapshot
        .as_ref()
        .and_then(|snapshot| serde_json::to_value(snapshot).ok())
        .and_then(|snapshot| snapshot_ref(&snapshot));
    lines.extend(snapshot.map(|snapshot| format!("- snapshot: {snapshot}")));
    if !lines.is_empty() {
        blocks.push(lines.join("\n"));
    }
}

fn data_fields(data: &Value, skipped: &[&str]) -> Vec<String> {
    data.as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !skipped.contains(&key.as_str()))
        .map(|(key, value)| render_field(&escape(key, &[]), value))
        .collect()
}

fn render_field(key: &str, value: &Value) -> String {
    match value {
        Value::String(text) => {
            let text = normalize_line_breaks(text);
            if text.contains('\n') || text.chars().count() > INLINE_STRING_MAX_CHARS {
                fenced(key, "text", &escape(&text, &['\n', '\t']))
            } else {
                format!("- {key}: {}", escape(&value.to_string(), &[]))
            }
        }
        Value::Array(_) | Value::Object(_) => {
            fenced(key, "json", &escape(&format!("{value:#}"), &['\n']))
        }
        _ => format!("- {key}: {value}"),
    }
}

/// The fence is longer than any run of backticks in the body, so the body cannot close it.
fn fenced(key: &str, info: &str, body: &str) -> String {
    let longest_run = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat((longest_run + 1).max(3));
    format!("- {key}:\n{fence}{info}\n{body}\n{fence}")
}

fn escape<'a>(text: &'a str, keep: &[char]) -> Cow<'a, str> {
    let needs_escape = |c: char| (c.is_control() || is_invisible_format(c)) && !keep.contains(&c);
    if !text.chars().any(needs_escape) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if needs_escape(c) {
            for unit in c.encode_utf16(&mut [0; 2]) {
                escaped.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            escaped.push(c);
        }
    }
    Cow::Owned(escaped)
}

fn truncate_to_limit(mut content: String) -> String {
    if content.len() <= FILE_MAX_BYTES {
        return content;
    }
    let room = FILE_MAX_BYTES - TRUNCATION_MARKER.len();
    let cut = content.as_bytes()[..room]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1);
    content.truncate(cut);
    content.push_str(TRUNCATION_MARKER);
    content
}

#[cfg(test)]
mod tests;
