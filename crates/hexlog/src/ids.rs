use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{ErrorCode, HexlogError};

const SEGMENT_MAX: usize = 63;
/// Formato do id (e não teto configurável): 1 a 200 unidades UTF-16.
pub(crate) const IDEMPOTENCY_KEY_MAX: usize = 200;
pub(crate) const EXTERNAL_ID_MAX: usize = 200;
const SEMANTIC_ID_MAX: usize = 200;
const UUID_LEN: usize = 36;

fn is_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    text.len() <= SEGMENT_MAX
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_semantic_id(text: &str) -> bool {
    text.len() <= SEMANTIC_ID_MAX && text.split('.').all(is_name)
}

pub(crate) fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_hash(text: &str) -> bool {
    text.len() == 64 && is_lower_hex(text)
}

fn is_uuid_v7(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == UUID_LEN
        && bytes.iter().enumerate().all(|(index, &b)| match index {
            8 | 13 | 18 | 23 => b == b'-',
            14 => b == b'7',
            19 => matches!(b, b'8' | b'9' | b'a' | b'b'),
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

fn is_record_id(text: &str) -> bool {
    text.split_once(':')
        .is_some_and(|(tenant, uuid)| is_name(tenant) && is_uuid_v7(uuid))
}

fn is_utf16_within(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.encode_utf16().count())
}

macro_rules! string_id {
    ($name:ident, $is_valid:expr, $code:expr, $detail_code:expr, $message:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(text: &str) -> Result<$name, HexlogError> {
                if ($is_valid)(text) {
                    Ok($name(text.to_owned()))
                } else {
                    Err(HexlogError::refused($code, $detail_code, $message))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(
    Name,
    is_name,
    ErrorCode::InvalidInput,
    "name-format",
    "invalid name"
);
string_id!(
    TenantId,
    is_name,
    ErrorCode::InvalidInput,
    "name-format",
    "invalid tenant name"
);
string_id!(
    TypeName,
    is_name,
    ErrorCode::InvalidInput,
    "name-format",
    "invalid type name"
);
string_id!(
    RelationTypeName,
    is_name,
    ErrorCode::InvalidInput,
    "name-format",
    "invalid relation type name"
);
string_id!(
    SemanticId,
    is_semantic_id,
    ErrorCode::InvalidInput,
    "semantic-id-format",
    "invalid semantic id"
);
string_id!(
    RecordId,
    is_record_id,
    ErrorCode::InvalidInput,
    "record-id-format",
    "invalid record id"
);
string_id!(
    Hash,
    is_hash,
    ErrorCode::InvalidInput,
    "hash-format",
    "invalid hash"
);
string_id!(
    IdempotencyKey,
    |text| is_utf16_within(text, IDEMPOTENCY_KEY_MAX),
    ErrorCode::InvalidInput,
    "idempotency-key-format",
    "invalid idempotency key"
);
string_id!(
    ExternalId,
    |text| is_utf16_within(text, EXTERNAL_ID_MAX),
    ErrorCode::InvalidRecord,
    "external-id-format",
    "invalid external id"
);

macro_rules! deserialize_name {
    ($name:ident) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<$name, D::Error> {
                struct NameVisitor;

                impl Visitor<'_> for NameVisitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("a valid name")
                    }

                    fn visit_str<E: de::Error>(self, text: &str) -> Result<$name, E> {
                        $name::parse(text).map_err(E::custom)
                    }
                }

                deserializer.deserialize_str(NameVisitor)
            }
        }
    };
}

deserialize_name!(TypeName);
deserialize_name!(RelationTypeName);

impl Hash {
    /// Quem chama garante 64 hexadecimais minúsculos (saída de `sha256hex`).
    pub(crate) fn from_lower_hex(hex: String) -> Hash {
        debug_assert!(is_hash(&hex));
        Hash(hex)
    }
}

#[cfg(test)]
mod tests;
