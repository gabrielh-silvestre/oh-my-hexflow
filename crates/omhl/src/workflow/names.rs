//! Gramática única da pasta de spec (`wi-<id>__<slug>`) e dos arquivos dentro dela.

use hexlog::Name;

pub const FOLDER_PREFIX: &str = "wi-";
pub const FOLDER_SEPARATOR: &str = "__";
pub const ENGLISH_PART_PREFIX: &str = "part-";
pub const ENGLISH_AMENDMENT_PREFIX: &str = "amendment-";
pub const PORTUGUESE_PART_PREFIX: &str = "parte-";
pub const PORTUGUESE_AMENDMENT_PREFIX: &str = "emenda-";
const PART_PREFIXES: [&str; 2] = [ENGLISH_PART_PREFIX, PORTUGUESE_PART_PREFIX];
const AMENDMENT_PREFIXES: [&str; 2] = [ENGLISH_AMENDMENT_PREFIX, PORTUGUESE_AMENDMENT_PREFIX];
pub const FILE_SUFFIX: &str = ".md";
pub const FIRST_PART_FILE: &str = "part-01.md";
/// Arquivo vazio na pasta de spec que a congela para sempre, mesmo sem branch do work item.
pub const FREEZE_MARK: &str = ".frozen";
pub const WORK_ITEM_ID_PREFIX: &str = "work-items.";

pub fn folder_prefix(work_item: &str) -> String {
    format!("{FOLDER_PREFIX}{work_item}{FOLDER_SEPARATOR}")
}

pub fn work_item_of_folder(folder: &str) -> Option<&str> {
    let (id, slug) = folder
        .strip_prefix(FOLDER_PREFIX)?
        .split_once(FOLDER_SEPARATOR)?;
    Name::parse(id).ok()?;

    (!slug.is_empty()).then_some(id)
}

pub fn is_part_name(name: &str) -> bool {
    PART_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

pub fn is_amendment_name(name: &str) -> bool {
    strip_amendment_prefix(name).is_some()
}

pub fn strip_amendment_prefix(name: &str) -> Option<&str> {
    AMENDMENT_PREFIXES
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
}
