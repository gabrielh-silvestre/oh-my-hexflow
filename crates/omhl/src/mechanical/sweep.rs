use std::path::Path;

use hexlog::chain::sha256hex;

pub const RUN_MARKER_VAR: &str = "OMHL_RUN_ID";
const ROOT_HASH_LEN: usize = 16;

/// Quem chamou o runner: pid, início (campo 22 de `/proc/<pid>/stat`), namespace de pid e a raiz do projeto (`root_hash`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunMarker {
    pub pid: u32,
    pub start: u64,
    pub pidns: u64,
    pub root: String,
}

/// Os 16 primeiros hex do sha256 do caminho da raiz: a marca identifica o projeto sem expor o caminho.
pub fn root_hash(root: &Path) -> String {
    sha256hex(&root.to_string_lossy()).as_str()[..ROOT_HASH_LEN].to_owned()
}

impl RunMarker {
    pub fn encode(&self) -> String {
        format!(
            "v1:{}:{}:{}:{}",
            self.pid, self.start, self.pidns, self.root
        )
    }

    pub fn parse(text: &str) -> Option<RunMarker> {
        let mut fields = text.strip_prefix("v1:")?.split(':');
        let marker = RunMarker {
            pid: fields.next()?.parse().ok()?,
            start: fields.next()?.parse().ok()?,
            pidns: fields.next()?.parse().ok()?,
            root: fields.next()?.to_owned(),
        };
        let root_is_hash = marker.root.len() == ROOT_HASH_LEN
            && marker
                .root
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        // só o formato exato: recusa `+`, zeros à esquerda, espaços e campos a mais
        (root_is_hash && marker.encode() == text).then_some(marker)
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
    own_root: Option<&str>,
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
                own_root,
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
    own_root: Option<&str>,
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
    // `None` varre qualquer raiz (`omhl check sweep`); `Some` só a do projeto que está rodando
    if own_root.is_some_and(|root| marker.root != root) {
        return false;
    }
    if candidate.pgrp != candidate.pid && unmarked_leader_alive(candidate.pgrp) {
        return false;
    }
    owner_start(marker.pid) != Some(marker.start)
}
