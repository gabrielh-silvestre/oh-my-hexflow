use hexlog::{Catalog, RelationTypeDef, TypeDef, TypeName};
use serde::Deserialize;
use serde_json::Value;

// Catálogo de referência das tabelas da seção 5 e da tabela de mutações: `note` (sem snapshot),
// `strict-note` (`requires_snapshot`), `task`, a relation `after` (`acyclic`, qualquer extremo) e
// `owned-by` (só de `task` para `note`). `note` não fecha `additionalProperties`: os testes de
// filtro gravam campos livres em `data`.
const REFERENCE: &str = r#"{
  "types": [
    {
      "name": "note",
      "schema": {
        "type": "object",
        "properties": {
          "text": {"type": "string", "maxLength": 200},
          "count": {"type": "integer", "minimum": 0}
        }
      }
    },
    {
      "name": "strict-note",
      "schema": {
        "type": "object",
        "properties": {
          "text": {"type": "string", "maxLength": 200}
        }
      },
      "requires_snapshot": true
    },
    {
      "name": "task",
      "schema": {"type": "object"}
    }
  ],
  "relations": [
    {"name": "after", "acyclic": true},
    {"name": "owned-by", "from": ["task"], "to": ["note"]}
  ]
}"#;

#[derive(Deserialize)]
struct Reference {
    types: Vec<TypeDef>,
    relations: Vec<RelationTypeDef>,
}

pub fn reference_catalog() -> Catalog {
    let reference: Reference =
        serde_json::from_str(REFERENCE).expect("the embedded reference definitions are valid JSON");

    Catalog::new(reference.types, reference.relations)
        .expect("the reference definitions are accepted by the catalog")
}

/// Catálogo local só com os tipos dados (nome e schema), sem relation types nem `requires_snapshot`.
pub fn catalog_of(types: Vec<(&str, Value)>) -> Catalog {
    let types = types
        .into_iter()
        .map(|(name, schema)| TypeDef {
            name: TypeName::parse(name).expect("valid type name"),
            schema,
            requires_snapshot: false,
            equivalent: None,
        })
        .collect();

    Catalog::new(types, Vec::new()).expect("the local definitions are accepted by the catalog")
}
