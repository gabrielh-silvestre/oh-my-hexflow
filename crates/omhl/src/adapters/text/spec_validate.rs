//! Texto do `spec validate`: as partes, as emendas com o estado e as violações de cada uma.
//! A reprovação das partes e das emendas aprovadas sai como erro pelo genérico.

use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Join, Line, Marker, dash, has_items, items, one_line, text};
use super::error;
use super::vocab::{State, plural};

pub static PROFILE: Profile = Profile {
    label: "spec validate",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "ok", Shape::Derived),
        Entry::new(Region::Root, "parts", Shape::Closed),
        Entry::new(Region::Data, "name", Shape::Closed)
            .under("amendments")
            .optional(),
        Entry::new(Region::Data, "state", Shape::Closed)
            .under("amendments")
            .optional(),
        Entry::new(Region::Data, "path", Shape::Closed)
            .under("violations")
            .optional(),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("violations")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub struct SpecValidate;

impl Specialization for SpecValidate {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    /// Violação de emenda pendente não reprova, mas fica à vista.
    fn state(&self, value: &Value) -> State {
        let has_violation =
            items(value, "amendments").any(|amendment| has_items(amendment, "violations"));
        match has_violation {
            true => State::Warning,
            false => State::Ok,
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        vec![
            plural(items(value, "parts").count(), "part"),
            plural(items(value, "amendments").count(), "amendment"),
        ]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let parts = items(value, "parts")
            .filter_map(Value::as_str)
            .map(|name| dash(vec!["part".to_owned(), one_line(name)]));
        let amendments = items(value, "amendments").flat_map(|amendment| {
            let violations = items(amendment, "violations").map(|violation| Line::Item {
                depth: 1,
                marker: Marker::Dash,
                parts: vec![text(violation, "path"), text(violation, "code")],
                join: Join::Dot,
            });
            let line = dash(vec![
                "amendment".to_owned(),
                text(amendment, "name"),
                text(amendment, "state"),
            ]);

            std::iter::once(line).chain(violations)
        });

        parts.chain(amendments).collect()
    }
}
