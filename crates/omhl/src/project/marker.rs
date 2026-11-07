use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::ProjectError;

const MARKER_FILE: &str = ".omhl.toml";
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
#[serde(deny_unknown_fields)]
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

pub fn parse_marker(path: &Path, bytes: &[u8]) -> Result<Marker, ProjectError> {
    if bytes.len() > MAX_MARKER_BYTES {
        return Err(invalid(
            path,
            format!("marker is larger than {MAX_MARKER_BYTES} bytes"),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| invalid(path, error))?;
    let raw: RawMarker = toml::from_str(text).map_err(|error| invalid(path, error))?;
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

/// Lê no máximo um byte além do limite, para que um marcador gigante não vá inteiro à memória.
fn read_capped(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MARKER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;

    Ok(bytes)
}

/// `start` é absoluto; o marcador mais próximo vence e, se for inválido, a busca não sobe.
pub fn find_marker(start: &Path) -> Result<Option<Located>, ProjectError> {
    for dir in start.ancestors() {
        let path = dir.join(MARKER_FILE);
        match read_capped(&path) {
            Ok(bytes) => {
                let marker = parse_marker(&path, &bytes)?;
                return Ok(Some(Located {
                    root: dir.to_path_buf(),
                    marker,
                }));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(invalid(&path, error)),
        }
    }

    Ok(None)
}
