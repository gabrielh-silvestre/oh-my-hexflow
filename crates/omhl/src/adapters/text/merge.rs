use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::doc::{Closing, Join, Line, Marker, field, has_items, items, joined, opt_text, text};
use super::error::{self, ErrorFacts};
use super::vocab::{HASH_PREFIX_LEN, State};

pub static PROFILE: Profile = Profile {
    label: "merge",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "work_item", Shape::Literal),
        Entry::new(Region::Root, "next", Shape::Closed),
        Entry::new(Region::Root, "warnings", Shape::Closed).optional(),
        Entry::new(Region::Data, "id", Shape::Literal).under("pull_request"),
        Entry::new(Region::Data, "number", Shape::Literal).under("pull_request"),
        Entry::new(Region::Data, "repo", Shape::Literal).under("pull_request"),
        Entry::new(Region::Data, "strategy", Shape::Closed).under("pull_request"),
        Entry::new(Region::Data, "merge_commit", Shape::Hash(HASH_PREFIX_LEN))
            .under("pull_request")
            .optional(),
        Entry::new(Region::Data, "step", Shape::Closed).under("steps"),
        Entry::new(Region::Data, "result", Shape::Closed).under("steps"),
        Entry::new(Region::Data, "expected", Shape::Literal)
            .under("steps")
            .optional(),
        Entry::new(Region::Data, "actual", Shape::Literal)
            .under("steps")
            .optional(),
        Entry::new(Region::Data, "forced", Shape::Literal)
            .under("steps")
            .optional(),
        Entry::new(Region::Data, "overridden", Shape::Literal)
            .under("steps")
            .optional(),
    ],
    failure: &FAILURE,
};

static FAILURE: [Entry; 32] = [
    Entry::new(Region::Root, "code", Shape::Closed),
    Entry::new(Region::Root, "message", Shape::Closed),
    Entry::new(Region::Data, "path", Shape::Closed)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "code", Shape::Closed)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "message", Shape::Closed)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "next_step", Shape::Closed)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "current", Shape::Literal)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "retired", Shape::Derived)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "kind", Shape::Derived)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "event", Shape::Closed)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "branch", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "work_item", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "os_error", Shape::Closed)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "label", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "check", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "next", Shape::Closed)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "state", Shape::Closed)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "pr_base", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "expected_base", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "parent", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "fork_pull_requests", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "pull_requests", Shape::Literal)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "hint", Shape::Closed)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "step", Shape::Closed)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "result", Shape::Closed)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "expected", Shape::Literal)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "actual", Shape::Literal)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "forced", Shape::Literal)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "overridden", Shape::Literal)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "red", Shape::Literal)
        .under("steps")
        .optional(),
    Entry::new(Region::Data, "base", Shape::Literal)
        .under("review_base")
        .optional(),
    Entry::new(Region::Data, "merge_base", Shape::Hash(HASH_PREFIX_LEN))
        .under("review_base")
        .optional(),
];

pub struct Merge;

impl Specialization for Merge {
    fn profile(&self) -> &'static Profile {
        &PROFILE
    }

    fn state(&self, value: &Value) -> State {
        match has_items(value, "warnings") {
            true => State::Warning,
            false => State::Ok,
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        let pull_request = &value["pull_request"];

        vec![
            text(value, "work_item"),
            text(pull_request, "id"),
            text(pull_request, "strategy"),
        ]
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let pull_request = &value["pull_request"];
        let mut lines = vec![field(
            1,
            "pull_request",
            format!(
                "#{} · {}",
                pull_request["number"],
                text(pull_request, "repo")
            ),
        )];
        if let Some(commit) = opt_text(pull_request, "merge_commit") {
            let short: String = commit.chars().take(HASH_PREFIX_LEN).collect();
            lines.push(field(1, "merge_commit", short));
        }
        for step in items(value, "steps") {
            let mut parts = vec![text(step, "step"), text(step, "result")];
            parts.extend(opt_text(step, "expected").map(|base| format!("expected {base}")));
            parts.extend(opt_text(step, "actual").map(|base| format!("actual {base}")));
            lines.push(Line::Item {
                depth: 1,
                marker: Marker::Dash,
                parts,
                join: Join::Dot,
            });
            for key in ["forced", "overridden"] {
                if has_items(step, key) {
                    lines.push(field(2, key, joined(step, key)));
                }
            }
        }
        lines.extend(
            items(value, "warnings")
                .filter_map(Value::as_str)
                .map(|warning| field(1, "warning", warning.to_owned())),
        );

        lines
    }

    /// O `next` do contexto já sai na linha `Next:`.
    fn failure_extras(&self, error: &Value, facts: &ErrorFacts) -> Vec<Line> {
        error::context_lines(&error["context"], 1, &facts.code, facts)
            .into_iter()
            .filter(|line| !matches!(line, Line::Field { label, .. } if label == "next"))
            .collect()
    }

    fn closing(&self, value: &Value) -> Closing {
        match opt_text(value, "next") {
            Some(next) => Closing::Next(next),
            None => Closing::None,
        }
    }
}
