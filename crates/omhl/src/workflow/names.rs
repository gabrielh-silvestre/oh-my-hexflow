//! Gramática única da pasta de spec (`wi-<id>__<slug>`) e dos arquivos dentro dela.

use hexlog::Name;

pub const FOLDER_PREFIX: &str = "wi-";
pub const FOLDER_SEPARATOR: &str = "__";
pub const PART_PREFIX: &str = "parte-";
pub const AMENDMENT_PREFIX: &str = "emenda-";
pub const FILE_SUFFIX: &str = ".md";
pub const FIRST_PART_FILE: &str = "parte-01.md";
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
