use std::fs;
use std::path::Path;

use super::model::{Extension, ExtensionIssue, ExtensionKind};
use super::parse::parse;
use super::scan::scan;
use crate::native::NativeBase;
use crate::project::layout::{ISSUES_MAX, OMHL_DIR};

#[derive(Debug, Default)]
pub struct Collected {
    pub extensions: Vec<Extension>,
    pub errors: Vec<ExtensionIssue>,
    pub omitted_errors: usize,
    pub warnings: Vec<ExtensionIssue>,
}

pub fn collect(root: &Path) -> Collected {
    let scanned = scan(root);
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
