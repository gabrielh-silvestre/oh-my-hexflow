use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

pub(crate) const FILE_TOO_LARGE: &str = "file-too-large";

/// Lê um arquivo regular em UTF-8; o erro é o código do relatório, nunca o conteúdo.
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

/// Abre sem seguir link e sem travar num FIFO sem escritor; o que o descritor aberto mostra
/// que não é arquivo regular volta como `not-regular-file`. Cobre a troca do arquivo depois da
/// pré-checagem por `symlink_metadata`.
pub(crate) fn open_regular_nonblocking(path: &Path) -> Result<File, &'static str> {
    let file = open_without_following_or_blocking(path)?;
    if !file.metadata().map_err(|_| "io-error")?.is_file() {
        return Err("not-regular-file");
    }
    Ok(file)
}

#[cfg(target_os = "linux")]
fn open_without_following_or_blocking(path: &Path) -> Result<File, &'static str> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| match error.raw_os_error() {
            Some(nix::libc::ELOOP) => "symlink",
            _ => "io-error",
        })
}

#[cfg(not(target_os = "linux"))]
fn open_without_following_or_blocking(path: &Path) -> Result<File, &'static str> {
    // shortcut: sem O_NONBLOCK fora do Linux, a pré-checagem cobre o caso comum
    File::open(path).map_err(|_| "io-error")
}

#[cfg(test)]
mod tests;

/// Nomes da pasta em ordem estável; para de listar ao passar do teto.
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
