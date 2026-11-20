//! Gramática única da pasta de spec (`wi-<id>__<slug>`) e dos arquivos dentro dela. Um teste de
//! arquitetura proíbe os literais desta gramática fora deste módulo.

use hexlog::Name;

pub const FOLDER_PREFIX: &str = "wi-";
pub const FOLDER_SEPARATOR: &str = "__";
pub const PART_PREFIX: &str = "parte-";
pub const AMENDMENT_PREFIX: &str = "emenda-";
pub const FILE_SUFFIX: &str = ".md";
pub const FIRST_PART_FILE: &str = "parte-01.md";
/// Arquivo vazio na pasta de spec que a congela para sempre, mesmo sem branch do work item.
pub const FREEZE_MARK: &str = ".frozen";
/// Prefixo do id semântico de um work item; o que vem depois é o `<id>` da pasta.
pub const WORK_ITEM_ID_PREFIX: &str = "work-items.";

/// `wi-<id>__`: o início do nome da pasta de qualquer slug do work item.
pub fn folder_prefix(work_item: &str) -> String {
    format!("{FOLDER_PREFIX}{work_item}{FOLDER_SEPARATOR}")
}

/// `wi-<id>__<slug>`: o `<id>` vai até o primeiro `__` e precisa ser nome válido; o slug não é vazio.
pub fn work_item_of_folder(folder: &str) -> Option<&str> {
    let (id, slug) = folder
        .strip_prefix(FOLDER_PREFIX)?
        .split_once(FOLDER_SEPARATOR)?;
    Name::parse(id).ok()?;

    (!slug.is_empty()).then_some(id)
}
