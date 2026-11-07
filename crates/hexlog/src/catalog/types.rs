use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::validate::{CompiledSchema, REPORT_MAX, compile, detail, violation};
use crate::engine::chain::{canonical, sha256hex};
use crate::engine::limits::json_issues;
use crate::engine::ports::{DataViolation, Definitions, NodeTypeInfo, RelationTypeInfo};
use crate::error::{Detail, ErrorCode, HexlogError};
use crate::ids::{Hash, RelationTypeName, TypeName};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDef {
    pub name: TypeName,
    pub schema: Value,
    #[serde(default)]
    pub requires_snapshot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationTypeDef {
    pub name: RelationTypeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Vec<TypeName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<TypeName>>,
    #[serde(default)]
    pub acyclic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
}

struct TypeEntry {
    def: TypeDef,
    info: NodeTypeInfo,
    schema: CompiledSchema,
}

struct RelationEntry {
    def: RelationTypeDef,
    info: RelationTypeInfo,
}

/// Definições validadas na construção; só lê depois disso, então é `Send + Sync`.
pub struct Catalog {
    types: HashMap<String, TypeEntry>,
    relations: HashMap<String, RelationEntry>,
}

/// URI bem formada: o `url` aceita e não há espaço nem controle (o `url` apara as pontas em silêncio).
fn is_wellformed_uri(text: &str) -> bool {
    !text.chars().any(|c| c <= ' ' || c == '\u{7f}') && Url::parse(text).is_ok()
}

fn definition_hash<T: Serialize>(def: &T) -> Result<Hash, HexlogError> {
    let document = serde_json::to_value(def)
        .map_err(|_| HexlogError::internal("definition cannot be serialized"))?;
    Ok(sha256hex(&canonical(&document)?))
}

fn refuse_equivalent(equivalent: Option<&str>, path: String, details: &mut Vec<Detail>) {
    if equivalent.is_some_and(|text| !is_wellformed_uri(text)) {
        details.push(detail(path, "equivalent-uri-format"));
    }
}

/// Recusas de um tipo; devolve o schema compilado quando ele está limpo.
fn refuse_type(def: &TypeDef, details: &mut Vec<Detail>) -> Option<CompiledSchema> {
    let schema_path = format!("/types/{}/schema", def.name.as_str());
    let compiled = refuse_schema(&def.schema, &schema_path, details);
    refuse_equivalent(
        def.equivalent.as_deref(),
        format!("/types/{}/equivalent", def.name.as_str()),
        details,
    );
    compiled
}

fn refuse_schema(
    schema: &Value,
    schema_path: &str,
    details: &mut Vec<Detail>,
) -> Option<CompiledSchema> {
    if !matches!(schema.get("type"), Some(Value::String(name)) if name == "object") {
        details.push(detail(schema_path.to_owned(), "root-not-object"));
        return None;
    }
    let issues = json_issues(schema, REPORT_MAX + 1);
    if !issues.is_empty() {
        details.extend(
            issues
                .into_iter()
                .map(|(pointer, code)| detail(format!("{schema_path}{pointer}"), code)),
        );
        return None;
    }
    match compile(schema) {
        Ok(compiled) => Some(compiled),
        Err(found) => {
            details.extend(found.into_iter().map(|mut problem| {
                problem.path.insert_str(0, schema_path);
                problem
            }));
            None
        }
    }
}

fn refuse_relation(def: &RelationTypeDef, details: &mut Vec<Detail>) {
    let base = format!("/relations/{}", def.name.as_str());
    for (field, list) in [("from", &def.from), ("to", &def.to)] {
        let Some(names) = list else { continue };
        let mut seen = HashSet::new();
        if names.is_empty() {
            details.push(detail(format!("{base}/{field}"), "empty-endpoint-list"));
        } else if names.iter().any(|name| !seen.insert(name.as_str())) {
            details.push(detail(format!("{base}/{field}"), "duplicate-endpoint-type"));
        }
    }
    refuse_equivalent(
        def.equivalent.as_deref(),
        format!("{base}/equivalent"),
        details,
    );
}

fn refuse_repeated_names<'n>(
    names: impl Iterator<Item = &'n str>,
    collection: &str,
    details: &mut Vec<Detail>,
) {
    let mut seen = HashSet::new();
    let mut reported = HashSet::new();
    for name in names {
        if !seen.insert(name) && reported.insert(name) {
            details.push(detail(format!("/{collection}/{name}"), "duplicate-name"));
        }
    }
}

impl Catalog {
    fn type_entry(&self, name: &TypeName) -> Option<&TypeEntry> {
        self.types.get(name.as_str())
    }

    fn relation_entry(&self, name: &RelationTypeName) -> Option<&RelationEntry> {
        self.relations.get(name.as_str())
    }

    pub fn new(
        types: Vec<TypeDef>,
        relations: Vec<RelationTypeDef>,
    ) -> Result<Catalog, HexlogError> {
        let mut details = Vec::new();
        let mut schemas = Vec::with_capacity(types.len());
        refuse_repeated_names(
            types.iter().map(|def| def.name.as_str()),
            "types",
            &mut details,
        );
        refuse_repeated_names(
            relations.iter().map(|def| def.name.as_str()),
            "relations",
            &mut details,
        );
        for def in &types {
            if details.len() > REPORT_MAX {
                break;
            }
            schemas.extend(refuse_type(def, &mut details));
        }
        for def in &relations {
            if details.len() > REPORT_MAX {
                break;
            }
            refuse_relation(def, &mut details);
        }
        if !details.is_empty() {
            details.truncate(REPORT_MAX + 1);
            if details.len() > REPORT_MAX {
                details[REPORT_MAX] = detail(String::new(), "too-many-errors");
            }
            return Err(HexlogError {
                code: ErrorCode::InvalidSchema,
                message: "invalid catalog definitions".to_owned(),
                details,
            });
        }

        let mut type_entries = HashMap::with_capacity(types.len());
        for (def, schema) in types.into_iter().zip(schemas) {
            let info = NodeTypeInfo {
                requires_snapshot: def.requires_snapshot,
                def_hash: definition_hash(&def)?,
            };
            type_entries.insert(
                def.name.as_str().to_owned(),
                TypeEntry { def, info, schema },
            );
        }
        let mut relation_entries = HashMap::with_capacity(relations.len());
        for def in relations {
            let info = RelationTypeInfo {
                from: def.from.clone(),
                to: def.to.clone(),
                acyclic: def.acyclic,
                def_hash: definition_hash(&def)?,
            };
            relation_entries.insert(def.name.as_str().to_owned(), RelationEntry { def, info });
        }
        Ok(Catalog {
            types: type_entries,
            relations: relation_entries,
        })
    }

    pub fn type_def(&self, name: &TypeName) -> Option<&TypeDef> {
        self.type_entry(name).map(|entry| &entry.def)
    }

    pub fn relation_type_def(&self, name: &RelationTypeName) -> Option<&RelationTypeDef> {
        self.relation_entry(name).map(|entry| &entry.def)
    }

    pub fn type_hash(&self, name: &TypeName) -> Option<&Hash> {
        self.type_entry(name).map(|entry| &entry.info.def_hash)
    }

    pub fn relation_type_hash(&self, name: &RelationTypeName) -> Option<&Hash> {
        self.relation_entry(name).map(|entry| &entry.info.def_hash)
    }

    /// Todas as violações de `data` contra o schema do tipo, até 50 e o marcador `too-many-errors`.
    pub fn validate_data(&self, name: &TypeName, data: &Value) -> Result<(), Vec<DataViolation>> {
        match self.type_entry(name) {
            Some(entry) => entry.schema.validate(data),
            None => Err(vec![violation(String::new(), "type-not-found")]),
        }
    }
}

impl Definitions for Catalog {
    fn node_type(&self, name: &TypeName) -> Option<&NodeTypeInfo> {
        self.type_entry(name).map(|entry| &entry.info)
    }

    fn relation_type(&self, name: &RelationTypeName) -> Option<&RelationTypeInfo> {
        self.relation_entry(name).map(|entry| &entry.info)
    }

    fn validate_data(&self, name: &TypeName, data: &Value) -> Result<(), Vec<DataViolation>> {
        Catalog::validate_data(self, name, data)
    }
}
