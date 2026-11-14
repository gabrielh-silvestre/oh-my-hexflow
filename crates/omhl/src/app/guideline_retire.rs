use hexlog::{
    AppendItem, AppendRequest, ItemBody, Lineage, NodeView, RelationTypeName, SemanticId,
};
use serde_json::{Value, json};

use super::action::Caller;
use super::engine::{Engine, parse_at, require_project_root};
use super::error::AppError;
use crate::extensions::collect::extension_file_exists;
use crate::extensions::model::ExtensionKind;

impl Engine<'_> {
    /// Aposenta o node de uma diretiva cujo arquivo sumiu da árvore; com `by`, grava no mesmo lote que a nova a substitui.
    pub(super) fn guideline_retire(
        &mut self,
        caller: &Caller,
        guideline: &str,
        by: Option<&str>,
    ) -> Result<Value, AppError> {
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let id = parse_at(SemanticId::parse, guideline, "/id")?;
        let node = self
            .current_guideline(&id)?
            .ok_or_else(|| AppError::guideline_not_found("/id"))?;
        if extension_file_exists(ExtensionKind::Guideline, &root, id.as_str()) {
            return Err(AppError::guideline_not_orphaned("/id"));
        }
        let successor = by.map(|by| self.successor(&id, by)).transpose()?;

        let mut items = Vec::with_capacity(2);
        if let Some(successor) = &successor {
            items.push(AppendItem {
                body: ItemBody::Relation {
                    relation_type: parse_at(RelationTypeName::parse, "supersedes", "/by")?,
                    from: successor.clone(),
                    to: id.clone(),
                },
                base: None,
                snapshot: None,
                external_id: None,
            });
        }
        items.push(AppendItem {
            body: ItemBody::Retirement {
                lineage: Lineage::Node(id.clone()),
            },
            base: Some(node.record_id),
            snapshot: None,
            external_id: None,
        });
        let request = AppendRequest {
            tenant: self.tenant.clone(),
            author: Caller::guideline_retirer(caller.agent).author(),
            key: None,
            items,
        };
        self.omhl
            .append(&request)
            .map_err(|error| AppError::from(error).into_supersedes_cycle("/by"))?;

        Ok(json!({ "retired": id, "superseded_by": successor }))
    }

    fn current_guideline(&mut self, id: &SemanticId) -> Result<Option<NodeView>, AppError> {
        Ok(self.omhl.get(&self.tenant, id)?.filter(|node| {
            node.type_name.as_str() == ExtensionKind::Guideline.type_name()
                && id
                    .as_str()
                    .starts_with(ExtensionKind::Guideline.id_prefix())
        }))
    }

    fn successor(&mut self, retired: &SemanticId, by: &str) -> Result<SemanticId, AppError> {
        let successor = parse_at(SemanticId::parse, by, "/by")?;
        if &successor == retired {
            return Err(AppError::successor_is_self("/by"));
        }
        match self.current_guideline(&successor)? {
            Some(_) => Ok(successor),
            None => Err(AppError::successor_not_current("/by")),
        }
    }
}
