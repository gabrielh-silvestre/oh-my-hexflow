use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::ports::files::read_regular_utf8;
use crate::project::layout::{EXT_FILE_MAX_BYTES, GITIGNORE_CONTENT, GITIGNORE_NAME};

/// Teto próprio da busca por prefixo; o `DIR_ENTRIES_MAX` (100) é das pastas de extension.
const SPEC_ENTRIES_MAX: usize = 10_000;
const FIRST_PART_FILE: &str = "parte-01.md";
const PART_PREFIX: &str = "parte-";
const AMENDMENT_PREFIX: &str = "emenda-";
const FILE_SUFFIX: &str = ".md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStoreError {
    /// Já existe uma pasta `wi-<id>__*`.
    Exists,
    /// O motivo nunca traz nome de arquivo nem conteúdo.
    Unusable(&'static str),
}

/// Um arquivo `parte-*.md` ou `emenda-*.md` da pasta de spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecDocument {
    pub name: String,
    pub text: String,
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
        let mut names = list_names(&folder)?;
        names.sort();
        let mut spec = SpecFolder {
            parts: Vec::new(),
            amendments: Vec::new(),
        };
        for name in names.into_iter().filter(|name| name.ends_with(FILE_SUFFIX)) {
            let target = if name.starts_with(PART_PREFIX) {
                &mut spec.parts
            } else if name.starts_with(AMENDMENT_PREFIX) {
                &mut spec.amendments
            } else {
                continue;
            };
            let text = read_regular_utf8(&folder.join(&name), EXT_FILE_MAX_BYTES)
                .map_err(|_| SpecStoreError::Unusable("a spec file cannot be read"))?;
            target.push(SpecDocument { name, text });
        }

        Ok(Some(spec))
    }

    fn find_folder(&self, work_item: &str) -> Result<Option<PathBuf>, SpecStoreError> {
        let prefix = folder_prefix(work_item);

        Ok(list_names(&self.dir)?
            .into_iter()
            .find(|name| name.starts_with(&prefix))
            .map(|name| self.dir.join(name)))
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

/// Passar do teto é recusa, nunca omissão silenciosa. Pasta ausente lista vazio.
fn list_names(dir: &Path) -> Result<Vec<String>, SpecStoreError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(SpecStoreError::Unusable("the specs folder cannot be read")),
    };
    let mut names = Vec::new();
    for entry in entries {
        if names.len() >= SPEC_ENTRIES_MAX {
            return Err(SpecStoreError::Unusable(
                "the specs folder has too many entries",
            ));
        }
        let entry =
            entry.map_err(|_| SpecStoreError::Unusable("the specs folder cannot be read"))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }

    Ok(names)
}

fn folder_prefix(work_item: &str) -> String {
    format!("wi-{work_item}__")
}

fn write_new(path: &Path, content: &str) -> std::io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(content.as_bytes())
}
