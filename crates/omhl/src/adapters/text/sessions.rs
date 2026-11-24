use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Fact, Line, items, joined, text};
use super::error;
use super::vocab::{NONE, counted, plural};

pub static PROFILE: Profile = Profile {
    label: "report sessions",
    nodes: None,
    success: &[
        Entry::new(Region::Data, "id", Shape::Literal)
            .under("sessions")
            .optional(),
        Entry::new(Region::Data, "first", Shape::Literal)
            .under("sessions")
            .optional(),
        Entry::new(Region::Data, "last", Shape::Literal)
            .under("sessions")
            .optional(),
        Entry::new(Region::Data, "nodes", Shape::Derived)
            .under("sessions")
            .optional(),
        Entry::new(Region::Data, "edges", Shape::Derived)
            .under("sessions")
            .optional(),
        Entry::new(Region::Data, "work_items", Shape::Literal)
            .under("sessions")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub struct Sessions;

impl Specialization for Sessions {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![counted(items(value, "sessions").count(), "session")]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "sessions")
            .map(|session| Line::Node {
                depth: 0,
                id: text(session, "id"),
                facts: session_facts(session),
            })
            .collect()
    }
}

fn session_facts(session: &Value) -> Vec<Fact> {
    let work_items = joined(session, "work_items");
    let (agent_work_items, human_work_items) = match work_items.is_empty() {
        true => ("no work items".to_owned(), NONE.to_owned()),
        false => (format!("work items {work_items}"), work_items),
    };

    vec![
        at_fact(session, "first"),
        at_fact(session, "last"),
        count_fact(session, "nodes", "node"),
        count_fact(session, "edges", "edge"),
        Fact {
            label: "work items",
            agent: agent_work_items,
            human: human_work_items,
        },
    ]
}

fn at_fact(session: &Value, key: &'static str) -> Fact {
    let at = text(session, key);

    Fact {
        label: key,
        agent: format!("{key} {at}"),
        human: at,
    }
}

fn count_fact(session: &Value, key: &'static str, noun: &str) -> Fact {
    let count = session[key].as_u64().unwrap_or_default();

    Fact {
        label: key,
        agent: plural(usize::try_from(count).unwrap_or_default(), noun),
        human: count.to_string(),
    }
}
