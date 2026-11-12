use super::spec::{CheckSpec, Outcome, StreamPattern};

#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub exit_code: Option<i32>,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub timed_out: bool,
    pub tree_changed: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Evaluation {
    pub outcome: Outcome,
    /// O veredito é `Failed` porque uma regra de padrão não pôde ser conferida com a saída cortada.
    pub pattern_unchecked: bool,
}

pub fn evaluate(spec: &CheckSpec, observed: &Observation<'_>) -> Outcome {
    evaluate_with_reason(spec, observed).outcome
}

/// A primeira regra que casa decide; nenhuma casa é `Failed`.
/// Com a saída cortada, uma regra de padrão que chegaria a decidir não pode ser conferida: falha fechada.
pub fn evaluate_with_reason(spec: &CheckSpec, observed: &Observation<'_>) -> Evaluation {
    let decided = |outcome| Evaluation {
        outcome,
        pattern_unchecked: false,
    };
    if observed.timed_out || observed.tree_changed {
        return decided(Outcome::Failed);
    }
    let Some(code) = observed.exit_code else {
        return decided(Outcome::Failed);
    };

    for rule in spec.expect.iter().filter(|rule| rule.exit == code) {
        if rule.pattern.is_some() && observed.truncated {
            return Evaluation {
                outcome: Outcome::Failed,
                pattern_unchecked: true,
            };
        }
        if pattern_matches(rule.pattern.as_ref(), observed) {
            return decided(rule.outcome);
        }
    }

    decided(Outcome::Failed)
}

/// O regex olha o fluxo inteiro, não linha a linha.
fn pattern_matches(pattern: Option<&StreamPattern>, observed: &Observation<'_>) -> bool {
    match pattern {
        None => true,
        Some(StreamPattern::Stdout(regex)) => regex.is_match(observed.stdout),
        Some(StreamPattern::Stderr(regex)) => regex.is_match(observed.stderr),
    }
}
