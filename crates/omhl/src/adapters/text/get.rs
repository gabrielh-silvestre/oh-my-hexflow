use serde_json::Value;

use super::base::{Entry, NodeContainer, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Join, Line, Marker, scalar_fields, text};
use super::error;
use super::vocab::{State, seq_label};

/// O `record_id` fica à vista porque é o `base` de uma revisão do nó.
pub static PROFILE: Profile = Profile {
    label: "get",
    nodes: Some(NodeContainer::Object("node")),
    success: &[
        Entry::new(Region::Root, "id", Shape::Literal),
        // O app compõe o id da attestation de SHA, check e work item.
        Entry::new(Region::Root, "id", Shape::Closed).scoped("attestation"),
        Entry::new(Region::Root, "type", Shape::Closed),
        Entry::new(Region::Root, "seq", Shape::Literal),
        Entry::new(Region::Root, "at", Shape::Literal),
        Entry::new(Region::Root, "shadowed", Shape::Derived),
        Entry::new(Region::Root, "record_id", Shape::Literal),
        Entry::new(Region::Root, "external_id", Shape::Literal).optional(),
        Entry::new(Region::Author, "agent", Shape::Literal),
        Entry::new(Region::Author, "client", Shape::Closed),
        Entry::new(Region::Author, "model", Shape::Literal).optional(),
        Entry::new(Region::Snapshot, "kind", Shape::Closed).optional(),
        Entry::new(Region::Snapshot, "id", Shape::Hash(40)).optional(),
        Entry::data("check", "how", Shape::Literal),
        Entry::data("check", "blocking", Shape::Derived),
        Entry::data("check", "event", Shape::Closed).under("when"),
        Entry::data("check", "vocabulary", Shape::Closed).under("when"),
        Entry::data("check", "matcher", Shape::Literal)
            .under("when")
            .optional(),
        Entry::data("check", "concern", Shape::Closed).optional(),
        Entry::data("check", "variant", Shape::Closed).optional(),
        Entry::data("check", "run", Shape::Closed).optional(),
        Entry::data("check", "timeout_secs", Shape::Literal).optional(),
        Entry::data("check", "exit", Shape::Derived)
            .under("expect")
            .optional(),
        Entry::data("check", "outcome", Shape::Closed)
            .under("expect")
            .optional(),
        Entry::data("check", "stdout", Shape::Literal)
            .under("expect")
            .optional(),
        Entry::data("check", "stderr", Shape::Literal)
            .under("expect")
            .optional(),
        Entry::data("attestation", "check", Shape::Literal),
        Entry::data("attestation", "work_item", Shape::Literal),
        Entry::data("attestation", "outcome", Shape::Closed),
        Entry::data("attestation", "ok", Shape::Derived),
        Entry::data("attestation", "note", Shape::Literal).optional(),
        Entry::data("attestation", "exit_code", Shape::Derived).optional(),
        Entry::data("attestation", "duration_ms", Shape::Derived).optional(),
        Entry::data("attestation", "timed_out", Shape::Derived).optional(),
        Entry::data("attestation", "truncated", Shape::Derived).optional(),
        Entry::data("attestation", "command", Shape::Closed).optional(),
        Entry::data("attestation", "spec_digest", Shape::Literal).optional(),
        Entry::data("attestation", "byproduct", Shape::Literal).optional(),
        Entry::data("premise", "text", Shape::Literal),
        Entry::data("premise", "status", Shape::Closed),
        Entry::data("premise", "kind", Shape::Closed).optional(),
        Entry::data("premise", "work_item", Shape::Literal).optional(),
        Entry::data("premise", "by", Shape::Closed)
            .under("confirmed_by")
            .optional(),
        Entry::data("premise", "question", Shape::Literal)
            .under("confirmed_by")
            .optional(),
        Entry::data("premise", "answer", Shape::Literal)
            .under("confirmed_by")
            .optional(),
        Entry::data("guideline", "text", Shape::Literal),
        Entry::data("guideline", "kind", Shape::Closed).optional(),
        Entry::data("guideline", "by", Shape::Closed)
            .under("confirmed_by")
            .optional(),
        Entry::data("guideline", "question", Shape::Literal)
            .under("confirmed_by")
            .optional(),
        Entry::data("guideline", "answer", Shape::Literal)
            .under("confirmed_by")
            .optional(),
        Entry::data("decision", "text", Shape::Literal),
        Entry::data("decision", "work_item", Shape::Literal).optional(),
        Entry::data("decision", "source", Shape::Literal).under("evidence"),
        Entry::data("decision", "summary", Shape::Literal).under("evidence"),
        Entry::data("gap", "question", Shape::Literal),
        Entry::data("gap", "context", Shape::Literal),
        Entry::data("gap", "work_item", Shape::Literal).optional(),
        Entry::data("gap", "kind", Shape::Closed).optional(),
        Entry::data("divergence", "source_ref", Shape::Literal),
        Entry::data("divergence", "kind", Shape::Closed),
        Entry::data("divergence", "text", Shape::Literal),
        Entry::data("divergence", "resolution", Shape::Closed),
        Entry::data("divergence", "work_item", Shape::Literal),
        Entry::data("divergence", "amendment", Shape::Literal).optional(),
        Entry::data("divergence", "source", Shape::Literal).under("evidence"),
        Entry::data("divergence", "summary", Shape::Literal).under("evidence"),
        Entry::data("divergence", "by", Shape::Closed)
            .under("confirmed_by")
            .optional(),
        Entry::data("divergence", "question", Shape::Literal)
            .under("confirmed_by")
            .optional(),
        Entry::data("divergence", "answer", Shape::Literal)
            .under("confirmed_by")
            .optional(),
        Entry::data("work-item", "title", Shape::Literal),
        Entry::data("work-item", "status", Shape::Closed),
        Entry::data("work-item", "name", Shape::Literal)
            .under("gate")
            .optional(),
        Entry::data("work-item", "passed", Shape::Derived)
            .under("gate")
            .optional(),
        Entry::data("work-item", "chain_head_seq", Shape::Literal)
            .under("gate")
            .optional(),
        Entry::data("work-item", "kind", Shape::Closed)
            .under("snapshot")
            .optional(),
        Entry::data("work-item", "id", Shape::Hash(40))
            .under("snapshot")
            .optional(),
        Entry::data("pull-request", "title", Shape::Literal),
        Entry::data("pull-request", "status", Shape::Closed),
        Entry::data("pull-request", "provider", Shape::Closed),
        Entry::data("pull-request", "repo", Shape::Literal),
        Entry::data("pull-request", "number", Shape::Literal),
        Entry::data("pull-request", "branch", Shape::Literal).optional(),
        Entry::data("pull-request", "name", Shape::Literal)
            .under("gate")
            .optional(),
        Entry::data("pull-request", "passed", Shape::Derived)
            .under("gate")
            .optional(),
        Entry::data("pull-request", "chain_head_seq", Shape::Literal)
            .under("gate")
            .optional(),
        Entry::data("pull-request", "kind", Shape::Closed)
            .under("snapshot")
            .optional(),
        Entry::data("pull-request", "id", Shape::Hash(40))
            .under("snapshot")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub struct Get;

const NOT_IN_IDENTITY: [&str; 4] = ["id", "type", "seq", "data"];

impl Specialization for Get {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn state(&self, value: &Value) -> State {
        match value["node"].is_null() {
            true => State::Warning,
            false => State::Ok,
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let node = &value["node"];
        match node.is_null() {
            true => vec!["node not found".to_owned()],
            false => vec![
                text(node, "id"),
                text(node, "type"),
                seq_label(&node["seq"]),
            ],
        }
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let node = &value["node"];
        let Some(fields) = node.as_object() else {
            return Vec::new();
        };
        let identity = fields
            .iter()
            .filter(|(key, _)| !NOT_IN_IDENTITY.contains(&key.as_str()))
            .flat_map(|(key, field)| scalar_fields(key, field));
        let data = scalar_fields("", &node["data"]);

        let mut lines: Vec<Line> = identity
            .map(|(label, value)| Line::Field {
                depth: 1,
                label,
                value,
            })
            .collect();
        if !data.is_empty() {
            lines.push(Line::Item {
                depth: 0,
                marker: Marker::None,
                parts: vec!["data".to_owned()],
                join: Join::Dot,
            });
            lines.extend(data.into_iter().map(|(label, value)| Line::Field {
                depth: 1,
                label,
                value,
            }));
        }

        lines
    }

    fn closing(&self, value: &Value) -> Closing {
        match value["node"].is_null() {
            true => Closing::Next(
                "confirm the id with omhl list; a native check is not stored in the database, so omhl get never finds it"
                    .to_owned(),
            ),
            false => Closing::None,
        }
    }
}
