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
        format!("v1:{}:{}:{}", self.pid, self.start, self.pidns)
    }

    pub fn parse(text: &str) -> Option<RunMarker> {
        let mut fields = text.strip_prefix("v1:")?.split(':');
        let marker = RunMarker {
            pid: fields.next()?.parse().ok()?,
            start: fields.next()?.parse().ok()?,
            pidns: fields.next()?.parse().ok()?,
        };
        // só o formato exato: recusa `+`, zeros à esquerda, espaços e campos a mais
        (marker.encode() == text).then_some(marker)
    }
}

/// Campo 22 de uma linha de `/proc/<pid>/stat`, contado depois do último `)` (o `comm` pode ter `)`).
pub fn start_from_stat(stat: &str) -> Option<u64> {
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(22 - 3)?.parse().ok()
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
    own_pid: u32,
    own_marker: Option<&str>,
    own_pidns: u64,
    candidates: &[Candidate],
    owner_start: &dyn Fn(u32) -> Option<u64>,
    unmarked_leader_alive: &dyn Fn(u32) -> bool,
) -> Vec<Candidate> {
    candidates
        .iter()
        .filter(|candidate| {
            is_stray(
                own_pid,
                own_marker,
                own_pidns,
                candidate,
                owner_start,
                unmarked_leader_alive,
            )
        })
        .cloned()
        .collect()
}

fn is_stray(
    own_pid: u32,
    own_marker: Option<&str>,
    own_pidns: u64,
    candidate: &Candidate,
    owner_start: &dyn Fn(u32) -> Option<u64>,
    unmarked_leader_alive: &dyn Fn(u32) -> bool,
) -> bool {
    if candidate.pid == own_pid {
        return false;
    }
    let Some(marker) = RunMarker::parse(&candidate.marker) else {
        return false;
    };
    if own_marker == Some(candidate.marker.as_str()) || marker.pidns != own_pidns {
        return false;
    }
    if candidate.pgrp != candidate.pid && unmarked_leader_alive(candidate.pgrp) {
        return false;
    }
    owner_start(marker.pid) != Some(marker.start)
}
