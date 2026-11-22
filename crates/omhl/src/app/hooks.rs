use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

use super::engine::{Engine, require_project_root};
use super::error::AppError;
use crate::ports::hooks::{GitHooks, HookInstaller};

const SCRIPTS: [(&str, &str); 4] = [
    ("pre-commit", "check run --event before-commit"),
    ("pre-merge-commit", "check run --event before-commit"),
    ("pre-push", "check run --event before-pr --pre-push"),
    ("post-commit", "hooks post-commit >/dev/null"),
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

fn install(
    git: &dyn HookInstaller,
    hooks_dir: Option<&Path>,
    force: bool,
) -> Result<Value, AppError> {
    let probe_failed = |reason: String| AppError::probe_failed(&reason);
    let current = git.resolved_hooks_dir().map_err(probe_failed)?;
    let hooks_text = hooks_dir.map(Path::to_string_lossy);
    if !force {
        let configured = git.configured_hooks_path().map_err(probe_failed)?;
        match configured {
            Some(path) if Some(path.as_str()) != hooks_text.as_deref() => {
                return Err(AppError::hooks_path_conflict(""));
            }
            None if git.has_foreign_hooks(&current) => return Err(AppError::hooks_existing("")),
            _ => {}
        }
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

fn script(binary: &str, command: &str) -> String {
    let p = double_quoted(binary);

    format!(
        "#!/bin/sh\n[ -x \"{p}\" ] || {{ echo \"omhl: binary not found at {p}; run omhl hooks install again\" >&2; exit 1; }}; exec \"{p}\" {command}\n"
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
