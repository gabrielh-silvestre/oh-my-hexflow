//! Erro genérico dos comandos de base: lê só o `{"error": {...}}` de `AppError::to_json` e mostra todo escalar.

use serde_json::Value;

use super::base::{Entry, Region, Shape, Specialization};
use super::doc::{Closing, Doc, Join, Line, Marker, items, opt_text, scalar_fields, text};
use super::vocab::{State, without_final_period};

/// Tabela de erro compartilhada pelos comandos de base: ruído vazio, tudo opcional exceto `code` e `message` da raiz
/// (cada comando monta contextos diferentes e o `no-marker` tem `details: []`).
pub static ENTRIES: [Entry; 14] = [
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
    Entry::new(Region::Data, "current", Shape::Literal)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "retired", Shape::Derived)
        .under("details")
        .optional(),
    Entry::new(Region::Data, "next_step", Shape::Closed)
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
    Entry::new(Region::Data, "hint", Shape::Closed)
        .under("context")
        .optional(),
    Entry::new(Region::Data, "os_error", Shape::Closed)
        .under("context")
        .optional(),
];

/// Os fatos do envelope que o detalhe e o fecho precisam.
pub struct ErrorFacts {
    pub code: String,
    pub message: String,
    /// A ação do erro: o primeiro `next_step` dos detalhes ou, sem ele, o primeiro `hint` do contexto.
    pub action: Option<String>,
}

impl ErrorFacts {
    fn of(error: &Value) -> ErrorFacts {
        let hint = |context: &Value| opt_text(context, "hint");
        let action = items(error, "details")
            .find_map(|detail| opt_text(detail, "next_step"))
            .or_else(|| hint(&error["context"]))
            .or_else(|| items(error, "details").find_map(|detail| hint(&detail["context"])));

        ErrorFacts {
            code: text(error, "code"),
            message: text(error, "message"),
            action,
        }
    }
}

/// `value` é o `{"error": {...}}`: cabeçalho, detalhes, extras e fecho.
pub fn failure<S: Specialization>(specialization: &S, value: &Value) -> Doc {
    let error = &value["error"];
    let facts = ErrorFacts::of(error);

    let mut lines = vec![specialization.failure_header(error)];
    lines.extend(
        items(error, "details")
            .flat_map(|detail| specialization.failure_detail_lines(detail, &facts)),
    );
    lines.extend(specialization.failure_extras(error, &facts));

    Doc {
        lines,
        closing: specialization.failure_closing(&facts),
    }
}

pub fn header(error: &Value) -> Line {
    Line::Header {
        state: State::Error,
        parts: vec![text(error, "code"), text(error, "message")],
    }
}

/// O que o detalhe repete do envelope (mesmo `code`, mesma `message`) fica só no cabeçalho.
pub fn detail_lines(detail: &Value, facts: &ErrorFacts) -> Vec<Line> {
    let code = text(detail, "code");
    let message = text(detail, "message");
    let mut parts: Vec<String> = [
        text(detail, "path"),
        if code == facts.code {
            String::new()
        } else {
            code.clone()
        },
        if message == facts.message {
            String::new()
        } else {
            message
        },
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect();
    if parts.is_empty() {
        parts.push(code.clone());
    }

    let mut lines = vec![Line::Item {
        depth: 1,
        marker: Marker::Dash,
        parts,
        join: Join::Dot,
    }];
    let own = [
        opt_text(detail, "current").map(|current| ("current".to_owned(), current)),
        detail["retired"]
            .as_bool()
            .map(|retired| ("retired".to_owned(), retired.to_string())),
        opt_text(detail, "next_step").map(|next_step| ("next_step".to_owned(), next_step)),
    ]
    .into_iter()
    .flatten();
    lines.extend(
        own.filter(|(label, value)| !is_action(label, value, facts))
            .map(|(label, value)| Line::Field {
                depth: 2,
                label,
                value,
            }),
    );
    lines.extend(context_lines(&detail["context"], 2, &code, facts));

    lines
}

/// Todo escalar do contexto como campo; `kind` só aparece quando difere do `code`, e a ação vai para o fecho.
/// `own_code` é o `code` do dono do contexto (o erro ou o detalhe).
pub fn context_lines(
    context: &Value,
    depth: usize,
    own_code: &str,
    facts: &ErrorFacts,
) -> Vec<Line> {
    scalar_fields("", context)
        .into_iter()
        .filter(|(label, value)| {
            !is_action(label, value, facts)
                && !(label == "kind" && (value == own_code || value == &facts.code))
        })
        .map(|(label, value)| Line::Field {
            depth,
            label,
            value,
        })
        .collect()
}

pub fn closing(facts: &ErrorFacts) -> Closing {
    match &facts.action {
        Some(action) => Closing::Next(without_final_period(action)),
        None => Closing::None,
    }
}

fn is_action(label: &str, value: &str, facts: &ErrorFacts) -> bool {
    matches!(label, "hint" | "next_step") && facts.action.as_deref() == Some(value)
}
