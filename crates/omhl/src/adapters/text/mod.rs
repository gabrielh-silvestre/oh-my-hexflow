//! Texto dos comandos: a chave de cada comando, o renderizador que a chave escolhe e os dois layouts.

mod check_run;
mod doc;
mod layout;

use serde_json::Value;

use doc::Doc;

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

    fn agent_success(&self, value: &Value) -> String {
        layout::agent(&self.success(value))
    }

    fn human_success(&self, value: &Value) -> String {
        layout::human(&self.success(value))
    }

    fn agent_failure(&self, value: &Value) -> String {
        layout::agent(&self.failure(value))
    }

    fn human_failure(&self, value: &Value) -> String {
        layout::human(&self.failure(value))
    }
}

/// O único fato guardado por comando: só o `check run` tem renderizador até as ondas seguintes.
pub fn renderer_for(key: CommandKey) -> Option<&'static dyn Renderer> {
    match key {
        CommandKey::CheckRun => Some(&check_run::CheckRunRenderer),
        CommandKey::List
        | CommandKey::Get
        | CommandKey::Append
        | CommandKey::Attest
        | CommandKey::Render
        | CommandKey::GatePr
        | CommandKey::GateWorktree
        | CommandKey::Close
        | CommandKey::Verify
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
        | CommandKey::ReportAnswers
        | CommandKey::ReportRecurrences
        | CommandKey::ReportRetro
        | CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole => None,
    }
}

/// Texto do sucesso do comando; sem renderizador (o `report` nunca chega aqui) devolve o próprio JSON.
pub fn render_success(key: CommandKey, value: &Value, layout: Layout) -> String {
    match renderer_for(key) {
        Some(renderer) => match layout {
            Layout::Agent => renderer.agent_success(value),
            Layout::Human => renderer.human_success(value),
        },
        None => value.to_string(),
    }
}

/// Texto do erro (`{"error": {...}}`); sem renderizador devolve o próprio JSON.
pub fn render_failure(key: CommandKey, value: &Value, layout: Layout) -> String {
    match renderer_for(key) {
        Some(renderer) => match layout {
            Layout::Agent => renderer.agent_failure(value),
            Layout::Human => renderer.human_failure(value),
        },
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

/// O `pre-switch` do `wt` roda em qualquer repositório: sem marcador o gate não tem o que decidir.
pub fn silent_without_marker(key: CommandKey) -> bool {
    matches!(key, CommandKey::GateWorktree)
}
