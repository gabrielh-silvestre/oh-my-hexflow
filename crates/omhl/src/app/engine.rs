use std::path::{Path, PathBuf};

use hexlog::{
    AppendRequest, HexlogError, ItemBody, Limits, Lineage, NodeView, PageRequest, SemanticId,
    Snapshot, SnapshotKind, TenantId,
};
use serde_json::{Value, json};

use super::action::{Action, Caller};
use super::error::AppError;
use super::list_filter::validate_list_filter;
use super::retry::RetryPolicy;
use super::wire::append_request;
use super::{context, output, work_item};
use crate::byproduct::{ByproductRetention, ByproductStore};
use crate::error::{OmhlError, RuleCode};
use crate::facade::{Closing, Omhl, WorkRequirements};
use crate::graphs::WORK_ITEM_TYPES;
use crate::mechanical::branch::{branch_prefix, work_item_id};
use crate::ports::git::GitProbe;
use crate::pr_gate::{Abandonment, Forcing, GateSnapshots};
use crate::probe::{
    DevelopMark, NativeRefusal, NativeResolver, ProbeError, ProbeOutcome, SnapshotProbe, TreeSource,
};
use crate::project::layout::{OMHL_DIR, PROJECTION_DIR};
use crate::projection::{build::build, write::write};
use crate::runner::CommandRunner;
use crate::workflow::config::{NativeConfig, RequireAtom};
use crate::workflow::matrix::unsatisfied_atoms;
use crate::workflow::retro::{Retro, RetroAbandonment, retro};
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
    pub(super) mark: Option<&'d dyn DevelopMark>,
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
            Action::GatePr { work_item } => self.gate_pr(work_item),
            Action::Close { work_item, note } => self.close(caller, work_item, note.as_deref()),
            Action::Abandon {
                work_item,
                reason,
                note,
            } => self.abandon(caller, work_item, reason, note),
            Action::Verify => {
                let report = self.omhl.verify_chain(&self.tenant, None)?;

                Ok(output::verify(&report))
            }
            Action::Load { dry_run } => self.load(*dry_run),
            Action::Project => self.project(),
            Action::CheckRun { target, pre_push } => self.check_run(target, pre_push.as_deref()),
            Action::CheckRetire { check } => self.check_retire(check),
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
            Action::ConsumeDevelopMark => work_item::consume_develop_mark(self),
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
        self.check_type_changes(&parsed)?;

        let outcome = self
            .omhl
            .append(&parsed)
            .map_err(|error| AppError::from(error).guide_relation_refusal(&parsed.items))?;

        Ok(output::appended(&outcome))
    }

    fn gate_pr(&mut self, work_item: &str) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshots = self.probe_gate_snapshots()?;
        let enabled = self.resolve_native()?;
        let requirements = self.work_requirements(&work_item, &enabled)?;
        let report = self.omhl.evaluate_pr_gate(
            &self.tenant,
            &work_item,
            &snapshots,
            &enabled,
            &requirements,
        )?;

        Ok(output::gate(&report))
    }

    fn close(
        &mut self,
        caller: &Caller,
        work_item: &str,
        note: Option<&str>,
    ) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshots = self.probe_gate_snapshots()?;
        let enabled = self.resolve_native()?;
        self.omhl.ensure_open(&self.tenant, &work_item)?;
        let requirements = self.work_requirements(&work_item, &enabled)?;
        // Lida antes de gravar: o `close` nunca devolve erro depois do record de fechamento.
        let mut facts = self.retro_facts(&work_item)?;
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
                &closing,
            )
            .map_err(AppError::from)?;
        // A retro mostra a nota que acabou de ser gravada, que os fatos lidos antes ainda não têm.
        if let Some(Forcing::Forced(gate_override)) = &closed.forcing {
            facts.gate_override = Some(gate_override.clone());
        }

        Ok(output::closed(&closed, retro(&facts).as_ref()))
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
    fn native_refusal(&self) -> Result<NativeConfig, NativeRefusal> {
        self.native.ok_or(NativeRefusal::MainDirUnknown)?.resolve()
    }

    fn resolve_native(&self) -> Result<NativeConfig, AppError> {
        self.native_refusal().map_err(|refusal| match refusal {
            NativeRefusal::MainDirUnknown => AppError::main_dir_unknown(),
            NativeRefusal::InvalidWorkflow(error) => AppError::invalid_workflow(error),
        })
    }

    /// Lê uma vez o que o tipo do item exige: o tipo, os átomos e as premissas sob uma mesma cabeça.
    fn work_requirements(
        &mut self,
        work_item: &SemanticId,
        enabled: &NativeConfig,
    ) -> Result<WorkRequirements, AppError> {
        let node = self.omhl.find_work_item(&self.tenant, work_item)?;
        let atoms = self.type_atoms(&node, enabled)?;
        if atoms.is_empty() {
            return Ok(WorkRequirements {
                head: None,
                atoms: Vec::new(),
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
            None => self.branch_prefixes(&node.id, node.type_name.as_str(), &node.data)?,
        };
        let names: Vec<&str> = prefixes.iter().map(String::as_str).collect();
        let name = effective_type(recorded, &names, &enabled.prefixes).ok_or_else(|| {
            AppError::from(OmhlError::rule(RuleCode::WorkItemTypeUnknown, "/work_item"))
        })?;

        Ok(enabled.prefixes[&name].clone())
    }

    /// Os prefixos dos branches que dão tipo ao item: o do `branch` do pull-request, ou os branches locais do work item.
    fn branch_prefixes(
        &self,
        id: &SemanticId,
        type_name: &str,
        data: &Value,
    ) -> Result<Vec<String>, AppError> {
        let mut prefixes: Vec<String> = if type_name == "pull-request" {
            data["branch"]
                .as_str()
                .and_then(branch_prefix)
                .map(str::to_owned)
                .into_iter()
                .collect()
        } else {
            let root = require_project_root(self.project_root.as_deref())?;
            GitProbe::new(root)
                .local_branches()
                .map_err(|error| AppError::probe_failed(&error.message))?
                .iter()
                .filter(|branch| work_item_id(branch).as_ref() == Some(id))
                .filter_map(|branch| branch_prefix(branch).map(str::to_owned))
                .collect()
        };
        prefixes.sort();
        prefixes.dedup();

        Ok(prefixes)
    }

    /// CMP-005: a troca do tipo para um tipo que exige menos sai antes da gravação.
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

        let prefixes = self.branch_prefixes(id, type_name, data)?;

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
