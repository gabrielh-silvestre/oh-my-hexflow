//! Texto do `omhl verify`, o primeiro comando de base: a cadeia fecha ou quebra, uma linha por quebra.

use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Join, Line, Marker, items, text};
use super::error;

pub static PROFILE: Profile = Profile {
    label: "verify",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "ok", Shape::Derived),
        Entry::new(Region::Root, "total_records", Shape::Derived),
        Entry::new(Region::Root, "total_breaks", Shape::Derived),
        Entry::new(Region::Root, "chain_head", Shape::Noise),
        Entry::new(Region::Data, "reason", Shape::Closed)
            .under("breaks")
            .optional(),
        Entry::new(Region::Data, "seq", Shape::Literal)
            .under("breaks")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub struct Verify;

fn chain_ok(value: &Value) -> bool {
    value["ok"] == true
}

impl Specialization for Verify {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn symbol(&self, value: &Value) -> &'static str {
        match chain_ok(value) {
            true => "✔",
            false => "✘",
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let records = format!("{} records", value["total_records"]);
        match chain_ok(value) {
            true => vec!["chain ok".to_owned(), records],
            false => vec![
                "chain broken".to_owned(),
                records,
                format!("{} breaks", value["total_breaks"]),
            ],
        }
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        items(value, "breaks")
            .map(|found| {
                let mut parts = vec![text(found, "reason")];
                if let Some(seq) = found["seq"].as_u64() {
                    parts.push(format!("seq {seq}"));
                }

                Line::Item {
                    depth: 1,
                    marker: Marker::Dash,
                    parts,
                    join: Join::Dot,
                }
            })
            .collect()
    }

    fn closing(&self, value: &Value) -> Closing {
        match chain_ok(value) {
            true => Closing::None,
            false => Closing::Next(
                "write nothing to the database and trust no attestation; tell the owner the reason and seq of the first break"
                    .to_owned(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_declare_the_boolean_ok_as_derived() {
        let shape = PROFILE
            .success
            .iter()
            .find(|entry| entry.key == "ok")
            .map(|entry| entry.shape);

        assert_eq!(shape, Some(Shape::Derived));
    }
}
