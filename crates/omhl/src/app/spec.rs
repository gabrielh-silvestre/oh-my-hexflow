use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::engine::{Engine, parse_at, require_project_root};
use super::error::AppError;
use super::work_item::work_item_refusal;
use crate::error::{OmhlError, RuleCode};
use crate::extensions::files::read_regular_utf8;
use crate::ports::git::GitProbe;
use crate::ports::specs::{SpecStore, SpecStoreError};
use crate::probe::GitTopology;
use crate::project::ProjectError;
use crate::project::layout::{EXT_FILE_MAX_BYTES, OMHL_DIR, WORKFLOW_FILE};
use crate::workflow::config::{WorkflowConfig, parse_workflow};

const WORK_ITEM_ID_PREFIX: &str = "work-items.";
const SLUG_MAX: usize = 64;
const FIRST_PART: &str = "## Objetivo\n\n## Diretivas e premissas aplicáveis\n\n## Comportamentos\n\n## Critérios de aceitação\n\n## Fora de escopo\n";

impl Engine<'_> {
    /// O slug vira nome de pasta: é validado antes de qualquer leitura ou escrita.
    pub(super) fn spec_init(&mut self, work_item: &str, slug: &str) -> Result<Value, AppError> {
        if !is_valid_slug(slug) {
            return Err(AppError::spec_slug_invalid("/slug"));
        }
        let id = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        if let Some(code) = work_item_refusal(self, &id)? {
            return Err(OmhlError::rule(code, "/work_item").into());
        }
        let Some(name) = id.as_str().strip_prefix(WORK_ITEM_ID_PREFIX) else {
            return Err(OmhlError::rule(RuleCode::WorkItemNotFound, "/work_item").into());
        };
        let config = read_workflow(&root)?;
        let main_dir = GitProbe::new(&root)
            .main_dir()
            .map_err(|error| AppError::probe_failed(&error.message))?;
        let store = SpecStore::new(main_dir.join(&config.specs.dir));
        if store.has_spec(name).map_err(spec_store_refusal)? {
            return Err(AppError::spec_exists("/work_item"));
        }
        let (dir, path) = store
            .create_first_part(name, slug, FIRST_PART)
            .map_err(spec_store_refusal)?;

        Ok(json!({
            "work_item": id,
            "slug": slug,
            "dir": dir.to_string_lossy(),
            "path": path.to_string_lossy(),
        }))
    }
}

fn is_valid_slug(slug: &str) -> bool {
    (1..=SLUG_MAX).contains(&slug.len())
        && !slug.starts_with('-')
        && slug
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
}

fn spec_store_refusal(error: SpecStoreError) -> AppError {
    match error {
        SpecStoreError::Exists => AppError::spec_exists("/work_item"),
        SpecStoreError::Unusable(reason) => AppError::probe_failed(reason),
    }
}

/// Arquivo ausente vale os padrões; presente e ilegível ou inválido é recusa.
fn read_workflow(root: &Path) -> Result<WorkflowConfig, AppError> {
    let path = root.join(OMHL_DIR).join(WORKFLOW_FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(WorkflowConfig::default()),
        _ => {}
    }
    let text = read_regular_utf8(&path, EXT_FILE_MAX_BYTES).map_err(|reason| ProjectError {
        code: "invalid-workflow-config",
        message: format!("{}: {reason}", path.display()),
    })?;

    Ok(parse_workflow(&path, text.as_bytes())?)
}
