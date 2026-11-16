use hexlog::{Catalog, Detail, ErrorCode, HexlogError, RelationTypeDef, TypeDef};
use serde_json::Value;

/// Tipos que contam como work item: o base e os filhos que o especializam.
pub const WORK_ITEM_TYPES: [&str; 2] = ["work-item", "pull-request"];

pub const PREMISE_NAMESPACE: &str = "premises";
pub const GUIDELINE_NAMESPACE: &str = "guidelines";
pub const GAP_NAMESPACE: &str = "gaps";
pub const DIVERGENCE_NAMESPACE: &str = "divergences";

/// Namespace que o id de um node deve ter, por tipo.
pub fn namespace_of(type_name: &str) -> Option<&'static str> {
    match type_name {
        "check" => Some("checks"),
        "guideline" => Some(GUIDELINE_NAMESPACE),
        "premise" => Some(PREMISE_NAMESPACE),
        "decision" => Some("decisions"),
        "gap" => Some(GAP_NAMESPACE),
        "divergence" => Some(DIVERGENCE_NAMESPACE),
        "attestation" => Some("attestations"),
        "work-item" | "pull-request" => Some("work-items"),
        _ => None,
    }
}

const SOURCES: [&str; 3] = [
    include_str!("decisions/catalog.json"),
    include_str!("checks/catalog.json"),
    include_str!("work_items/catalog.json"),
];

/// Os tipos e as relações dos três grafos, na ordem das fontes.
fn definitions() -> Result<(Vec<TypeDef>, Vec<RelationTypeDef>), HexlogError> {
    let mut types = Vec::new();
    let mut relations = Vec::new();
    for source in SOURCES {
        let mut file: Value =
            serde_json::from_str(source).map_err(|error| invalid_schema(&error))?;
        let listed_types = serde_json::from_value::<Vec<TypeDef>>(take_section(&mut file, "types"));
        types.extend(listed_types.map_err(|error| invalid_schema(&error))?);
        let listed_relations =
            serde_json::from_value::<Vec<RelationTypeDef>>(take_section(&mut file, "relations"));
        relations.extend(listed_relations.map_err(|error| invalid_schema(&error))?);
    }

    Ok((types, relations))
}

/// Junta os catálogos dos três grafos; o `Catalog` é montado a cada chamada.
pub fn catalog() -> Result<Catalog, HexlogError> {
    let (types, relations) = definitions()?;

    Catalog::new(types, relations)
}

/// Os tipos dos três grafos, para quem precisa percorrê-los (o `Catalog` não os lista).
pub fn type_defs() -> Result<Vec<TypeDef>, HexlogError> {
    Ok(definitions()?.0)
}

fn take_section(file: &mut Value, key: &str) -> Value {
    file.get_mut(key).map_or(Value::Null, Value::take)
}

fn invalid_schema(error: &serde_json::Error) -> HexlogError {
    let message = error.to_string();
    HexlogError {
        code: ErrorCode::InvalidSchema,
        details: vec![Detail {
            path: String::new(),
            code: "invalid-catalog-json".to_owned(),
            message: message.clone(),
            current: None,
            retired: None,
        }],
        message,
    }
}
