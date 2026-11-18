//! Texto dos relatórios `report answers`, `report recurrences` e `report retro`: uma linha por item.

use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Fact, Line, dash, field, items, joined, one_line, opt_text, text};
use super::error;
use super::vocab::{NONE, counted};

pub static ANSWERS_PROFILE: Profile = Profile {
    label: "report answers",
    nodes: None,
    success: &[
        Entry::new(Region::Data, "id", Shape::Literal)
            .under("gaps")
            .optional(),
        Entry::new(Region::Data, "state", Shape::Closed)
            .under("gaps")
            .optional(),
        Entry::new(Region::Data, "work_item", Shape::Literal)
            .under("gaps")
            .optional(),
        Entry::new(Region::Data, "answered_by", Shape::Literal)
            .under("gaps")
            .optional(),
        Entry::new(Region::Data, "answerer", Shape::Closed)
            .under("gaps")
            .optional(),
        Entry::new(Region::Data, "no_criterion", Shape::Derived)
            .under("gaps")
            .optional(),
        // Quem gravou a resposta não muda a decisão de quem lê o relatório.
        Entry::new(Region::Data, "agent", Shape::Noise)
            .under("author")
            .optional(),
        Entry::new(Region::Data, "client", Shape::Noise)
            .under("author")
            .optional(),
        Entry::new(Region::Data, "model", Shape::Noise)
            .under("author")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub static RECURRENCES_PROFILE: Profile = Profile {
    label: "report recurrences",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "min", Shape::Literal),
        Entry::new(Region::Data, "kind", Shape::Closed)
            .under("recurrences")
            .optional(),
        Entry::new(Region::Data, "text", Shape::Literal)
            .under("recurrences")
            .optional(),
        Entry::new(Region::Data, "work_items", Shape::Literal)
            .under("recurrences")
            .optional(),
        Entry::new(Region::Data, "premises", Shape::Literal)
            .under("recurrences")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub static RETRO_PROFILE: Profile = Profile {
    label: "report retro",
    nodes: None,
    success: &[
        Entry::new(Region::Data, "kind", Shape::Closed)
            .under("divergences")
            .optional(),
        Entry::new(Region::Data, "resolution", Shape::Closed)
            .under("divergences")
            .optional(),
        Entry::new(Region::Data, "ids", Shape::Literal)
            .under("divergences")
            .optional(),
        Entry::new(Region::Data, "name", Shape::Literal)
            .under("approved")
            .optional(),
        Entry::new(Region::Data, "divergence", Shape::Literal)
            .under("approved")
            .optional(),
        Entry::new(Region::Data, "pending", Shape::Literal)
            .under("amendments")
            .optional(),
        Entry::new(Region::Data, "gap", Shape::Literal)
            .under("no_criterion")
            .optional(),
        Entry::new(Region::Data, "answered_by", Shape::Literal)
            .under("no_criterion")
            .optional(),
        Entry::new(Region::Data, "gap", Shape::Literal)
            .under("open_gaps")
            .optional(),
        Entry::new(Region::Data, "state", Shape::Closed)
            .under("open_gaps")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub struct Answers;

impl Specialization for Answers {
    fn profile(&self) -> &'static Profile {
        &ANSWERS_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![counted(items(value, "gaps").count(), "gap")]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "gaps")
            .map(|gap| Line::Node {
                depth: 0,
                id: text(gap, "id"),
                facts: gap_facts(gap),
            })
            .collect()
    }
}

fn gap_facts(gap: &Value) -> Vec<Fact> {
    let mut facts = vec![Fact::same("state", text(gap, "state"))];
    facts.extend(optional_fact(
        gap,
        "answered_by",
        "answered by",
        |premise| format!("by {premise}"),
    ));
    facts.extend(optional_fact(gap, "answerer", "answerer", |answerer| {
        format!("{answerer} answer")
    }));
    if let Some(no_criterion) = gap["no_criterion"].as_bool() {
        facts.push(Fact {
            label: "criterion",
            agent: match no_criterion {
                true => "no criterion".to_owned(),
                false => "criterion found".to_owned(),
            },
            human: match no_criterion {
                true => NONE.to_owned(),
                false => "found".to_owned(),
            },
        });
    }
    facts.extend(optional_fact(gap, "work_item", "work item", |work_item| {
        format!("of {work_item}")
    }));

    facts
}

/// Fato presente só quando `key` é texto; o agente vê `agent(valor)`, o humano o valor.
fn optional_fact(
    node: &Value,
    key: &str,
    label: &'static str,
    agent: impl FnOnce(&str) -> String,
) -> Option<Fact> {
    opt_text(node, key).map(|human| Fact {
        label,
        agent: agent(&human),
        human,
    })
}

pub struct Recurrences;

impl Specialization for Recurrences {
    fn profile(&self) -> &'static Profile {
        &RECURRENCES_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![
            format!("min {}", value["min"]),
            counted(items(value, "recurrences").count(), "group"),
        ]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "recurrences")
            .flat_map(|group| {
                [
                    dash(vec![text(group, "kind"), text(group, "text")]),
                    field(1, "work items", joined(group, "work_items")),
                    field(1, "premises", joined(group, "premises")),
                ]
            })
            .collect()
    }
}

pub struct Retro;

impl Specialization for Retro {
    fn profile(&self) -> &'static Profile {
        &RETRO_PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let retro = &value["retro"];
        let divergences: usize = items(retro, "divergences")
            .map(|group| items(group, "ids").count())
            .sum();
        let parts: Vec<String> = [
            (divergences, "divergence"),
            (
                items(&retro["amendments"], "approved").count(),
                "approved amendment",
            ),
            (
                items(&retro["amendments"], "pending").count(),
                "pending amendment",
            ),
            (items(retro, "no_criterion").count(), "no-criterion gap"),
            (items(retro, "open_gaps").count(), "open gap"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, noun)| counted(count, noun))
        .collect();

        match parts.is_empty() {
            true => vec![NONE.to_owned()],
            false => parts,
        }
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let retro = &value["retro"];
        let divergences = items(retro, "divergences").map(|group| {
            dash(vec![
                "divergence".to_owned(),
                text(group, "kind"),
                text(group, "resolution"),
                joined(group, "ids"),
            ])
        });
        let approved = items(&retro["amendments"], "approved").map(|amendment| {
            dash(vec![
                "amendment".to_owned(),
                text(amendment, "name"),
                text(amendment, "divergence"),
            ])
        });
        let pending = items(&retro["amendments"], "pending")
            .filter_map(Value::as_str)
            .map(|name| dash(vec!["pending amendment".to_owned(), one_line(name)]));
        let no_criterion = items(retro, "no_criterion").map(|answer| {
            dash(vec![
                "no criterion".to_owned(),
                text(answer, "gap"),
                format!("answered by {}", text(answer, "answered_by")),
            ])
        });
        let open_gaps = items(retro, "open_gaps").map(|gap| {
            dash(vec![
                "open gap".to_owned(),
                text(gap, "gap"),
                text(gap, "state"),
            ])
        });

        divergences
            .chain(approved)
            .chain(pending)
            .chain(no_criterion)
            .chain(open_gaps)
            .collect()
    }

    fn closing(&self, value: &Value) -> Closing {
        match value["retro"].is_null() {
            true => Closing::None,
            false => Closing::Next(
                "propose one improvement per item to the owner and change nothing without the answer"
                    .to_owned(),
            ),
        }
    }
}
