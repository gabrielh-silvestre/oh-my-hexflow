use std::collections::HashMap;

use hexlog::{
    AppendItem, AppendRequest, Author, Hexlog, ItemBody, Lineage, NativeNodes, NodeView, RecordId,
    SemanticId, Snapshot, TenantId,
};
use serde_json::Value;

use super::confirmation::check_confirmation;
use super::rests_on::rests_on_allowed;
use crate::error::{OmhlError, RuleCode};
use crate::extensions::parse::GUIDELINE_KINDS;
use crate::graphs::{WORK_ITEM_TYPES, namespace_of};
use crate::mechanical::is_mechanical;
use crate::native::NativeBase;
use crate::probe::{ProbeError, ProbeOutcome, SnapshotProbe, TreeSource};

/// Único client que grava node `check` com `run`; também fica isento da guarda da confirmação.
const LOADER_CLIENT: &str = "loader";

/// Único client que retira um node `guideline`; só o `omhl guideline retire` o usa, depois das conferências do comando.
pub(crate) const GUIDELINE_RETIRE_CLIENT: &str = "guideline-retire";

/// Um item `Node` do lote, com o índice para montar o ponteiro dos erros.
pub(super) struct NodeItem<'i> {
    index: usize,
    pub(super) id: &'i SemanticId,
    pub(super) type_name: &'i str,
    pub(super) data: &'i Value,
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

/// Tipo e `kind` de um node, lidos do lote ou do banco.
#[derive(Clone)]
pub(super) struct Endpoint {
    type_name: String,
    kind: Option<String>,
}

pub(super) struct Guard<'a, 'd> {
    pub(super) hexlog: &'a mut Hexlog<'d>,
    pub(super) tenant: &'a TenantId,
    pub(super) author: &'a Author,
    probe: &'a dyn SnapshotProbe,
    probed: Option<Result<ProbeOutcome, ProbeError>>,
    probed_index_tree: Option<Result<ProbeOutcome, ProbeError>>,
    /// Nodes já vistos no lote, na ordem dos itens; prevalecem sobre o banco.
    pub(super) batch: HashMap<SemanticId, Endpoint>,
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
        batch: HashMap::new(),
    };
    for (index, item) in req.items.iter().enumerate() {
        guard.check_role(index, item)?;
        if let Some(node) = NodeItem::from_item(index, item) {
            guard.check_node(&node)?;
        } else if let ItemBody::Retirement {
            lineage: Lineage::Node(id),
        } = &item.body
        {
            guard.check_retirement(index, id)?;
        } else if let ItemBody::Relation {
            relation_type,
            from,
            to,
        } = &item.body
            && relation_type.as_str() == "rests-on"
        {
            guard.check_rests_on(index, from, to)?;
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

/// Diretriz nativa vale como diretriz sem `kind`; check nativo segue para o motor.
fn native_guideline_endpoint(id: &SemanticId) -> Option<Endpoint> {
    let native = NativeBase.lookup(id)?;

    (native.type_name.as_str() == "guideline").then(|| Endpoint {
        type_name: "guideline".to_owned(),
        kind: None,
    })
}

fn check_guideline_kind(node: &NodeItem<'_>) -> Result<(), OmhlError> {
    match node.field("kind") {
        Some(kind) if GUIDELINE_KINDS.contains(&kind) => Ok(()),
        _ if node.has_field("kind") => Err(node.error(RuleCode::KindInvalid, "body/data/kind")),
        _ => Err(node.error(RuleCode::KindMissing, "body/data/kind")),
    }
}

impl Guard<'_, '_> {
    fn check_node(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        check_namespace(node)?;
        self.batch.insert(
            node.id.clone(),
            Endpoint {
                type_name: node.type_name.to_owned(),
                kind: node.field("kind").map(str::to_owned),
            },
        );
        match node.type_name {
            name if WORK_ITEM_TYPES.contains(&name) => {
                self.check_work_item_status(node)?;
                self.check_reactivation(node)
            }
            "premise" | "decision" | "gap" | "divergence" => {
                self.check_work_item(node)?;
                self.check_reactivation(node)?;
                self.check_human_confirmation(node)
            }
            "guideline" => {
                check_guideline_kind(node)?;
                self.check_reactivation(node)?;
                self.check_human_confirmation(node)
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

    /// Premissa, gap e divergência novos exigem work item aberto; a revisão de um node existente não.
    fn check_work_item(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let needs_open = node.base.is_none()
            && self.author.client != LOADER_CLIENT
            && matches!(node.type_name, "premise" | "gap" | "divergence");
        if needs_open {
            self.find_open_work_item(node)?;
        } else {
            self.find_work_item(node)?;
        }

        Ok(())
    }

    fn check_human_confirmation(&self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        if self.author.client == LOADER_CLIENT {
            return Ok(());
        }

        check_confirmation(node)
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
        if self.author.client == LOADER_CLIENT {
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

    /// Extremo ausente do lote e do banco fica para o motor: nativo é aceito, inexistente é `endpoint-not-found`.
    fn check_rests_on(
        &mut self,
        index: usize,
        from: &SemanticId,
        to: &SemanticId,
    ) -> Result<(), OmhlError> {
        let (Some(source), target) = (self.endpoint_of(from)?, self.endpoint_of(to)?) else {
            return Ok(());
        };
        let target = target
            .as_ref()
            .map(|node| (node.type_name.as_str(), node.kind.as_deref()));

        rests_on_allowed(&source.type_name, target)
            .map_err(|code| OmhlError::rule(code, &format!("/items/{index}/body/to")))
    }

    fn endpoint_of(&mut self, id: &SemanticId) -> Result<Option<Endpoint>, OmhlError> {
        if let Some(node) = self.batch.get(id) {
            return Ok(Some(node.clone()));
        }
        let Some(view) = self.hexlog.get(self.tenant, id)? else {
            return Ok(native_guideline_endpoint(id));
        };

        Ok(Some(Endpoint {
            type_name: view.type_name.as_str().to_owned(),
            kind: view
                .data
                .get("kind")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }))
    }

    fn check_retirement(&mut self, index: usize, id: &SemanticId) -> Result<(), OmhlError> {
        let Some(current) = self.hexlog.get(self.tenant, id)? else {
            return Ok(());
        };
        let pointer = format!("/items/{index}/base");
        if current.type_name.as_str() == "guideline"
            && self.author.client != GUIDELINE_RETIRE_CLIENT
        {
            return Err(OmhlError::rule(
                RuleCode::GuidelineRetireByCommand,
                &pointer,
            ));
        }
        if self.author.client != LOADER_CLIENT
            && current.type_name.as_str() == "check"
            && is_mechanical(&current.data)
        {
            return Err(OmhlError::rule(RuleCode::CheckRunAuthor, &pointer));
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
