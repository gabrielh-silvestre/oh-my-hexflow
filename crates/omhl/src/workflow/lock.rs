use std::path::{Component, Path, PathBuf};

use hexlog::Name;

use super::spec::amendment_number;

const FOLDER_PREFIX: &str = "wi-";
const FOLDER_SEPARATOR: &str = "__";
const PART_PREFIX: &str = "parte-";
const AMENDMENT_PREFIX: &str = "emenda-";
const FILE_SUFFIX: &str = ".md";
const FROZEN: &str = "spec-frozen";
const IN_LINKED_WORKTREE: &str = "spec-in-linked-worktree";

/// Um arquivo sob `<specs.dir>/wi-<id>__<slug>/`; `file` é o restante do caminho depois da pasta.
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
    /// Existe branch local cujo work item é o do segmento da pasta.
    pub frozen: bool,
    /// O caminho está dentro de uma worktree ligada, não no diretório principal.
    pub linked_worktree: bool,
    pub file_exists: bool,
    /// Nomes dos arquivos da pasta de spec no diretório principal.
    pub folder_files: &'a [String],
    /// `<diretório principal>/<specs.dir>/<pasta>`.
    pub main_folder: &'a Path,
}

/// Só texto: `.` e `..` são resolvidos sem tocar o disco, para `specs/../specs/wi-x__y/...` não escapar.
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

/// Maior `emenda-NN.md` (dois dígitos) mais um; `emenda-01.md` quando não há nenhuma.
pub fn next_amendment(folder_files: &[String]) -> String {
    let highest = folder_files
        .iter()
        .filter_map(|name| amendment_number(name))
        .max()
        .unwrap_or(0);

    format!("{AMENDMENT_PREFIX}{:02}{FILE_SUFFIX}", highest + 1)
}

/// Ordem: worktree ligada; sem branch libera; congelada só admite `Write` de arquivo novo que não seja `parte-*` nem `emenda-*` fora de ordem.
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
    if !facts.frozen || creates_allowed_file(tool, spec, facts) {
        return Verdict::Allow;
    }
    let amendment: PathBuf = facts.main_folder.join(next_amendment(facts.folder_files));

    Verdict::Deny {
        code: FROZEN,
        reason: format!(
            "{FROZEN}: the spec in {} is frozen because the work item has a branch; to correct it, record a divergence and create the amendment {} (Write of a new file)",
            facts.main_folder.display(),
            amendment.display()
        ),
    }
}

fn creates_allowed_file(tool: Tool, spec: &SpecPath, facts: &Facts) -> bool {
    if tool != Tool::Write || facts.file_exists {
        return false;
    }
    let spec_family = spec.file.starts_with(PART_PREFIX) || spec.file.starts_with(AMENDMENT_PREFIX);

    !spec_family || spec.file == next_amendment(facts.folder_files)
}

/// Componentes `Normal` do caminho depois de resolver `.` e `..` no texto; `None` se algum não é UTF-8.
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

/// `wi-<id>__<slug>`: o `<id>` vai até o primeiro `__` e precisa ser nome válido; o slug não é vazio.
fn work_item_of_folder(folder: &str) -> Option<&str> {
    let (id, slug) = folder
        .strip_prefix(FOLDER_PREFIX)?
        .split_once(FOLDER_SEPARATOR)?;
    Name::parse(id).ok()?;

    (!slug.is_empty()).then_some(id)
}
