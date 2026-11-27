use serde_json::Value;

use super::base::{Entry, Profile, Region, Shape, Specialization};
use super::check_run::warning_line;
use super::doc::{
    Closing, Join, Line, Marker, field, has_items, items, joined, one_line, opt_text, text,
};
use super::error;
use super::report::Retro;
use super::simple::Append;
use super::vocab::{State, plural};

pub static GATE_DELIVERY_PROFILE: Profile = Profile {
    label: "gate-delivery",
    nodes: None,
    success: &[
        Entry::new(Region::Root, "gate", Shape::Closed),
        Entry::new(Region::Root, "passed", Shape::Derived),
        Entry::new(Region::Data, "label", Shape::Closed).under("questions"),
        Entry::new(Region::Data, "kind", Shape::Closed).under("questions"),
        Entry::new(Region::Data, "passed", Shape::Derived).under("questions"),
        Entry::new(Region::Data, "matched", Shape::Derived).under("questions"),
        Entry::new(Region::Data, "failing", Shape::Derived)
            .under("questions")
            .optional(),
        Entry::new(Region::Data, "class", Shape::Closed)
            .under("questions")
            .optional(),
        Entry::new(Region::Data, "omitted", Shape::Derived).under("questions"),
        Entry::new(Region::Data, "next_step", Shape::Closed)
            .under("questions")
            .optional(),
        // O cabeçalho da cadeia só serve a quem verifica a cadeia (`omhl verify`).
        Entry::new(Region::Data, "seq", Shape::Noise).under("chain_head"),
        Entry::new(Region::Data, "hash", Shape::Noise).under("chain_head"),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "check", Shape::Literal)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "from", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "hint", Shape::Closed)
            .under("warnings")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

pub static CLOSE_PROFILE: Profile = Profile {
    label: "close",
    nodes: None,
    success: &[
        Entry::new(Region::Data, "gate", Shape::Closed).under("report"),
        Entry::new(Region::Data, "passed", Shape::Derived).under("report"),
        Entry::new(Region::Data, "label", Shape::Closed).under("questions"),
        Entry::new(Region::Data, "kind", Shape::Closed).under("questions"),
        Entry::new(Region::Data, "passed", Shape::Derived).under("questions"),
        Entry::new(Region::Data, "matched", Shape::Derived).under("questions"),
        Entry::new(Region::Data, "failing", Shape::Derived)
            .under("questions")
            .optional(),
        Entry::new(Region::Data, "class", Shape::Closed)
            .under("questions")
            .optional(),
        Entry::new(Region::Data, "omitted", Shape::Derived).under("questions"),
        Entry::new(Region::Data, "next_step", Shape::Closed)
            .under("questions")
            .optional(),
        Entry::new(Region::Data, "seq", Shape::Noise).under("chain_head"),
        Entry::new(Region::Data, "hash", Shape::Noise).under("chain_head"),
        Entry::new(Region::Data, "note", Shape::Literal)
            .under("gate_override")
            .optional(),
        Entry::new(Region::Data, "overridden", Shape::Literal)
            .under("gate_override")
            .optional(),
        Entry::new(Region::Data, "overridden", Shape::Literal)
            .under("forced")
            .optional(),
        Entry::new(Region::Data, "note", Shape::Literal)
            .under("forced")
            .optional(),
        Entry::new(Region::Root, "note_discarded", Shape::Derived).optional(),
        Entry::new(Region::Data, "id", Shape::Literal).under("records"),
        Entry::new(Region::Data, "seq", Shape::Literal).under("records"),
        Entry::new(Region::Data, "replayed", Shape::Derived).under("outcome"),
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
        Entry::new(Region::Data, "note", Shape::Literal)
            .under("delivery_failure")
            .optional(),
        Entry::new(Region::Data, "label", Shape::Literal)
            .under("failed")
            .optional(),
        Entry::new(Region::Data, "class", Shape::Closed)
            .under("failed")
            .optional(),
        Entry::new(Region::Data, "reason", Shape::Literal)
            .under("abandonment")
            .optional(),
        Entry::new(Region::Data, "note", Shape::Literal)
            .under("abandonment")
            .optional(),
        Entry::new(Region::Data, "gap", Shape::Literal)
            .under("open_gaps")
            .optional(),
        Entry::new(Region::Data, "state", Shape::Closed)
            .under("open_gaps")
            .optional(),
        Entry::new(Region::Data, "code", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "check", Shape::Literal)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "from", Shape::Closed)
            .under("warnings")
            .optional(),
        Entry::new(Region::Data, "hint", Shape::Closed)
            .under("warnings")
            .optional(),
    ],
    failure: &error::ENTRIES,
};

fn gate_passed(report: &Value) -> bool {
    report["passed"] == true
}

fn failed_questions(report: &Value) -> impl Iterator<Item = &Value> {
    items(report, "questions").filter(|question| question["passed"] != true)
}

fn first_next_step(report: &Value) -> Option<String> {
    failed_questions(report).find_map(|question| opt_text(question, "next_step"))
}

fn gate_summary(report: &Value) -> Vec<String> {
    let failed = failed_questions(report).count();
    let mut parts = vec![
        text(report, "gate"),
        match gate_passed(report) {
            true => "passed".to_owned(),
            false => "not passed".to_owned(),
        },
        plural(items(report, "questions").count(), "question"),
    ];
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }

    parts
}

fn question_lines(report: &Value, action: Option<&str>) -> Vec<Line> {
    items(report, "questions")
        .flat_map(|question| {
            let state = match question["passed"] == true {
                true => State::Ok,
                false => State::Error,
            };
            let label = text(question, "label");
            let kind = text(question, "kind");
            let mut parts = vec![label.clone()];
            if question["passed"] != true && question["class"].is_string() {
                parts.push(text(question, "class"));
            }
            // Nas perguntas sem check o `kind` repete o `label`.
            if kind != label {
                parts.push(kind);
            }
            parts.push(format!("matched {}", question["matched"]));
            if has_items(question, "failing") {
                parts.push(format!("failing {}", joined(question, "failing")));
            }
            if question["omitted"].as_u64().is_some_and(|count| count > 0) {
                parts.push(format!("{} omitted", question["omitted"]));
            }
            let item = Line::Item {
                depth: 0,
                marker: Marker::Symbol(state),
                parts,
                join: Join::Dot,
            };
            let next_step = opt_text(question, "next_step")
                .filter(|step| action != Some(step.as_str()))
                .map(|step| field(1, "next_step", step));

            std::iter::once(item).chain(next_step)
        })
        .collect()
}

fn review_base_line(value: &Value) -> Option<Line> {
    let review = &value["review_base"];
    if review.is_null() {
        return None;
    }
    let text = match review["base"].as_str() {
        Some(base) => format!(
            "base {}, merge-base {}, triggering paths {}",
            one_line(base),
            one_line(review["merge_base"].as_str().unwrap_or_default()),
            joined(review, "paths"),
        ),
        None => "no base".to_owned(),
    };

    let text = match review["warning"].as_str() {
        Some(warning) => format!("{text}; warning: {warning}"),
        None => text,
    };

    Some(field(0, "review_base", text))
}

fn warning_lines(value: &Value) -> impl Iterator<Item = Line> {
    items(value, "warnings").map(|warning| warning_line(warning, ""))
}

pub struct GateDelivery;

impl Specialization for GateDelivery {
    fn profile(&self) -> &'static Profile {
        &GATE_DELIVERY_PROFILE
    }

    fn state(&self, value: &Value) -> State {
        match gate_passed(value) {
            true => State::Ok,
            false => State::Error,
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        gate_summary(value)
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        let mut lines = question_lines(value, first_next_step(value).as_deref());
        lines.extend(review_base_line(value));
        lines.extend(warning_lines(value));
        lines
    }

    fn closing(&self, value: &Value) -> Closing {
        match first_next_step(value) {
            Some(step) => Closing::Next(step),
            None => Closing::None,
        }
    }
}

/// Um fechamento forçado ou entregue com falha sai com aviso: o dono precisa ver o que foi contornado.
fn closed_header(value: &Value) -> Line {
    let failed = &value["retro"]["delivery_failure"];
    if failed.is_object() {
        return Line::Header {
            state: State::Warning,
            parts: vec![
                "closed".to_owned(),
                "delivered with failure".to_owned(),
                format!("{} failed", items(failed, "failed").count()),
            ],
        };
    }

    if value["forced"].is_object() {
        return Line::Header {
            state: State::Warning,
            parts: vec![
                "closed".to_owned(),
                "forced".to_owned(),
                format!(
                    "{} overridden",
                    items(&value["forced"], "overridden").count()
                ),
            ],
        };
    }

    Line::Header {
        state: State::Ok,
        parts: std::iter::once("closed".to_owned())
            .chain(Append.summary(&value["outcome"]))
            .collect(),
    }
}

/// O abandono não avalia o gate: só a retro dele vai ao terminal.
fn is_abandonment(value: &Value) -> bool {
    value["abandoned"].is_object()
}

pub struct Close;

impl Specialization for Close {
    fn profile(&self) -> &'static Profile {
        &CLOSE_PROFILE
    }

    /// O work item fecha mesmo com o gate reprovado: o resultado fica gravado.
    fn state(&self, value: &Value) -> State {
        if is_abandonment(value) {
            return State::Ok;
        }
        match gate_passed(&value["report"]) {
            true => State::Ok,
            false => State::Warning,
        }
    }

    fn summary(&self, value: &Value) -> Vec<String> {
        if is_abandonment(value) {
            return std::iter::once("retro".to_owned())
                .chain(Retro.summary(value))
                .collect();
        }
        gate_summary(&value["report"])
    }

    fn detail_lines(&self, value: &Value) -> Vec<Line> {
        if is_abandonment(value) {
            return Retro.detail_lines(value);
        }
        let outcome = &value["outcome"];
        let mut lines = question_lines(&value["report"], None);
        lines.extend(review_base_line(value));
        lines.extend(warning_lines(value));
        lines.push(closed_header(value));
        if value["note_discarded"] == true {
            lines.push(Line::Header {
                state: State::Warning,
                parts: vec!["note discarded".to_owned(), "gate passed".to_owned()],
            });
        }
        lines.extend(Append.detail_lines(outcome));
        if !value["retro"].is_null() {
            lines.push(Line::Header {
                state: State::Ok,
                parts: std::iter::once("retro".to_owned())
                    .chain(Retro.summary(value))
                    .collect(),
            });
            lines.extend(Retro.detail_lines(value));
        }

        lines
    }

    fn closing(&self, value: &Value) -> Closing {
        match Retro.detail_lines(value).is_empty() {
            true => Closing::None,
            false => Retro.closing(value),
        }
    }
}

#[cfg(test)]
mod tests;
