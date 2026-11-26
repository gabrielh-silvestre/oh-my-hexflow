use crate::probe::GitVersion;

const MINIMUM: (u32, u32) = (2, 31);

/// A linha de aviso quando o `git` falta ou é anterior ao 2.31; saída ilegível não avisa, o `probe-failed` tardio cobre.
pub(super) fn notice(version: &GitVersion) -> Option<String> {
    match version {
        GitVersion::NotFound => {
            Some("WARNING: git was not found; omhl needs git 2.31 or newer".to_owned())
        }
        GitVersion::Reported(text) => match parse_version(text) {
            Some(found) if found < MINIMUM => Some(format!(
                "WARNING: git {}.{} is too old; omhl needs git 2.31 or newer",
                found.0, found.1
            )),
            _ => None,
        },
        GitVersion::Unreadable => None,
    }
}

/// Só os dois primeiros componentes numéricos; o que vem depois (`(Apple Git-143)`, `.windows.1`) não entra.
fn parse_version(text: &str) -> Option<(u32, u32)> {
    let number = text
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut components = number.split('.');

    Some((
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
    ))
}

#[cfg(test)]
mod tests;
