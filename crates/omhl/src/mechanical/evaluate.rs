use super::spec::{CheckSpec, Outcome, StreamPattern};

#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub exit_code: Option<i32>,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub timed_out: bool,
    pub tree_changed: bool,
}

/// A primeira regra que casa decide; nenhuma casa é `Failed`.
pub fn evaluate(spec: &CheckSpec, observed: &Observation<'_>) -> Outcome {
    if observed.timed_out || observed.tree_changed {
        return Outcome::Failed;
    }
    let Some(code) = observed.exit_code else {
        return Outcome::Failed;
    };

    spec.expect
        .iter()
        .find(|rule| rule.exit == code && pattern_matches(rule.pattern.as_ref(), observed))
        .map_or(Outcome::Failed, |rule| rule.outcome)
}

/// O regex olha o fluxo inteiro, não linha a linha.
fn pattern_matches(pattern: Option<&StreamPattern>, observed: &Observation<'_>) -> bool {
    match pattern {
        None => true,
        Some(StreamPattern::Stdout(regex)) => regex.is_match(observed.stdout),
        Some(StreamPattern::Stderr(regex)) => regex.is_match(observed.stderr),
    }
}
