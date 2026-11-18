//! Texto dos comandos: a chave de cada comando, o renderizador que a chave escolhe e os dois layouts.

mod base;
mod check_run;
mod doc;
mod error;
mod get;
mod layout;
mod list;
mod render;
mod report;
mod verify;

use serde_json::Value;

use base::BaseRenderer;
use doc::Doc;

pub use base::{Entry, NodeContainer, Outcome, Profile, Region, Scope, Shape, WILDCARD};

/// Os dois layouts de texto; o JSON não passa por aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Agent,
    Human,
}

/// Uma chave por comando da CLI, folhas dos grupos `check`, `guideline`, `hooks`, `spec`, `report` e `hook` incluídas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKey {
    List,
    Get,
    Append,
    Attest,
    Render,
    GatePr,
    GateWorktree,
    Close,
    Verify,
    Load,
    Project,
    CheckRun,
    CheckSweep,
    CheckRetire,
    GuidelineRetire,
    Byproduct,
    HooksInstall,
    HooksPostCommit,
    SpecInit,
    SpecValidate,
    ReportAnswers,
    ReportRecurrences,
    ReportRetro,
    HookSessionStart,
    HookSpecLock,
    HookAgentRole,
}

impl CommandKey {
    pub const ALL: [CommandKey; 26] = [
        Self::List,
        Self::Get,
        Self::Append,
        Self::Attest,
        Self::Render,
        Self::GatePr,
        Self::GateWorktree,
        Self::Close,
        Self::Verify,
        Self::Load,
        Self::Project,
        Self::CheckRun,
        Self::CheckSweep,
        Self::CheckRetire,
        Self::GuidelineRetire,
        Self::Byproduct,
        Self::HooksInstall,
        Self::HooksPostCommit,
        Self::SpecInit,
        Self::SpecValidate,
        Self::ReportAnswers,
        Self::ReportRecurrences,
        Self::ReportRetro,
        Self::HookSessionStart,
        Self::HookSpecLock,
        Self::HookAgentRole,
    ];
}

/// Cada comando ativo produz um `Doc` do `Value` do JSON; o layout só serializa.
pub trait Renderer {
    fn success(&self, value: &Value) -> Doc;

    /// `value` é o `{"error": {...}}` de `AppError::to_json`.
    fn failure(&self, value: &Value) -> Doc;
}

/// O único fato guardado por comando: os comandos de consulta (`list`, `get`, `render`, `report *`), o `verify` e o `check run` já têm renderizador; os demais ativos ganham o seu nas ondas seguintes.
pub fn renderer_for(key: CommandKey) -> Option<&'static dyn Renderer> {
    match key {
        CommandKey::CheckRun => Some(&BaseRenderer(check_run::CheckRun)),
        CommandKey::Verify => Some(&BaseRenderer(verify::Verify)),
        CommandKey::List => Some(&BaseRenderer(list::List)),
        CommandKey::Get => Some(&BaseRenderer(get::Get)),
        CommandKey::Render => Some(&render::Render),
        CommandKey::ReportAnswers => Some(&BaseRenderer(report::Answers)),
        CommandKey::ReportRecurrences => Some(&BaseRenderer(report::Recurrences)),
        CommandKey::ReportRetro => Some(&BaseRenderer(report::Retro)),
        CommandKey::Append
        | CommandKey::Attest
        | CommandKey::GatePr
        | CommandKey::GateWorktree
        | CommandKey::Close
        | CommandKey::Load
        | CommandKey::Project
        | CommandKey::CheckSweep
        | CommandKey::CheckRetire
        | CommandKey::GuidelineRetire
        | CommandKey::Byproduct
        | CommandKey::HooksInstall
        | CommandKey::HooksPostCommit
        | CommandKey::SpecInit
        | CommandKey::SpecValidate
        | CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole => None,
    }
}

/// A tabela de formas do comando; os comandos sem tabela devolvem a vazia, em que toda chave está fora dela.
pub fn profile_for(key: CommandKey) -> &'static Profile {
    match key {
        CommandKey::Verify => &verify::PROFILE,
        CommandKey::CheckRun => &check_run::PROFILE,
        CommandKey::List => &list::PROFILE,
        CommandKey::Get => &get::PROFILE,
        CommandKey::Render => &render::PROFILE,
        CommandKey::ReportAnswers => &report::ANSWERS_PROFILE,
        CommandKey::ReportRecurrences => &report::RECURRENCES_PROFILE,
        CommandKey::ReportRetro => &report::RETRO_PROFILE,
        CommandKey::Append
        | CommandKey::Attest
        | CommandKey::GatePr
        | CommandKey::GateWorktree
        | CommandKey::Close
        | CommandKey::Load
        | CommandKey::Project
        | CommandKey::CheckSweep
        | CommandKey::CheckRetire
        | CommandKey::GuidelineRetire
        | CommandKey::Byproduct
        | CommandKey::HooksInstall
        | CommandKey::HooksPostCommit
        | CommandKey::SpecInit
        | CommandKey::SpecValidate
        | CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole => &Profile::EMPTY,
    }
}

/// Texto do sucesso do comando; sem renderizador (o `report` nunca chega aqui) devolve o próprio JSON.
pub fn render_success(key: CommandKey, value: &Value, layout: Layout) -> String {
    match renderer_for(key) {
        Some(renderer) => {
            let doc = renderer.success(value);
            match layout {
                Layout::Agent => layout::agent(&doc),
                Layout::Human => layout::human(&doc),
            }
        }
        None => value.to_string(),
    }
}

/// Texto do erro (`{"error": {...}}`); sem renderizador devolve o próprio JSON.
pub fn render_failure(key: CommandKey, value: &Value, layout: Layout) -> String {
    match renderer_for(key) {
        Some(renderer) => {
            let doc = renderer.failure(value);
            match layout {
                Layout::Agent => layout::agent(&doc),
                Layout::Human => layout::human(&doc),
            }
        }
        None => value.to_string(),
    }
}

/// Os comandos lidos por máquina: nunca ganham texto e recusam `--format agent|human`.
pub fn deferred(key: CommandKey) -> bool {
    match key {
        CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole
        | CommandKey::HooksPostCommit
        | CommandKey::GateWorktree => true,
        CommandKey::List
        | CommandKey::Get
        | CommandKey::Append
        | CommandKey::Attest
        | CommandKey::Render
        | CommandKey::GatePr
        | CommandKey::Close
        | CommandKey::Verify
        | CommandKey::Load
        | CommandKey::Project
        | CommandKey::CheckRun
        | CommandKey::CheckSweep
        | CommandKey::CheckRetire
        | CommandKey::GuidelineRetire
        | CommandKey::Byproduct
        | CommandKey::HooksInstall
        | CommandKey::SpecInit
        | CommandKey::SpecValidate
        | CommandKey::ReportAnswers
        | CommandKey::ReportRecurrences
        | CommandKey::ReportRetro => false,
    }
}
