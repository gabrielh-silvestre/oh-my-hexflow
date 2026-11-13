use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::project::layout::{GITIGNORE_CONTENT, GITIGNORE_NAME};

/// Teto próprio da busca por prefixo; o `DIR_ENTRIES_MAX` (100) é das pastas de extension.
const SPEC_ENTRIES_MAX: usize = 10_000;
const FIRST_PART_FILE: &str = "parte-01.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStoreError {
    /// Já existe uma pasta `wi-<id>__*`.
    Exists,
    /// O motivo nunca traz nome de arquivo nem conteúdo.
    Unusable(&'static str),
}

/// As pastas `wi-<id>__<slug>` de um diretório de specs (`specs.dir` sob o diretório principal).
pub struct SpecStore {
    dir: PathBuf,
}

impl SpecStore {
    pub fn new(dir: PathBuf) -> SpecStore {
        SpecStore { dir }
    }

    /// Diretório ausente não tem pasta de spec. Passar do teto é recusa, nunca omissão silenciosa.
    pub fn has_spec(&self, work_item: &str) -> Result<bool, SpecStoreError> {
        let prefix = folder_prefix(work_item);
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(SpecStoreError::Unusable("the specs folder cannot be read")),
        };
        for (count, entry) in entries.enumerate() {
            if count >= SPEC_ENTRIES_MAX {
                return Err(SpecStoreError::Unusable(
                    "the specs folder has too many entries",
                ));
            }
            let entry =
                entry.map_err(|_| SpecStoreError::Unusable("the specs folder cannot be read"))?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                return Ok(true);
            }
        }

        Ok(false)
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
