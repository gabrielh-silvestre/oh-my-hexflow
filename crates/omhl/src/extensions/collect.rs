use std::fs;
use std::path::{Component, Path};

use super::model::{Extension, ExtensionIssue, ExtensionKind};
use super::parse::parse;
use super::scan::scan_with;
use crate::native::NativeBase;
use crate::ports::workflow::read_policy;
use crate::project::ProjectError;
use crate::project::layout::{ISSUES_MAX, OMHL_DIR, WORKFLOW_FILE};
use crate::workflow::config::WorkflowConfig;

#[derive(Debug, Default)]
pub struct Collected {
    pub extensions: Vec<Extension>,
    pub errors: Vec<ExtensionIssue>,
    pub omitted_errors: usize,
    pub warnings: Vec<ExtensionIssue>,
}

pub fn collect(root: &Path) -> Collected {
    let (config, config_warning) = match read_policy(root) {
        Ok(config) => (config, None),
        Err(error) => (
            WorkflowConfig::default(),
            Some(invalid_workflow_warning(&error)),
        ),
    };
    let specs_name = configured_specs_name(&config);
    let extra_ignored: Vec<&str> = specs_name.iter().map(String::as_str).collect();
    let scanned = scan_with(root, &extra_ignored);
    let mut errors = scanned.errors;
    let mut warnings = scanned.warnings;
    warnings.extend(config_warning);
    let mut extensions = Vec::new();

    for raw in &scanned.files {
        match parse(raw) {
            Ok(extension) if is_native_id(&extension.id) => errors.push(ExtensionIssue {
                path: extension.path,
                code: "reserved-id",
                message: "id is reserved for a native check or guideline".to_owned(),
                next_step: None,
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
        warnings,
    }
}

/// A configuração inválida cai nos padrões no scan, mas o aviso sobe: quem recusa é outro caminho.
fn invalid_workflow_warning(error: &ProjectError) -> ExtensionIssue {
    ExtensionIssue {
        path: format!("{OMHL_DIR}/{WORKFLOW_FILE}"),
        code: error.code,
        message: error.message.clone(),
        next_step: None,
    }
}

/// Nome que o `specs.dir` configurado ocupa dentro de `.omhl/`; `specs.dir` fora de `.omhl/` não muda o scan.
fn configured_specs_name(config: &WorkflowConfig) -> Option<String> {
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
