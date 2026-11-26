use std::path::{Component, Path, PathBuf};

use super::config::parse_workflow;
use super::names::{
    ENGLISH_AMENDMENT_PREFIX, ENGLISH_PART_PREFIX, FILE_SUFFIX, FREEZE_MARK,
    PORTUGUESE_AMENDMENT_PREFIX, PORTUGUESE_PART_PREFIX, is_amendment_name, is_part_name,
    work_item_of_folder,
};
use super::spec::amendment_number;
use crate::probe::GIT_TIMED_OUT;
use crate::project::layout::{DOT_GIT_DIR, EXT_FILE_MAX_BYTES, NOTES_DIR, OMHL_DIR, WORKFLOW_FILE};
use crate::project::marker::MARKER_FILE;

const FROZEN: &str = "spec-frozen";
const SPEC_DIR_FROZEN: &str = "spec-dir-frozen";
const GIT_DIR_PROTECTED: &str = "git-dir-protected";
const MARKER_PROTECTED: &str = "marker-protected";
const UNAVAILABLE: &str = "spec-lock-unavailable";
const TEXT_MAX_CHARS: usize = 200;
const FIX_CLOSING: &str = "the edit stays denied until this is fixed";
const RETRY_CLOSING: &str = "the edit stays denied; retry it";
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub old: String,
    pub new: String,
    pub replace_all: bool,
}

/// O que a ferramenta vai gravar, já lido do payload pelo adapter; `Unreadable` é campo ausente ou com tipo errado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditInput {
    Write { content: String },
    Edit(Replacement),
    MultiEdit(Vec<Replacement>),
    Unreadable,
}

/// Por que a decisão não pôde ser tomada depois de reconhecer a spec; montada no `app`, que mapeia o erro do git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    GitTimedOut,
    Git(String),
    /// Motivo fixo, sem nome de arquivo nem conteúdo.
    Read(&'static str),
}

/// Negação por falha interna depois do reconhecimento da spec: a edição fica negada em vez de liberada.
/// `subject` é o work item (`wi-<id>`) ou o caminho de quem não tem work item; tudo sai numa linha só.
pub fn unavailable(subject: &str, step: &str, cause: &Cause) -> Verdict {
    let (cause, closing) = match cause {
        Cause::GitTimedOut => (GIT_TIMED_OUT.to_owned(), RETRY_CLOSING),
        Cause::Git(message) if message.trim().is_empty() => {
            ("git failed without a message".to_owned(), FIX_CLOSING)
        }
        Cause::Git(message) => (one_line(message), FIX_CLOSING),
        Cause::Read(reason) => ((*reason).to_owned(), FIX_CLOSING),
    };

    Verdict::Deny {
        code: UNAVAILABLE,
        reason: format!(
            "{UNAVAILABLE}: the spec lock could not decide the edit for {}: {step} failed ({cause}); {closing}",
            one_line(subject)
        ),
    }
}

fn one_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(TEXT_MAX_CHARS)
        .collect()
}

/// O `specs.dir` efetivo de um `workflow.toml`: o que a política do hook usaria. Ausente (`None`), acima do teto, que não é UTF-8 ou que `parse_workflow` recusa vale o padrão.
pub fn specs_dir_of(bytes: Option<&[u8]>) -> String {
    bytes
        .filter(|bytes| bytes.len() as u64 <= EXT_FILE_MAX_BYTES)
        .and_then(|bytes| parse_workflow(Path::new(WORKFLOW_FILE), bytes).ok())
        .unwrap_or_default()
        .specs
        .dir
}

/// Os textos que o arquivo pode ter depois da edição, um por versão em que todas as trocas se aplicam.
///
/// `None` sem payload gravável ou sem versão que aceite a lista; testa com e sem a normalização de CRLF da ferramenta.
pub fn resulting_texts(input: &EditInput, current: Option<&str>) -> Option<Vec<String>> {
    let replacements = match input {
        EditInput::Write { content } => return Some(vec![content.clone()]),
        EditInput::Edit(replacement) => std::slice::from_ref(replacement),
        EditInput::MultiEdit(replacements) => replacements.as_slice(),
        EditInput::Unreadable => return None,
    };
    let current = current?;
    let normalized = to_lf(current);
    let texts: Vec<String> = [
        apply(current, replacements, false),
        apply(&normalized, replacements, false),
        apply(&normalized, replacements, true),
    ]
    .into_iter()
    .flatten()
    .collect();

    (!texts.is_empty()).then_some(texts)
}

fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Aplica as trocas na ordem; uma que não casa (ou com `old` vazio) invalida a versão inteira.
fn apply(text: &str, replacements: &[Replacement], normalize_strings: bool) -> Option<String> {
    replacements
        .iter()
        .try_fold(text.to_owned(), |text, replacement| {
            let (old, new) = if normalize_strings {
                (to_lf(&replacement.old), to_lf(&replacement.new))
            } else {
                (replacement.old.clone(), replacement.new.clone())
            };
            if old.is_empty() || !text.contains(&old) {
                return None;
            }

            Some(if replacement.replace_all {
                text.replace(&old, &new)
            } else {
                text.replacen(&old, &new, 1)
            })
        })
}

/// Os dois últimos nomes, com o `..` resolvido, são `.omhl` e `workflow.toml`.
pub fn is_workflow_candidate(file_path: &Path) -> bool {
    matches!(
        normalized_names(file_path).as_deref(),
        Some([.., OMHL_DIR, WORKFLOW_FILE])
    )
}

/// Algum nome normalizado abaixo da raiz é `.git`; sem a raiz como prefixo, contam todos os nomes.
/// Pura: não chama git, então vale com o git quebrado. Com a raiz `/`, contam todos.
pub fn has_git_component(file_path: &Path, root: &Path) -> bool {
    let Some(names) = normalized_names(file_path) else {
        return false;
    };
    let below_root = normalized_names(root)
        .and_then(|root| names.strip_prefix(root.as_slice()))
        .unwrap_or(&names);

    below_root.contains(&DOT_GIT_DIR)
}

/// O último nome normalizado é `.omhl.toml` e nenhum anterior é `.omhl`: um marcador sob `.omhl/` não é raiz de projeto.
pub fn is_marker_candidate(file_path: &Path) -> bool {
    matches!(
        normalized_names(file_path).as_deref(),
        Some([init @ .., MARKER_FILE]) if !init.contains(&OMHL_DIR)
    )
}

/// Um nome qualquer sob `.omhl/notes/` e nenhum `..` no caminho: o filtro barato, puro, sem git nem arquivo.
/// Que a pasta seja a da raiz do marcador só se sabe depois de achá-lo (`is_notes_path_of`).
pub fn is_notes_candidate(file_path: &Path) -> bool {
    !has_parent_dir(file_path)
        && normalized_names(file_path).is_some_and(|names| {
            names
                .windows(3)
                .any(|window| window[..2] == [OMHL_DIR, NOTES_DIR])
        })
}

/// O caminho começa exatamente em `<root>/.omhl/notes/<nome>` e não tem `..`.
pub fn is_notes_path_of(file_path: &Path, root: &Path) -> bool {
    !has_parent_dir(file_path)
        && normalized_names(file_path)
            .zip(normalized_names(root))
            .is_some_and(|(names, root)| {
                matches!(
                    names.strip_prefix(root.as_slice()),
                    Some([OMHL_DIR, NOTES_DIR, _, ..])
                )
            })
}

fn has_parent_dir(file_path: &Path) -> bool {
    file_path
        .components()
        .any(|component| component == Component::ParentDir)
}

/// A razão não sugere o `Bash`: quem a lê tem `Bash`, e o dono muda o `.git` fora do Claude Code.
pub fn git_dir_protected(file_path: &Path) -> Verdict {
    Verdict::Deny {
        code: GIT_DIR_PROTECTED,
        reason: format!(
            "{GIT_DIR_PROTECTED}: {} is under a .git folder of an omhl project, which the spec lock relies on; only the owner changes it, outside Claude Code: ask the owner and do not change it through any tool",
            one_line(&file_path.display().to_string())
        ),
    }
}

pub fn marker_protected(file_path: &Path) -> Verdict {
    Verdict::Deny {
        code: MARKER_PROTECTED,
        reason: format!(
            "{MARKER_PROTECTED}: this edit would leave {} without a valid project marker, which the spec lock relies on; keep it a valid marker or ask the owner, and do not change it through any tool",
            one_line(&file_path.display().to_string())
        ),
    }
}

/// Igualdade pelos nomes normalizados: `./a/../b` é `b`.
pub fn is_same_path(left: &Path, right: &Path) -> bool {
    matches!((normalized_names(left), normalized_names(right)), (Some(left), Some(right)) if left == right)
}

/// `work_item` é o `wi-<id>`; a razão não sugere o `Bash`: quem a lê tem `Bash`, e o dono muda a chave fora do Claude Code.
pub fn spec_dir_frozen(file_path: &Path, old: &str, new: &str, work_item: &str) -> Verdict {
    Verdict::Deny {
        code: SPEC_DIR_FROZEN,
        reason: format!(
            "{SPEC_DIR_FROZEN}: this edit would change specs.dir from \"{}\" to \"{}\" in {} while the spec of work item {} is frozen, which would release the spec lock; only the owner changes specs.dir, outside Claude Code: ask the owner and do not change it through any tool",
            one_line(old),
            one_line(new),
            one_line(&file_path.display().to_string()),
            one_line(work_item)
        ),
    }
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

    let has = |prefixes: [&str; 2]| {
        folder_files
            .iter()
            .any(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
    };
    // Pasta antiga só em português segue com `emenda-`; qualquer outra nasce em inglês.
    let prefix = if has([PORTUGUESE_PART_PREFIX, PORTUGUESE_AMENDMENT_PREFIX])
        && !has([ENGLISH_PART_PREFIX, ENGLISH_AMENDMENT_PREFIX])
    {
        PORTUGUESE_AMENDMENT_PREFIX
    } else {
        ENGLISH_AMENDMENT_PREFIX
    };

    format!("{prefix}{:02}{FILE_SUFFIX}", highest + 1)
}

/// Congelada, só passa o `Write` de arquivo novo, fora da família da spec ou exatamente a próxima emenda; o resto é negado para a spec aprovada nunca ser editada.
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
    if names_project_marker(spec) {
        return Verdict::Deny {
            code: FROZEN,
            reason: format!(
                "{FROZEN}: the spec in {} is frozen, and the project marker files ({MARKER_FILE} and {OMHL_DIR}/) are neither created nor edited in the spec folder of a frozen work item",
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

/// Um `.omhl.toml` plantado na pasta congelada soltaria a trava das specs do projeto.
fn names_project_marker(spec: &SpecPath) -> bool {
    spec.file
        .split('/')
        .any(|name| name == MARKER_FILE || name == OMHL_DIR)
}

fn creates_allowed_file(tool: Tool, spec: &SpecPath, facts: &Facts) -> bool {
    if tool != Tool::Write || facts.file_exists {
        return false;
    }

    !is_spec_family(spec) || spec.file == next_amendment(facts.folder_files)
}

fn is_spec_family(spec: &SpecPath) -> bool {
    is_part_name(&spec.file) || is_amendment_name(&spec.file)
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
