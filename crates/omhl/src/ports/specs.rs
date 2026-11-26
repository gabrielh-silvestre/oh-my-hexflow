use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::ports::files::{FILE_TOO_LARGE, list_sorted_capped, read_regular_utf8};
use crate::project::layout::{EXT_FILE_MAX_BYTES, GITIGNORE_CONTENT, GITIGNORE_NAME};
use crate::workflow::names::{
    FILE_SUFFIX, FIRST_PART_FILE, FREEZE_MARK, folder_prefix, is_amendment_name, is_part_name,
    work_item_of_folder,
};

/// Teto próprio da busca por prefixo.
const SPEC_ENTRIES_MAX: usize = 10_000;

/// Teto agregado: a leitura carrega todos os arquivos da pasta de spec em memória.
const SPEC_FOLDER_BYTES_MAX: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStoreError {
    Exists,
    /// O motivo nunca traz nome de arquivo nem conteúdo.
    Unusable(&'static str),
}

/// `AlreadyMarked` vale para qualquer coisa que ocupe o nome da marca: arquivo, link, diretório ou FIFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreezeMarkWrite {
    Written,
    AlreadyMarked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecDocument {
    pub name: String,
    pub text: String,
    pub oversized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFolder {
    pub parts: Vec<SpecDocument>,
    pub amendments: Vec<SpecDocument>,
}

pub struct SpecStore {
    dir: PathBuf,
}

impl SpecStore {
    pub fn new(dir: PathBuf) -> SpecStore {
        SpecStore { dir }
    }

    pub fn has_spec(&self, work_item: &str) -> Result<bool, SpecStoreError> {
        Ok(self.find_folder(work_item)?.is_some())
    }

    pub fn read_folder(&self, work_item: &str) -> Result<Option<SpecFolder>, SpecStoreError> {
        let Some(folder) = self.find_folder(work_item)? else {
            return Ok(None);
        };
        let mut spec = SpecFolder {
            parts: Vec::new(),
            amendments: Vec::new(),
        };
        let mut total_bytes = 0;
        for name in list_names(&folder)?
            .into_iter()
            .filter(|name| name.ends_with(FILE_SUFFIX))
        {
            let target = if is_part_name(&name) {
                &mut spec.parts
            } else if is_amendment_name(&name) {
                &mut spec.amendments
            } else {
                continue;
            };
            let (text, oversized) = match read_regular_utf8(&folder.join(&name), EXT_FILE_MAX_BYTES)
            {
                Ok(text) => (text, false),
                Err(FILE_TOO_LARGE) => (String::new(), true),
                Err(_) => return Err(SpecStoreError::Unusable("a spec file cannot be read")),
            };
            total_bytes += text.len();
            if total_bytes > SPEC_FOLDER_BYTES_MAX {
                return Err(SpecStoreError::Unusable("the spec folder is too large"));
            }
            target.push(SpecDocument {
                name,
                text,
                oversized,
            });
        }

        Ok(Some(spec))
    }

    pub fn amendment_names(&self, work_item: &str) -> Result<Vec<String>, SpecStoreError> {
        let Some(folder) = self.find_folder(work_item)? else {
            return Ok(Vec::new());
        };

        Ok(list_names(&folder)?
            .into_iter()
            .filter(|name| is_amendment_name(name) && name.ends_with(FILE_SUFFIX))
            .collect())
    }

    pub fn folder_files(&self, folder: &str) -> Result<Vec<String>, SpecStoreError> {
        list_names(&self.dir.join(folder))
    }

    pub fn file_exists(&self, folder: &str, file: &str) -> Result<bool, SpecStoreError> {
        exists_without_following(&self.dir.join(folder).join(file))
    }

    pub fn folder_exists(&self, folder: &str) -> Result<bool, SpecStoreError> {
        exists_without_following(&self.dir.join(folder))
    }

    /// Todas as pastas `wi-<id>__*` do work item, sem recusar a segunda: quem lê a marca precisa ver todas.
    pub fn folders_of(&self, work_item: &str) -> Result<Vec<String>, SpecStoreError> {
        let prefix = folder_prefix(work_item);

        Ok(list_names(&self.dir)?
            .into_iter()
            .filter(|name| name.starts_with(&prefix))
            .collect())
    }

    /// As pastas do work item que podem receber a marca: o que não é pasta nem link (arquivo comum,
    /// FIFO) fica de fora; o link entra porque `write_freeze_mark` o recusa com erro.
    pub fn markable_folders(&self, work_item: &str) -> Result<Vec<String>, SpecStoreError> {
        Ok(self
            .folders_of(work_item)?
            .into_iter()
            .filter(|folder| {
                fs::symlink_metadata(self.dir.join(folder))
                    .map_or(true, |metadata| metadata.is_dir() || metadata.is_symlink())
            })
            .collect())
    }

    /// A marca em qualquer pasta do work item; não segue link: uma marca que é link também congela.
    /// Na dúvida congela: quem chega aqui já tem um caminho reconhecido como spec do work item, então falha ao listar
    /// ou ao ler a marca nunca libera a escrita.
    pub fn has_freeze_mark(&self, work_item: &str) -> bool {
        let Ok(folders) = self.folders_of(work_item) else {
            return true;
        };

        folders.iter().any(|folder| self.is_marked(folder))
    }

    /// O primeiro work item com a marca em alguma pasta `wi-*` da pasta de specs (que pode não existir); a regra de
    /// `has_freeze_mark` vale por pasta. Listar a pasta com erro devolve `Err`: quem pergunta nega, não congela por palpite.
    pub fn frozen_work_item(&self) -> Result<Option<String>, SpecStoreError> {
        Ok(list_names(&self.dir)?.into_iter().find_map(|folder| {
            let work_item = work_item_of_folder(&folder)?.to_owned();

            self.is_marked(&folder).then_some(work_item)
        }))
    }

    fn is_marked(&self, folder: &str) -> bool {
        match fs::symlink_metadata(self.dir.join(folder).join(FREEZE_MARK)) {
            Ok(_) => true,
            Err(error) => !matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory),
        }
    }

    /// A única gravação da marca. Pasta que é link é recusada (o link é a própria pasta); `create_new`
    /// não segue link no último componente.
    pub fn write_freeze_mark(&self, folder: &str) -> Result<FreezeMarkWrite, SpecStoreError> {
        let folder = self.dir.join(folder);
        if fs::symlink_metadata(&folder).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err(SpecStoreError::Unusable("the spec folder is a symlink"));
        }

        match write_new(&folder.join(FREEZE_MARK), "") {
            Ok(()) => Ok(FreezeMarkWrite::Written),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                Ok(FreezeMarkWrite::AlreadyMarked)
            }
            Err(_) => Err(SpecStoreError::Unusable(
                "the freeze mark cannot be written",
            )),
        }
    }

    /// Mais de uma pasta `wi-<id>__*` é recusa: escolher uma deixaria a outra ser lida ou ignorada ao acaso.
    fn find_folder(&self, work_item: &str) -> Result<Option<PathBuf>, SpecStoreError> {
        let prefix = folder_prefix(work_item);
        let mut matches = list_names(&self.dir)?
            .into_iter()
            .filter(|name| name.starts_with(&prefix));
        let first = matches.next();
        if matches.next().is_some() {
            return Err(SpecStoreError::Unusable(
                "the work item has more than one spec folder",
            ));
        }

        Ok(first.map(|name| self.dir.join(name)))
    }

    // shortcut: dois `init` simultâneos com slugs diferentes passam ambos pela busca; trocar por um lock se aparecer.
    pub fn create_first_part(
        &self,
        work_item: &str,
        slug: &str,
        content: &str,
    ) -> Result<(PathBuf, PathBuf), SpecStoreError> {
        let unusable = |_| SpecStoreError::Unusable("the specs folder cannot be written");
        fs::create_dir_all(&self.dir).map_err(unusable)?;
        write_new(&self.dir.join(GITIGNORE_NAME), GITIGNORE_CONTENT).or_else(
            |error| match error.kind() {
                ErrorKind::AlreadyExists => Ok(()),
                _ => Err(unusable(error)),
            },
        )?;
        let folder = self.dir.join(format!("{}{slug}", folder_prefix(work_item)));
        fs::create_dir(&folder).map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists => SpecStoreError::Exists,
            _ => unusable(error),
        })?;
        let part = folder.join(FIRST_PART_FILE);
        if let Err(error) = write_new(&part, content) {
            let _ = fs::remove_dir(&folder);

            return Err(unusable(error));
        }

        Ok((folder, part))
    }
}

fn list_names(dir: &Path) -> Result<Vec<String>, SpecStoreError> {
    if matches!(fs::metadata(dir), Err(error) if error.kind() == ErrorKind::NotFound) {
        return Ok(Vec::new());
    }
    let names = list_sorted_capped(dir, SPEC_ENTRIES_MAX).map_err(|code| match code {
        "too-many-entries" => SpecStoreError::Unusable("the specs folder has too many entries"),
        _ => SpecStoreError::Unusable("the specs folder cannot be read"),
    })?;

    Ok(names
        .into_iter()
        .map(|name| name.to_string_lossy().into_owned())
        .collect())
}

fn exists_without_following(path: &Path) -> Result<bool, SpecStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(_) => Err(SpecStoreError::Unusable(
            "the spec file cannot be inspected",
        )),
    }
}

fn write_new(path: &Path, content: &str) -> std::io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(content.as_bytes())
}

#[cfg(test)]
mod tests;
