use std::path::{Path, PathBuf};

use hexlog::{
    FieldName, HexlogError, ListFilter, PageRequest, Scalar, SemanticId, Snapshot, TenantId,
    TypeName,
};
use serde_json::{Value, json};

use super::action::{Action, Caller};
use super::error::AppError;
use super::retry::RetryPolicy;
use super::wire::append_request;
use super::{context, output, work_item};
use crate::byproduct::ByproductStore;
use crate::error::{OmhlError, RuleCode};
use crate::facade::Omhl;
use crate::pr_gate::GateSnapshots;
use crate::probe::{DevelopMark, ProbeError, ProbeOutcome, SnapshotProbe, TreeSource};
use crate::project::layout::{OMHL_DIR, PROJECTION_DIR};
use crate::projection::{build::build, write::write};
use crate::runner::CommandRunner;
use crate::workflow::retro::retro;

/// Tamanho de página quando o pedido não traz `limit` (o padrão do motor).
const DEFAULT_PAGE_LIMIT: u32 = 50;
/// Teto de um `SemanticId` no `hexlog`; um prefixo maior nunca casa.
const ID_PREFIX_MAX: usize = 200;

/// Estado que as ações compartilham; as tentativas ficam por conta do `App`.
pub(super) struct Engine<'d> {
    pub(super) omhl: Omhl<'d>,
    pub(super) tenant: TenantId,
    pub(super) probe: &'d dyn SnapshotProbe,
    pub(super) project_root: Option<PathBuf>,
    pub(super) runner: Option<&'d dyn CommandRunner>,
    pub(super) byproducts: Option<&'d dyn ByproductStore>,
    pub(super) mark: Option<&'d dyn DevelopMark>,
    /// Cópia da política do `App`, para a leitura inicial do `check run`.
    pub(super) retry: RetryPolicy,
    pub(super) retry_after_run: RetryPolicy,
    /// Pasta de usuário dos hooks do git; `None` só com `--db` e sem `HOME` nem `XDG_DATA_HOME`.
    pub(super) hooks_dir: Option<PathBuf>,
}

/// Id ou nome do pedido que não passa em `parse`: erro do motor com o ponteiro do campo.
pub(super) fn parse_at<T>(
    parse: fn(&str) -> Result<T, HexlogError>,
    text: &str,
    pointer: &str,
) -> Result<T, AppError> {
    parse(text).map_err(|error| AppError::from(error.at(pointer)))
}

fn is_id_prefix(prefix: &str) -> bool {
    prefix.len() <= ID_PREFIX_MAX
        && prefix
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.'))
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
            Action::Render => context::render_context(&mut self.omhl, &self.tenant),
            Action::GatePr { work_item } => self.gate_pr(work_item),
            Action::Close { work_item } => self.close(caller, work_item),
            Action::Verify => {
                let report = self.omhl.verify_chain(&self.tenant, None)?;

                Ok(output::verify(&report))
            }
            Action::Load { dry_run } => self.load(*dry_run),
            Action::Project => self.project(),
            Action::CheckRun { target, pre_push } => self.check_run(target, pre_push.as_deref()),
            Action::CheckRetire { check } => self.check_retire(check),
            Action::Byproduct {
                check,
                work_item,
                tail,
            } => self.byproduct(check, work_item, *tail),
            Action::HooksInstall { force } => self.hooks_install(*force),
            Action::SpecInit { work_item, slug } => self.spec_init(work_item, slug),
            Action::SpecValidate { work_item } => self.spec_validate(work_item),
            Action::ReportAnswers { work_item } => self.report_answers(work_item.as_deref()),
            Action::ReportRecurrences { min } => self.report_recurrences(*min),
            Action::ReportRetro { work_item } => self.report_retro(work_item),
            Action::ConsumeDevelopMark => work_item::consume_develop_mark(self),
            Action::CheckSweep => unreachable!("check sweep is dispatched by the runtime"),
            Action::GateWorktree { .. } => {
                unreachable!("gate worktree is dispatched by the runtime")
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
        if limit == Some(0) {
            return Err(AppError::invalid_arguments("/limit", "unknown value"));
        }
        if id_prefix.is_some_and(|prefix| !is_id_prefix(prefix)) {
            return Err(AppError::invalid_arguments("/id_prefix", "unknown value"));
        }
        let filter = ListFilter {
            type_name: type_name
                .map(|name| parse_at(TypeName::parse, name, "/type"))
                .transpose()?,
            id_prefix: id_prefix.map(str::to_owned),
            snapshot: None,
            where_eq: where_eq
                .iter()
                .map(|(field, value)| {
                    let name = parse_at(FieldName::parse, field, &format!("/where/{field}"))?;
                    Ok((name, Scalar::Str(value.clone())))
                })
                .collect::<Result<_, AppError>>()?,
        };
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

        let outcome = self
            .omhl
            .append(&parsed)
            .map_err(|error| AppError::from(error).guide_relation_refusal(&parsed.items))?;

        Ok(output::appended(&outcome))
    }

    fn gate_pr(&mut self, work_item: &str) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshots = self.probe_gate_snapshots()?;
        let report = self
            .omhl
            .evaluate_pr_gate(&self.tenant, &work_item, &snapshots)?;

        Ok(output::gate(&report))
    }

    fn close(&mut self, caller: &Caller, work_item: &str) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshots = self.probe_gate_snapshots()?;
        // Lida antes de gravar: o `close` nunca devolve erro depois do record de fechamento.
        let facts = self.retro_facts(&work_item)?;
        let closed =
            self.omhl
                .close_work_item(&self.tenant, &work_item, &snapshots, &caller.author())?;

        Ok(output::closed(&closed, retro(&facts).as_ref()))
    }

    /// Regenera `.omhl/projection` a partir do estado vigente; não grava nenhum record.
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

    /// O snapshot de uma árvore limpa; árvore suja e sonda com falha são recusas em `/snapshot`.
    pub(super) fn probe_snapshot(&self) -> Result<Snapshot, AppError> {
        reproducible(self.probe.probe())
    }

    /// O commit e a árvore do `HEAD`, com a mesma recusa de `probe_snapshot`.
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
