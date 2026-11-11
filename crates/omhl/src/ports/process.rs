use std::path::Path;

use crate::runner::{CommandRunner, RunError, RunReport, RunSpec};

pub struct ProcessRunner {}

impl ProcessRunner {
    /// Sem amarração ao processo pai; para os testes.
    pub fn new() -> ProcessRunner {
        unimplemented!()
    }

    /// `parent_pid` é o `parent_id()` que o `main` leu na primeira linha.
    pub fn bound_to_parent(_parent_pid: u32) -> ProcessRunner {
        unimplemented!()
    }
}

impl Default for ProcessRunner {
    fn default() -> ProcessRunner {
        ProcessRunner::new()
    }
}

impl CommandRunner for ProcessRunner {
    fn run(&self, _spec: &RunSpec) -> Result<RunReport, RunError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweptProcess {
    pub pid: u32,
    pub comm: String,
    pub owner_pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepReport {
    pub swept: Vec<SweptProcess>,
    pub unreadable: usize,
    pub scan_timed_out: bool,
}

pub fn sweep_strays() -> Result<SweepReport, RunError> {
    sweep_strays_in(Path::new("/proc"))
}

pub fn sweep_strays_in(_proc_root: &Path) -> Result<SweepReport, RunError> {
    unimplemented!()
}
