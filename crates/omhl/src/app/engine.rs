use hexlog::{HexlogError, ListFilter, PageRequest, SemanticId, Snapshot, TenantId, TypeName};
use serde_json::{Value, json};

use super::action::{Action, Caller};
use super::error::AppError;
use super::wire::append_request;
use super::{context, output};
use crate::error::{OmhlError, RuleCode};
use crate::facade::Omhl;
use crate::probe::{ProbeOutcome, SnapshotProbe};

/// Tamanho de página quando o pedido não traz `limit` (o padrão do motor).
const DEFAULT_PAGE_LIMIT: u32 = 50;
/// Teto de um `SemanticId` no `hexlog`; um prefixo maior nunca casa.
const ID_PREFIX_MAX: usize = 200;

/// Estado que as ações compartilham; as tentativas ficam por conta do `App`.
pub(super) struct Engine<'d> {
    pub(super) omhl: Omhl<'d>,
    pub(super) tenant: TenantId,
    pub(super) probe: &'d dyn SnapshotProbe,
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

impl Engine<'_> {
    pub(super) fn run(&mut self, caller: &Caller, action: &Action) -> Result<Value, AppError> {
        match action {
            Action::List {
                type_name,
                id_prefix,
                limit,
                after_seq,
            } => self.list(
                type_name.as_deref(),
                id_prefix.as_deref(),
                *limit,
                *after_seq,
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
        }
    }

    fn list(
        &mut self,
        type_name: Option<&str>,
        id_prefix: Option<&str>,
        limit: Option<u32>,
        after_seq: Option<u64>,
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
            where_eq: Vec::new(),
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
        let request = append_request(request, &self.tenant, caller)?;

        Ok(output::appended(&self.omhl.append(&request)?))
    }

    fn gate_pr(&mut self, work_item: &str) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshot = self.probe_snapshot()?;
        let report = self
            .omhl
            .evaluate_pr_gate(&self.tenant, &work_item, &snapshot)?;

        Ok(output::gate(&report))
    }

    fn close(&mut self, caller: &Caller, work_item: &str) -> Result<Value, AppError> {
        let work_item = parse_at(SemanticId::parse, work_item, "/work_item")?;
        let snapshot = self.probe_snapshot()?;
        let closed =
            self.omhl
                .close_work_item(&self.tenant, &work_item, &snapshot, &caller.author())?;

        Ok(output::closed(&closed))
    }

    /// O snapshot de uma árvore limpa; árvore suja e sonda com falha são recusas em `/snapshot`.
    pub(super) fn probe_snapshot(&self) -> Result<Snapshot, AppError> {
        match self.probe.probe() {
            Ok(ProbeOutcome::Reproducible(snapshot)) => Ok(snapshot),
            Ok(ProbeOutcome::Dirty) => {
                Err(OmhlError::rule(RuleCode::SnapshotDirty, "/snapshot").into())
            }
            Err(error) => Err(AppError::probe_failed(&error.message)),
        }
    }
}
