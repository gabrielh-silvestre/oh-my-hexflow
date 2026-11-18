//! Texto do `load`: contagens no cabeçalho e uma linha por item dos grupos de lista não vazios.

use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Line, dash, items, joined, text};
use super::error;
use super::vocab::{State, UNCHANGED, WARNING, plural};

pub static PROFILE: Profile = Profile {
    label: "",
    nodes: None,
    success: &[
        // O banco tem um só tenant por projeto: repeti-lo não ajuda a decidir nada.
        Entry::new(Region::Root, "tenant", Shape::Noise),
        Entry::new(Region::Root, "dry_run", Shape::Derived),
        Entry::new(Region::Root, "created", Shape::Literal).optional(),
        Entry::new(Region::Root, "revised", Shape::Literal).optional(),
        // Os ids que não mudaram não ajudam a decidir nada: o texto só os conta.
        Entry::new(Region::Root, "unchanged", Shape::Noise).optional(),
        Entry::new(Region::Root, "orphans", Shape::Literal).optional(),
        Entry::new(Region::Data, "id", Shape::Literal)
            .under("skipped")
            .optional(),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("skipped")
            .optional(),
        Entry::new(Region::Data, "from", Shape::Literal)
            .under("relations_created")
            .optional(),
        Entry::new(Region::Data, "to", Shape::Literal)
            .under("relations_created")
            .optional(),
        Entry::new(Region::Data, "from", Shape::Literal)
            .under("orphan_relations")
            .optional(),
        Entry::new(Region::Data, "to", Shape::Literal)
            .under("orphan_relations")
            .optional(),
        Entry::new(Region::Data, "path", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "message", Shape::Closed)
            .under("warnings")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

fn relation_line(label: &str, relation: &Value) -> Line {
    dash(vec![
        label.to_owned(),
        format!("{} after {}", text(relation, "from"), text(relation, "to")),
    ])
}

pub struct Load;

impl Specialization for Load {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn state(&self, value: &Value) -> State {
        match items(value, "warnings").next() {
            Some(_) => State::Warning,
            None => State::Ok,
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let count = |key: &str| items(value, key).count();
        let mode = match value["dry_run"] == true {
            true => "dry run",
            false => "loaded",
        };
        let mut parts = vec![
            mode.to_owned(),
            format!("{} created", count("created")),
            format!("{} revised", count("revised")),
            format!("{} {UNCHANGED}", count("unchanged")),
        ];
        if count("skipped") > 0 {
            parts.push(format!("{} skipped", count("skipped")));
        }
        let nouns = [
            ("relations_created", "relation"),
            ("orphans", "orphan"),
            ("orphan_relations", "orphan relation"),
            ("warnings", WARNING),
        ];
        parts.extend(
            nouns
                .into_iter()
                .filter(|(key, _)| count(key) > 0)
                .map(|(key, noun)| plural(count(key), noun)),
        );

        parts
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let id_groups = [
            ("created", "created"),
            ("revised", "revised"),
            ("orphans", "orphan"),
        ]
        .into_iter()
        .filter(|(key, _)| items(value, key).next().is_some())
        .map(|(key, label)| dash(vec![label.to_owned(), joined(value, key)]));
        let skipped = items(value, "skipped").map(|entry| {
            dash(vec![
                "skipped".to_owned(),
                text(entry, "id"),
                text(entry, "code"),
            ])
        });
        let relations =
            items(value, "relations_created").map(|relation| relation_line("relation", relation));
        let orphan_relations = items(value, "orphan_relations")
            .map(|relation| relation_line("orphan relation", relation));
        let warnings = items(value, "warnings").map(|warning| {
            dash(vec![
                WARNING.to_owned(),
                text(warning, "path"),
                text(warning, "code"),
                text(warning, "message"),
            ])
        });

        id_groups
            .chain(skipped)
            .chain(relations)
            .chain(orphan_relations)
            .chain(warnings)
            .collect()
    }
}
