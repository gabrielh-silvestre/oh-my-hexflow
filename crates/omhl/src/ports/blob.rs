use std::fmt::Write as _;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Read, Write as _};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use hexlog::TenantId;
use sha2::{Digest, Sha256};

use crate::byproduct::{ByproductError, ByproductErrorKind, ByproductHash, ByproductStore};
use crate::ports::files::{OpenError, open_without_following_or_blocking};

const TEMP_ATTEMPTS: u32 = 16;

pub struct BlobStore {
    /// `blobs/by-db/<arquivo do banco>/<tenant>`: onde `put` grava e onde a poda varre.
    root: PathBuf,
    /// `blobs/`: o layout antigo, só lido (`get`) e nunca podado.
    legacy_root: PathBuf,
}

impl BlobStore {
    /// O nome do arquivo do banco separa dois bancos na mesma pasta; `prepare_db_file` já recusa o caminho sem nome de arquivo.
    pub fn new(db_path: &Path, tenant: &TenantId) -> BlobStore {
        let legacy_root = db_path.parent().unwrap_or(Path::new("")).join("blobs");
        let root = legacy_root
            .join("by-db")
            .join(db_path.file_name().unwrap_or_default())
            .join(tenant.as_str());

        BlobStore { root, legacy_root }
    }
}

fn blob_dir(root: &Path, hash: &ByproductHash) -> PathBuf {
    root.join(&hash.as_str()[..2])
}

impl ByproductStore for BlobStore {
    fn put(&self, bytes: &[u8]) -> Result<ByproductHash, ByproductError> {
        let hash = sha256_hash(bytes)?;
        let dir = blob_dir(&self.root, &hash);
        // Modo 0700 em `blobs/`, `blobs/by-db/<banco>/<tenant>` e `<xx>`: a saída pode ter segredos (R8).
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(|error| io_error(&error))?;
        let target = dir.join(hash.as_str());
        // Blob existente com bytes diferentes (truncado, corrompido ou que não é arquivo regular) é regravado: o `rename` o substitui.
        if matches!(read_stored(&target), Ok(Stored::Bytes(existing)) if sha256_hash(&existing).is_ok_and(|actual| actual == hash))
        {
            return Ok(hash);
        }
        // Temporário de outra execução fica como arquivo sem uso: nunca é apagado.
        for attempt in 0..TEMP_ATTEMPTS {
            let temp = dir.join(format!(
                "{}.tmp-{}-{attempt}",
                hash.as_str(),
                std::process::id()
            ));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_error(&error)),
            };
            file.write_all(bytes).map_err(|error| io_error(&error))?;
            file.sync_all().map_err(|error| io_error(&error))?;
            fs::rename(&temp, &target).map_err(|error| io_error(&error))?;
            return Ok(hash);
        }
        Err(ByproductError {
            kind: ByproductErrorKind::Io,
            message: format!("no free temporary file name for blob {}", hash.as_str()),
        })
    }

    fn get(&self, hash: &ByproductHash) -> Result<Option<Vec<u8>>, ByproductError> {
        let stored = match read_stored(&blob_dir(&self.root, hash).join(hash.as_str())) {
            // Ausente na pasta nova cai no layout antigo; presente e corrompido não cai.
            Ok(Stored::Missing) => {
                read_stored(&blob_dir(&self.legacy_root, hash).join(hash.as_str()))
            }
            other => other,
        };
        let bytes = match stored {
            Ok(Stored::Bytes(bytes)) => bytes,
            Ok(Stored::Missing) => return Ok(None),
            Ok(Stored::NotRegular) => {
                return Err(corrupt(format!(
                    "blob {} is not a regular file",
                    hash.as_str()
                )));
            }
            Err(error) => return Err(io_error(&error)),
        };
        if sha256_hash(&bytes)? != *hash {
            return Err(corrupt(format!(
                "blob bytes do not match hash {}",
                hash.as_str()
            )));
        }
        Ok(Some(bytes))
    }
}

enum Stored {
    Missing,
    NotRegular,
    Bytes(Vec<u8>),
}

/// Lê o blob sem seguir link e sem travar num FIFO sem escritor (o que não é arquivo regular volta como `NotRegular`).
fn read_stored(path: &Path) -> io::Result<Stored> {
    let mut file = match open_without_following_or_blocking(path) {
        Ok(file) => file,
        Err(OpenError::NotFound) => return Ok(Stored::Missing),
        Err(OpenError::Symlink) => return Ok(Stored::NotRegular),
        Err(OpenError::Io(error)) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Ok(Stored::NotRegular);
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    Ok(Stored::Bytes(bytes))
}

fn corrupt(message: String) -> ByproductError {
    ByproductError {
        kind: ByproductErrorKind::Corrupt,
        message,
    }
}

fn sha256_hash(bytes: &[u8]) -> Result<ByproductHash, ByproductError> {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes).iter() {
        // Escrever em `String` não falha.
        let _ = write!(hex, "{byte:02x}");
    }
    ByproductHash::parse(&hex)
}

fn io_error(error: &io::Error) -> ByproductError {
    ByproductError {
        kind: ByproductErrorKind::Io,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
