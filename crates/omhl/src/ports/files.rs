use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

pub(crate) const FILE_TOO_LARGE: &str = "file-too-large";

pub(crate) fn read_regular_utf8(path: &Path, max_bytes: u64) -> Result<String, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "io-error")?;
    if metadata.is_symlink() {
        return Err("symlink");
    }
    if !metadata.is_file() {
        return Err("not-regular-file");
    }
    let file = open_regular_nonblocking(path)?;
    if file.metadata().map_err(|_| "io-error")?.len() > max_bytes {
        return Err(FILE_TOO_LARGE);
    }
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "io-error")?;
    // O arquivo pode ter crescido entre o metadata e a leitura.
    if bytes.len() as u64 > max_bytes {
        return Err(FILE_TOO_LARGE);
    }
    String::from_utf8(bytes).map_err(|_| "not-utf8")
}

/// Como `read_regular_utf8`, mas segue link simbólico (dotfiles): só lê se o alvo for arquivo regular.
pub(crate) fn read_followed_regular_utf8(
    path: &Path,
    max_bytes: u64,
) -> Result<String, &'static str> {
    // `fs::metadata` segue o link; checar antes do `open` evita travar num FIFO.
    if !fs::metadata(path).map_err(|_| "io-error")?.is_file() {
        return Err("not-regular-file");
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "io-error")?
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "io-error")?;
    if bytes.len() as u64 > max_bytes {
        return Err(FILE_TOO_LARGE);
    }
    String::from_utf8(bytes).map_err(|_| "not-utf8")
}

/// Abre sem seguir link e sem travar num FIFO sem escritor; o que o descritor aberto mostra
/// que não é arquivo regular volta como `not-regular-file`. Cobre a troca do arquivo depois da
/// pré-checagem por `symlink_metadata`.
fn open_regular_nonblocking(path: &Path) -> Result<File, &'static str> {
    let file = open_without_following_or_blocking(path).map_err(|error| match error {
        OpenError::Symlink => "symlink",
        OpenError::NotFound | OpenError::Io(_) => "io-error",
    })?;
    if !file.metadata().map_err(|_| "io-error")?.is_file() {
        return Err("not-regular-file");
    }
    Ok(file)
}

/// Por que `open_without_following_or_blocking` falhou: o chamador decide o que cada motivo significa.
pub(crate) enum OpenError {
    Symlink,
    NotFound,
    Io(io::Error),
}

#[cfg(target_os = "linux")]
pub(crate) fn open_without_following_or_blocking(path: &Path) -> Result<File, OpenError> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        // O_NOCTTY: se a troca trouxer um tty, ele não vira terminal de controle do processo
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY)
        .open(path)
        .map_err(|error| match (error.raw_os_error(), error.kind()) {
            (Some(nix::libc::ELOOP), _) => OpenError::Symlink,
            (_, io::ErrorKind::NotFound) => OpenError::NotFound,
            _ => OpenError::Io(error),
        })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn open_without_following_or_blocking(path: &Path) -> Result<File, OpenError> {
    // shortcut: sem O_NONBLOCK fora do Linux, a pré-checagem cobre o caso comum
    File::open(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => OpenError::NotFound,
        _ => OpenError::Io(error),
    })
}

#[cfg(test)]
mod tests;

pub(crate) fn list_sorted_capped(dir: &Path, cap: usize) -> Result<Vec<OsString>, &'static str> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| "io-error")? {
        names.push(entry.map_err(|_| "io-error")?.file_name());
        if names.len() > cap {
            return Err("too-many-entries");
        }
    }
    names.sort();
    Ok(names)
}
