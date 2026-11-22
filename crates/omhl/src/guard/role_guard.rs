use hexlog::{
    AppendItem, EndpointStatus, ItemBody, Lineage, RecordId, RelationQuery, RelationTypeName,
    RelationView, SemanticId,
};
use serde_json::Value;

use super::append::Guard;
use super::role::{
    CLAUDE, CRITIC, Revision, contradicts_retirement_allowed, creation_allowed, known_role,
    matrix_lists, node_allowed, revision_allowed,
};
use crate::error::{OmhlError, RuleCode};

const CONTRADICTS: &str = "contradicts";
const ANSWERS: &str = "answers";

fn refusal(code: RuleCode, index: usize, suffix: &str) -> OmhlError {
    OmhlError::rule(code, &format!("/items/{index}/{suffix}"))
}

impl Guard<'_, '_> {
    pub(super) fn check_role(&mut self, index: usize, item: &AppendItem) -> Result<(), OmhlError> {
        match &item.body {
            ItemBody::Node {
                id,
                type_name,
                data,
            } => self.check_role_node(index, id, type_name.as_str(), data, item.base.as_ref()),
            ItemBody::Relation {
                relation_type,
                from,
                to,
            } => self.check_role_relation(index, relation_type.as_str(), from, to),
            ItemBody::Retirement {
                lineage: Lineage::Node(id),
            } => self.check_role_node_retirement(index, id),
            ItemBody::Retirement {
                lineage:
                    Lineage::Relation {
                        relation_type,
                        from,
                        to,
                    },
            } => self.check_role_relation_retirement(index, relation_type, from, to),
        }
    }

    fn check_role_node(
        &mut self,
        index: usize,
        id: &SemanticId,
        type_name: &str,
        data: &Value,
        base: Option<&RecordId>,
    ) -> Result<(), OmhlError> {
        let role = known_role(&self.author.agent);
        if role.is_some_and(|role| !node_allowed(role, type_name, data)) {
            return Err(refusal(RuleCode::RoleRecordForbidden, index, "body/type"));
        }
        if base.is_none() {
            if role.is_some() && !creation_allowed(type_name, data) {
                let pointer = "body/data/resolution";
                return Err(refusal(RuleCode::RoleRecordForbidden, index, pointer));
            }
            return Ok(());
        }

        self.check_role_revision(index, id, type_name, data, role)
    }

    fn check_role_revision(
        &mut self,
        index: usize,
        id: &SemanticId,
        type_name: &str,
        data: &Value,
        role: Option<&'static str>,
    ) -> Result<(), OmhlError> {
        let Some(current) = self.hexlog.get(self.tenant, id)? else {
            return Ok(());
        };
        let revision = Revision {
            role: role.unwrap_or_default(),
            type_name,
            current_author: &current.author.agent,
            current: &current.data,
            revised: data,
        };
        if role.is_some() && !revision_allowed(&revision) {
            return Err(refusal(RuleCode::RoleRevisionForbidden, index, "base"));
        }
        // Mudar o work item de um node em contradição esconderia a contradição do gate daquele work item.
        let moved = current.data.get("work_item") != data.get("work_item");
        if moved && !self.current_contradictions_of(id)?.is_empty() {
            let pointer = "body/data/work_item";
            return Err(refusal(RuleCode::RoleRevisionForbidden, index, pointer));
        }

        Ok(())
    }

    fn check_role_relation(
        &mut self,
        index: usize,
        relation_type: &str,
        from: &SemanticId,
        to: &SemanticId,
    ) -> Result<(), OmhlError> {
        let Some(role) = known_role(&self.author.agent) else {
            return Ok(());
        };
        if !matrix_lists(role, |matrix| matrix.relations, relation_type) {
            let pointer = "body/relation_type";
            return Err(refusal(RuleCode::RoleRecordForbidden, index, pointer));
        }
        // Node do próprio lote foi gravado por este mesmo autor.
        let own = self.batch.contains_key(from)
            || self
                .hexlog
                .get(self.tenant, from)?
                .is_some_and(|view| view.author.agent == role);

        if !own {
            return Err(refusal(RuleCode::RoleRecordForbidden, index, "body/from"));
        }
        if role == CRITIC && relation_type == ANSWERS {
            return self.check_critic_answer(index, to);
        }

        Ok(())
    }

    fn check_critic_answer(&mut self, index: usize, to: &SemanticId) -> Result<(), OmhlError> {
        let Some(target) = self.endpoint_of(to)? else {
            return Ok(());
        };

        if target.type_name == "gap" && target.kind.as_deref() == Some("technical") {
            Ok(())
        } else {
            Err(refusal(RuleCode::RoleAnswerForbidden, index, "body/to"))
        }
    }

    fn check_role_node_retirement(
        &mut self,
        index: usize,
        id: &SemanticId,
    ) -> Result<(), OmhlError> {
        let forbidden = known_role(&self.author.agent).is_some()
            || (self.author.agent == CLAUDE && self.contradicts_a_decision(id)?);

        if forbidden {
            Err(refusal(RuleCode::RoleRetirementForbidden, index, "base"))
        } else {
            Ok(())
        }
    }

    fn check_role_relation_retirement(
        &mut self,
        index: usize,
        relation_type: &RelationTypeName,
        from: &SemanticId,
        to: &SemanticId,
    ) -> Result<(), OmhlError> {
        let relation_type = relation_type.as_str();
        let agent = self.author.agent.as_str();
        let outside_matrix = known_role(agent)
            .is_some_and(|role| !matrix_lists(role, |matrix| matrix.retire, relation_type));
        if outside_matrix {
            return Err(refusal(RuleCode::RoleRetirementForbidden, index, "base"));
        }
        if relation_type != CONTRADICTS {
            return Ok(());
        }
        let relation_author = self.relation_author(relation_type, from, to)?;
        if contradicts_retirement_allowed(&self.author.agent, relation_author.as_deref()) {
            Ok(())
        } else {
            Err(refusal(RuleCode::RoleRetirementForbidden, index, "base"))
        }
    }

    fn relation_author(
        &mut self,
        relation_type: &str,
        from: &SemanticId,
        to: &SemanticId,
    ) -> Result<Option<String>, OmhlError> {
        let query = RelationQuery {
            relation_type: Some(RelationTypeName::parse(relation_type)?),
            from: Some(from.clone()),
            to: Some(to.clone()),
            include_retired: false,
        };
        let found = self.hexlog.relations(self.tenant, &query)?;

        Ok(found.into_iter().next().map(|view| view.author.agent))
    }

    fn current_contradictions_of(
        &mut self,
        id: &SemanticId,
    ) -> Result<Vec<RelationView>, OmhlError> {
        let query = RelationQuery {
            relation_type: Some(RelationTypeName::parse(CONTRADICTS)?),
            from: None,
            to: None,
            include_retired: false,
        };
        let all = self.hexlog.relations(self.tenant, &query)?;

        Ok(all
            .into_iter()
            .filter(|view| view.from_status == EndpointStatus::Current)
            .filter(|view| view.to_status == EndpointStatus::Current)
            .filter(|view| view.from == *id || view.to == *id)
            .collect())
    }

    /// Retirar a ponta fecharia o `no-open-contradiction` sem tocar na relação (N1).
    fn contradicts_a_decision(&mut self, id: &SemanticId) -> Result<bool, OmhlError> {
        for relation in self.current_contradictions_of(id)? {
            let target = self.hexlog.get(self.tenant, &relation.to)?;
            if target.is_some_and(|view| view.type_name.as_str() == "decision") {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
