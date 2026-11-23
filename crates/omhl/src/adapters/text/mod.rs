mod base;
mod check_run;
mod doc;
mod error;
mod gate;
mod get;
mod layout;
mod list;
mod load;
mod render;
mod report;
mod simple;
mod spec_validate;
#[cfg(test)]
mod tests;
mod verify;
mod vocab;

use serde_json::Value;

use base::BaseRenderer;
use doc::Doc;

pub use base::{Entry, NodeContainer, Outcome, Profile, Region, Scope, Shape, WILDCARD};
pub use vocab::{
    AgentVocabulary, HumanVocabulary, NEXT_LABEL, NOTHING_BLOCKED, SECRET_WARNING, SYMBOL_ERROR,
    SYMBOL_OK, SYMBOL_WARNING, State, Vocabulary,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Agent,
    Human,
}

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
    ByproductPrune,
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
    HookWtPostStart,
}

impl CommandKey {
    pub const ALL: [CommandKey; 28] = [
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
        Self::ByproductPrune,
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
        Self::HookWtPostStart,
    ];
}

pub trait Renderer {
    fn success(&self, value: &Value) -> Doc;

    /// `value` é o `{"error": {...}}` de `AppError::to_json`.
    fn failure(&self, value: &Value) -> Doc;
}

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
        CommandKey::Project => Some(&BaseRenderer(simple::Project)),
        CommandKey::CheckSweep => Some(&BaseRenderer(simple::CheckSweep)),
        CommandKey::CheckRetire => Some(&BaseRenderer(simple::CheckRetire)),
        CommandKey::GuidelineRetire => Some(&BaseRenderer(simple::GuidelineRetire)),
        CommandKey::HooksInstall => Some(&BaseRenderer(simple::HooksInstall)),
        CommandKey::SpecInit => Some(&BaseRenderer(simple::SpecInit)),
        CommandKey::Byproduct => Some(&BaseRenderer(simple::Byproduct)),
        CommandKey::ByproductPrune => Some(&BaseRenderer(simple::ByproductPrune)),
        CommandKey::Append => Some(&BaseRenderer(simple::Append)),
        CommandKey::Attest => Some(&BaseRenderer(simple::Attest)),
        CommandKey::Load => Some(&BaseRenderer(load::Load)),
        CommandKey::SpecValidate => Some(&BaseRenderer(spec_validate::SpecValidate)),
        CommandKey::GatePr => Some(&BaseRenderer(gate::GatePr)),
        CommandKey::Close => Some(&BaseRenderer(gate::Close)),
        CommandKey::GateWorktree
        | CommandKey::HooksPostCommit
        | CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole
        | CommandKey::HookWtPostStart => None,
    }
}

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
        CommandKey::Project => &simple::PROJECT_PROFILE,
        CommandKey::CheckSweep => &simple::CHECK_SWEEP_PROFILE,
        CommandKey::CheckRetire => &simple::CHECK_RETIRE_PROFILE,
        CommandKey::GuidelineRetire => &simple::GUIDELINE_RETIRE_PROFILE,
        CommandKey::HooksInstall => &simple::HOOKS_INSTALL_PROFILE,
        CommandKey::SpecInit => &simple::SPEC_INIT_PROFILE,
        CommandKey::Byproduct => &simple::BYPRODUCT_PROFILE,
        CommandKey::ByproductPrune => &simple::BYPRODUCT_PRUNE_PROFILE,
        CommandKey::Append => &simple::APPEND_PROFILE,
        CommandKey::Attest => &simple::ATTEST_PROFILE,
        CommandKey::GatePr => &gate::GATE_PR_PROFILE,
        CommandKey::Close => &gate::CLOSE_PROFILE,
        CommandKey::Load => &load::PROFILE,
        CommandKey::SpecValidate => &spec_validate::PROFILE,
        CommandKey::GateWorktree
        | CommandKey::HooksPostCommit
        | CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole
        | CommandKey::HookWtPostStart => &Profile::EMPTY,
    }
}

pub fn render_success(key: CommandKey, value: &Value, layout: Layout) -> String {
    match renderer_for(key) {
        Some(renderer) => {
            let doc = renderer.success(value);
            match layout {
                Layout::Agent => layout::agent(&doc, &AgentVocabulary),
                Layout::Human => layout::human(&doc, &HumanVocabulary),
            }
        }
        None => value.to_string(),
    }
}

pub fn render_failure(key: CommandKey, value: &Value, layout: Layout) -> String {
    match renderer_for(key) {
        Some(renderer) => {
            let doc = renderer.failure(value);
            match layout {
                Layout::Agent => layout::agent(&doc, &AgentVocabulary),
                Layout::Human => layout::human(&doc, &HumanVocabulary),
            }
        }
        None => value.to_string(),
    }
}

/// Os comandos lidos por máquina. A lista é independente de `renderer_for`:
/// o teste de fechamento confronta as duas.
pub fn deferred(key: CommandKey) -> bool {
    match key {
        CommandKey::HookSessionStart
        | CommandKey::HookSpecLock
        | CommandKey::HookAgentRole
        | CommandKey::HookWtPostStart
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
        | CommandKey::ByproductPrune
        | CommandKey::HooksInstall
        | CommandKey::SpecInit
        | CommandKey::SpecValidate
        | CommandKey::ReportAnswers
        | CommandKey::ReportRecurrences
        | CommandKey::ReportRetro => false,
    }
}
