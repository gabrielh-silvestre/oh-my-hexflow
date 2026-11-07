use std::borrow::Cow;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::engine::chain::canonical;
use crate::engine::snapshot::Snapshot;
use crate::error::{ErrorCode, HexlogError};
use crate::ids::{
    ExternalId, Hash, IdempotencyKey, RecordId, RelationTypeName, SemanticId, TenantId, TypeName,
};

#[cfg(test)]
mod tests;

// Entrada de escrita

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub agent: String,
    pub model: Option<String>,
    pub client: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppendRequest {
    pub tenant: TenantId,
    pub author: Author,
    pub key: Option<IdempotencyKey>,
    pub items: Vec<AppendItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppendItem {
    pub body: ItemBody,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<RecordId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Snapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<ExternalId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemBody {
    Node {
        id: SemanticId,
        #[serde(rename = "type")]
        type_name: TypeName,
        /// Chave duplicada é resolvida por quem converteu o texto em `Value` (a última vence); noncharacters são aceitos.
        data: Value,
    },
    Relation {
        relation_type: RelationTypeName,
        from: SemanticId,
        to: SemanticId,
    },
    Retirement {
        lineage: Lineage,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lineage {
    Node(SemanticId),
    Relation {
        relation_type: RelationTypeName,
        from: SemanticId,
        to: SemanticId,
    },
}

// Um `Node(SemanticId)` não cabe em enum com tag interna do serde: a tag precisa de um objeto.
impl Serialize for Lineage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Lineage::Node(id) => {
                let mut object = serializer.serialize_struct("Lineage", 2)?;
                object.serialize_field("kind", "node")?;
                object.serialize_field("id", id)?;
                object.end()
            }
            Lineage::Relation {
                relation_type,
                from,
                to,
            } => {
                let mut object = serializer.serialize_struct("Lineage", 4)?;
                object.serialize_field("kind", "relation")?;
                object.serialize_field("relation_type", relation_type)?;
                object.serialize_field("from", from)?;
                object.serialize_field("to", to)?;
                object.end()
            }
        }
    }
}

/// Tipo da linhagem em texto e a chave que a identifica: o id do node, ou `tipo|de|para` da relation.
pub(crate) fn lineage_columns(lineage: &Lineage) -> (&'static str, Cow<'_, str>) {
    match lineage {
        Lineage::Node(id) => ("node", Cow::Borrowed(id.as_str())),
        Lineage::Relation {
            relation_type,
            from,
            to,
        } => (
            "relation",
            Cow::Owned(format!(
                "{}|{}|{}",
                relation_type.as_str(),
                from.as_str(),
                to.as_str()
            )),
        ),
    }
}

// Saída de escrita

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Tenant,
    Native,
}

impl Origin {
    /// Texto gravado no corpo do record.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Origin::Tenant => "tenant",
            Origin::Native => "native",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Origin> {
        match text {
            "tenant" => Some(Origin::Tenant),
            "native" => Some(Origin::Native),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHead {
    pub seq: u64,
    pub hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendedRecord {
    pub id: RecordId,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendOutcome {
    pub records: Vec<AppendedRecord>,
    pub replayed: bool,
    pub chain_head: ChainHead,
}

// Consulta

/// Nome de campo escalar de primeiro nível de `data`: nada além disso entra no caminho JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldName(String);

impl FieldName {
    pub fn parse(text: &str) -> Result<FieldName, HexlogError> {
        let valid = (1..=64).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if valid {
            return Ok(FieldName(text.to_owned()));
        }
        Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "field-name-format",
            "invalid field name",
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Str(String),
    Int(i64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListFilter {
    pub type_name: Option<TypeName>,
    pub id_prefix: Option<String>,
    pub snapshot: Option<Snapshot>,
    pub where_eq: Vec<(FieldName, Scalar)>,
    pub include_retired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRequest {
    pub limit: u32,
    pub after_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationQuery {
    pub relation_type: Option<RelationTypeName>,
    pub from: Option<SemanticId>,
    pub to: Option<SemanticId>,
    pub include_retired: bool,
}

// Corpo do record (JCS desta estrutura é o que entra no hash)

// O serde só empresta `Cow<str>` direto; `Option<Cow<str>>` viria sempre dono sem este passo.
fn borrow_optional<'de: 'a, 'a, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Cow<'a, str>>, D::Error> {
    #[derive(Deserialize)]
    struct Borrowed<'a>(#[serde(borrow)] Cow<'a, str>);
    Ok(Option::<Borrowed<'a>>::deserialize(deserializer)?.map(|text| text.0))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BodyLineage<'a> {
    Node {
        #[serde(borrow)]
        id: Cow<'a, str>,
    },
    Relation {
        #[serde(borrow)]
        relation_type: Cow<'a, str>,
        #[serde(borrow)]
        from: Cow<'a, str>,
        #[serde(borrow)]
        to: Cow<'a, str>,
        #[serde(borrow)]
        from_origin: Cow<'a, str>,
        #[serde(borrow)]
        to_origin: Cow<'a, str>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BodyAuthor<'a> {
    #[serde(borrow)]
    pub(crate) agent: Cow<'a, str>,
    #[serde(
        borrow,
        default,
        deserialize_with = "borrow_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) model: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(crate) client: Cow<'a, str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BodySnapshot<'a> {
    #[serde(borrow)]
    pub(crate) kind: Cow<'a, str>,
    #[serde(borrow)]
    pub(crate) id: Cow<'a, str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BodyBatch<'a> {
    #[serde(borrow)]
    pub(crate) fingerprint: Cow<'a, str>,
    #[serde(
        borrow,
        default,
        deserialize_with = "borrow_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) key: Option<Cow<'a, str>>,
    pub(crate) count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordBody<'a> {
    pub(crate) v: u8,
    #[serde(borrow)]
    pub(crate) tenant: Cow<'a, str>,
    pub(crate) seq: u64,
    #[serde(borrow)]
    pub(crate) id: Cow<'a, str>,
    #[serde(borrow)]
    pub(crate) at: Cow<'a, str>,
    #[serde(rename = "type", borrow)]
    pub(crate) type_name: Cow<'a, str>,
    #[serde(borrow)]
    pub(crate) lineage: BodyLineage<'a>,
    #[serde(
        borrow,
        default,
        deserialize_with = "borrow_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) revises: Option<Cow<'a, str>>,
    pub(crate) retirement: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<Cow<'a, Value>>,
    #[serde(borrow)]
    pub(crate) author: BodyAuthor<'a>,
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub(crate) snapshot: Option<BodySnapshot<'a>>,
    #[serde(
        borrow,
        default,
        deserialize_with = "borrow_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) external_id: Option<Cow<'a, str>>,
    #[serde(
        borrow,
        default,
        deserialize_with = "borrow_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) native_catalog: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(crate) def_hash: Cow<'a, str>,
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub(crate) batch: Option<BodyBatch<'a>>,
}

impl RecordBody<'_> {
    pub(crate) fn canonical(&self) -> Result<String, HexlogError> {
        let value = serde_json::to_value(self)
            .map_err(|_| HexlogError::internal("record body is not serializable"))?;
        canonical(&value)
    }
}

/// Só `verify_chain` acusa adulteração; fora dela um corpo ilegível é defeito do motor.
pub(crate) fn invalid_body() -> HexlogError {
    HexlogError::refused(
        ErrorCode::Internal,
        "invalid-body",
        "stored record body is not readable",
    )
}

pub(crate) fn read_body(text: &str) -> Result<RecordBody<'_>, HexlogError> {
    serde_json::from_str(text).map_err(|_| invalid_body())
}
