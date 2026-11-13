use std::path::Path;

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::engine::{Engine, parse_at, require_project_root};
use super::error::AppError;
use super::work_item::work_item_refusal;
use crate::error::{OmhlError, RuleCode};
use crate::ports::git::GitProbe;
use crate::ports::specs::{SpecDocument, SpecStore, SpecStoreError};
use crate::ports::workflow::read_workflow;
use crate::probe::GitTopology;
use crate::workflow::config::WorkflowConfig;
use crate::workflow::spec::{
    AmendmentState, SpecFile, Violation, amendment_state, validate_graph, validate_spec,
};

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
        let (_, store) = open_store(&root)?;
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

    /// Estrutura e grafo: as violações das partes e das emendas aprovadas reprovam; as de emenda
    /// pendente só são listadas.
    pub(super) fn spec_validate(&mut self, work_item: &str) -> Result<Value, AppError> {
        let id = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let name = id
            .as_str()
            .strip_prefix(WORK_ITEM_ID_PREFIX)
            .ok_or_else(|| AppError::spec_not_found("/work_item"))?;
        let (config, store) = open_store(&root)?;
        let folder = store
            .read_folder(name)
            .map_err(spec_store_refusal)?
            .filter(|folder| !folder.parts.is_empty())
            .ok_or_else(|| AppError::spec_not_found("/work_item"))?;
        let facts = self.load_spec_facts()?;
        let parts = spec_files(&folder.parts);
        let amendments = spec_files(&folder.amendments);
        let mut violations = validate_spec(&parts, &amendments, &config.specs);
        violations.extend(validate_graph(
            id.as_str(),
            &parts,
            &amendments,
            &config.specs,
            &facts,
        ));
        let files: Vec<&str> = folder
            .parts
            .iter()
            .chain(&folder.amendments)
            .map(|document| document.name.as_str())
            .collect();
        violations.sort_by_key(|violation| {
            (
                files.iter().position(|name| *name == file_of(violation)),
                line_of(violation),
            )
        });

        let approved: Vec<&str> = folder
            .amendments
            .iter()
            .filter(|document| {
                amendment_state(id.as_str(), &document.name, &facts) == AmendmentState::Approved
            })
            .map(|document| document.name.as_str())
            .collect();
        let blocking: Vec<Violation> = violations
            .iter()
            .filter(|violation| {
                let file = file_of(violation);
                folder.parts.iter().any(|part| part.name == file) || approved.contains(&file)
            })
            .cloned()
            .collect();
        if !blocking.is_empty() {
            return Err(AppError::spec_invalid(&blocking));
        }

        Ok(json!({
            "ok": true,
            "parts": folder.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>(),
            "amendments": amendments_report(&folder.amendments, &violations, &approved),
        }))
    }
}

fn open_store(root: &Path) -> Result<(WorkflowConfig, SpecStore), AppError> {
    let config = read_workflow(root)?;
    let main_dir = GitProbe::new(root)
        .main_dir()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    let store = SpecStore::new(main_dir.join(&config.specs.dir));

    Ok((config, store))
}

fn spec_files(documents: &[SpecDocument]) -> Vec<SpecFile<'_>> {
    documents
        .iter()
        .map(|document| SpecFile {
            name: &document.name,
            text: &document.text,
        })
        .collect()
}

fn file_of(violation: &Violation) -> &str {
    violation
        .path
        .rsplit_once(':')
        .map_or(&violation.path, |(file, _)| file)
}

fn line_of(violation: &Violation) -> Option<usize> {
    violation.path.rsplit_once(':')?.1.parse().ok()
}

fn amendments_report(
    documents: &[SpecDocument],
    violations: &[Violation],
    approved: &[&str],
) -> Vec<Value> {
    documents
        .iter()
        .map(|document| {
            let state = if approved.contains(&document.name.as_str()) {
                "approved"
            } else {
                "pending"
            };
            let violations: Vec<Value> = violations
                .iter()
                .filter(|violation| file_of(violation) == document.name)
                .map(|violation| json!({ "path": violation.path, "code": violation.code }))
                .collect();

            json!({ "name": document.name, "state": state, "violations": violations })
        })
        .collect()
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
