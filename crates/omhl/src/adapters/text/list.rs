//! Texto do `omhl list`: uma linha por nó com os campos do tipo e a paginação em `Next:`.

use serde_json::Value;

use super::base::{Entry, NodeContainer, Profile, Region, Shape, Specialization, WILDCARD};
use super::doc::{Closing, Fact, Line, items, opt_text, text};
use super::error;
use super::vocab::{BLOCKING, NON_BLOCKING, plural, seq_label};

/// O `list` é um resumo por tipo: o que o tipo mostra é `Literal` ou `Closed` e o resto de `data` cai no curinga
/// `Noise` do tipo (o `get` mostra o nó inteiro). Fora de `data`, `record_id`, `at`, o autor e o snapshot são ruído.
pub static PROFILE: Profile = Profile {
    label: "",
    nodes: Some(NodeContainer::Array("items")),
    success: &[
        Entry::new(Region::Root, "id", Shape::Literal),
        // O app compõe o id da attestation de SHA, check e work item.
        Entry::new(Region::Root, "id", Shape::Closed).scoped("attestation"),
        Entry::new(Region::Root, "type", Shape::Closed),
        Entry::new(Region::Root, "seq", Shape::Literal),
        Entry::new(Region::Root, "shadowed", Shape::Derived),
        Entry::new(Region::Root, "next_after_seq", Shape::Literal).optional(),
        Entry::new(Region::Root, "record_id", Shape::Noise),
        Entry::new(Region::Root, "at", Shape::Noise),
        Entry::new(Region::Root, "external_id", Shape::Noise).optional(),
        Entry::new(Region::Author, "agent", Shape::Noise),
        Entry::new(Region::Author, "client", Shape::Noise),
        Entry::new(Region::Author, "model", Shape::Noise).optional(),
        Entry::new(Region::Snapshot, "kind", Shape::Noise).optional(),
        Entry::new(Region::Snapshot, "id", Shape::Noise).optional(),
        Entry::data("check", "event", Shape::Closed).under("when"),
        Entry::data("check", "blocking", Shape::Derived),
        Entry::data("check", "concern", Shape::Closed).optional(),
        Entry::data("check", "variant", Shape::Closed).optional(),
        Entry::data("attestation", "check", Shape::Literal),
        Entry::data("attestation", "work_item", Shape::Literal),
        Entry::data("attestation", "outcome", Shape::Closed),
        Entry::data("premise", "status", Shape::Closed),
        Entry::data("premise", "kind", Shape::Closed).optional(),
        Entry::data("premise", "work_item", Shape::Literal).optional(),
        Entry::data("premise", "text", Shape::Literal),
        Entry::data("work-item", "status", Shape::Closed),
        Entry::data("work-item", "title", Shape::Literal),
        Entry::data("guideline", "kind", Shape::Closed).optional(),
        Entry::data("guideline", "text", Shape::Literal),
        Entry::data("decision", "text", Shape::Literal),
        Entry::data("gap", "question", Shape::Literal),
        Entry::data("divergence", "resolution", Shape::Closed),
        Entry::data("divergence", "text", Shape::Literal),
        Entry::data("pull-request", "status", Shape::Closed),
        Entry::data("pull-request", "title", Shape::Literal),
        Entry::data("check", WILDCARD, Shape::Noise),
        Entry::data("attestation", WILDCARD, Shape::Noise),
        Entry::data("premise", WILDCARD, Shape::Noise),
        Entry::data("work-item", WILDCARD, Shape::Noise),
        Entry::data("guideline", WILDCARD, Shape::Noise),
        Entry::data("decision", WILDCARD, Shape::Noise),
        Entry::data("gap", WILDCARD, Shape::Noise),
        Entry::data("divergence", WILDCARD, Shape::Noise),
        Entry::data("pull-request", WILDCARD, Shape::Noise),
    ],
    failure: &error::ENTRIES,
};

pub struct List;

impl Specialization for List {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let seqs: Vec<u64> = items(value, "items")
            .filter_map(|node| node["seq"].as_u64())
            .collect();
        let count = plural(seqs.len(), "node");
        match (seqs.iter().min(), seqs.iter().max()) {
            (Some(first), Some(last)) if first != last => {
                vec![count, format!("seq {first}–{last}")]
            }
            (Some(only), _) => vec![count, seq_label(only)],
            (None, _) => vec![count],
        }
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "items")
            .map(|node| Line::Node {
                depth: 1,
                id: text(node, "id"),
                facts: node_facts(node),
            })
            .collect()
    }

    fn closing(&self, value: &Value) -> Closing {
        match value["next_after_seq"].as_u64() {
            Some(next) => Closing::Next(format!(
                "omhl list --after-seq {next} with the same filters for more. One node in full: omhl get <id>"
            )),
            None => Closing::None,
        }
    }
}

fn node_facts(node: &Value) -> Vec<Fact> {
    let node_type = text(node, "type");
    let mut facts = vec![
        Fact::same("type", node_type.clone()),
        Fact {
            label: "seq",
            agent: seq_label(&node["seq"]),
            human: node["seq"].to_string(),
        },
    ];
    if node["shadowed"] == true {
        facts.push(Fact {
            label: "shadowed",
            agent: "shadowed".to_owned(),
            human: "yes".to_owned(),
        });
    }
    let data = &node["data"];
    match node_type.as_str() {
        "check" => facts.extend(check_facts(data)),
        other => {
            facts.extend(shown_keys(other).iter().filter_map(|(label, key)| {
                opt_text(data, key).map(|value| Fact::same(label, value))
            }))
        }
    }

    facts
}

fn check_facts(data: &Value) -> Vec<Fact> {
    let blocking = data["blocking"] == true;
    let concern = opt_text(data, "concern").map(|concern| {
        let shown = match opt_text(data, "variant") {
            Some(variant) => format!("{concern}/{variant}"),
            None => concern,
        };
        Fact::same("concern", shown)
    });

    let mut facts: Vec<Fact> = opt_text(&data["when"], "event")
        .map(|event| Fact::same("event", event))
        .into_iter()
        .collect();
    facts.push(Fact {
        label: "blocking",
        agent: match blocking {
            true => BLOCKING.to_owned(),
            false => NON_BLOCKING.to_owned(),
        },
        human: match blocking {
            true => "yes".to_owned(),
            false => "no".to_owned(),
        },
    });
    facts.extend(concern);

    facts
}

/// Rótulo e chave de `data` que cada tipo mostra, na ordem da linha; `of` é o work item.
fn shown_keys(node_type: &str) -> &'static [(&'static str, &'static str)] {
    match node_type {
        "attestation" => &[
            ("outcome", "outcome"),
            ("check", "check"),
            ("of", "work_item"),
        ],
        "premise" => &[
            ("status", "status"),
            ("kind", "kind"),
            ("of", "work_item"),
            ("text", "text"),
        ],
        "work-item" | "pull-request" => &[("status", "status"), ("title", "title")],
        "guideline" => &[("kind", "kind"), ("text", "text")],
        "decision" => &[("text", "text")],
        "gap" => &[("question", "question")],
        "divergence" => &[("resolution", "resolution"), ("text", "text")],
        _ => &[],
    }
}
