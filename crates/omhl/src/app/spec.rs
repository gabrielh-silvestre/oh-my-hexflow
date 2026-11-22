use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use hexlog::SemanticId;
use serde_json::{Value, json};

use super::engine::{Engine, parse_at, require_project_root};
use super::error::AppError;
use super::spec_facts::GraphFacts;
use super::work_item::work_item_refusal;
use crate::error::{OmhlError, RuleCode};
use crate::mechanical::branch::work_item_id;
use crate::ports::files::{FILE_TOO_LARGE, read_regular_utf8};
use crate::ports::git::{GitProbe, SPEC_LOCK_GIT_BUDGET};
use crate::ports::paths::resolve_physical;
use crate::ports::specs::{FreezeMarkWrite, SpecDocument, SpecFolder, SpecStore, SpecStoreError};
use crate::ports::workflow::{read_policy, read_workflow};
use crate::probe::{GIT_TIMED_OUT, ProbeError};
use crate::project::layout::{EXT_FILE_MAX_BYTES, OMHL_DIR, WORKFLOW_FILE};
use crate::project::marker::{find_marker, find_valid_marker};
use crate::workflow::config::{SpecsConfig, WorkflowConfig};
use crate::workflow::lock::{
    Cause, EditInput, Facts, SpecPath, Tool, Verdict, decide, has_spec_folder_component,
    is_same_path, is_workflow_candidate, parse_spec_path, resulting_texts, spec_dir_frozen,
    specs_dir_of, unavailable,
};
use crate::workflow::names::{FOLDER_PREFIX, WORK_ITEM_ID_PREFIX};
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

/// O que o lote já leu do disco e do banco: a pasta da spec de cada work item e os fatos do grafo.
#[derive(Default)]
struct AmendmentBatch {
    folders: HashMap<String, (WorkflowConfig, SpecFolder)>,
    facts: Option<GraphFacts>,
}

impl Engine<'_> {
    pub(super) fn check_amendment_approvals(&mut self, request: &Value) -> Result<(), AppError> {
        let items = request["items"].as_array().map_or(&[][..], Vec::as_slice);
        let mut batch = AmendmentBatch::default();
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
            self.check_amendment_approval(index, body, amendment, &mut batch)?;
        }

        Ok(())
    }

    fn check_amendment_approval(
        &mut self,
        index: usize,
        body: &Value,
        amendment: &str,
        batch: &mut AmendmentBatch,
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

        let not_found = || AppError::amendment_not_found(&format!("{data_pointer}/amendment"));
        if !batch.folders.contains_key(work_item) {
            let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
            let (config, folder) = read_spec_folder(&root, work_item)?;
            let folder = folder.ok_or_else(not_found)?;
            batch.folders.insert(work_item.to_owned(), (config, folder));
        }
        let (config, folder) = &batch.folders[work_item];
        let file = format!("{amendment}.md");
        let document = folder
            .amendments
            .iter()
            .find(|document| document.name == file)
            .ok_or_else(not_found)?;

        let facts = match &batch.facts {
            Some(facts) => facts,
            None => batch.facts.insert(self.load_spec_facts()?),
        };
        let violations: Vec<Violation> = folder_violations(work_item, folder, &config.specs, facts)
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

/// O evento `PreToolUse` já lido pelo adapter: `app` não conhece o formato do payload.
pub enum SpecLockEvent {
    NotAnEdit,
    Edit {
        tool: Tool,
        file_path: PathBuf,
        input: EditInput,
    },
}

/// A decisão da trava e os avisos que a acompanham: os avisos vão ao stderr sem mudar a decisão.
pub struct SpecLockOutcome {
    pub verdict: Verdict,
    pub notices: Vec<AppError>,
}

/// A trava da spec para o evento `PreToolUse` do Claude Code; não abre o banco. Decide pelo caminho
/// lexical e pelo físico (links resolvidos), e a negação de qualquer um vence; se o físico não se
/// resolve, vale o lexical. A ordem importa: o git só é consultado para o caminho em que algum
/// componente tem a forma da pasta de work item.
/// As chamadas de git de uma execução dividem o prazo `SPEC_LOCK_GIT_BUDGET`.
pub fn spec_lock(event: SpecLockEvent) -> Result<SpecLockOutcome, AppError> {
    let SpecLockEvent::Edit {
        tool,
        file_path,
        input,
    } = event
    else {
        return Ok(SpecLockOutcome {
            verdict: Verdict::Allow,
            notices: Vec::new(),
        });
    };
    let physical = resolve_physical(&file_path);
    let mut notices: Vec<AppError> = (physical.is_err() && is_lock_candidate(&file_path))
        .then(AppError::path_unresolvable)
        .into_iter()
        .collect();
    let mut paths = vec![file_path];
    if let Ok(physical) = physical
        && !paths.contains(&physical)
    {
        paths.push(physical);
    }

    let deadline = Instant::now() + SPEC_LOCK_GIT_BUDGET;
    let mut failure = None;
    for path in paths.iter().filter(|path| is_lock_candidate(path)) {
        match decide_path(tool, &input, path, deadline) {
            Ok((verdict, healed)) => {
                // Os dois caminhos curam o mesmo projeto: o aviso do mesmo work item sai uma vez.
                for notice in healed {
                    if !notices.contains(&notice) {
                        notices.push(notice);
                    }
                }
                if matches!(verdict, Verdict::Deny { .. }) {
                    return Ok(SpecLockOutcome { verdict, notices });
                }
            }
            Err(error) => failure = failure.or(Some(error)),
        }
    }

    match failure {
        Some(error) => Err(error),
        None => Ok(SpecLockOutcome {
            verdict: Verdict::Allow,
            notices,
        }),
    }
}

/// O filtro barato pelo texto: só estes caminhos gastam o git.
fn is_lock_candidate(file_path: &Path) -> bool {
    has_spec_folder_component(file_path) || is_workflow_candidate(file_path)
}

/// O veredito para um caminho já filtrado pelo texto, mais os avisos da cura das marcas; sem
/// marcador válido acima do arquivo, libera. Antes de `parse_spec_path` reconhecer a spec, o erro
/// libera; depois dele, qualquer falha nega com `spec-lock-unavailable`.
fn decide_path(
    tool: Tool,
    input: &EditInput,
    file_path: &Path,
    deadline: Instant,
) -> Result<(Verdict, Vec<AppError>), AppError> {
    let Some(located) = find_valid_marker(nearest_existing_dir(file_path))? else {
        return Ok((Verdict::Allow, Vec::new()));
    };
    // Um `main_dir` só, com prazo: ele também diz de onde vem a política. Com o git falhando, da pasta do marcador.
    let probe = GitProbe::new(&located.root).with_deadline(deadline);
    let main_dir = probe
        .main_dir()
        .map_err(git_step("checking the main directory"));
    let policy_dir = main_dir.as_ref().map_or(located.root.as_path(), |dir| dir);
    // Arquivo inválido cai nos padrões: liberar aqui deixaria a spec congelada editável por um typo.
    let config = read_workflow(policy_dir).unwrap_or_default();
    let (verdict, healed) = match parse_spec_path(&config.specs.dir, file_path) {
        Some(spec) => decide_recognized(tool, &spec, &config.specs.dir, &probe, &main_dir)
            .unwrap_or_else(|failure| {
                let subject = format!("{FOLDER_PREFIX}{}", spec.work_item);

                (
                    unavailable(&subject, failure.step, &failure.cause),
                    Vec::new(),
                )
            }),
        None => (Verdict::Allow, Vec::new()),
    };
    if matches!(verdict, Verdict::Deny { .. }) || !is_workflow_candidate(file_path) {
        return Ok((verdict, healed));
    }

    Ok((decide_workflow(input, file_path, &probe, &main_dir), healed))
}

/// Falha de leitura depois do reconhecimento da spec: o passo que falhou e a causa.
#[derive(Clone)]
struct Unavailable {
    step: &'static str,
    cause: Cause,
}

fn git_step(step: &'static str) -> impl Fn(ProbeError) -> Unavailable {
    move |error| Unavailable {
        step,
        cause: if error.message == GIT_TIMED_OUT {
            Cause::GitTimedOut
        } else {
            Cause::Git(error.message)
        },
    }
}

fn read_step(step: &'static str) -> impl Fn(SpecStoreError) -> Unavailable {
    move |error| Unavailable {
        step,
        cause: Cause::Read(match error {
            SpecStoreError::Exists => "the spec folder already exists",
            SpecStoreError::Unusable(reason) => reason,
        }),
    }
}

fn decide_recognized(
    tool: Tool,
    spec: &SpecPath,
    specs_dir: &str,
    probe: &GitProbe,
    main_dir: &Result<PathBuf, Unavailable>,
) -> Result<(Verdict, Vec<AppError>), Unavailable> {
    let main_dir = main_dir.as_ref().map_err(Clone::clone)?;
    let work_item = format!("{WORK_ITEM_ID_PREFIX}{}", spec.work_item);
    let branches = probe
        .local_branches()
        .map_err(git_step("listing the local branches"))?;
    let has_branch = branches
        .iter()
        .any(|branch| work_item_id(branch).is_some_and(|id| id.as_str() == work_item));
    // Do marcador, não da pasta do arquivo: com `specs.dir` link para fora do repositório, o git a partir da pasta do arquivo não acha repositório.
    let checkout = probe
        .checkout_dir()
        .map_err(git_step("checking the checkout"))?;
    let specs_dir = main_dir.join(specs_dir);
    let store = SpecStore::new(specs_dir.clone());
    let folder_files = store
        .folder_files(&spec.folder)
        .map_err(read_step("listing the files of the spec folder"))?;
    let file_exists = store
        .file_exists(&spec.folder, &spec.file)
        .map_err(read_step("checking the spec file"))?;
    let folder_exists = store
        .folder_exists(&spec.folder)
        .map_err(read_step("checking the spec folder"))?;
    let marked = store.has_freeze_mark(&spec.work_item);
    let facts = Facts {
        has_branch,
        marked,
        folder_exists,
        linked_worktree: checkout != *main_dir,
        file_exists,
        folder_files: &folder_files,
        main_folder: &specs_dir.join(&spec.folder),
    };

    Ok((
        decide(tool, spec, &facts),
        heal_freeze_marks(&store, &branches),
    ))
}

/// Só o `workflow.toml` principal conta: o de uma worktree ligada ou de outro projeto não é a política do hook.
fn decide_workflow(
    input: &EditInput,
    file_path: &Path,
    probe: &GitProbe,
    main_dir: &Result<PathBuf, Unavailable>,
) -> Verdict {
    decide_specs_dir_change(input, file_path, probe, main_dir).unwrap_or_else(|failure| {
        unavailable(
            &file_path.display().to_string(),
            failure.step,
            &failure.cause,
        )
    })
}

fn decide_specs_dir_change(
    input: &EditInput,
    file_path: &Path,
    probe: &GitProbe,
    main_dir: &Result<PathBuf, Unavailable>,
) -> Result<Verdict, Unavailable> {
    let main_dir = main_dir.as_ref().map_err(Clone::clone)?;
    let workflow = main_dir.join(OMHL_DIR).join(WORKFLOW_FILE);
    if !is_same_path(file_path, &workflow) {
        return Ok(Verdict::Allow);
    }
    // Os dois lados passam por `specs_dir_of`: ausente, ilegível, link ou grande demais vale o padrão.
    let current = read_regular_utf8(&workflow, EXT_FILE_MAX_BYTES).ok();
    let old_dir = specs_dir_of(current.as_deref().map(str::as_bytes));
    let texts = resulting_texts(input, current.as_deref()).ok_or(Unavailable {
        step: "reading the new content of workflow.toml",
        cause: Cause::Read("the new content cannot be computed"),
    })?;
    let Some(new_dir) = texts
        .iter()
        .map(|text| specs_dir_of(Some(text.as_bytes())))
        .find(|dir| !is_same_path(Path::new(dir), Path::new(&old_dir)))
    else {
        return Ok(Verdict::Allow);
    };
    let frozen = frozen_work_item(probe, main_dir.join(&old_dir))?;

    Ok(frozen.map_or(Verdict::Allow, |work_item| {
        spec_dir_frozen(
            file_path,
            &old_dir,
            &new_dir,
            &format!("{FOLDER_PREFIX}{work_item}"),
        )
    }))
}

/// O primeiro work item congelado: a marca em alguma pasta da pasta de specs antiga (a branch pode ter sido apagada) ou, sem marca, uma branch local dele.
fn frozen_work_item(probe: &GitProbe, specs_dir: PathBuf) -> Result<Option<String>, Unavailable> {
    const STEP: &str = "looking for frozen work items";
    if let Some(work_item) = SpecStore::new(specs_dir)
        .frozen_work_item()
        .map_err(read_step(STEP))?
    {
        return Ok(Some(work_item));
    }
    let branches = probe.local_branches().map_err(git_step(STEP))?;

    Ok(branches.iter().find_map(|branch| {
        let id = work_item_id(branch)?;

        id.as_str()
            .strip_prefix(WORK_ITEM_ID_PREFIX)
            .map(str::to_owned)
    }))
}

/// Grava a marca nas pastas dos work items que têm branch local: é a rede para a branch apagada
/// depois. Nenhuma falha vira erro, só um aviso, e a decisão da trava não depende dela.
fn heal_freeze_marks(store: &SpecStore, branches: &[String]) -> Vec<AppError> {
    let work_items: BTreeSet<String> = branches
        .iter()
        .filter_map(|branch| work_item_id(branch))
        .filter_map(|id| {
            id.as_str()
                .strip_prefix(WORK_ITEM_ID_PREFIX)
                .map(str::to_owned)
        })
        .collect();
    work_items
        .into_iter()
        .filter(|work_item| mark_work_item(store, work_item).is_err())
        .map(|work_item| AppError::freeze_mark_unwritable(&work_item))
        .collect()
}

/// Marca todas as pastas possíveis; a falha de uma não deixa as outras sem marca, e o primeiro erro sai no fim.
fn mark_work_item(store: &SpecStore, work_item: &str) -> Result<(), SpecStoreError> {
    mark_folders(store, store.markable_folders(work_item)?).map(|_| ())
}

/// Grava a marca em cada pasta e devolve `(gravadas, já marcadas)`; a falha de uma não deixa as outras sem marca, e o primeiro erro sai no fim.
fn mark_folders(
    store: &SpecStore,
    folders: Vec<String>,
) -> Result<(Vec<String>, Vec<String>), SpecStoreError> {
    let (mut marked, mut already) = (Vec::new(), Vec::new());
    let mut first_error = None;
    for folder in folders {
        match store.write_freeze_mark(&folder) {
            Ok(FreezeMarkWrite::Written) => marked.push(folder),
            Ok(FreezeMarkWrite::AlreadyMarked) => already.push(folder),
            Err(error) => first_error = first_error.or(Some(error)),
        }
    }

    first_error.map_or(Ok((marked, already)), Err)
}

/// O nascimento de uma branch de work item (o `post-start` do `wt`): grava a marca em cada pasta
/// do work item, pela mesma gravação da cura. Não abre o banco e não exige o work item nele;
/// branch que não é de work item não tem efeito.
pub fn wt_post_start(start: &Path, branch: &str) -> Result<Value, AppError> {
    let located = find_marker(start)?.ok_or_else(AppError::no_marker)?;
    let Some(work_item) = work_item_id(branch).and_then(|id| {
        id.as_str()
            .strip_prefix(WORK_ITEM_ID_PREFIX)
            .map(str::to_owned)
    }) else {
        return Ok(json!({ "marked": [], "already": [] }));
    };
    let main_dir = GitProbe::new(&located.root)
        .main_dir()
        .map_err(|error| AppError::probe_failed(&error.message))?;
    let config = read_policy(&located.root).unwrap_or_default();
    let store = SpecStore::new(main_dir.join(&config.specs.dir));

    let folders = store
        .markable_folders(&work_item)
        .map_err(spec_store_refusal)?;
    let (marked, already) = mark_folders(&store, folders).map_err(spec_store_refusal)?;

    Ok(json!({ "marked": marked, "already": already }))
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
