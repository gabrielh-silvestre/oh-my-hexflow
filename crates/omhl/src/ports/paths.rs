use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// Mesmo teto de saltos do kernel (`MAXSYMLINKS`).
const HOPS_MAX: usize = 40;
const PARENT: &str = "..";

/// O caminho sem link simbólico, mesmo que o alvo ainda não exista: o trecho existente é resolvido, o
/// trecho que falta é anexado no texto e um link pendente (dangling) é seguido até o destino dele.
/// O erro é o motivo estável da falha (laço de links, leitura negada), nunca o caminho.
pub(crate) fn resolve_physical(path: &Path) -> Result<PathBuf, &'static str> {
    let mut resolved = PathBuf::new();
    let mut pending: VecDeque<OsString> = names_after_root(path, &mut resolved).into();
    let mut hops = 0;
    while let Some(name) = pending.pop_front() {
        // `resolved` nunca tem link, então subir um nível no texto é subir no disco.
        if name == PARENT {
            resolved.pop();
            continue;
        }
        let next = resolved.join(&name);
        match fs::symlink_metadata(&next) {
            Ok(metadata) if metadata.is_symlink() => {
                hops += 1;
                if hops > HOPS_MAX {
                    return Err("too-many-hops");
                }
                let target = fs::read_link(&next).map_err(|_| "io-error")?;
                // O alvo substitui o componente; um alvo absoluto recomeça da raiz.
                if target.is_absolute() {
                    resolved = PathBuf::new();
                }
                for name in names_after_root(&target, &mut resolved).into_iter().rev() {
                    pending.push_front(name);
                }
            }
            Ok(_) => resolved = next,
            Err(error) if error.kind() == ErrorKind::NotFound => resolved = next,
            Err(_) => return Err("io-error"),
        }
    }

    Ok(resolved)
}

/// A raiz (ou o prefixo) vai para `resolved`; `.` some; o resto vira a fila de nomes a resolver.
fn names_after_root(path: &Path, resolved: &mut PathBuf) -> Vec<OsString> {
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => resolved.push(component),
            Component::CurDir => {}
            Component::ParentDir => names.push(OsString::from(PARENT)),
            Component::Normal(name) => names.push(name.to_owned()),
        }
    }

    names
}

#[cfg(test)]
mod tests;
