use regex::Regex;
use serde_json::Value;

/// Só `Failed` não é `ok`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    PassedWithWarning,
    Failed,
}

/// No máximo um padrão por regra: os dois fluxos no TOML são erro no load (DP-12).
#[derive(Debug, Clone)]
pub enum StreamPattern {
    Stdout(Regex),
    Stderr(Regex),
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub exit: i32,
    pub pattern: Option<StreamPattern>,
    pub outcome: Outcome,
}

/// Sem campo de snapshot: ele sai de `snapshot_for` (DP-6).
#[derive(Debug, Clone)]
pub struct CheckSpec {
    pub run: Vec<String>,
    pub timeout_secs: u32,
    pub expect: Vec<Rule>,
}

/// Nunca ecoa o conteúdo recusado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError {
    pub path: String,
    pub code: &'static str,
}

/// Só mede check com `run`.
pub const CHECK_DATA_MAX_UNITS: usize = 15_000;
pub const RUN_MAX_UNITS: usize = 6_000;
pub const RUNNABLE_EVENTS: [&str; 3] = ["before-commit", "before-pr", "before-merge"];

impl CheckSpec {
    /// `None` é check só de texto.
    pub fn from_data(_data: &Value) -> Result<Option<CheckSpec>, SpecError> {
        unimplemented!()
    }
}

/// Único decodificador do tipo de snapshot de um check (DP-6).
pub fn snapshot_for(_data: &Value) -> hexlog::SnapshotKind {
    unimplemented!()
}
