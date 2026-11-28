use std::path::{Path, PathBuf};

use hexlog::{
    AppendRequest, HexlogError, ItemBody, Limits, Lineage, NodeView, PageRequest, SemanticId,
    Snapshot, SnapshotKind, TenantId,
};
use serde_json::{Value, json};

use super::action::{Action, Caller};
use super::error::{AppError, INVALID_WORKFLOW_NEXT_STEP};
use super::list_filter::validate_list_filter;
use super::retry::RetryPolicy;
use super::wire::append_request;
use super::{context, output, work_item};
use crate::byproduct::{ByproductRetention, ByproductStore};
use crate::delivery_gate::{Abandonment, FailedDelivery, Forcing, GateSnapshots, Integration};
use crate::error::{OmhlError, RuleCode};
use crate::facade::{Closing, Omhl, WorkRequirements};
use crate::graphs::WORK_ITEM_TYPES;
use crate::mechanical::branch::{BranchKind, classify, integration};
use crate::native::{CONFIG_LOOSENING_REVIEW_ID, review_trigger_paths};
use crate::ports::git::{BlobReason, GitProbe, WorkflowSource, workflow_text};
use crate::probe::{
    CommitMark, NativeRefusal, NativeResolver, ProbeError, ProbeOutcome, SnapshotProbe, TreeSource,
};
use crate::project::ProjectError;
use crate::project::layout::{OMHL_DIR, PROJECTION_DIR};
use crate::projection::{build::build, write::write};
use crate::runner::CommandRunner;
use crate::workflow::branch_policy::BranchConfig;
use crate::workflow::config::{
    NativeConfig, RequireAtom, parse_evaluated_workflow, parse_workflow,
};
use crate::workflow::matrix::unsatisfied_atoms;
use crate::workflow::retro::{DeliveryFailure, Retro, RetroAbandonment, retro};
use crate::workflow::work_type::{effective_type, loosens};

const DEFAULT_PAGE_LIMIT: u32 = 50;

#[derive(Clone, Copy)]
pub(super) struct Exec<'d> {
    pub(super) runner: &'d dyn CommandRunner,
    pub(super) byproducts: &'d dyn ByproductStore,
}

pub(super) struct Engine<'d> {
    pub(super) omhl: Omhl<'d>,
    pub(super) tenant: TenantId,
    pub(super) probe: &'d dyn SnapshotProbe,
    pub(super) project_root: Option<PathBuf>,
    pub(super) exec: Option<Exec<'d>>,
    pub(super) retention: Option<&'d dyn ByproductRetention>,
    pub(super) mark: Option<&'d dyn CommitMark>,
    pub(super) native: Option<&'d dyn NativeResolver>,
    pub(super) retry: RetryPolicy,
    pub(super) retry_after_run: RetryPolicy,
    pub(super) hooks_dir: Option<PathBuf>,
}

pub(super) fn parse_at<T>(
    parse: fn(&str) -> Result<T, HexlogError>,
    text: &str,
    pointer: &str,
) -> Result<T, AppError> {
    parse(text).map_err(|error| AppError::from(error.at(pointer)))
}

const ABANDON_TEXT_MAX: usize = 500;

fn bounded_text(text: &str, pointer: &str) -> Result<(), AppError> {
    match (1..=ABANDON_TEXT_MAX).contains(&text.chars().count()) {
        true => Ok(()),
        false => Err(AppError::at_pointer(
            "INVALID_INPUT",
            pointer,
            "must have 1 to 500 characters",
        )),
    }
}

fn reproducible(probed: Result<ProbeOutcome, ProbeError>) -> Result<Snapshot, AppError> {
    match probed {
        Ok(ProbeOutcome::Reproducible(snapshot)) => Ok(snapshot),
        Ok(ProbeOutcome::Dirty) => {
            Err(OmhlError::rule(RuleCode::SnapshotDirty, "/snapshot").into())
        }
        Err(error) => Err(AppError::probe_failed(&error.message)),
    }
}

impl Engine<'_> {
    pub(super) fn run(&mut self, caller: &Caller, action: &Action) -> Result<Value, AppError> {
        match action {
            Action::List {
                type_name,
                id_prefix,
                limit,
                after_seq,
                where_eq,
            } => self.list(
                type_name.as_deref(),
                id_prefix.as_deref(),
                *limit,
                *after_seq,
                where_eq,
            ),
            Action::Get { id } => self.get(id),
            Action::Append { request } => self.append(caller, request),
            Action::Attest {
                check,
                work_item,
                outcome,
                note,
            } => self.attest(caller, check, work_item, outcome, note.as_deref()),
            Action::Render => {
                let native = self.native_refusal();

                context::render_context(&mut self.omhl, &self.tenant, native)
            }
            Action::GateDelivery { work_item, base } => {
                self.gate_delivery(work_item, base.as_deref())
            }
            Action::Close {
                work_item,
                note,
                base,
            } => self.close(caller, work_item, note.as_deref(), base.as_deref()),
            Action::CloseFailedDelivery {
                work_item,
                note,
                base,
            } => self.close_failed_delivery(caller, work_item, note, base.as_deref()),
            Action::Abandon {
                work_item,
                reason,
                note,
            } => self.abandon(caller, work_item, reason, note),
            Action::Verify => {
                let report = self.omhl.verify_chain(&self.tenant, None)?;

                Ok(output::verify(&report))
            }
            Action::Load { dry_run } => self.load(*dry_run, caller.owner_override),
            Action::Project => self.project(),
            Action::CheckRun { target, pre_push } => self.check_run(target, pre_push.as_deref()),
            Action::CheckRetire { check } => self.check_retire(check, caller.owner_override),
            Action::GuidelineRetire { id, by } => self.guideline_retire(caller, id, by.as_deref()),
            Action::Byproduct {
                check,
                work_item,
                tail,
            } => self.byproduct(check, work_item, *tail),
            Action::ByproductPrune => self.byproduct_prune(),
            Action::HooksInstall { force } => self.hooks_install(*force),
            Action::SpecInit { work_item, slug } => self.spec_init(work_item, slug),
            Action::SpecValidate { work_item } => self.spec_validate(work_item),
            Action::ReportAnswers { work_item } => self.report_answers(work_item.as_deref()),
            Action::ReportRecurrences { min } => self.report_recurrences(*min),
            Action::ReportRetro { work_item } => self.report_retro(work_item),
            Action::ReportSessions { work_items } => self.report_sessions(work_items),
            Action::ConsumeCommitMark => work_item::consume_commit_mark(self),
            Action::CheckSweep | Action::GateWorktree { .. } => {
                Err(AppError::action_outside_engine())
            }
        }
    }

    fn list(
        &mut self,
        type_name: Option<&str>,
        id_prefix: Option<&str>,
        limit: Option<u32>,
        after_seq: Option<u64>,
        where_eq: &[(String, String)],
    ) -> Result<Value, AppError> {
        let page_max = Limits::default().page_max;
        if limit.is_some_and(|limit| limit == 0 || limit > page_max) {
            return Err(AppError::invalid_arguments(
                "/limit",
                &format!("limit must be between 1 and {page_max}"),
            ));
        }
        let filter = validate_list_filter(self.omhl.catalog(), type_name, id_prefix, where_eq)?;
        let request = PageRequest {
            limit: limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            after_seq,
        };
        let page = self.omhl.list(&self.tenant, &filter, &request)?;

        Ok(output::page(&page))
    }

    fn get(&mut self, id: &str) -> Result<Value, AppError> {
        let id = parse_at(SemanticId::parse, id, "/id")?;
        let node = self.omhl.get(&self.tenant, &id)?;

        Ok(json!({ "node": node.as_ref().map(output::node) }))
    }

    fn append(&mut self, caller: &Caller, request: &Value) -> Result<Value, AppError> {
        let parsed = append_request(request, &self.tenant, caller)?;
        self.check_amendment_approvals(request)?;
        if !caller.owner_override {
            self.check_type_changes(&parsed)?;
        }

        let appended = match caller.owner_override {
            true => self.omhl.append_owner_override(&parsed),
            false => self.omhl.append(&parsed),
        };
        let outcome = appended
            .map_err(|error| AppError::from(error).guide_relation_refusal(&parsed.items))?;

        Ok(output::appended(&outcome))
    }

    fn gate_delivery(&mut self, work_item: &str, base: Option<&str>) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshots = self.probe_gate_snapshots()?;
        let (enabled, repair) = self.resolve_native_for_pr(base)?;
        let requirements = self.work_requirements(&work_item, &enabled)?;
        let (mut review_base, review_required) = self.review_base(base, &enabled)?;
        mark_repair(&mut review_base, repair);
        let report = self.omhl.evaluate_delivery_gate(
            &self.tenant,
            &work_item,
            &snapshots,
            &enabled,
            &requirements,
            review_required,
        )?;
        let mut value = output::gate(&report);
        if !review_base.is_null() {
            value["review_base"] = review_base;
        }
        self.add_ignored_run_warnings(&mut value, &enabled);

        Ok(value)
    }

    fn close(
        &mut self,
        caller: &Caller,
        work_item: &str,
        note: Option<&str>,
        base: Option<&str>,
    ) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshots = self.probe_gate_snapshots()?;
        let (enabled, repair) = self.resolve_native_for_pr(base)?;
        self.omhl.ensure_open(&self.tenant, &work_item)?;
        let requirements = self.work_requirements(&work_item, &enabled)?;
        let (mut review_base, review_required) = self.review_base(base, &enabled)?;
        mark_repair(&mut review_base, repair);
        // Lida antes de gravar: o `close` nunca devolve erro depois do record de fechamento.
        let mut facts = self.retro_facts(&work_item, false)?;
        let author = caller.author();
        let closing = Closing {
            author: &author,
            note,
        };
        let closed = self
            .omhl
            .close_work_item(
                &self.tenant,
                &work_item,
                &snapshots,
                &enabled,
                &requirements,
                review_required,
                &closing,
            )
            .map_err(|error| AppError::from(error).with_review_base(&review_base))?;
        // A retro mostra a nota que acabou de ser gravada, que os fatos lidos antes ainda não têm.
        if let Some(Forcing::Forced(gate_override)) = &closed.forcing {
            facts.gate_override = Some(gate_override.clone());
        }

        let mut value = output::closed(&closed, retro(&facts).as_ref());
        if !review_base.is_null() {
            value["review_base"] = review_base;
        }
        self.add_ignored_run_warnings(&mut value, &enabled);

        Ok(value)
    }

    /// Com `--base` a base é conferida mesmo com o check desligado; a revisão só é exigida com o check ligado e, havendo base, com caminho gatilho.
    fn review_base(
        &self,
        base: Option<&str>,
        enabled: &NativeConfig,
    ) -> Result<(Value, bool), AppError> {
        let on = enabled.checks.contains(CONFIG_LOOSENING_REVIEW_ID);
        let declared = integration(&enabled.branch);
        if base.is_none() && declared.is_none() {
            return Ok((no_base_note(on, "no base was given"), on));
        }
        let root = require_project_root(self.project_root.as_deref())?;
        let git = GitProbe::new(root);
        let probe_failed = |error: ProbeError| AppError::probe_failed(&error.message);
        let head = git.ref_commit("HEAD").map_err(probe_failed)?;
        let range_from = |base: &str| -> Result<String, AppError> {
            git.merge_base(base)
                .map_err(probe_failed)?
                .filter(|found| Some(found) != head.as_ref())
                .ok_or_else(AppError::review_base_unresolved)
        };
        let (shown, merge_base, origin) = match (declared, base) {
            (None, base) => {
                let base = base.unwrap_or_default();
                (
                    base.to_owned(),
                    range_from(base)?,
                    "caller --base".to_owned(),
                )
            }
            (Some(name), base) => {
                // Só a ref local: o nome nu pode resolver para uma tag.
                let integration_ref = format!("refs/heads/{name}");
                git.ref_commit(&integration_ref)
                    .map_err(probe_failed)?
                    .ok_or_else(AppError::review_integration_missing)?;
                let merged = git
                    .is_ancestor("HEAD", &integration_ref)
                    .map_err(probe_failed)?;
                match (merged, base) {
                    (true, None) => {
                        let note = format!(
                            "integration branch {name} contains HEAD and no base was given"
                        );
                        return Ok((no_base_note(on, &note), on));
                    }
                    (true, Some(base)) => {
                        let commit = git
                            .ref_commit(base)
                            .map_err(probe_failed)?
                            .ok_or_else(AppError::review_base_unresolved)?;
                        if !git
                            .is_ancestor(&commit, &integration_ref)
                            .map_err(probe_failed)?
                        {
                            return Err(AppError::review_base_not_in_integration());
                        }
                        (
                            base.to_owned(),
                            range_from(&commit)?,
                            format!("caller --base; integration branch {name} contains HEAD"),
                        )
                    }
                    (false, base) => {
                        let merge_base = range_from(&integration_ref)?;
                        if let Some(base) = base
                            && range_from(base)? != merge_base
                        {
                            return Err(AppError::review_base_other_merge_base());
                        }
                        (
                            name.to_owned(),
                            merge_base,
                            format!("integration branch {name}"),
                        )
                    }
                }
            }
        };
        let changed = git.changed_names(&merge_base).map_err(probe_failed)?;
        let paths = review_trigger_paths(&changed);
        let required = on && !paths.is_empty();

        Ok((
            json!({
                "base": shown,
                "merge_base": merge_base,
                "paths": paths,
                "origin": origin,
            }),
            required,
        ))
    }

    fn close_failed_delivery(
        &mut self,
        caller: &Caller,
        work_item: &str,
        note: &str,
        base: Option<&str>,
    ) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        bounded_text(note, "/note")?;
        let snapshots = self.probe_gate_snapshots()?;
        let (enabled, _) = self.resolve_native_for_pr(base)?;
        let node = self.omhl.ensure_open(&self.tenant, &work_item)?;
        let requirements = self.work_requirements(&work_item, &enabled)?;
        let integration = self.read_integration(&node, &snapshots, &enabled.branch)?;
        let (_, review_required) = self.review_base(base, &enabled)?;
        let mut facts = self.retro_facts(&work_item, true)?;
        let author = caller.author();
        let delivery = FailedDelivery {
            author: &author,
            note,
            integration: &integration,
        };
        let (closed, failed) = self
            .omhl
            .close_with_failed_delivery(
                &self.tenant,
                &work_item,
                &snapshots,
                &enabled,
                &requirements,
                review_required,
                &delivery,
            )
            .map_err(AppError::from)?;
        facts.delivery_failure = Some(DeliveryFailure {
            note: note.to_owned(),
            failed,
        });

        Ok(output::closed(&closed, retro(&facts).as_ref()))
    }

    /// O commit avaliado só vale como entregue se o branch do item o tem dentro da integração declarada local.
    fn read_integration(
        &self,
        node: &NodeView,
        snapshots: &GateSnapshots,
        policy: &BranchConfig,
    ) -> Result<Integration, AppError> {
        let root = require_project_root(self.project_root.as_deref())?;
        let git = GitProbe::new(root);
        let probe_failed = |error: ProbeError| AppError::probe_failed(&error.message);
        let branch = git.current_branch().map_err(probe_failed)?;
        let on_item_branch = match (node.type_name.as_str(), &branch) {
            (_, None) => false,
            ("pull-request", Some(branch)) => node.data["branch"].as_str() == Some(branch),
            (_, Some(branch)) => matches!(
                classify(policy, branch),
                BranchKind::WorkItem { id, .. } if id == node.id
            ),
        };
        if !on_item_branch {
            return Err(AppError::delivery_not_on_work_item_branch());
        }
        let reference = integration(policy)
            .ok_or_else(AppError::delivery_integration_undeclared)?
            .to_owned();
        let integration_ref = format!("refs/heads/{reference}");
        let commit = git
            .ref_commit(&integration_ref)
            .map_err(probe_failed)?
            .ok_or_else(AppError::delivery_not_integrated)?;
        if !git
            .is_ancestor(snapshots.head_commit.id(), &commit)
            .map_err(probe_failed)?
        {
            return Err(AppError::delivery_not_integrated());
        }

        Ok(Integration { reference, commit })
    }

    fn abandon(
        &mut self,
        caller: &Caller,
        work_item: &str,
        reason: &str,
        note: &str,
    ) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        bounded_text(reason, "/reason")?;
        bounded_text(note, "/note")?;
        self.omhl.ensure_open(&self.tenant, &work_item)?;
        let head = self
            .probe
            .head_facts()
            .ok()
            .and_then(|facts| facts.head)
            .and_then(|head| Snapshot::new(SnapshotKind::GitCommit, &head).ok())
            .ok_or_else(|| AppError::probe_failed("HEAD has no commit"))?;
        let abandonment = Abandonment { reason, note };
        let outcome = self.omhl.abandon_work_item(
            &self.tenant,
            &work_item,
            &caller.author(),
            &head,
            &abandonment,
        )?;

        let report = Retro {
            abandonment: Some(RetroAbandonment {
                reason: reason.to_owned(),
                note: note.to_owned(),
            }),
            ..Retro::default()
        };

        Ok(json!({
            "outcome": output::appended(&outcome),
            "abandoned": { "reason": reason, "note": note },
            "retro": output::retro(Some(&report)),
        }))
    }

    fn project(&mut self) -> Result<Value, AppError> {
        let root = require_project_root(self.project_root.as_deref())?;
        let state = self.omhl.read_projection_state(&self.tenant)?;
        let files = build(&state).map_err(|error| AppError::projection_failed(&error))?;
        let report = write(root, &files).map_err(|error| AppError::projection_failed(&error))?;

        Ok(json!({
            "dir": format!("{OMHL_DIR}/{PROJECTION_DIR}"),
            "written": report.written,
            "unchanged": report.unchanged,
            "removed": report.removed,
        }))
    }

    pub(super) fn probe_snapshot(&self) -> Result<Snapshot, AppError> {
        reproducible(self.probe.probe())
    }

    /// Sem resolvedor injetado não há diretório principal: nunca vale um conjunto vazio no lugar.
    pub(super) fn native_refusal(&self) -> Result<NativeConfig, NativeRefusal> {
        self.native.ok_or(NativeRefusal::MainDirUnknown)?.resolve()
    }

    fn resolve_native(&self) -> Result<NativeConfig, AppError> {
        self.native_refusal().map_err(|refusal| match refusal {
            NativeRefusal::MainDirUnknown => AppError::main_dir_unknown(),
            NativeRefusal::InvalidWorkflow(error) => AppError::invalid_workflow(error),
        })
    }

    /// Com o arquivo principal ilegível, só o PR que traz um `.omhl/workflow.toml` aceitável no `HEAD` segue, com a revisão exigida.
    fn resolve_native_for_pr(&self, base: Option<&str>) -> Result<(NativeConfig, bool), AppError> {
        match self.native_refusal() {
            Ok(config) => Ok((config, false)),
            Err(NativeRefusal::MainDirUnknown) => Err(AppError::main_dir_unknown()),
            Err(NativeRefusal::InvalidWorkflow(error)) => base
                .and_then(|base| self.repaired_native(base))
                .map(|config| (config, true))
                .ok_or_else(|| AppError::invalid_workflow(error)),
        }
    }

    /// Tabela `[native.run.<id>]` de nativo desligado no próprio arquivo é aceita e ignorada; o aviso diz de qual arquivo.
    fn add_ignored_run_warnings(&self, value: &mut Value, main: &NativeConfig) {
        add_warnings(
            value,
            run_warnings(main, self.read_evaluated_head(), "HEAD"),
        );
    }

    pub(super) fn evaluated_native(&self) -> NativeConfig {
        self.read_evaluated_head().0.unwrap_or_default()
    }

    fn read_evaluated_head(&self) -> (Option<NativeConfig>, Option<Value>) {
        read_evaluated_native(
            self.project_root.as_deref(),
            WorkflowSource::Rev("HEAD"),
            "HEAD",
        )
    }

    fn repaired_native(&self, base: &str) -> Option<NativeConfig> {
        let git = GitProbe::new(self.project_root.as_deref()?);
        let merge_base = git.merge_base(base).ok()??;
        let changed = git.changed_names(&merge_base).ok()?;
        if !review_trigger_paths(&changed).contains(&REPAIR_PATH.to_owned()) {
            return None;
        }
        let text = workflow_text(&git, WorkflowSource::Rev("HEAD")).ok()?;
        let config = parse_workflow(Path::new(REPAIR_PATH), text.as_bytes()).ok()?;
        let mut checks = config.native.checks;
        checks.insert(CONFIG_LOOSENING_REVIEW_ID);

        Some(NativeConfig {
            checks,
            run: Default::default(),
            invalid_run: Default::default(),
            prefixes: config.prefixes,
            // A política vem do leitor estável, nunca da cópia da branch.
            branch: self
                .native
                .map(|native| native.stable_branch())
                .unwrap_or_default(),
        })
    }

    /// Leitores que não recusam um principal ilegível: a política sai do leitor estável.
    pub(super) fn lenient_branch_policy(&self) -> BranchConfig {
        match self.native_refusal() {
            Ok(config) => config.branch,
            Err(NativeRefusal::InvalidWorkflow(_)) => self
                .native
                .map(|native| native.stable_branch())
                .unwrap_or_default(),
            Err(NativeRefusal::MainDirUnknown) => BranchConfig::default(),
        }
    }

    fn work_requirements(
        &mut self,
        work_item: &SemanticId,
        enabled: &NativeConfig,
    ) -> Result<WorkRequirements, AppError> {
        let evaluated = self.evaluated_native();
        let node = self.omhl.find_work_item(&self.tenant, work_item)?;
        let atoms = self.type_atoms(&node, enabled)?;
        if atoms.is_empty() {
            return Ok(WorkRequirements {
                head: None,
                atoms: Vec::new(),
                evaluated,
            });
        }
        let head = self.omhl.chain_head(&self.tenant)?;
        let facts = self.premise_facts(work_item)?;
        let spec_valid = atoms.contains(&RequireAtom::Spec) && self.spec_is_valid(work_item)?;
        let unsatisfied = unsatisfied_atoms(&atoms, &facts, spec_valid);

        Ok(WorkRequirements {
            head: Some(head),
            atoms: atoms
                .into_iter()
                .map(|atom| (atom, !unsatisfied.contains(&atom)))
                .collect(),
            evaluated,
        })
    }

    /// O tipo vem do campo `type`; sem ele, do prefixo do branch do próprio item (nunca do `HEAD`).
    fn type_atoms(
        &self,
        node: &NodeView,
        enabled: &NativeConfig,
    ) -> Result<Vec<RequireAtom>, AppError> {
        let recorded = node.data["type"].as_str();
        let prefixes = match recorded {
            Some(_) => Vec::new(),
            None => self.branch_prefixes(
                &enabled.branch,
                &node.id,
                node.type_name.as_str(),
                &node.data,
            )?,
        };
        let names: Vec<&str> = prefixes.iter().map(String::as_str).collect();
        let name = effective_type(recorded, &names, &enabled.prefixes).ok_or_else(|| {
            AppError::from(OmhlError::rule(RuleCode::WorkItemTypeUnknown, "/work_item"))
        })?;

        Ok(enabled.prefixes[&name].clone())
    }

    fn branch_prefixes(
        &self,
        policy: &BranchConfig,
        id: &SemanticId,
        type_name: &str,
        data: &Value,
    ) -> Result<Vec<String>, AppError> {
        let mut prefixes: Vec<String> = if type_name == "pull-request" {
            data["branch"]
                .as_str()
                .and_then(|branch| branch_type(policy, branch))
                .map(|(_, kind)| kind)
                .into_iter()
                .collect()
        } else {
            let root = require_project_root(self.project_root.as_deref())?;
            GitProbe::new(root)
                .local_branches()
                .map_err(|error| AppError::probe_failed(&error.message))?
                .iter()
                .filter_map(|branch| branch_type(policy, branch))
                .filter(|(found, _)| found == id)
                .map(|(_, kind)| kind)
                .collect()
        };
        prefixes.sort();
        prefixes.dedup();

        Ok(prefixes)
    }

    /// A troca do tipo para um tipo que exige menos sai antes da gravação.
    fn check_type_changes(&mut self, request: &AppendRequest) -> Result<(), AppError> {
        for (index, item) in request.items.iter().enumerate() {
            let ItemBody::Node {
                id,
                type_name,
                data,
            } = &item.body
            else {
                continue;
            };
            if !WORK_ITEM_TYPES.contains(&type_name.as_str()) {
                continue;
            }
            // Um nó novo não tem revisão anterior para comparar.
            if self.omhl.get(&self.tenant, id)?.is_none() {
                continue;
            }
            let next = data["type"].as_str();
            let Some(previous) = self.previous_type(id, type_name.as_str(), data, next)? else {
                continue;
            };
            if next == Some(previous.as_str()) {
                continue;
            }
            // Com o arquivo principal ilegível a recusa é `invalid-workflow-config`, antes de qualquer comparação.
            let table = self.resolve_native()?.prefixes;
            let Some(previous_atoms) = table.get(&previous) else {
                continue;
            };
            let next_atoms = next
                .and_then(|name| table.get(name))
                .map_or(&[][..], Vec::as_slice);
            if loosens(previous_atoms, next_atoms) {
                return Err(OmhlError::rule(
                    RuleCode::TypeChangeLoosens,
                    &format!("/items/{index}/body/data/type"),
                )
                .into());
            }
        }

        Ok(())
    }

    /// O último `type` não vazio da linhagem (uma remoção não o apaga); sem nenhum, o do branch, e só se a revisão grava um tipo.
    fn previous_type(
        &mut self,
        id: &SemanticId,
        type_name: &str,
        data: &Value,
        next: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let mut revisions = self
            .omhl
            .lineage_records(&self.tenant, &Lineage::Node(id.clone()))?;
        revisions.sort_by_key(|revision| std::cmp::Reverse(revision.seq));
        let recorded = revisions
            .iter()
            .filter_map(|revision| revision.data.as_ref())
            .find_map(|data| data["type"].as_str().filter(|name| !name.is_empty()));
        if let Some(name) = recorded {
            return Ok(Some(name.to_owned()));
        }
        if next.is_none() {
            return Ok(None);
        }

        let stored = self.omhl.get(&self.tenant, id)?.map(|node| node.data);
        let policy = self.resolve_native()?.branch;
        let prefixes =
            self.branch_prefixes(&policy, id, type_name, stored.as_ref().unwrap_or(data))?;

        Ok(match prefixes.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        })
    }

    fn probe_gate_snapshots(&self) -> Result<GateSnapshots, AppError> {
        Ok(GateSnapshots {
            head_commit: self.probe_snapshot()?,
            head_tree: reproducible(self.probe.probe_tree(TreeSource::Head))?,
        })
    }
}

pub(super) fn require_project_root(root: Option<&Path>) -> Result<&Path, AppError> {
    root.ok_or_else(AppError::no_project_root)
}

const REPAIR_PATH: &str = ".omhl/workflow.toml";
const REPAIR_WARNING: &str = "the main workflow file is not accepted; this pull request repairs it, so its HEAD version was used and the review is required";

fn no_base_note(on: bool, note: &str) -> Value {
    match on {
        true => json!({ "base": null, "note": note }),
        false => Value::Null,
    }
}

fn mark_repair(review_base: &mut Value, repair: bool) {
    if repair {
        review_base["warning"] = json!(REPAIR_WARNING);
    }
}

pub(super) fn ignored_run_warnings<'a>(
    config: &'a NativeConfig,
    from: &'a str,
) -> impl Iterator<Item = Value> + 'a {
    config
        .run
        .keys()
        .filter(|id| !config.checks.iter().any(|enabled| enabled == id))
        .map(move |id| {
            json!({
                "code": "native-run-ignored",
                "check": id,
                "from": from,
                "hint": format!("enable the native in [native] checks or delete the table [native.run.\"{id}\"]"),
            })
        })
}

/// Avisos de tabela ignorada: do principal, do arquivo avaliado em `from` e o de leitura que falhou.
pub(super) fn run_warnings(
    main: &NativeConfig,
    (evaluated, unreadable): (Option<NativeConfig>, Option<Value>),
    from: &str,
) -> Vec<Value> {
    ignored_run_warnings(main, "main-directory")
        .chain(evaluated_run_warnings(
            main,
            &evaluated.unwrap_or_default(),
            from,
        ))
        .chain(unreadable)
        .collect()
}

/// Tabela igual à do diretório principal já foi avisada por ele; repetir o aviso só duplicaria.
fn evaluated_run_warnings<'a>(
    main: &'a NativeConfig,
    evaluated: &'a NativeConfig,
    from: &'a str,
) -> impl Iterator<Item = Value> + 'a {
    ignored_run_warnings(evaluated, from).filter(|warning| {
        warning["check"]
            .as_str()
            .is_none_or(|id| evaluated.run.get(id) != main.run.get(id))
    })
}

/// Declaração avaliada de uma origem (índice ou `HEAD`) e o aviso de arquivo ilegível.
pub(super) fn read_evaluated_native(
    root: Option<&Path>,
    source: WorkflowSource<'_>,
    from: &str,
) -> (Option<NativeConfig>, Option<Value>) {
    let Some(root) = root else {
        return (None, None);
    };
    match workflow_text(&GitProbe::new(root), source) {
        Ok(text) => (
            parse_evaluated_workflow(Path::new(REPAIR_PATH), text.as_bytes())
                .ok()
                .map(|config| config.native),
            None,
        ),
        Err(BlobReason::NotUtf8) => (
            None,
            Some(json!({
                "code": "evaluated-workflow-unreadable",
                "from": from,
                "reason": "not-utf8",
            })),
        ),
        Err(_) => (None, None),
    }
}

pub(super) fn invalid_workflow_warning(error: &ProjectError) -> Value {
    json!({
        "code": error.code,
        "message": error.message,
        "hint": INVALID_WORKFLOW_NEXT_STEP,
    })
}

pub(super) fn add_warnings(value: &mut Value, warnings: Vec<Value>) {
    if warnings.is_empty() {
        return;
    }
    match value["warnings"].as_array_mut() {
        Some(existing) => existing.extend(warnings),
        None => value["warnings"] = json!(warnings),
    }
}

fn branch_type(policy: &BranchConfig, branch: &str) -> Option<(SemanticId, String)> {
    match classify(policy, branch) {
        BranchKind::WorkItem {
            id,
            kind: Some(kind),
        } => Some((id, kind)),
        _ => None,
    }
}
