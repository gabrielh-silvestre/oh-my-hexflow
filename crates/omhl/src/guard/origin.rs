use hexlog::{FieldName, ListFilter, PageRequest, Scalar, TypeName};
use serde_json::Value;

use super::append::{Guard, NodeItem};
use crate::error::{OmhlError, RuleCode};

const ORIGIN: &str = "body/data/origin";

impl Guard<'_, '_> {
    /// A marca `origin` nunca muda numa revisão, e a premissa de negócio nova (ou que vira de
    /// negócio) depois da primeira `decision` do work item nasce tardia.
    pub(super) fn check_origin(&mut self, node: &NodeItem<'_>) -> Result<(), OmhlError> {
        let current = match node.base {
            Some(_) => self.hexlog.get(self.tenant, node.id)?,
            None => None,
        };
        if let Some(view) = &current
            && view.data.get("origin") != node.data.get("origin")
        {
            return Err(node.error(RuleCode::OriginImmutable, ORIGIN));
        }

        let unmarked_business = node.field("kind") == Some("business") && !node.has_field("origin");
        let was_business = current
            .is_some_and(|view| view.data.get("kind").and_then(Value::as_str) == Some("business"));
        if unmarked_business && !was_business && self.work_item_has_decision(node)? {
            return Err(node.error(RuleCode::OriginRequired, ORIGIN));
        }

        Ok(())
    }

    /// Conta o lote (o `list` só vê o que já foi gravado) e só a `decision` vigente.
    fn work_item_has_decision(&mut self, node: &NodeItem<'_>) -> Result<bool, OmhlError> {
        let Some(work_item) = node.field("work_item") else {
            return Ok(false);
        };
        if self.batch_decision_work_items.contains(work_item) {
            return Ok(true);
        }
        let filter = ListFilter {
            type_name: Some(TypeName::parse("decision")?),
            id_prefix: None,
            snapshot: None,
            where_eq: vec![(
                FieldName::parse("work_item")?,
                Scalar::Str(work_item.to_owned()),
            )],
        };
        let first = PageRequest {
            limit: 1,
            after_seq: None,
        };

        Ok(!self
            .hexlog
            .list(self.tenant, &filter, &first)?
            .items
            .is_empty())
    }
}
