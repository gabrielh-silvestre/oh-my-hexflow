use hexlog::{Catalog, FieldName, ListFilter, Scalar, SemanticId, TypeDef, TypeName};
use serde_json::Value;

use super::engine::parse_at;
use super::error::AppError;
use crate::graphs::namespace_of;

/// Teto de um segmento de `SemanticId` no `hexlog` (`hexlog/src/ids.rs`, privado); a mensagem do prefixo o repete.
const ID_SEGMENT_MAX: usize = 63;
/// Teto de um `SemanticId` no `hexlog`; um prefixo maior nunca casa.
const ID_PREFIX_MAX: usize = 200;

/// Campo de texto para o `--where`: `type` igual a `string`; sem `type`, `enum` só de strings.
pub fn is_text_field(schema: &Value) -> bool {
    match (schema.get("type"), schema.get("enum")) {
        (Some(kind), _) => kind == "string",
        (None, Some(Value::Array(values))) => values.iter().all(Value::is_string),
        _ => false,
    }
}

/// Recusa o filtro que nunca casa nada; o primeiro erro vence. A ordem: sintaxe do `--id-prefix`,
/// sintaxe do `--type`, `--type` no catálogo, namespace do `--id-prefix`, campo repetido com valor
/// diferente e, por fim, cada `--where` em ordem de índice (campo, texto, valor aceito).
pub(super) fn validate_list_filter(
    catalog: &Catalog,
    type_name: Option<&str>,
    id_prefix: Option<&str>,
    where_eq: &[(String, String)],
) -> Result<ListFilter, AppError> {
    if let Some(prefix) = id_prefix {
        check_id_prefix_syntax(prefix)?;
    }
    let type_name = type_name
        .map(|name| parse_at(TypeName::parse, name, "/type"))
        .transpose()?;
    let defs: Vec<&TypeDef> = catalog.type_defs().collect();
    let typed = type_name
        .as_ref()
        .map(|name| find_type(catalog, name))
        .transpose()?;
    if let Some(prefix) = id_prefix {
        check_prefix_namespace(&defs, typed, prefix)?;
    }
    check_repeated_fields(where_eq)?;
    let pairs = check_where_fields(&defs, typed, id_prefix, where_eq)?;

    Ok(ListFilter {
        type_name,
        id_prefix: id_prefix.map(str::to_owned),
        snapshot: None,
        where_eq: pairs,
    })
}

/// `SemanticId::parse` só como predicado: o código e a mensagem do hexlog não saem daqui.
fn check_id_prefix_syntax(prefix: &str) -> Result<(), AppError> {
    if SemanticId::parse(prefix).is_ok() {
        return Ok(());
    }

    Err(AppError::invalid_arguments(
        "/id_prefix",
        &format!(
            "id prefix must be dot-separated names of 1 to {ID_SEGMENT_MAX} characters, each made of lowercase letters, digits and hyphens and not starting with a hyphen, up to {ID_PREFIX_MAX} characters in total"
        ),
    ))
}

fn find_type<'a>(catalog: &'a Catalog, name: &TypeName) -> Result<&'a TypeDef, AppError> {
    catalog
        .type_def(name)
        .ok_or_else(|| AppError::invalid_arguments("/type", "type is not defined in the catalog"))
}

fn first_segment(prefix: &str) -> &str {
    prefix.split('.').next().unwrap_or(prefix)
}

fn in_namespace(def: &TypeDef, namespace: &str) -> bool {
    namespace_of(def.name.as_str()) == Some(namespace)
}

fn check_prefix_namespace(
    defs: &[&TypeDef],
    typed: Option<&TypeDef>,
    prefix: &str,
) -> Result<(), AppError> {
    let namespace = first_segment(prefix);
    let message = match typed {
        Some(def) if in_namespace(def, namespace) => return Ok(()),
        Some(_) => "id prefix is outside the namespace of the type",
        None if defs.iter().any(|def| in_namespace(def, namespace)) => return Ok(()),
        None => "id prefix must start with the namespace of a type",
    };

    Err(AppError::invalid_arguments("/id_prefix", message))
}

fn declared_schema<'a>(def: &'a TypeDef, field: &str) -> Option<&'a Value> {
    def.schema.get("properties")?.get(field)
}

fn accepts(def: &TypeDef, field: &str, value: &str) -> bool {
    declared_schema(def, field).is_some_and(|schema| {
        is_text_field(schema)
            && schema
                .get("enum")
                .and_then(Value::as_array)
                .is_none_or(|values| values.iter().any(|allowed| allowed.as_str() == Some(value)))
    })
}

fn declares_text(defs: &[&TypeDef], field: &str) -> bool {
    defs.iter()
        .any(|def| declared_schema(def, field).is_some_and(is_text_field))
}

/// Reduz o conjunto de tipos candidatos a cada `--where`; esvaziá-lo recusa o filtro.
fn check_where_fields(
    defs: &[&TypeDef],
    typed: Option<&TypeDef>,
    id_prefix: Option<&str>,
    where_eq: &[(String, String)],
) -> Result<Vec<(FieldName, Scalar)>, AppError> {
    let initial: Vec<&TypeDef> = match (typed, id_prefix) {
        (Some(def), _) => vec![def],
        (None, Some(prefix)) => {
            let namespace = first_segment(prefix);
            defs.iter()
                .copied()
                .filter(|def| in_namespace(def, namespace))
                .collect()
        }
        (None, None) => defs.to_vec(),
    };
    let mut candidates = initial.clone();
    let mut pairs = Vec::with_capacity(where_eq.len());
    for (index, (field, value)) in where_eq.iter().enumerate() {
        let pointer = format!("/where/{index}");
        let name = parse_at(FieldName::parse, field, &pointer)?;
        let remaining: Vec<&TypeDef> = candidates
            .iter()
            .copied()
            .filter(|def| accepts(def, field, value))
            .collect();
        if remaining.is_empty() {
            let message = empty_set_reason(typed, &initial, &candidates, field);

            return Err(AppError::invalid_arguments(&pointer, message));
        }
        candidates = remaining;
        pairs.push((name, Scalar::Str(value.clone())));
    }

    Ok(pairs)
}

/// O motivo de o conjunto esvaziar, olhando o conjunto antes do `--where` e sem repetir o valor.
fn empty_set_reason(
    typed: Option<&TypeDef>,
    initial: &[&TypeDef],
    before: &[&TypeDef],
    field: &str,
) -> &'static str {
    if !declares_text(initial, field) {
        return match typed {
            Some(def) if declared_schema(def, field).is_none() => {
                "field is not declared by the type"
            }
            Some(_) => "field is not a text field; where compares text only",
            None => "field is not a text field of any type the filter allows",
        };
    }
    if !declares_text(before, field) {
        return "no type declares every where field";
    }

    "value is not one of the values the field allows"
}

/// Um campo tem um valor só: dois valores distintos nunca casam; o par idêntico segue aceito.
fn check_repeated_fields(where_eq: &[(String, String)]) -> Result<(), AppError> {
    for (index, (field, value)) in where_eq.iter().enumerate() {
        if where_eq[..index]
            .iter()
            .any(|(earlier, other)| earlier == field && other != value)
        {
            return Err(AppError::invalid_arguments(
                &format!("/where/{index}"),
                "field repeated in where with different values",
            ));
        }
    }

    Ok(())
}
