//! Texto dos comandos: a chave de cada comando e o que ela diz sobre o formato de saída.

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

/// Só o `check run` tem texto até a W1.2 trocar isto por `renderer_for(key).is_some()`.
pub fn has_renderer(key: CommandKey) -> bool {
    matches!(key, CommandKey::CheckRun)
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
