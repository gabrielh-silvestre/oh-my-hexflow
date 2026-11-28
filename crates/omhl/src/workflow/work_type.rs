use std::collections::BTreeMap;

use super::config::RequireAtom;

/// A técnica confirmada implica a premissa do agente.
fn closure(atoms: &[RequireAtom]) -> Vec<RequireAtom> {
    let mut closed = atoms.to_vec();
    if closed.contains(&RequireAtom::TechnicalPremiseConfirmed)
        && !closed.contains(&RequireAtom::AgentPremise)
    {
        closed.push(RequireAtom::AgentPremise);
    }

    closed
}

pub fn loosens(previous: &[RequireAtom], next: &[RequireAtom]) -> bool {
    let next_closed = closure(next);

    closure(previous)
        .iter()
        .any(|atom| !next_closed.contains(atom))
}

pub fn effective_type(
    recorded: Option<&str>,
    branch_prefixes: &[&str],
    table: &BTreeMap<String, Vec<RequireAtom>>,
) -> Option<String> {
    let name = match recorded {
        Some(name) => name,
        None => match branch_prefixes {
            [only] => only,
            _ => return None,
        },
    };

    table.contains_key(name).then(|| name.to_owned())
}

#[cfg(test)]
mod tests;
