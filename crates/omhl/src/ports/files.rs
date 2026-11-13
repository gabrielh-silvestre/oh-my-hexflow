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
    if metadata.len() > max_bytes {
        return Err(FILE_TOO_LARGE);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(max_bytes + 1).read_to_end(&mut bytes))
        .map_err(|_| "io-error")?;
    // O arquivo pode ter crescido entre o metadata e a leitura.
    if bytes.len() as u64 > max_bytes {
        return Err(FILE_TOO_LARGE);
    }
    String::from_utf8(bytes).map_err(|_| "not-utf8")
}

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
