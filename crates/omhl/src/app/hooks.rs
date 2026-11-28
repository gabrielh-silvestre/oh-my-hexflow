use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

use super::engine::{Engine, require_project_root};
use super::error::AppError;
use crate::ports::git::GitProbe;
use crate::ports::hooks::GitHooks;
use crate::ports::workflow::read_workflow;

const SCRIPTS: [(&str, &str); 5] = [
    ("pre-commit", "check run --event before-commit"),
    ("pre-merge-commit", "check run --event before-commit"),
    ("pre-push", "check run --event before-push --pre-push"),
    ("post-commit", "hooks post-commit >/dev/null"),
    ("post-merge", "hooks post-merge \"$1\" >/dev/null"),
];
const UNWRITABLE_HINT: &str =
    "the hooks folder of the user cannot be written; check XDG_DATA_HOME and permissions";
const UNSTABLE_BINARY_HINT: &str = "install an omhl binary outside any worktree (cargo install or ~/.local/bin) and run omhl hooks install again; every clone shares these hooks";

impl Engine<'_> {
    pub(super) fn hooks_install(&self, force: bool) -> Result<Value, AppError> {
        let root = require_project_root(self.project_root.as_deref())?;

        install(&GitHooks::new(root), self.hooks_dir.as_deref(), force)
    }
}

/// O que o clone tem diante do que `omhl hooks install` gravaria agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HooksDrift {
    Missing,
    Outdated(Vec<String>),
    PathConflict(String),
    Existing,
}

const PERSON_ACTION: &str = "installing the git hooks is an action of the person: the agent runs omhl hooks install only when the person asks";

impl Engine<'_> {
    /// Só lê: nenhum arquivo, config ou script é escrito.
    pub(super) fn hooks_drift(&self) -> Result<Option<HooksDrift>, AppError> {
        let root = require_project_root(self.project_root.as_deref())?;
        let git = GitHooks::new(root);
        let probe_failed = |reason: String| AppError::probe_failed(&reason);
        let hooks_text = self.hooks_dir.as_deref().map(Path::to_string_lossy);
        match git.configured_hooks_path().map_err(probe_failed)? {
            Some(path) if Some(path.as_str()) != hooks_text.as_deref() => {
                return Ok(Some(HooksDrift::PathConflict(path)));
            }
            Some(_) => {}
            None => {
                let current = git.resolved_hooks_dir().map_err(probe_failed)?;
                return Ok(Some(if git.has_foreign_hooks(&current) {
                    HooksDrift::Existing
                } else {
                    HooksDrift::Missing
                }));
            }
        }
        let Some(hooks_dir) = self.hooks_dir.as_deref() else {
            return Ok(Some(HooksDrift::Missing));
        };
        let binary = git
            .binary()
            .map_err(|_| AppError::binary_path_unreadable())?;
        let binary_text = binary.to_string_lossy();
        let outdated: Vec<String> = SCRIPTS
            .iter()
            .filter(|(name, command)| {
                std::fs::read_to_string(hooks_dir.join(name)).ok()
                    != Some(script(&binary_text, command))
            })
            .map(|(name, _)| (*name).to_owned())
            .collect();

        Ok((!outdated.is_empty()).then_some(HooksDrift::Outdated(outdated)))
    }

    /// A linha do aviso do `session-start`; cala sem `[git] hooks = true` ou com o `workflow.toml` ilegível.
    pub(super) fn git_hooks_notice(&self) -> Option<String> {
        let root = self.project_root.as_deref()?;
        let main_dir = GitProbe::new(root).main_dir().ok()?;
        if !read_workflow(&main_dir).ok()?.git.hooks {
            return None;
        }
        self.hooks_drift()
            .ok()
            .flatten()
            .map(|drift| drift_line(&drift))
    }
}

fn drift_line(drift: &HooksDrift) -> String {
    match drift {
        HooksDrift::Missing => format!(
            "git-hooks-missing: this project declares [git] hooks = true but the clone has no omhl git hooks; {PERSON_ACTION}"
        ),
        HooksDrift::Outdated(names) => format!(
            "git-hooks-outdated: the omhl git hooks of this clone differ from what omhl hooks install writes now ({}); {PERSON_ACTION}",
            names.join(", ")
        ),
        HooksDrift::PathConflict(_) => format!(
            "hooks-path-conflict: core.hooksPath points to a folder of another origin, and omhl hooks install refuses it without --force; {PERSON_ACTION}"
        ),
        HooksDrift::Existing => format!(
            "hooks-existing: the default hooks folder has scripts of another origin, and omhl hooks install refuses them without --force; {PERSON_ACTION}"
        ),
    }
}

fn install(git: &GitHooks, hooks_dir: Option<&Path>, force: bool) -> Result<Value, AppError> {
    let probe_failed = |reason: String| AppError::probe_failed(&reason);
    let current = git.resolved_hooks_dir().map_err(probe_failed)?;
    let hooks_text = hooks_dir.map(Path::to_string_lossy);
    if !force {
        refuse_conflict(git, &current, hooks_text.as_deref())?;
    }
    let unwritable =
        || AppError::hooks_dir_unwritable("").with_context(json!({ "hint": UNWRITABLE_HINT }));
    let hooks_dir = hooks_dir.ok_or_else(unwritable)?;
    let binary = git
        .binary()
        .map_err(|_| AppError::binary_path_unreadable())?;
    let worktrees = git.linked_worktree_roots().map_err(probe_failed)?;
    // shortcut: caminho que não é UTF-8 vira texto com perda e o hook diz "binary not found"; trocar por bytes se aparecer.
    let binary_text = binary.to_string_lossy();
    let scripts = SCRIPTS.map(|(name, command)| (name, script(&binary_text, command)));
    git.write_scripts(hooks_dir, &scripts)
        .map_err(|_| unwritable())?;
    git.set_local_hooks_path(hooks_dir).map_err(probe_failed)?;
    let resolved = git.resolved_hooks_dir().map_err(probe_failed)?;
    // O `rev-parse` resolve links simbólicos e tira a barra final: compara com o caminho canônico.
    if git.canonical(hooks_dir).ok() != Some(resolved) {
        return Err(AppError::hooks_path_unreachable(""));
    }
    let mut installed = json!({
        "hooks_path": hooks_text,
        "omhl": binary_text,
        "scripts": SCRIPTS.map(|(name, _)| name),
    });
    if let Some(code) = unstable_binary_code(&binary, &worktrees) {
        installed["warnings"] = json!([{
            "code": code,
            "path": binary_text,
            "hint": UNSTABLE_BINARY_HINT,
        }]);
    }

    Ok(installed)
}

fn refuse_conflict(
    git: &GitHooks,
    current: &Path,
    hooks_text: Option<&str>,
) -> Result<(), AppError> {
    let configured = git
        .configured_hooks_path()
        .map_err(|reason| AppError::probe_failed(&reason))?;
    match configured {
        Some(path) if Some(path.as_str()) != hooks_text => Err(AppError::hooks_path_conflict("")),
        None if git.has_foreign_hooks(current) => Err(AppError::hooks_existing("")),
        _ => Ok(()),
    }
}

fn script(binary: &str, command: &str) -> String {
    let p = double_quoted(binary);

    format!(
        "#!/bin/sh\n[ -x \"{p}\" ] || {{ printf '%s\n' \"omhl: binary not found at {p}; run omhl hooks install again\" >&2; exit 1; }}; exec \"{p}\" {command}\n"
    )
}

/// Escapa o que as aspas duplas do `sh` interpretam (POSIX): `\`, `"`, `$` e crase.
fn double_quoted(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            quoted.push('\\');
        }
        quoted.push(c);
    }

    quoted
}

/// O hook falha fechado se o binário some; um binário dentro de uma worktree ligada some com ela (`wt remove`); a principal não é removida.
fn unstable_binary_code(binary: &Path, worktrees: &[PathBuf]) -> Option<&'static str> {
    if in_build_dir(binary) {
        return Some("binary-in-build-dir");
    }

    worktrees
        .iter()
        .any(|root| binary.starts_with(root))
        .then_some("binary-in-worktree")
}

fn in_build_dir(binary: &Path) -> bool {
    let components: Vec<Component<'_>> = binary.components().collect();

    components.windows(2).any(|pair| {
        pair[0].as_os_str() == "target"
            && (pair[1].as_os_str() == "debug" || pair[1].as_os_str() == "release")
    })
}

#[cfg(test)]
mod tests;
