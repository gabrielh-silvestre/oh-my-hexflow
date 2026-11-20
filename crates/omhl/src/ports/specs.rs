use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::ports::files::{FILE_TOO_LARGE, list_sorted_capped, read_regular_utf8};
use crate::project::layout::{EXT_FILE_MAX_BYTES, GITIGNORE_CONTENT, GITIGNORE_NAME};
use crate::workflow::names::{
    AMENDMENT_PREFIX, FILE_SUFFIX, FIRST_PART_FILE, FREEZE_MARK, PART_PREFIX, folder_prefix,
};

/// Teto próprio da busca por prefixo; o `DIR_ENTRIES_MAX` (100) é das pastas de extension.
const SPEC_ENTRIES_MAX: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStoreError {
    /// Já existe uma pasta `wi-<id>__*`.
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

/// Um arquivo `parte-*.md` ou `emenda-*.md` da pasta de spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecDocument {
    pub name: String,
    /// Vazio quando `oversized`.
    pub text: String,
    /// Passa de `EXT_FILE_MAX_BYTES`: o arquivo não é lido, a validação reporta `file-too-large`.
    pub oversized: bool,
}

/// O conteúdo de uma pasta de spec, cada lista ordenada pelo nome do arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFolder {
    pub parts: Vec<SpecDocument>,
    pub amendments: Vec<SpecDocument>,
}

/// As pastas `wi-<id>__<slug>` de um diretório de specs (`specs.dir` sob o diretório principal).
pub struct SpecStore {
    dir: PathBuf,
}

impl SpecStore {
    pub fn new(dir: PathBuf) -> SpecStore {
        SpecStore { dir }
    }

    /// Diretório ausente não tem pasta de spec.
    pub fn has_spec(&self, work_item: &str) -> Result<bool, SpecStoreError> {
        Ok(self.find_folder(work_item)?.is_some())
    }

    /// Lê as partes e as emendas da pasta do work item; qualquer outro arquivo é ignorado.
    pub fn read_folder(&self, work_item: &str) -> Result<Option<SpecFolder>, SpecStoreError> {
        let Some(folder) = self.find_folder(work_item)? else {
            return Ok(None);
        };
        let mut spec = SpecFolder {
            parts: Vec::new(),
            amendments: Vec::new(),
        };
        for name in list_names(&folder)?
            .into_iter()
            .filter(|name| name.ends_with(FILE_SUFFIX))
        {
            let target = if name.starts_with(PART_PREFIX) {
                &mut spec.parts
            } else if name.starts_with(AMENDMENT_PREFIX) {
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
            target.push(SpecDocument {
                name,
                text,
                oversized,
            });
        }

        Ok(Some(spec))
    }

    /// Os `emenda-NN.md` da pasta do work item, só pelo nome: o conteúdo não é lido.
    pub fn amendment_names(&self, work_item: &str) -> Result<Vec<String>, SpecStoreError> {
        let Some(folder) = self.find_folder(work_item)? else {
            return Ok(Vec::new());
        };

        Ok(list_names(&folder)?
            .into_iter()
            .filter(|name| name.starts_with(AMENDMENT_PREFIX) && name.ends_with(FILE_SUFFIX))
            .collect())
    }

    /// Nomes dos arquivos da pasta `folder`; pasta ausente lista vazio.
    pub fn folder_files(&self, folder: &str) -> Result<Vec<String>, SpecStoreError> {
        list_names(&self.dir.join(folder))
    }

    /// Não segue link simbólico: um link pendente também ocupa o nome.
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

    /// A marca em qualquer pasta do work item; não segue link: uma marca que é link também congela.
    pub fn has_freeze_mark(&self, work_item: &str) -> Result<bool, SpecStoreError> {
        for folder in self.folders_of(work_item)? {
            if exists_without_following(&self.dir.join(folder).join(FREEZE_MARK))? {
                return Ok(true);
            }
        }

        Ok(false)
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

    /// Cria o `.gitignore` do diretório (sem sobrescrever o que existe), a pasta e a primeira parte; devolve a pasta e o arquivo.
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

/// Em ordem estável. Passar do teto é recusa, nunca omissão silenciosa. Pasta ausente lista vazio.
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
