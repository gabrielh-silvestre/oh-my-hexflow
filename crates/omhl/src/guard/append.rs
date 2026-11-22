use std::collections::{HashMap, HashSet};

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

const LOADER_CLIENT: &str = "loader";

pub(crate) const GUIDELINE_RETIRE_CLIENT: &str = "guideline-retire";

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

    pub(super) fn error(&self, code: RuleCode, suffix: &str) -> OmhlError {
        OmhlError::rule(code, &format!("/items/{}/{suffix}", self.index))
    }

    pub(super) fn field(&self, key: &str) -> Option<&'i str> {
        self.data.get(key).and_then(Value::as_str)
    }

    pub(super) fn has_field(&self, key: &str) -> bool {
        self.data.get(key).is_some()
    }
}

#[derive(Clone)]
pub(super) struct Endpoint {
    pub(super) type_name: String,
    pub(super) kind: Option<String>,
}

pub(super) struct Guard<'a, 'd> {
    pub(super) hexlog: &'a mut Hexlog<'d>,
    pub(super) tenant: &'a TenantId,
    pub(super) author: &'a Author,
    probe: &'a dyn SnapshotProbe,
    probed: Option<Result<ProbeOutcome, ProbeError>>,
    probed_index_tree: Option<Result<ProbeOutcome, ProbeError>>,
    pub(super) batch: HashMap<SemanticId, Endpoint>,
    pub(super) batch_decision_work_items: HashSet<String>,
}

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
        batch_decision_work_items: HashSet::new(),
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
        if node.type_name == "decision"
            && let Some(work_item) = node.field("work_item")
        {
            self.batch_decision_work_items.insert(work_item.to_owned());
        }
        match node.type_name {
            name if WORK_ITEM_TYPES.contains(&name) => {
                self.check_work_item_status(node)?;
                self.check_reactivation(node)
            }
            "premise" => {
                self.check_work_item(node)?;
                self.check_reactivation(node)?;
                self.check_human_confirmation(node)?;
                self.check_origin(node)
            }
            "decision" | "gap" | "divergence" => {
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

    fn check_work_item(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let needs_open =
            node.base.is_none() && matches!(node.type_name, "premise" | "gap" | "divergence");
        if needs_open {
            self.find_open_work_item(node)?;
        } else {
            self.find_work_item(node)?;
        }

        Ok(())
    }

    /// Só a diretiva de extension, que o `loader` grava, vale sem `confirmed_by`; a premissa do `loader` o exige.
    fn check_human_confirmation(&self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        if self.author.client == LOADER_CLIENT && node.type_name == "guideline" {
            return Ok(());
        }

        check_confirmation(node, &self.author.agent)
    }

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

    pub(super) fn endpoint_of(&mut self, id: &SemanticId) -> Result<Option<Endpoint>, OmhlError> {
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

    pub(super) fn probe_once(&mut self) -> &Result<ProbeOutcome, ProbeError> {
        let probe = self.probe;

        self.probed.get_or_insert_with(|| probe.probe())
    }

    pub(super) fn probe_index_tree_once(&mut self) -> &Result<ProbeOutcome, ProbeError> {
        let probe = self.probe;

        self.probed_index_tree
            .get_or_insert_with(|| probe.probe_tree(TreeSource::Index))
    }
}
