//! Garante que a pasta das worktrees ligadas não apareça como não rastreada no diretório principal.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path};

use super::error::AppError;
use crate::ports::git::GitProbe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeOutcome {
    Written,
    AlreadyIgnored,
    NoLinkedWorktree,
    OutsideMainDir,
}

pub(crate) fn ensure_worktree_exclude(
    main_dir: &Path,
    common_dir: &Path,
    dir: &Path,
) -> Result<ExcludeOutcome, AppError> {
    let folder = main_dir.join(dir);
    let Some(relative) = relative_folder(main_dir, &folder) else {
        return Ok(ExcludeOutcome::OutsideMainDir);
    };
    let probe = GitProbe::new(main_dir);
    let worktrees = probe
        .linked_worktrees()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    if !worktrees.iter().any(|root| root.starts_with(&folder)) {
        return Ok(ExcludeOutcome::NoLinkedWorktree);
    }
    // Sem a barra no fim o `check-ignore` sai 1 para uma pasta ausente mesmo com a linha presente.
    if probe
        .is_ignored(&format!("{relative}/"))
        .map_err(|_| AppError::worktree_exclude_unreadable())?
    {
        return Ok(ExcludeOutcome::AlreadyIgnored);
    }
    append_line(common_dir, &format!("/{relative}/"))?;

    Ok(ExcludeOutcome::Written)
}

/// A pasta como o `exclude` a escreve: relativa ao diretório principal, com `/`, nunca ele mesmo.
fn relative_folder(main_dir: &Path, folder: &Path) -> Option<String> {
    let names: Vec<&str> = folder
        .strip_prefix(main_dir)
        .ok()?
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str(),
            Component::CurDir => Some(""),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect();
    (!names.is_empty()).then(|| names.join("/"))
}

fn append_line(common_dir: &Path, line: &str) -> Result<(), AppError> {
    let info = common_dir.join("info");
    let unwritable = |_| AppError::probe_failed("info/exclude could not be written");
    fs::create_dir_all(&info).map_err(unwritable)?;
    let path = info.join("exclude");
    let ends_open = match fs::read(&path) {
        Ok(bytes) => bytes.last().is_some_and(|byte| *byte != b'\n'),
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(unwritable(error)),
    };
    let text = if ends_open {
        format!("\n{line}\n")
    } else {
        format!("{line}\n")
    };
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(unwritable)
}

#[cfg(test)]
mod tests;
