#![allow(dead_code)]

pub mod agent_examples;
pub mod appfx;
pub mod busy;
pub mod check_run_tree;
pub mod child;
pub mod cycle;
pub mod directives_flow_e2e;
pub mod ext;
pub mod fences;
pub mod forge;
pub mod gate;
pub mod git;
pub mod guideline_retire;
pub mod hook;
pub mod inproc;
pub mod legacy;
pub mod omhl;
pub mod output_agent_leaves;
pub mod output_agent_query;
pub mod output_agent_report;
pub mod pr_gate;
pub mod project;
pub mod query;
pub mod raw;
pub mod repo_checks;
pub mod report;
pub mod run;
pub mod runner;
pub mod scan;
pub mod session;
#[cfg(target_os = "linux")]
pub mod sweep_lock;
pub mod views;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use ::omhl::facade::SessionSource;
use ::omhl::probe::{DevelopMark, HeadFacts, ProbeError, ProbeOutcome, SnapshotProbe, TreeSource};
use hexlog::{
    Author, Clock, IdGenerator, NativeNode, NativeNodes, RelationTypeName, SemanticId, Snapshot,
    SnapshotKind, TenantId, TypeName,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// Sem `CLAUDE_CODE_SESSION_ID`: o registro leva o marcador `outside-claude-code`.
pub static NO_SESSION: SessionSource = SessionSource::Absent;

pub const DEFAULT_INSTANT: &str = "2026-11-07T12:00:00.000Z";
const UUID_V7_BASE: u128 = 0x0192_a000_0000_7000_8000_0000_0000_0000;

pub struct FixedClock {
    instant: String,
}

impl FixedClock {
    pub fn at(instant: &str) -> FixedClock {
        FixedClock {
            instant: instant.to_owned(),
        }
    }
}

impl Default for FixedClock {
    fn default() -> FixedClock {
        FixedClock::at(DEFAULT_INSTANT)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> String {
        self.instant.clone()
    }
}

pub struct SeqIds(AtomicU64);

impl SeqIds {
    pub fn new() -> SeqIds {
        SeqIds(AtomicU64::new(1))
    }
}

impl Default for SeqIds {
    fn default() -> SeqIds {
        SeqIds::new()
    }
}

impl IdGenerator for SeqIds {
    fn next(&self) -> Uuid {
        let n = self.0.fetch_add(1, Ordering::Relaxed);
        Uuid::from_u128(UUID_V7_BASE | u128::from(n))
    }
}

/// Sonda sem estado mutável: uma resposta fixa por método. Para mudá-la no meio de um teste,
/// larga-se o `Omhl`, troca-se o falso e reabre-se. Método sem resposta configurada devolve erro,
/// para o teste falhar alto em vez de passar por acaso.
pub struct FakeProbe {
    result: Result<ProbeOutcome, ProbeError>,
    index_tree: Option<Result<ProbeOutcome, ProbeError>>,
    head_tree: Option<Result<ProbeOutcome, ProbeError>>,
    tree_dirty: Option<bool>,
    head_facts: Option<HeadFacts>,
    develop_mark: Option<bool>,
    consume_log: Option<mpsc::Sender<()>>,
}

impl FakeProbe {
    fn answering(result: Result<ProbeOutcome, ProbeError>) -> FakeProbe {
        FakeProbe {
            result,
            index_tree: None,
            head_tree: None,
            tree_dirty: None,
            head_facts: None,
            develop_mark: None,
            consume_log: None,
        }
    }

    pub fn clean(snapshot: Snapshot) -> FakeProbe {
        FakeProbe::answering(Ok(ProbeOutcome::Reproducible(snapshot)))
    }

    pub fn dirty() -> FakeProbe {
        FakeProbe::answering(Ok(ProbeOutcome::Dirty))
    }

    pub fn failing() -> FakeProbe {
        FakeProbe::answering(Err(probe_error("probe unavailable")))
    }

    pub fn with_tree(
        mut self,
        source: TreeSource,
        result: Result<ProbeOutcome, ProbeError>,
    ) -> FakeProbe {
        match source {
            TreeSource::Index => self.index_tree = Some(result),
            TreeSource::Head => self.head_tree = Some(result),
        }
        self
    }

    pub fn with_tree_dirty(mut self, dirty: bool) -> FakeProbe {
        self.tree_dirty = Some(dirty);
        self
    }

    pub fn with_head_facts(mut self, facts: HeadFacts) -> FakeProbe {
        self.head_facts = Some(facts);
        self
    }

    pub fn with_develop_mark(mut self, present: bool) -> FakeProbe {
        self.develop_mark = Some(present);
        self
    }

    pub fn with_consume_log(mut self, log: mpsc::Sender<()>) -> FakeProbe {
        self.consume_log = Some(log);
        self
    }
}

fn probe_error(message: &str) -> ProbeError {
    ProbeError {
        message: message.to_owned(),
    }
}

impl SnapshotProbe for FakeProbe {
    fn probe(&self) -> Result<ProbeOutcome, ProbeError> {
        self.result.clone()
    }

    fn probe_tree(&self, source: TreeSource) -> Result<ProbeOutcome, ProbeError> {
        let configured = match source {
            TreeSource::Index => &self.index_tree,
            TreeSource::Head => &self.head_tree,
        };
        configured
            .clone()
            .unwrap_or_else(|| Err(probe_error("tree probe not configured")))
    }

    fn tree_dirty(&self) -> Result<bool, ProbeError> {
        self.tree_dirty
            .ok_or_else(|| probe_error("tree dirty probe not configured"))
    }

    fn head_facts(&self) -> Result<HeadFacts, ProbeError> {
        self.head_facts
            .clone()
            .ok_or_else(|| probe_error("head facts probe not configured"))
    }
}

impl DevelopMark for FakeProbe {
    fn present(&self) -> Result<bool, ProbeError> {
        self.develop_mark
            .ok_or_else(|| probe_error("develop mark probe not configured"))
    }

    fn consume(&self) -> Result<(), ProbeError> {
        if let Some(log) = &self.consume_log {
            // o teste pode já ter largado o Receiver; a chamada continua valendo
            let _ = log.send(());
        }
        Ok(())
    }
}

/// Base nativa sem nenhum id.
pub struct EmptyNative;

impl NativeNodes for EmptyNative {
    fn lookup(&self, _id: &SemanticId) -> Option<NativeNode> {
        None
    }

    fn native_relations(&self, _relation_type: &RelationTypeName) -> Vec<(SemanticId, SemanticId)> {
        Vec::new()
    }

    fn catalog_version(&self) -> &str {
        "test"
    }
}

pub fn tenant(text: &str) -> TenantId {
    TenantId::parse(text).expect("valid tenant")
}

pub fn sid(text: &str) -> SemanticId {
    SemanticId::parse(text).expect("valid semantic id")
}

pub fn rty(text: &str) -> RelationTypeName {
    RelationTypeName::parse(text).expect("valid relation type name")
}

pub fn tname(text: &str) -> TypeName {
    TypeName::parse(text).expect("valid type name")
}

pub fn author() -> Author {
    Author {
        agent: "tester".to_owned(),
        model: None,
        client: "tests".to_owned(),
    }
}

/// A sessão principal sob o Claude Code: `claude` pelo client `cli`.
pub fn agent_author() -> Author {
    Author {
        agent: "claude".to_owned(),
        model: None,
        client: "cli".to_owned(),
    }
}

/// O dono num terminal fora do Claude Code: `user` pelo client `cli`.
pub fn cli_author() -> Author {
    Author {
        agent: "user".to_owned(),
        model: None,
        client: "cli".to_owned(),
    }
}

/// Um papel portado como o `OMHL_ROLE` o grava: `role` inteiro (`omhl:critic`) no agent, client `cli`.
pub fn role_author(role: &str) -> Author {
    Author {
        agent: role.to_owned(),
        model: None,
        client: "cli".to_owned(),
    }
}

pub fn loader_author() -> Author {
    Author {
        agent: "loader".to_owned(),
        model: None,
        client: "loader".to_owned(),
    }
}

pub fn executor_author() -> Author {
    Author {
        agent: "executor".to_owned(),
        model: None,
        client: "executor".to_owned(),
    }
}

/// Fatos crus de um HEAD na branch `feat/wi-w1__x`, a única que o contém.
pub fn wi_head_facts() -> HeadFacts {
    HeadFacts {
        current: Some("feat/wi-w1__x".to_owned()),
        rebase_head: None,
        containing: vec!["feat/wi-w1__x".to_owned()],
        head: Some("a".repeat(40)),
    }
}

/// Dados completos de uma attestation de check com `run`; `byproduct` é o hash de 64 hex.
pub fn executor_attestation_data(
    check: &str,
    work_item: &str,
    outcome: &str,
    byproduct: &str,
) -> Value {
    json!({
        "check": check,
        "work_item": work_item,
        "outcome": outcome,
        "ok": outcome != "failed",
        "command": ["true"],
        "spec_digest": "d".repeat(64),
        "exit_code": if outcome == "failed" { 1 } else { 0 },
        "duration_ms": 1,
        "timed_out": false,
        "truncated": false,
        "byproduct": byproduct,
    })
}

/// Commit de 40 caracteres iguais a `fill`.
pub fn snap(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(40)).expect("valid snapshot")
}

/// Commit de 64 caracteres iguais a `fill`.
pub fn snap64(fill: char) -> Snapshot {
    Snapshot::new(SnapshotKind::GitCommit, &fill.to_string().repeat(64)).expect("valid snapshot")
}

/// O `command` de uma attestation de `check run` real: `argv[0]` vira o arquivo que rodou.
pub fn assert_resolved_command(command: &Value, program: &str, args: &[&str]) {
    let items = command.as_array().expect("command is a list");
    let path = std::path::Path::new(items[0].as_str().expect("command[0] is text"));
    assert!(path.is_absolute(), "command[0] is not resolved: {path:?}");
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(program)
    );
    assert_eq!(
        items[1..],
        args.iter().map(|arg| json!(arg)).collect::<Vec<_>>()
    );
}
