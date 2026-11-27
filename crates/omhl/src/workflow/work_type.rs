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

/// O tipo que o `close` e o `gate-delivery` aplicam: o `type` gravado, ou então o único prefixo dos branches do item.
/// `None` quando o tipo fica fora da tabela efetiva ou quando não há um único prefixo.
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
