use hexlog::{
    AppendItem, AppendRequest, Author, Hexlog, ItemBody, Lineage, NodeView, RecordId, SemanticId,
    Snapshot, TenantId,
};
use serde_json::Value;

use crate::error::{OmhlError, RuleCode};
use crate::graphs::WORK_ITEM_TYPES;
use crate::mechanical::is_mechanical;
use crate::probe::{ProbeError, ProbeOutcome, SnapshotProbe, TreeSource};

/// Namespace que o id de um node deve ter, por tipo.
fn namespace_of(type_name: &str) -> Option<&'static str> {
    match type_name {
        "check" => Some("checks"),
        "guideline" => Some("guidelines"),
        "premise" => Some("premises"),
        "decision" => Some("decisions"),
        "gap" => Some("gaps"),
        "divergence" => Some("divergences"),
        "attestation" => Some("attestations"),
        "work-item" | "pull-request" => Some("work-items"),
        _ => None,
    }
}

/// Único client que grava node `check` com `run`.
const CHECK_AUTHOR: &str = "loader";

/// Um item `Node` do lote, com o índice para montar o ponteiro dos erros.
pub(super) struct NodeItem<'i> {
    index: usize,
    pub(super) id: &'i SemanticId,
    pub(super) type_name: &'i str,
    data: &'i Value,
    pub(super) base: Option<&'i RecordId>,
    pub(super) snapshot: Option<&'i Snapshot>,
}

impl<'i> NodeItem<'i> {
    fn from_item(index: usize, item: &'i AppendItem) -> Option<NodeItem<'i>> {
        let ItemBody::Node {
            id,
            type_name,
            data,
        } = &item.body
        else {
            return None;
        };

        Some(NodeItem {
            index,
            id,
            type_name: type_name.as_str(),
            data,
            base: item.base.as_ref(),
            snapshot: item.snapshot.as_ref(),
        })
    }

    /// Erro de regra com o ponteiro `/items/<i>/<suffix>`.
    pub(super) fn error(&self, code: RuleCode, suffix: &str) -> OmhlError {
        OmhlError::rule(code, &format!("/items/{}/{suffix}", self.index))
    }

    /// Campo de primeiro nível de `data`, só quando é texto.
    pub(super) fn field(&self, key: &str) -> Option<&'i str> {
        self.data.get(key).and_then(Value::as_str)
    }

    pub(super) fn has_field(&self, key: &str) -> bool {
        self.data.get(key).is_some()
    }
}

pub(super) struct Guard<'a, 'd> {
    pub(super) hexlog: &'a mut Hexlog<'d>,
    pub(super) tenant: &'a TenantId,
    pub(super) author: &'a Author,
    probe: &'a dyn SnapshotProbe,
    probed: Option<Result<ProbeOutcome, ProbeError>>,
    probed_index_tree: Option<Result<ProbeOutcome, ProbeError>>,
}

/// Valida o lote inteiro, lendo só o banco; o primeiro achado encerra com erro.
pub(crate) fn check_append(
    hexlog: &mut Hexlog<'_>,
    probe: &dyn SnapshotProbe,
    req: &AppendRequest,
) -> Result<(), OmhlError> {
    let mut guard = Guard {
        hexlog,
        tenant: &req.tenant,
        author: &req.author,
        probe,
        probed: None,
        probed_index_tree: None,
    };
    for (index, item) in req.items.iter().enumerate() {
        if let Some(node) = NodeItem::from_item(index, item) {
            guard.check_node(&node)?;
        } else if let ItemBody::Retirement {
            lineage: Lineage::Node(id),
        } = &item.body
        {
            guard.check_retirement(index, id)?;
        }
    }

    Ok(())
}

pub(super) fn status_of(data: &Value) -> Option<&str> {
    data.get("status").and_then(Value::as_str)
}

fn check_namespace(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    let inside = namespace_of(node.type_name).is_none_or(|namespace| {
        node.id
            .as_str()
            .strip_prefix(namespace)
            .is_some_and(|rest| rest.starts_with('.'))
    });

    if inside {
        Ok(())
    } else {
        Err(node.error(RuleCode::IdNamespace, "body/id"))
    }
}

impl Guard<'_, '_> {
    fn check_node(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        check_namespace(node)?;
        match node.type_name {
            name if WORK_ITEM_TYPES.contains(&name) => {
                self.check_work_item_status(node)?;
                self.check_reactivation(node)
            }
            "premise" | "decision" | "gap" | "divergence" => {
                self.find_work_item(node)?;
                self.check_reactivation(node)
            }
            "check" => {
                self.check_reactivation(node)?;
                self.check_run_author(node)
            }
            "attestation" => {
                self.check_reactivation(node)?;
                self.check_attestation(node)
            }
            _ => Ok(()),
        }
    }

    /// Fechar só por `close_work_item`, e work item fechado é definitivo.
    fn check_work_item_status(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        if node.field("status") == Some("closed") {
            return Err(node.error(RuleCode::UseCloseWorkItem, "body/data/status"));
        }
        if node.base.is_none() {
            return Ok(());
        }
        let current = self.hexlog.get(self.tenant, node.id)?;
        match current {
            Some(view) if status_of(&view.data) == Some("closed") => {
                Err(node.error(RuleCode::InvalidTransition, "body/data/status"))
            }
            _ => Ok(()),
        }
    }

    /// `None` quando `data.work_item` não é texto; erro quando não aponta para um work item vigente.
    pub(super) fn find_work_item(
        &mut self,
        node: &NodeItem<'_>,
    ) -> Result<Option<NodeView>, OmhlError> {
        let Some(raw) = node.field("work_item") else {
            return Ok(None);
        };
        let found = match SemanticId::parse(raw) {
            Ok(id) => self.hexlog.get(self.tenant, &id)?,
            Err(_) => None,
        };

        match found {
            Some(view) if WORK_ITEM_TYPES.contains(&view.type_name.as_str()) => Ok(Some(view)),
            _ => Err(node.error(RuleCode::WorkItemNotFound, "body/data/work_item")),
        }
    }

    /// Só o `loader` escreve, tira ou retira o `run` de um check.
    fn check_run_author(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        if self.author.client == CHECK_AUTHOR {
            return Ok(());
        }
        if is_mechanical(node.data) {
            return Err(node.error(RuleCode::CheckRunAuthor, "body/data/run"));
        }
        if node.base.is_some() && self.current_check_has_run(node.id)? {
            return Err(node.error(RuleCode::CheckRunAuthor, "base"));
        }

        Ok(())
    }

    fn check_retirement(&mut self, index: usize, id: &SemanticId) -> Result<(), OmhlError> {
        if self.author.client != CHECK_AUTHOR && self.current_check_has_run(id)? {
            return Err(OmhlError::rule(
                RuleCode::CheckRunAuthor,
                &format!("/items/{index}/base"),
            ));
        }

        Ok(())
    }

    fn current_check_has_run(&mut self, id: &SemanticId) -> Result<bool, OmhlError> {
        let current = self.hexlog.get(self.tenant, id)?;

        Ok(current
            .is_some_and(|view| view.type_name.as_str() == "check" && is_mechanical(&view.data)))
    }

    /// Revision de um node retirado seria reativação, que o `hexlog` ainda não suporta.
    fn check_reactivation(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        if node.base.is_none() || self.hexlog.get(self.tenant, node.id)?.is_some() {
            return Ok(());
        }
        let lineage = Lineage::Node(node.id.clone());
        let revisions = self.hexlog.lineage_records(self.tenant, &lineage)?;

        match revisions.last() {
            Some(last) if last.retired => {
                Err(node.error(RuleCode::ReactivationUnsupported, "base"))
            }
            _ => Ok(()),
        }
    }

    /// A sonda roda no máximo uma vez por `append`.
    pub(super) fn probe_once(&mut self) -> &Result<ProbeOutcome, ProbeError> {
        let probe = self.probe;

        self.probed.get_or_insert_with(|| probe.probe())
    }

    /// A sonda de árvore do índice roda no máximo uma vez por `append`.
    pub(super) fn probe_index_tree_once(&mut self) -> &Result<ProbeOutcome, ProbeError> {
        let probe = self.probe;

        self.probed_index_tree
            .get_or_insert_with(|| probe.probe_tree(TreeSource::Index))
    }
}
