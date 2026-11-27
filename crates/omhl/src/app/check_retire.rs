use hexlog::{AppendItem, AppendRequest, ItemBody, Lineage, SemanticId};
use serde_json::{Value, json};

use super::action::Caller;
use super::engine::{Engine, parse_at, require_project_root};
use super::error::AppError;
use crate::error::{OmhlError, RuleCode};
use crate::extensions::collect::extension_file_exists;
use crate::extensions::model::ExtensionKind;

impl Engine<'_> {
    /// É o `loader` quem grava, porque a guarda só deixa ele retirar um check com `run`.
    pub(super) fn check_retire(
        &mut self,
        check: &str,
        owner_override: bool,
    ) -> Result<Value, AppError> {
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let id = parse_at(SemanticId::parse, check, "/check")?;
        let node = self
            .omhl
            .get(&self.tenant, &id)?
            .filter(|node| {
                node.type_name.as_str() == ExtensionKind::Check.type_name()
                    && id.as_str().starts_with(ExtensionKind::Check.id_prefix())
            })
            .ok_or_else(|| OmhlError::rule(RuleCode::CheckNotFound, "/check"))?;
        if extension_file_exists(ExtensionKind::Check, &root, id.as_str()) {
            return Err(AppError::check_not_orphaned("/check"));
        }
        if !owner_override {
            return Err(AppError::check_retire_refused("/check"));
        }
        let item = AppendItem {
            body: ItemBody::Retirement {
                lineage: Lineage::Node(id.clone()),
            },
            base: Some(node.record_id),
            snapshot: None,
            external_id: None,
        };
        let request = AppendRequest {
            tenant: self.tenant.clone(),
            author: Caller::loader().author(),
            key: None,
            items: vec![item],
        };
        self.omhl.append(&request)?;

        Ok(json!({ "retired": id }))
    }
}
