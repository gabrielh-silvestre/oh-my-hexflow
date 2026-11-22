use std::path::{Component, Path, PathBuf};

use super::names::{AMENDMENT_PREFIX, FILE_SUFFIX, FREEZE_MARK, PART_PREFIX, work_item_of_folder};
use super::spec::amendment_number;

const FROZEN: &str = "spec-frozen";
const IN_LINKED_WORKTREE: &str = "spec-in-linked-worktree";
const FOLDER_UNKNOWN: &str = "spec-folder-unknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecPath {
    pub work_item: String,
    pub folder: String,
    pub file: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Write,
    Edit,
    MultiEdit,
}

impl Tool {
    /// Caixa exata: o Claude Code manda o nome da ferramenta como está.
    pub fn parse(name: &str) -> Option<Tool> {
        match name {
            "Write" => Some(Tool::Write),
            "Edit" => Some(Tool::Edit),
            "MultiEdit" => Some(Tool::MultiEdit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny { code: &'static str, reason: String },
}

pub struct Facts<'a> {
    pub has_branch: bool,
    /// Alguma pasta `wi-<id>__*` do work item tem a marca `.frozen`.
    pub marked: bool,
    pub linked_worktree: bool,
    pub folder_exists: bool,
    pub file_exists: bool,
    pub folder_files: &'a [String],
    pub main_folder: &'a Path,
}

/// A pasta tem forma fixa mesmo com `specs.dir` customizado, então sem ela o caminho nunca é de spec.
pub fn has_spec_folder_component(file_path: &Path) -> bool {
    file_path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| work_item_of_folder(name).is_some())
    })
}

/// Para `specs/../specs/wi-x__y/...` não escapar.
pub fn parse_spec_path(specs_dir: &str, file_path: &Path) -> Option<SpecPath> {
    let names = normalized_names(file_path)?;
    let dir = normalized_names(Path::new(specs_dir))?;
    if dir.is_empty() {
        return None;
    }

    names
        .windows(dir.len())
        .enumerate()
        .filter(|(_, window)| *window == dir.as_slice())
        .find_map(|(at, _)| {
            let [folder, file @ ..] = &names[at + dir.len()..] else {
                return None;
            };
            let work_item = work_item_of_folder(folder)?;

            (!file.is_empty()).then(|| SpecPath {
                work_item: work_item.to_owned(),
                folder: (*folder).to_owned(),
                file: file.join("/"),
            })
        })
}

pub fn next_amendment(folder_files: &[String]) -> String {
    let highest = folder_files
        .iter()
        .filter_map(|name| amendment_number(name))
        .max()
        .unwrap_or(0);

    format!("{AMENDMENT_PREFIX}{:02}{FILE_SUFFIX}", highest + 1)
}

/// Ordem: worktree ligada; sem branch e sem marca libera; congelada nega qualquer arquivo em pasta que não existe; congelada só admite `Write` de arquivo novo que não seja `parte-*` nem `emenda-*` fora de ordem.
pub fn decide(tool: Tool, spec: &SpecPath, facts: &Facts) -> Verdict {
    if facts.linked_worktree {
        return Verdict::Deny {
            code: IN_LINKED_WORKTREE,
            reason: format!(
                "{IN_LINKED_WORKTREE}: specs live in the main directory, not in a linked worktree; write it under {}",
                facts.main_folder.display()
            ),
        };
    }
    if !(facts.has_branch || facts.marked) {
        return Verdict::Allow;
    }
    if !facts.folder_exists {
        return Verdict::Deny {
            code: FOLDER_UNKNOWN,
            reason: format!(
                "{FOLDER_UNKNOWN}: the work item is frozen (it has a branch or the {FREEZE_MARK} mark, or the mark could not be read) and {} does not exist; spec files are only written in the existing spec folder of the work item",
                facts.main_folder.display()
            ),
        };
    }
    if creates_allowed_file(tool, spec, facts) {
        return Verdict::Allow;
    }
    let amendment: PathBuf = facts.main_folder.join(next_amendment(facts.folder_files));

    Verdict::Deny {
        code: FROZEN,
        reason: format!(
            "{FROZEN}: the spec in {} is frozen because the work item has a branch or the {FREEZE_MARK} mark, or the mark could not be read; to correct it, record a divergence and create the amendment {} (Write of a new file)",
            facts.main_folder.display(),
            amendment.display()
        ),
    }
}

fn creates_allowed_file(tool: Tool, spec: &SpecPath, facts: &Facts) -> bool {
    if tool != Tool::Write || facts.file_exists {
        return false;
    }

    !is_spec_family(spec) || spec.file == next_amendment(facts.folder_files)
}

fn is_spec_family(spec: &SpecPath) -> bool {
    spec.file.starts_with(PART_PREFIX) || spec.file.starts_with(AMENDMENT_PREFIX)
}

fn normalized_names(path: &Path) -> Option<Vec<&str>> {
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => names.push(name.to_str()?),
            Component::ParentDir => {
                names.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
        }
    }

    Some(names)
}

#[cfg(test)]
mod tests;
