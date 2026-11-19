use super::config::{RequireAtom, WorkflowConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PremiseKind {
    Business,
    Technical,
}

/// Fatos de uma premissa do work item da branch; `confirmed` exige `status` confirmado e `confirmed_by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiseFact {
    pub kind: Option<PremiseKind>,
    pub confirmed: bool,
}

/// Os átomos de `require` do prefixo que as premissas e a validação da spec não satisfazem, na ordem do `require`; `None` quando o prefixo não está na tabela.
pub fn missing_atoms(
    prefix: &str,
    config: &WorkflowConfig,
    premises: &[PremiseFact],
    spec_valid: bool,
) -> Option<Vec<RequireAtom>> {
    let required = config.prefixes.get(prefix)?;

    Some(
        required
            .iter()
            .copied()
            .filter(|atom| !is_satisfied(*atom, premises, spec_valid))
            .collect(),
    )
}

fn is_satisfied(atom: RequireAtom, premises: &[PremiseFact], spec_valid: bool) -> bool {
    let any = |kind: PremiseKind, needs_confirmation: bool| {
        premises
            .iter()
            .any(|premise| premise.kind == Some(kind) && (premise.confirmed || !needs_confirmation))
    };

    match atom {
        RequireAtom::BusinessPremise => any(PremiseKind::Business, true),
        RequireAtom::TechnicalPremiseConfirmed => any(PremiseKind::Technical, true),
        RequireAtom::AgentPremise => any(PremiseKind::Technical, false),
        RequireAtom::Spec => spec_valid,
    }
}

#[cfg(test)]
mod tests;
