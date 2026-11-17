use std::collections::HashMap;

use serde_json::{Map, Value};

pub(super) const CLAUDE: &str = "claude";
pub(super) const USER: &str = "user";
const ANALYST: &str = "omhl:analyst";
const VERIFIER: &str = "omhl:verifier";

/// O que um papel portado pode gravar; as condições finas (ex.: `kind` do gap) ficam nas funções abaixo.
struct Matrix {
    role: &'static str,
    nodes: &'static [&'static str],
    relations: &'static [&'static str],
    retire: &'static [&'static str],
}

/// Os sete papéis que o `OMHL_ROLE` aceita; lista fechada, o valor é gravado como `agent` do record.
const MATRICES: [Matrix; 7] = [
    Matrix {
        role: ANALYST,
        nodes: &["premise", "gap"],
        relations: &[],
        retire: &[],
    },
    Matrix {
        role: "omhl:planner",
        nodes: &[],
        relations: &[],
        retire: &[],
    },
    Matrix {
        role: "omhl:architect",
        nodes: &["decision"],
        relations: &["rests-on"],
        retire: &[],
    },
    Matrix {
        role: "omhl:critic",
        nodes: &["decision", "premise"],
        relations: &["supports", "contradicts", "answers", "rests-on"],
        retire: &[],
    },
    Matrix {
        role: "omhl:code-reviewer",
        nodes: &["divergence", "decision"],
        relations: &["contradicts", "rests-on"],
        retire: &[],
    },
    Matrix {
        role: "omhl:security-reviewer",
        nodes: &["divergence", "decision"],
        relations: &["contradicts", "rests-on"],
        retire: &[],
    },
    Matrix {
        role: VERIFIER,
        nodes: &["divergence"],
        relations: &[],
        retire: &["contradicts"],
    },
];

/// Leitura vale para os sete papéis; vem depois dos comandos próprios de cada um.
macro_rules! commands {
    ($($own:literal),*) => {
        &[
            $($own,)*
            "list", "get", "render", "verify", "byproduct",
            "report answers", "report recurrences", "report retro", "help",
        ]
    };
}

/// `commands` de referência por papel, só para os testes (paridade com o `omhl-allow` dos agentes).
#[doc(hidden)]
pub const ROLE_REFERENCE_COMMANDS: [(&str, &[&str]); 7] = [
    (ANALYST, commands!["append"]),
    ("omhl:planner", commands!["spec init", "spec validate"]),
    ("omhl:architect", commands!["append"]),
    ("omhl:critic", commands!["append", "spec validate"]),
    ("omhl:code-reviewer", commands!["append"]),
    ("omhl:security-reviewer", commands!["append"]),
    (VERIFIER, commands!["append", "spec validate"]),
];

/// Devolve o papel estático igual ao valor, ou `None` se estiver fora da lista.
pub(crate) fn known_role(value: &str) -> Option<&'static str> {
    MATRICES
        .iter()
        .map(|matrix| matrix.role)
        .find(|role| *role == value)
}

fn matrix_of(role: &str) -> Option<&'static Matrix> {
    MATRICES.iter().find(|matrix| matrix.role == role)
}

/// Tipo de node na matriz do papel e conteúdo dentro das condições finas.
pub(super) fn node_allowed(role: &str, type_name: &str, data: &Value) -> bool {
    let listed = matrix_of(role).is_some_and(|matrix| matrix.nodes.contains(&type_name));
    let content_ok = match (role, type_name) {
        (ANALYST, "premise") => data.get("status").and_then(Value::as_str) == Some("assumed"),
        (ANALYST, "gap") => data.get("kind").is_some(),
        _ => true,
    };

    listed && content_ok
}

pub(super) fn relation_allowed(role: &str, relation_type: &str) -> bool {
    matrix_of(role).is_some_and(|matrix| matrix.relations.contains(&relation_type))
}

pub(super) fn retirement_allowed(role: &str, relation_type: &str) -> bool {
    matrix_of(role).is_some_and(|matrix| matrix.retire.contains(&relation_type))
}

/// Fatos de uma revisão feita por um papel portado.
pub(super) struct Revision<'a> {
    pub(super) role: &'a str,
    pub(super) type_name: &'a str,
    pub(super) current_author: &'a str,
    pub(super) current: &'a Value,
    pub(super) revised: &'a Value,
}

/// Só o próprio papel revisa o que gravou; `kind` de premissa e gap não muda e divergência não vira `accepted`.
pub(super) fn revision_allowed(revision: &Revision<'_>) -> bool {
    if changes_kind(revision) || accepts_divergence(revision) {
        return false;
    }

    revision.current_author == revision.role || verifier_closes_divergence(revision)
}

fn changes_kind(revision: &Revision<'_>) -> bool {
    matches!(revision.type_name, "premise" | "gap")
        && revision.current.get("kind") != revision.revised.get("kind")
}

fn resolution_of(data: &Value) -> Option<&str> {
    data.get("resolution").and_then(Value::as_str)
}

fn accepts_divergence(revision: &Revision<'_>) -> bool {
    revision.type_name == "divergence" && resolution_of(revision.revised) == Some("accepted")
}

/// O verifier confere a correção: `open` vira `code-fixed` e a evidência só cresce; nada mais muda.
fn verifier_closes_divergence(revision: &Revision<'_>) -> bool {
    let (Some(current), Some(revised)) =
        (revision.current.as_object(), revision.revised.as_object())
    else {
        return false;
    };

    revision.role == VERIFIER
        && revision.type_name == "divergence"
        && resolution_of(revision.current) == Some("open")
        && resolution_of(revision.revised) == Some("code-fixed")
        && evidence_of(revised).starts_with(evidence_of(current))
        && other_fields(current) == other_fields(revised)
}

fn evidence_of(data: &Map<String, Value>) -> &[Value] {
    data.get("evidence")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Tudo menos `resolution` e `evidence`, os dois campos que o verifier pode mexer.
fn other_fields(data: &Map<String, Value>) -> HashMap<&String, &Value> {
    data.iter()
        .filter(|(key, _)| !matches!(key.as_str(), "resolution" | "evidence"))
        .collect()
}

/// Quem retira um `contradicts`: o verifier e o `user`, e nunca o `agent` que o gravou; `claude` não retira.
/// Outros autores (loader, hooks, testes) seguem como antes.
pub(super) fn contradicts_retirement_allowed(agent: &str, relation_author: Option<&str>) -> bool {
    match agent {
        CLAUDE => false,
        VERIFIER | USER => relation_author != Some(agent),
        _ => true,
    }
}
