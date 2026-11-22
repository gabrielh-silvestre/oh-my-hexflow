//! Fio JSON do `append`: conversão à mão sobre `serde_json::Value`, sem `deny_unknown_fields`,
//! para que cada recusa leve o ponteiro exato da chave.

use hexlog::{
    AppendItem, AppendRequest, ExternalId, HexlogError, IdempotencyKey, ItemBody, Lineage,
    RecordId, RelationTypeName, SemanticId, Snapshot, SnapshotKind, TenantId, TypeName,
    pointer_segment,
};
use serde_json::{Map, Value};

use super::action::Caller;
use super::error::AppError;

const REQUEST_KEYS: [&str; 2] = ["key", "items"];
const ITEM_KEYS: [&str; 4] = ["body", "base", "snapshot", "external_id"];
const NODE_KEYS: [&str; 4] = ["kind", "id", "type", "data"];
const RELATION_KEYS: [&str; 4] = ["kind", "relation_type", "from", "to"];
const RETIREMENT_KEYS: [&str; 2] = ["kind", "lineage"];
const LINEAGE_NODE_KEYS: [&str; 2] = ["kind", "id"];
const SNAPSHOT_KEYS: [&str; 2] = ["kind", "id"];

pub fn append_request(
    request: &Value,
    tenant: &TenantId,
    caller: &Caller,
) -> Result<AppendRequest, AppError> {
    let fields = Fields::object(request, String::new())?.only(&REQUEST_KEYS)?;
    let key = fields.optional_parsed("key", IdempotencyKey::parse)?;
    let items = fields.array("items")?;

    Ok(AppendRequest {
        tenant: tenant.clone(),
        author: caller.author(),
        key,
        items: items
            .iter()
            .enumerate()
            .map(|(index, item)| append_item(item, format!("/items/{index}")))
            .collect::<Result<_, _>>()?,
    })
}

fn append_item(item: &Value, pointer: String) -> Result<AppendItem, AppError> {
    let fields = Fields::object(item, pointer)?.only(&ITEM_KEYS)?;
    let base = fields.optional_parsed("base", RecordId::parse)?;
    let external_id = fields.optional_parsed("external_id", ExternalId::parse)?;

    Ok(AppendItem {
        body: item_body(fields.required("body")?, fields.child("body"))?,
        base,
        snapshot: snapshot(&fields)?,
        external_id,
    })
}

fn snapshot(item: &Fields<'_>) -> Result<Option<Snapshot>, AppError> {
    let Some(value) = item.optional("snapshot") else {
        return Ok(None);
    };
    let fields = Fields::object(value, item.child("snapshot"))?.only(&SNAPSHOT_KEYS)?;
    let kind = match fields.text("kind")? {
        "git-commit" => SnapshotKind::GitCommit,
        "git-tree" => SnapshotKind::GitTree,
        _ => {
            return Err(AppError::append_invalid(
                &fields.child("kind"),
                "unknown kind",
            ));
        }
    };
    let id = fields.text("id")?;

    Snapshot::new(kind, id)
        .map(Some)
        .map_err(|error| fields.refused(error))
}

fn item_body(body: &Value, pointer: String) -> Result<ItemBody, AppError> {
    let fields = Fields::object(body, pointer)?;
    match fields.kind(&["node", "relation", "retirement"])? {
        "node" => node_body(&fields.only(&NODE_KEYS)?),
        "relation" => {
            let (relation_type, from, to) = relation_ends(&fields.only(&RELATION_KEYS)?)?;

            Ok(ItemBody::Relation {
                relation_type,
                from,
                to,
            })
        }
        _ => retirement_body(&fields.only(&RETIREMENT_KEYS)?),
    }
}

fn node_body(fields: &Fields<'_>) -> Result<ItemBody, AppError> {
    let data = fields.required("data")?;
    if !data.is_object() {
        return Err(fields.wrong_type("data"));
    }

    Ok(ItemBody::Node {
        id: fields.parsed("id", fields.text("id")?, SemanticId::parse)?,
        type_name: fields.parsed("type", fields.text("type")?, TypeName::parse)?,
        data: data.clone(),
    })
}

fn retirement_body(fields: &Fields<'_>) -> Result<ItemBody, AppError> {
    let lineage = Fields::object(fields.required("lineage")?, fields.child("lineage"))?;
    let lineage = match lineage.kind(&["node", "relation"])? {
        "node" => {
            let node = lineage.only(&LINEAGE_NODE_KEYS)?;

            Lineage::Node(node.parsed("id", node.text("id")?, SemanticId::parse)?)
        }
        _ => {
            let (relation_type, from, to) = relation_ends(&lineage.only(&RELATION_KEYS)?)?;

            Lineage::Relation {
                relation_type,
                from,
                to,
            }
        }
    };

    Ok(ItemBody::Retirement { lineage })
}

fn relation_ends(
    fields: &Fields<'_>,
) -> Result<(RelationTypeName, SemanticId, SemanticId), AppError> {
    Ok((
        fields.parsed(
            "relation_type",
            fields.text("relation_type")?,
            RelationTypeName::parse,
        )?,
        fields.parsed("from", fields.text("from")?, SemanticId::parse)?,
        fields.parsed("to", fields.text("to")?, SemanticId::parse)?,
    ))
}

struct Fields<'a> {
    map: &'a Map<String, Value>,
    pointer: String,
}

impl<'a> Fields<'a> {
    fn object(value: &'a Value, pointer: String) -> Result<Fields<'a>, AppError> {
        match value {
            Value::Object(map) => Ok(Fields { map, pointer }),
            _ => Err(AppError::append_invalid(&pointer, "wrong type")),
        }
    }

    fn child(&self, key: &str) -> String {
        format!("{}/{}", self.pointer, pointer_segment(key))
    }

    fn wrong_type(&self, key: &str) -> AppError {
        AppError::append_invalid(&self.child(key), "wrong type")
    }

    fn only(self, allowed: &[&str]) -> Result<Fields<'a>, AppError> {
        match self.map.keys().find(|key| !allowed.contains(&key.as_str())) {
            Some(key) => Err(AppError::append_invalid(&self.child(key), "unknown key")),
            None => Ok(self),
        }
    }

    fn optional(&self, key: &str) -> Option<&'a Value> {
        self.map.get(key)
    }

    fn required(&self, key: &str) -> Result<&'a Value, AppError> {
        self.optional(key)
            .ok_or_else(|| AppError::append_invalid(&self.child(key), "missing key"))
    }

    fn text(&self, key: &str) -> Result<&'a str, AppError> {
        self.required(key)?
            .as_str()
            .ok_or_else(|| self.wrong_type(key))
    }

    fn optional_text(&self, key: &str) -> Result<Option<&'a str>, AppError> {
        match self.optional(key) {
            None => Ok(None),
            Some(value) => value.as_str().map(Some).ok_or_else(|| self.wrong_type(key)),
        }
    }

    fn array(&self, key: &str) -> Result<&'a [Value], AppError> {
        self.required(key)?
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| self.wrong_type(key))
    }

    fn kind(&self, kinds: &[&str]) -> Result<&'a str, AppError> {
        let kind = self.text("kind")?;
        match kinds.contains(&kind) {
            true => Ok(kind),
            false => Err(AppError::append_invalid(
                &self.child("kind"),
                "unknown kind",
            )),
        }
    }

    fn parsed<T>(
        &self,
        key: &str,
        text: &str,
        parse: fn(&str) -> Result<T, HexlogError>,
    ) -> Result<T, AppError> {
        parse(text).map_err(|error| AppError::from(error.at(&self.child(key))))
    }

    fn optional_parsed<T>(
        &self,
        key: &str,
        parse: fn(&str) -> Result<T, HexlogError>,
    ) -> Result<Option<T>, AppError> {
        self.optional_text(key)?
            .map(|text| self.parsed(key, text, parse))
            .transpose()
    }

    fn refused(&self, error: HexlogError) -> AppError {
        AppError::from(error.at(&self.pointer))
    }
}
