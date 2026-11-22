use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::workflow::gap::{CRITIC_CONFIRMATION, confirmer, is_confirmed_premise};

/// O papel gravado em `OMHL_ROLE` e no `agent` dos records é `omhl:<papel>`.
pub const ROLE_PREFIX: &str = "omhl:";
pub const ROLE_VARIABLE: &str = "OMHL_ROLE";
pub(super) const CLAUDE: &str = "claude";
pub(super) const USER: &str = "user";
pub(super) const CRITIC: &str = "omhl:critic";
const ANALYST: &str = "omhl:analyst";
const VERIFIER: &str = "omhl:verifier";

pub(super) struct Matrix {
    role: &'static str,
    nodes: &'static [&'static str],
    pub(super) relations: &'static [&'static str],
    pub(super) retire: &'static [&'static str],
}

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
        role: CRITIC,
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

macro_rules! commands {
    ($($own:literal),*) => {
        &[
            $($own,)*
            "list", "get", "render", "verify", "byproduct",
            "report answers", "report recurrences", "report retro", "help",
        ]
    };
}

/// Só para os testes (paridade com o `omhl-allow` dos agentes).
#[doc(hidden)]
pub const ROLE_REFERENCE_COMMANDS: [(&str, &[&str]); 7] = [
    (ANALYST, commands!["append"]),
    ("omhl:planner", commands!["spec init", "spec validate"]),
    ("omhl:architect", commands!["append"]),
    (CRITIC, commands!["append", "spec validate"]),
    ("omhl:code-reviewer", commands!["append"]),
    ("omhl:security-reviewer", commands!["append"]),
    (VERIFIER, commands!["append", "spec validate"]),
];

pub(crate) fn known_role(value: &str) -> Option<&'static str> {
    MATRICES
        .iter()
        .map(|matrix| matrix.role)
        .find(|role| *role == value)
}

fn matrix_of(role: &str) -> Option<&'static Matrix> {
    MATRICES.iter().find(|matrix| matrix.role == role)
}

type RoleMatrixRow = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

/// Lida de `MATRICES`, a mesma fonte da guarda; só para os testes (paridade com a doc).
#[doc(hidden)]
pub const ROLE_REFERENCE_MATRIX: [RoleMatrixRow; 7] = {
    let mut table: [RoleMatrixRow; 7] = [("", &[], &[], &[]); 7];
    let mut index = 0;
    while index < MATRICES.len() {
        let matrix = &MATRICES[index];
        table[index] = (matrix.role, matrix.nodes, matrix.relations, matrix.retire);
        index += 1;
    }

    table
};

pub(super) fn matrix_lists(
    role: &str,
    pick: fn(&Matrix) -> &'static [&'static str],
    name: &str,
) -> bool {
    matrix_of(role).is_some_and(|matrix| pick(matrix).contains(&name))
}

pub(super) fn node_allowed(role: &str, type_name: &str, data: &Value) -> bool {
    let listed = matrix_lists(role, |matrix| matrix.nodes, type_name);
    let content_ok = match (role, type_name) {
        (ANALYST, "premise") => data.get("status").and_then(Value::as_str) == Some("assumed"),
        (ANALYST, "gap") => data.get("kind").is_some(),
        // O `kind: technical` fica com a guarda da confirmação, que responde `confirmation-invalid`.
        (CRITIC, "premise") => confirmed_by_critic(data),
        _ => true,
    };

    listed && content_ok
}

pub(super) fn creation_allowed(type_name: &str, data: &Value) -> bool {
    type_name != "divergence" || resolution_of(data) == Some("open")
}

fn confirmed_by_critic(data: &Value) -> bool {
    is_confirmed_premise(data) && confirmer(data) == Some(CRITIC_CONFIRMATION)
}

pub(super) struct Revision<'a> {
    pub(super) role: &'a str,
    pub(super) type_name: &'a str,
    pub(super) current_author: &'a str,
    pub(super) current: &'a Value,
    pub(super) revised: &'a Value,
}

pub(super) fn revision_allowed(revision: &Revision<'_>) -> bool {
    if changes_kind(revision)
        || leaves_open_outside_verifier_close(revision)
        || is_settled(revision)
    {
        return false;
    }

    revision.current_author == revision.role
        || verifier_closes_divergence(revision)
        || critic_confirms_premise(revision)
}

fn changes_kind(revision: &Revision<'_>) -> bool {
    matches!(revision.type_name, "premise" | "gap")
        && revision.current.get("kind") != revision.revised.get("kind")
}

fn is_settled(revision: &Revision<'_>) -> bool {
    match revision.type_name {
        "premise" => is_confirmed_premise(revision.current),
        "divergence" => resolution_of(revision.current) != Some("open"),
        _ => false,
    }
}

fn resolution_of(data: &Value) -> Option<&str> {
    data.get("resolution").and_then(Value::as_str)
}

fn leaves_open_outside_verifier_close(revision: &Revision<'_>) -> bool {
    revision.type_name == "divergence"
        && resolution_of(revision.revised) != Some("open")
        && !verifier_closes_divergence(revision)
}

fn verifier_closes_divergence(revision: &Revision<'_>) -> bool {
    let (Some(current), Some(revised)) =
        (revision.current.as_object(), revision.revised.as_object())
    else {
        return false;
    };

    revision.role == VERIFIER
        && revision.type_name == "divergence"
        && resolution_of(revision.revised) == Some("code-fixed")
        && evidence_of(revised).starts_with(evidence_of(current))
        && evidence_of(revised).len() > evidence_of(current).len()
        && fields_except(current, &["resolution", "evidence"])
            == fields_except(revised, &["resolution", "evidence"])
}

fn critic_confirms_premise(revision: &Revision<'_>) -> bool {
    let (Some(current), Some(revised)) =
        (revision.current.as_object(), revision.revised.as_object())
    else {
        return false;
    };
    let confirmation_fields = ["status", "confirmed_by"];

    revision.role == CRITIC
        && revision.type_name == "premise"
        && current.get("kind").and_then(Value::as_str) == Some("technical")
        && confirmed_by_critic(revision.revised)
        && fields_except(current, &confirmation_fields)
            == fields_except(revised, &confirmation_fields)
}

fn evidence_of(data: &Map<String, Value>) -> &[Value] {
    data.get("evidence")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Os campos que o papel não pode mexer: tudo menos os de `ignored`.
fn fields_except<'a>(
    data: &'a Map<String, Value>,
    ignored: &[&str],
) -> HashMap<&'a String, &'a Value> {
    data.iter()
        .filter(|(key, _)| !ignored.contains(&key.as_str()))
        .collect()
}

pub(super) fn contradicts_retirement_allowed(agent: &str, relation_author: Option<&str>) -> bool {
    match agent {
        CLAUDE => false,
        VERIFIER | USER => relation_author != Some(agent),
        _ => true,
    }
}
