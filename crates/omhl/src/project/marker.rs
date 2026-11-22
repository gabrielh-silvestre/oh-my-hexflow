use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::ProjectError;
use super::layout::OMHL_DIR;
use super::text::toml_error_position;

pub const MARKER_FILE: &str = ".omhl.toml";
const MAX_MARKER_BYTES: usize = 4096;
const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub version: u32,
    pub tenant: hexlog::TenantId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub root: PathBuf,
    pub marker: Marker,
}

#[derive(Deserialize)]
struct RawMarker {
    version: u32,
    tenant: String,
}

fn invalid(path: &Path, reason: impl std::fmt::Display) -> ProjectError {
    ProjectError {
        code: "invalid-marker",
        message: format!("{}: {reason}", path.display()),
    }
}

/// O texto de erro do TOML repete trechos do arquivo.
fn parse_raw(path: &Path, text: &str) -> Result<RawMarker, ProjectError> {
    let table: toml::Table = text.parse().map_err(|error: toml::de::Error| {
        let (line, column) = toml_error_position(text, &error);
        invalid(
            path,
            format!("not valid TOML at line {line}, column {column}"),
        )
    })?;
    if let Some(key) = table
        .keys()
        .find(|key| !matches!(key.as_str(), "version" | "tenant"))
    {
        return Err(invalid(path, format!("unknown key `{key}`")));
    }

    table
        .try_into()
        .map_err(|_| invalid(path, "needs an integer version and a string tenant"))
}

pub fn parse_marker(path: &Path, bytes: &[u8]) -> Result<Marker, ProjectError> {
    if bytes.len() > MAX_MARKER_BYTES {
        return Err(invalid(
            path,
            format!("marker is larger than {MAX_MARKER_BYTES} bytes"),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| invalid(path, error))?;
    let raw = parse_raw(path, text)?;
    if raw.version != SUPPORTED_VERSION {
        return Err(invalid(
            path,
            format!("does not understand version = {}", raw.version),
        ));
    }
    let tenant = hexlog::TenantId::parse(&raw.tenant).map_err(|error| invalid(path, error))?;

    Ok(Marker {
        version: raw.version,
        tenant,
    })
}

/// A definição de "marcador válido" que `find_marker`, `find_valid_marker` e o hook `spec-lock` compartilham.
pub fn is_valid_marker(path: &Path, bytes: &[u8]) -> bool {
    parse_marker(path, bytes).is_ok()
}

/// Para que um marcador gigante não vá inteiro à memória.
fn read_capped(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MARKER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;

    Ok(bytes)
}

/// O marcador desta pasta; `None` quando não há arquivo, erro quando há e ele é recusado.
fn marker_at(dir: &Path) -> Result<Option<Located>, ProjectError> {
    let path = dir.join(MARKER_FILE);
    // Symlink, FIFO e dispositivo são recusados antes de abrir: abrir um FIFO sem escritor trava.
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(invalid(&path, "marker is not a regular file")),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(invalid(&path, error)),
    }
    let bytes = read_capped(&path).map_err(|error| invalid(&path, error))?;
    let marker = parse_marker(&path, &bytes)?;

    Ok(Some(Located {
        root: dir.to_path_buf(),
        marker,
    }))
}

pub fn find_marker(start: &Path) -> Result<Option<Located>, ProjectError> {
    for dir in start.ancestors() {
        if let Some(located) = marker_at(dir)? {
            return Ok(Some(located));
        }
    }

    Ok(None)
}

/// Só para o hook `spec-lock`: um marcador que `find_marker` recusaria, ou válido mas sob `.omhl/`
/// (que não é raiz de projeto), é pulado e a subida continua até o primeiro válido. Sem nenhum válido,
/// devolve o erro do primeiro pulado.
pub fn find_valid_marker(start: &Path) -> Result<Option<Located>, ProjectError> {
    let mut first_skipped = None;
    for dir in start.ancestors() {
        match marker_at(dir) {
            Ok(None) => {}
            Ok(Some(_)) if is_under_omhl_dir(dir) => {
                first_skipped.get_or_insert_with(|| {
                    invalid(
                        &dir.join(MARKER_FILE),
                        "a marker under .omhl/ is not a project root",
                    )
                });
            }
            Ok(Some(located)) => return Ok(Some(located)),
            Err(error) => {
                first_skipped.get_or_insert(error);
            }
        }
    }

    first_skipped.map_or(Ok(None), Err)
}

fn is_under_omhl_dir(dir: &Path) -> bool {
    dir.components()
        .any(|component| component.as_os_str() == OMHL_DIR)
}

#[cfg(test)]
mod tests;
