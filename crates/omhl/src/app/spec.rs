use std::path::{Path, PathBuf};

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::engine::{Engine, parse_at, require_project_root};
use super::error::AppError;
use super::spec_facts::GraphFacts;
use super::work_item::work_item_refusal;
use crate::error::{OmhlError, RuleCode};
use crate::mechanical::branch::work_item_id;
use crate::ports::files::FILE_TOO_LARGE;
use crate::ports::git::GitProbe;
use crate::ports::specs::{SpecDocument, SpecFolder, SpecStore, SpecStoreError};
use crate::ports::workflow::{read_policy, read_workflow};
use crate::probe::{GitTopology, ProbeError};
use crate::project::marker::{Located, find_marker};
use crate::workflow::config::{SpecsConfig, WorkflowConfig};
use crate::workflow::lock::{
    Facts, Tool, Verdict, decide, has_spec_folder_component, parse_spec_path,
};
use crate::workflow::names::WORK_ITEM_ID_PREFIX;
use crate::workflow::spec::{
    AmendmentState, SPEC_STALE, SpecFile, Violation, amendment_divergence_mismatch,
    amendment_state, is_amendment_id, validate_graph, validate_spec,
};

const SLUG_MAX: usize = 64;
const FIRST_PART: &str = "## Objetivo\n\n## Diretivas e premissas aplicáveis\n\n## Comportamentos\n\n## Critérios de aceitação\n\n## Fora de escopo\n";

enum SpecCheck {
    Missing,
    Invalid(Vec<Violation>),
    Valid(Value),
}

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

    pub(super) fn spec_validate(&mut self, work_item: &str) -> Result<Value, AppError> {
        let id = parse_at(SemanticId::parse, work_item, "/work_item")?;

        match self.check_spec(&id)? {
            SpecCheck::Missing => Err(AppError::spec_not_found("/work_item")),
            SpecCheck::Invalid(blocking) => Err(AppError::spec_invalid(&blocking)),
            SpecCheck::Valid(report) => Ok(report),
        }
    }

    pub(super) fn spec_is_valid(&mut self, id: &SemanticId) -> Result<bool, AppError> {
        Ok(matches!(self.check_spec(id)?, SpecCheck::Valid(_)))
    }

    fn check_spec(&mut self, id: &SemanticId) -> Result<SpecCheck, AppError> {
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let (config, folder) = read_spec_folder(&root, id.as_str())?;
        let Some(folder) = folder.filter(|folder| !folder.parts.is_empty()) else {
            return Ok(SpecCheck::Missing);
        };
        let facts = self.load_spec_facts()?;
        let violations = folder_violations(id.as_str(), &folder, &config.specs, &facts);

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
            return Ok(SpecCheck::Invalid(blocking));
        }

        Ok(SpecCheck::Valid(json!({
            "ok": true,
            "parts": folder.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>(),
            "amendments": amendments_report(&folder.amendments, &violations, &approved),
        })))
    }
}

impl Engine<'_> {
    pub(super) fn check_amendment_approvals(&mut self, request: &Value) -> Result<(), AppError> {
        let items = request["items"].as_array().map_or(&[][..], Vec::as_slice);
        for (index, item) in items.iter().enumerate() {
            let body = &item["body"];
            let data = &body["data"];
            if body["kind"] != "node"
                || body["type"] != "divergence"
                || data["resolution"] != SPEC_STALE
            {
                continue;
            }
            let Some(amendment) = data["amendment"]
                .as_str()
                .filter(|name| is_amendment_id(name))
            else {
                continue;
            };
            self.check_amendment_approval(index, body, amendment)?;
        }

        Ok(())
    }

    fn check_amendment_approval(
        &mut self,
        index: usize,
        body: &Value,
        amendment: &str,
    ) -> Result<(), AppError> {
        let data_pointer = format!("/items/{index}/body/data");
        let id = parse_at(
            SemanticId::parse,
            body["id"].as_str().unwrap_or_default(),
            &format!("/items/{index}/body/id"),
        )?;
        let Some(current) = self
            .omhl
            .get(&self.tenant, &id)?
            .filter(|node| node.data["resolution"] == "open")
        else {
            return Err(AppError::amendment_divergence_not_open(&format!(
                "{data_pointer}/resolution"
            )));
        };
        let Some(work_item) = current.data["work_item"].as_str() else {
            return Ok(());
        };
        if body["data"]["work_item"]
            .as_str()
            .is_some_and(|revised| revised != work_item)
        {
            return Err(AppError::amendment_invalid(&[Violation {
                path: format!("{data_pointer}/work_item"),
                code: "amendment-work-item-mismatch",
            }]));
        }

        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let (config, folder) = read_spec_folder(&root, work_item)?;
        let file = format!("{amendment}.md");
        let not_found = || AppError::amendment_not_found(&format!("{data_pointer}/amendment"));
        let folder = folder.ok_or_else(not_found)?;
        let document = folder
            .amendments
            .iter()
            .find(|document| document.name == file)
            .ok_or_else(not_found)?;

        let facts = self.load_spec_facts()?;
        let violations: Vec<Violation> =
            folder_violations(work_item, &folder, &config.specs, &facts)
                .into_iter()
                .filter(|violation| file_of(violation) == file)
                .collect();
        if !violations.is_empty() {
            return Err(AppError::amendment_invalid(&violations));
        }
        let spec_file = SpecFile {
            name: &document.name,
            text: &document.text,
        };
        match amendment_divergence_mismatch(&spec_file, id.as_str()) {
            Some(mismatch) => Err(AppError::amendment_invalid(&[mismatch])),
            None => Ok(()),
        }
    }
}

impl Engine<'_> {
    pub(super) fn amendment_files(&self, work_item: &str) -> Result<Vec<String>, AppError> {
        let Some(root) = self.project_root.as_deref() else {
            return Ok(Vec::new());
        };
        let Some(name) = work_item.strip_prefix(WORK_ITEM_ID_PREFIX) else {
            return Ok(Vec::new());
        };
        let (_, store) = open_store(root)?;

        store.amendment_names(name).map_err(spec_store_refusal)
    }
}

fn read_spec_folder(
    root: &Path,
    work_item: &str,
) -> Result<(WorkflowConfig, Option<SpecFolder>), AppError> {
    let (config, store) = open_store(root)?;
    let folder = match work_item.strip_prefix(WORK_ITEM_ID_PREFIX) {
        Some(name) => store.read_folder(name).map_err(spec_store_refusal)?,
        None => None,
    };

    Ok((config, folder))
}

fn folder_violations(
    work_item: &str,
    folder: &SpecFolder,
    config: &SpecsConfig,
    facts: &GraphFacts,
) -> Vec<Violation> {
    let parts = spec_files(&folder.parts);
    let amendments = spec_files(&folder.amendments);
    let mut violations = validate_spec(&parts, &amendments, config);
    violations.extend(
        folder
            .parts
            .iter()
            .chain(&folder.amendments)
            .filter(|document| document.oversized)
            .map(|document| Violation {
                path: format!("{}:1", document.name),
                code: FILE_TOO_LARGE,
            }),
    );
    violations.extend(validate_graph(
        work_item,
        &parts,
        &amendments,
        config,
        facts,
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

    violations
}

pub fn spec_lock(input: &Value) -> Result<Verdict, AppError> {
    let Some(tool) = input["tool_name"].as_str().and_then(Tool::parse) else {
        return Ok(Verdict::Allow);
    };
    let cwd = absolute_path(&input["cwd"])?;
    let file_path = absolute_path(&input["tool_input"]["file_path"])?;
    if !has_spec_folder_component(&file_path) {
        return Ok(Verdict::Allow);
    }
    let Some(located) = find_project(&file_path, &cwd)? else {
        return Ok(Verdict::Allow);
    };
    // Arquivo inválido cai nos padrões: liberar aqui deixaria a spec congelada editável por um typo.
    let config = read_policy(&located.root).unwrap_or_default();
    let Some(spec) = parse_spec_path(&config.specs.dir, &file_path) else {
        return Ok(Verdict::Allow);
    };

    let git_failed = |error: ProbeError| AppError::probe_failed(&error.message);
    let probe = GitProbe::new(&located.root);
    let main_dir = probe.main_dir().map_err(git_failed)?;
    let work_item = format!("{WORK_ITEM_ID_PREFIX}{}", spec.work_item);
    let frozen = probe
        .local_branches()
        .map_err(git_failed)?
        .iter()
        .any(|branch| work_item_id(branch).is_some_and(|id| id.as_str() == work_item));
    let checkout = GitProbe::new(nearest_existing_dir(&file_path))
        .checkout_dir()
        .map_err(git_failed)?;
    let specs_dir = main_dir.join(&config.specs.dir);
    let store = SpecStore::new(specs_dir.clone());
    let folder_files = store
        .folder_files(&spec.folder)
        .map_err(spec_store_refusal)?;
    let file_exists = store
        .file_exists(&spec.folder, &spec.file)
        .map_err(spec_store_refusal)?;
    let folder_exists = store
        .folder_exists(&spec.folder)
        .map_err(spec_store_refusal)?;
    let facts = Facts {
        frozen,
        folder_exists,
        linked_worktree: checkout != main_dir,
        file_exists,
        folder_files: &folder_files,
        main_folder: &specs_dir.join(&spec.folder),
    };

    Ok(decide(tool, &spec, &facts))
}

fn find_project(file_path: &Path, cwd: &Path) -> Result<Option<Located>, AppError> {
    match find_marker(nearest_existing_dir(file_path))? {
        Some(located) => Ok(Some(located)),
        None => Ok(find_marker(cwd)?),
    }
}

fn absolute_path(value: &Value) -> Result<PathBuf, AppError> {
    match value.as_str().map(Path::new) {
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        _ => Err(AppError::invalid_hook_input()),
    }
}

/// O alvo de um `Write` pode ainda não existir, nem a pasta dele.
fn nearest_existing_dir(path: &Path) -> &Path {
    path.ancestors()
        .skip(1)
        .find(|dir| dir.is_dir())
        .unwrap_or(Path::new("/"))
}

fn open_store(root: &Path) -> Result<(WorkflowConfig, SpecStore), AppError> {
    let main_dir = GitProbe::new(root)
        .main_dir()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    let config = read_workflow(&main_dir)?;
    let store = SpecStore::new(main_dir.join(&config.specs.dir));

    Ok((config, store))
}

fn spec_files(documents: &[SpecDocument]) -> Vec<SpecFile<'_>> {
    documents
        .iter()
        .filter(|document| !document.oversized)
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
        SpecStoreError::Unusable(reason) => AppError::spec_store_unusable(reason),
    }
}
