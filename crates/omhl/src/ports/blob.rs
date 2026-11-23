use std::fmt::Write as _;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Read, Write as _};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use hexlog::TenantId;
use sha2::{Digest, Sha256};

use crate::byproduct::{
    ByproductError, ByproductErrorKind, ByproductHash, ByproductRetention, ByproductStore,
    StaleBlob,
};
use crate::ports::files::{OpenError, open_without_following_or_blocking};

const TEMP_ATTEMPTS: u32 = 16;
const TEMP_INFIX: &str = ".tmp-";
const PRUNE_STAMP: &str = ".pruned-at";

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
            // A data renovada segura o blob contra a poda de um `check run` concorrente.
            renew_modified(&target)?;
            return Ok(hash);
        }
        // Temporário de outra execução fica como arquivo sem uso: só a poda o apaga, depois da retenção.
        for attempt in 0..TEMP_ATTEMPTS {
            let temp = dir.join(format!(
                "{}{TEMP_INFIX}{}-{attempt}",
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

impl ByproductRetention for BlobStore {
    fn stale(&self, max_age: Duration) -> Result<Vec<StaleBlob>, ByproductError> {
        let mut stale = Vec::new();
        let shards = match fs::read_dir(&self.root) {
            Ok(shards) => shards,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(stale),
            Err(error) => return Err(io_error(&error)),
        };
        for shard in shards {
            let shard = shard.map_err(|error| io_error(&error))?;
            // `file_type` não segue link: um `<xx>` que aponta para fora nunca é varrido.
            if !is_shard(&shard).map_err(|error| io_error(&error))? {
                continue;
            }
            for entry in fs::read_dir(shard.path()).map_err(|error| io_error(&error))? {
                let entry = entry.map_err(|error| io_error(&error))?;
                let metadata = match entry.metadata() {
                    Ok(metadata) => metadata,
                    // Um `prune` concorrente já removeu o arquivo.
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(io_error(&error)),
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                let hash = blob_file_hash(&name);
                if let Some(hash) =
                    hash.filter(|_| metadata.is_file() && is_stale(&metadata, max_age))
                {
                    stale.push(StaleBlob {
                        hash,
                        name,
                        bytes: metadata.len(),
                    });
                }
            }
        }

        Ok(stale)
    }

    fn remove(&self, blob: &StaleBlob, max_age: Duration) -> Result<bool, ByproductError> {
        let path = blob_dir(&self.root, &blob.hash).join(&blob.name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error(&error)),
        };
        // Confere a data de novo: um `put` concorrente pode ter renovado o blob depois da listagem.
        if !metadata.is_file() || !is_stale(&metadata, max_age) {
            return Ok(false);
        }
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(io_error(&error)),
        }
    }

    fn claim_prune_slot(&self, min_interval: Duration) -> bool {
        let stamp = self.root.join(PRUNE_STAMP);
        // Sem a pasta do (banco, tenant) ainda não houve `put`: nada a podar e nada a criar.
        if !self.root.is_dir() {
            return false;
        }
        // Data no futuro (relógio que voltou) não segura a poda: o carimbo conta como vencido.
        let fresh = match fs::symlink_metadata(&stamp) {
            // Link ou FIFO plantado no lugar do carimbo: não é seguido nem aberto.
            Ok(metadata) if !metadata.is_file() => return false,
            Ok(metadata) => metadata
                .modified()
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_some_and(|age| age < min_interval),
            Err(_) => false,
        };
        if fresh {
            return false;
        }

        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&stamp)
            .and_then(|file| file.set_modified(SystemTime::now()))
            .is_ok()
    }
}

/// Dois hexadecimais minúsculos e pasta de verdade.
fn is_shard(entry: &fs::DirEntry) -> io::Result<bool> {
    let name = entry.file_name();
    let is_hex_pair = name
        .to_str()
        .is_some_and(|name| name.len() == 2 && name.bytes().all(is_lower_hex));

    Ok(is_hex_pair && entry.file_type()?.is_dir())
}

fn is_lower_hex(byte: u8) -> bool {
    matches!(byte, b'0'..=b'9' | b'a'..=b'f')
}

/// O hash de `<64 hex>` ou `<64 hex>.tmp-<pid>-<n>`; outro nome (inclusive o carimbo) não é blob.
fn blob_file_hash(name: &str) -> Option<ByproductHash> {
    let (hash, temp) = match name.split_once(TEMP_INFIX) {
        Some((hash, temp)) => (hash, Some(temp)),
        None => (name, None),
    };
    let is_numeric =
        |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if let Some(temp) = temp
        && !temp
            .split_once('-')
            .is_some_and(|(pid, attempt)| is_numeric(pid) && is_numeric(attempt))
    {
        return None;
    }

    ByproductHash::parse(hash).ok()
}

/// Data de modificação no futuro não é de blob velho.
fn is_stale(metadata: &fs::Metadata, max_age: Duration) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > max_age)
}

fn renew_modified(path: &Path) -> Result<(), ByproductError> {
    let file = open_without_following_or_blocking(path).map_err(|error| match error {
        OpenError::Io(error) => io_error(&error),
        OpenError::NotFound | OpenError::Symlink => io_error(&io::Error::other(
            "the blob changed while its date was renewed",
        )),
    })?;

    file.set_modified(SystemTime::now())
        .map_err(|error| io_error(&error))
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
