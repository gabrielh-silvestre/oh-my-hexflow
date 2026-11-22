use serde_json::Value;

use crate::engine::chain::canonical;
use crate::engine::limits::{JSON_NESTING_MAX, Limits, json_nesting_exceeds};
use crate::engine::model::{AppendItem, AppendRequest, Author, ItemBody};
use crate::engine::ports::{Definitions, NativeNodes, RelationTypeInfo};
use crate::error::{Detail, ErrorCode, HexlogError, TOO_MANY_ERRORS};
use crate::ids::{RelationTypeName, SemanticId, TypeName};

#[cfg(test)]
mod tests;

const INSTANT_SHAPE: &[u8] = b"dddd-dd-ddTdd:dd:dd.dddZ";

/// Só a forma de 24 caracteres, não o calendário: quem implementa o `Clock` responde pelo valor.
pub fn instant_is_well_formed(text: &str) -> bool {
    text.len() == INSTANT_SHAPE.len()
        && text.bytes().zip(INSTANT_SHAPE).all(|(found, &shape)| {
            if shape == b'd' {
                found.is_ascii_digit()
            } else {
                found == shape
            }
        })
}

pub fn validate_shape(req: &AppendRequest, limits: &Limits) -> Result<(), HexlogError> {
    if !(1..=limits.batch_items).contains(&req.items.len()) {
        return Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "batch-size",
            "batch must hold between 1 and the maximum number of items",
        )
        .at("/items"));
    }
    validate_author(&req.author, limits.author_field_max)?;
    req.items
        .iter()
        .enumerate()
        .try_for_each(|(index, item)| validate_item_shape(index, &item.body))
}

fn validate_author(author: &Author, max: usize) -> Result<(), HexlogError> {
    let fields = [
        ("/author/agent", Some(author.agent.as_str())),
        ("/author/client", Some(author.client.as_str())),
        ("/author/model", author.model.as_deref()),
    ];
    let misfit = fields.into_iter().find(|(_, text)| {
        text.is_some_and(|text| !(1..=max).contains(&text.encode_utf16().count()))
    });
    match misfit {
        Some((path, _)) => Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "author-format",
            "author field must have between 1 and the maximum number of characters",
        )
        .at(path)),
        None => Ok(()),
    }
}

fn validate_item_shape(index: usize, body: &ItemBody) -> Result<(), HexlogError> {
    if let ItemBody::Node { data, .. } = body
        && json_nesting_exceeds(data, JSON_NESTING_MAX)
    {
        return Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "max-depth-exceeded",
            "value nested too deeply",
        )
        .at_item(index, "body/data"));
    }
    Ok(())
}

pub fn static_rules(
    req: &AppendRequest,
    defs: &dyn Definitions,
    native: &dyn NativeNodes,
    limits: &Limits,
) -> Result<(), HexlogError> {
    for (index, item) in req.items.iter().enumerate() {
        validate_identity(index, &item.body, defs, native)?;
    }
    let mut details = Vec::new();
    let mut marker = false;
    for (index, item) in req.items.iter().enumerate() {
        marker |= item_details(index, item, defs, limits, &mut details)?;
    }
    if details.is_empty() {
        return Ok(());
    }
    if marker || details.len() > limits.details_max {
        details.truncate(limits.details_max);
        details.push(Detail::new(
            String::new(),
            TOO_MANY_ERRORS,
            "too many errors to list",
        ));
    }
    Err(HexlogError {
        code: ErrorCode::InvalidRecord,
        message: "record content is invalid".to_owned(),
        details,
    })
}

fn validate_identity(
    index: usize,
    body: &ItemBody,
    defs: &dyn Definitions,
    native: &dyn NativeNodes,
) -> Result<(), HexlogError> {
    match body {
        ItemBody::Node { id, type_name, .. } => {
            validate_node_identity(index, id, type_name, defs, native)
        }
        ItemBody::Relation { relation_type, .. } => {
            validate_relation_type(index, relation_type, defs)
        }
        ItemBody::Retirement { .. } => Ok(()),
    }
}

fn validate_node_identity(
    index: usize,
    id: &SemanticId,
    type_name: &TypeName,
    defs: &dyn Definitions,
    native: &dyn NativeNodes,
) -> Result<(), HexlogError> {
    if defs.node_type(type_name).is_none() {
        return Err(HexlogError::refused(
            ErrorCode::TypeNotFound,
            "type-not-found",
            "node type is not in the catalog",
        )
        .at_item(index, "body/type"));
    }
    if native.lookup(id).is_some() {
        return Err(HexlogError::refused(
            ErrorCode::ReservedId,
            "reserved-id",
            "semantic id is reserved by a native node",
        )
        .at_item(index, "body/id"));
    }
    Ok(())
}

fn validate_relation_type(
    index: usize,
    relation_type: &RelationTypeName,
    defs: &dyn Definitions,
) -> Result<(), HexlogError> {
    if defs.relation_type(relation_type).is_some() {
        return Ok(());
    }
    Err(HexlogError::refused(
        ErrorCode::RelationTypeNotFound,
        "relation-type-not-found",
        "relation type is not in the catalog",
    )
    .at_item(index, "body/relation_type"))
}

fn item_details(
    index: usize,
    item: &AppendItem,
    defs: &dyn Definitions,
    limits: &Limits,
    details: &mut Vec<Detail>,
) -> Result<bool, HexlogError> {
    let (requires_snapshot, marker) = match &item.body {
        ItemBody::Node {
            type_name, data, ..
        } => {
            let marker = data_details(index, type_name, data, defs, limits, details)?;
            let requires = defs
                .node_type(type_name)
                .is_some_and(|info| info.requires_snapshot);
            (requires, marker)
        }
        ItemBody::Relation { from, to, .. } => {
            if from == to {
                details.push(Detail::new(
                    format!("/items/{index}/body/to"),
                    "self-relation",
                    "a relation cannot connect a node to itself",
                ));
            }
            (false, false)
        }
        ItemBody::Retirement { .. } => (false, false),
    };
    details.extend(snapshot_detail(
        index,
        requires_snapshot,
        item.snapshot.is_some(),
    ));
    if item.external_id.is_some() && matches!(item.body, ItemBody::Retirement { .. }) {
        details.push(Detail::new(
            format!("/items/{index}/external_id"),
            "external-id-not-allowed",
            "external id is not allowed here",
        ));
    }
    Ok(marker)
}

fn snapshot_detail(index: usize, required: bool, present: bool) -> Option<Detail> {
    let path = format!("/items/{index}/snapshot");
    match (required, present) {
        (true, false) => Some(Detail::new(
            path,
            "snapshot-required",
            "snapshot is required for this type",
        )),
        (false, true) => Some(Detail::new(
            path,
            "snapshot-not-allowed",
            "snapshot is not allowed here",
        )),
        _ => None,
    }
}

/// Violações do schema; só quando não há nenhuma o tamanho é medido, para a guarda de número vir antes do `canonical`.
fn data_details(
    index: usize,
    type_name: &TypeName,
    data: &Value,
    defs: &dyn Definitions,
    limits: &Limits,
    details: &mut Vec<Detail>,
) -> Result<bool, HexlogError> {
    let path = format!("/items/{index}/body/data");
    let Err(violations) = defs.validate_data(type_name, data) else {
        if canonical(data)?.encode_utf16().count() > limits.data_max_chars {
            details.push(Detail::new(
                path,
                "data-too-large",
                "data exceeds the maximum size",
            ));
        }
        return Ok(false);
    };
    let mut marker = false;
    for violation in violations {
        if violation.code == TOO_MANY_ERRORS {
            marker = true;
        } else {
            details.push(Detail::new(
                format!("{path}{}", violation.pointer),
                &violation.code,
                &violation.code.replace('-', " "),
            ));
        }
    }
    Ok(marker)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    From,
    To,
}

impl Endpoint {
    pub fn field(self) -> &'static str {
        match self {
            Endpoint::From => "body/from",
            Endpoint::To => "body/to",
        }
    }
}

pub fn endpoint_types_allowed(
    info: &RelationTypeInfo,
    from: &str,
    to: &str,
) -> Result<(), Endpoint> {
    let allows = |allowed: &Option<Vec<TypeName>>, found: &str| {
        allowed
            .as_ref()
            .is_none_or(|types| types.iter().any(|name| name.as_str() == found))
    };
    if !allows(&info.from, from) {
        return Err(Endpoint::From);
    }
    if !allows(&info.to, to) {
        return Err(Endpoint::To);
    }
    Ok(())
}
