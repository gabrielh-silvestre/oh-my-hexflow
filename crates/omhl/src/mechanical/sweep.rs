pub const RUN_MARKER_VAR: &str = "OMHL_RUN_ID";

/// Quem chamou o runner: pid, início (campo 22 de `/proc/<pid>/stat`) e namespace de pid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunMarker {
    pub pid: u32,
    pub start: u64,
    pub pidns: u64,
}

impl RunMarker {
    pub fn encode(&self) -> String {
        unimplemented!()
    }

    pub fn parse(_text: &str) -> Option<RunMarker> {
        unimplemented!()
    }
}

pub fn start_from_stat(_stat: &str) -> Option<u64> {
    unimplemented!()
}

/// Processo com a variável da marca no ambiente; `pgrp` é o campo 5 do stat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub pid: u32,
    pub start: u64,
    pub pgrp: u32,
    pub marker: String,
}

pub fn select_strays(
    _own_pid: u32,
    _own_marker: Option<&str>,
    _own_pidns: u64,
    _candidates: &[Candidate],
    _owner_start: &dyn Fn(u32) -> Option<u64>,
    _unmarked_leader_alive: &dyn Fn(u32) -> bool,
) -> Vec<Candidate> {
    unimplemented!()
}
