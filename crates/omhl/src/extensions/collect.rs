use std::fs;
use std::path::{Component, Path};

use super::files::read_regular_utf8;
use super::model::{Extension, ExtensionIssue, ExtensionKind};
use super::parse::parse;
use super::scan::scan_with;
use crate::native::NativeBase;
use crate::project::layout::{EXT_FILE_MAX_BYTES, ISSUES_MAX, OMHL_DIR, WORKFLOW_FILE};
use crate::workflow::config::parse_workflow;

#[derive(Debug, Default)]
pub struct Collected {
    pub extensions: Vec<Extension>,
    pub errors: Vec<ExtensionIssue>,
    pub omitted_errors: usize,
    pub warnings: Vec<ExtensionIssue>,
}

pub fn collect(root: &Path) -> Collected {
    let specs_name = configured_specs_name(root);
    let extra_ignored: Vec<&str> = specs_name.iter().map(String::as_str).collect();
    let scanned = scan_with(root, &extra_ignored);
    let mut errors = scanned.errors;
    let mut extensions = Vec::new();

    for raw in &scanned.files {
        match parse(raw) {
            Ok(extension) if is_native_id(&extension.id) => errors.push(ExtensionIssue {
                path: extension.path,
                code: "reserved-id",
                message: "id is reserved for a native check or guideline".to_owned(),
            }),
            Ok(extension) => extensions.push(extension),
            Err(issues) => errors.extend(issues),
        }
    }

    errors.sort_by(|left, right| (&left.path, left.code).cmp(&(&right.path, right.code)));
    let omitted_errors = errors.len().saturating_sub(ISSUES_MAX);
    errors.truncate(ISSUES_MAX);

    Collected {
        extensions,
        errors,
        omitted_errors,
        warnings: scanned.warnings,
    }
}

/// Nome que o `specs.dir` configurado ocupa dentro de `.omhl/`. Arquivo ausente ou inválido cai nos
/// padrões (quem recusa a configuração é outro caminho), e `specs.dir` fora de `.omhl/` não muda o scan.
fn configured_specs_name(root: &Path) -> Option<String> {
    let path = root.join(OMHL_DIR).join(WORKFLOW_FILE);
    let text = read_regular_utf8(&path, EXT_FILE_MAX_BYTES).ok()?;
    let config = parse_workflow(&path, text.as_bytes()).ok()?;
    let mut components = Path::new(&config.specs.dir).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(first)), Some(Component::Normal(name))) if first == OMHL_DIR => {
            name.to_str().map(str::to_owned)
        }
        _ => None,
    }
}

/// O arquivo TOML que declara o check `id` ainda está na working tree, válido ou não.
pub fn check_file_exists(root: &Path, id: &str) -> bool {
    id.strip_prefix(ExtensionKind::Check.id_prefix())
        .is_some_and(|slug| {
            let file = Path::new(OMHL_DIR)
                .join(ExtensionKind::Check.dir())
                .join(format!("{slug}.toml"));

            fs::symlink_metadata(root.join(file)).is_ok()
        })
}

fn is_native_id(id: &str) -> bool {
    let native = NativeBase;
    native.checks().iter().any(|check| check.id == id)
        || native
            .guidelines()
            .iter()
            .any(|guideline| guideline.id == id)
}
