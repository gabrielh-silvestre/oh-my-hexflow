use hexlog::{Catalog, Detail, ErrorCode, HexlogError, RelationTypeDef, TypeDef};
use serde_json::Value;

/// Tipos que contam como work item: o base e os filhos que o especializam.
pub const WORK_ITEM_TYPES: [&str; 2] = ["work-item", "pull-request"];

const SOURCES: [&str; 3] = [
    include_str!("decisions/catalog.json"),
    include_str!("checks/catalog.json"),
    include_str!("work_items/catalog.json"),
];

/// Junta os catálogos dos três grafos; o `Catalog` é montado a cada chamada.
pub fn catalog() -> Result<Catalog, HexlogError> {
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
    Catalog::new(types, relations)
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
