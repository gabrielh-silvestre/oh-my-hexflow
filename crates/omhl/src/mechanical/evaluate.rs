use super::spec::{CheckSpec, Outcome};

#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub exit_code: Option<i32>,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub timed_out: bool,
    pub tree_changed: bool,
}

pub fn evaluate(_spec: &CheckSpec, _observed: &Observation<'_>) -> Outcome {
    unimplemented!()
}
