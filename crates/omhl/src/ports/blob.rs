use std::fmt::Write as _;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::byproduct::{ByproductError, ByproductErrorKind, ByproductHash, ByproductStore};

const TEMP_ATTEMPTS: u32 = 16;

pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn new(db_dir: &Path) -> BlobStore {
        BlobStore {
            root: db_dir.join("blobs"),
        }
    }

    fn blob_dir(&self, hash: &ByproductHash) -> PathBuf {
        self.root.join(&hash.as_str()[..2])
    }
}

impl ByproductStore for BlobStore {
    fn put(&self, bytes: &[u8]) -> Result<ByproductHash, ByproductError> {
        let hash = sha256_hash(bytes)?;
        let dir = self.blob_dir(&hash);
        // Modo 0700 em `blobs/` e `blobs/<xx>`: a saída pode ter segredos (R8).
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(|error| io_error(&error))?;
        let target = dir.join(hash.as_str());
        // Blob existente com bytes diferentes (truncado ou corrompido) é regravado: o `rename` o substitui.
        if fs::read(&target)
            .is_ok_and(|existing| sha256_hash(&existing).is_ok_and(|actual| actual == hash))
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
        let bytes = match fs::read(self.blob_dir(hash).join(hash.as_str())) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(&error)),
        };
        if sha256_hash(&bytes)? != *hash {
            return Err(ByproductError {
                kind: ByproductErrorKind::Corrupt,
                message: format!("blob bytes do not match hash {}", hash.as_str()),
            });
        }
        Ok(Some(bytes))
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
