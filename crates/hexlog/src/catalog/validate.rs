use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use petgraph::Directed;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::Graph;
use serde_json::{Map, Value};

use crate::engine::limits::{JSON_NESTING_MAX, json_issues};
use crate::engine::ports::DataViolation;
use crate::error::{Detail, TOO_MANY_ERRORS, push_pointer_segment};
use crate::ids::Hash;

type NodeId = usize;

const ROOT: NodeId = 0;
/// Corte de achados: o catálogo e `validate_data` guardam 50 e o 51º só mostra que passou.
pub(super) const REPORT_MAX: usize = 50;
const PROBLEM_MAX: usize = REPORT_MAX + 1;
/// Quadros de avaliação: com 2 MiB de pilha em build de teste sobra margem de 1,8x (D-7).
const EVAL_DEPTH_MAX: usize = 1_024;
const SCHEMA_URI: &str = "https://json-schema.org/draft/2020-12/schema";
const ATTACHMENT: &str = "attachment";

const TYPE_NULL: u8 = 1;
const TYPE_BOOLEAN: u8 = 2;
const TYPE_OBJECT: u8 = 4;
const TYPE_ARRAY: u8 = 8;
const TYPE_NUMBER: u8 = 16;
const TYPE_STRING: u8 = 32;
const TYPE_INTEGER: u8 = 64;

/// `message` repete o código sem hífens: a mensagem nunca carrega valor do schema.
pub(super) fn detail(path: String, code: &str) -> Detail {
    Detail::new(path, code, &code.replace('-', " "))
}

pub(super) fn violation(pointer: String, code: &str) -> DataViolation {
    DataViolation {
        pointer,
        code: code.to_owned(),
    }
}

/// Mais de 50 achados viram os 50 primeiros e um `too-many-errors` em `""`.
fn capped(mut found: Vec<DataViolation>) -> Vec<DataViolation> {
    if found.len() > REPORT_MAX {
        found.truncate(REPORT_MAX);
        found.push(violation(String::new(), TOO_MANY_ERRORS));
    }
    found
}

fn type_bit(name: &str) -> Option<u8> {
    match name {
        "null" => Some(TYPE_NULL),
        "boolean" => Some(TYPE_BOOLEAN),
        "object" => Some(TYPE_OBJECT),
        "array" => Some(TYPE_ARRAY),
        "number" => Some(TYPE_NUMBER),
        "string" => Some(TYPE_STRING),
        "integer" => Some(TYPE_INTEGER),
        _ => None,
    }
}

/// `type` é um nome ou uma lista de nomes únicos; qualquer outra coisa é `None`.
fn type_mask(value: &Value) -> Option<u8> {
    match value {
        Value::String(name) => type_bit(name),
        Value::Array(names) if !names.is_empty() => names.iter().try_fold(0u8, |mask, name| {
            let bit = type_bit(name.as_str()?)?;
            (mask & bit == 0).then_some(mask | bit)
        }),
        _ => None,
    }
}

fn type_bits_of(value: &Value) -> u8 {
    match value {
        Value::Null => TYPE_NULL,
        Value::Bool(_) => TYPE_BOOLEAN,
        Value::Object(_) => TYPE_OBJECT,
        Value::Array(_) => TYPE_ARRAY,
        Value::String(_) => TYPE_STRING,
        // A guarda `json_issues` já recusou o que não é inteiro seguro.
        Value::Number(_) => TYPE_NUMBER | TYPE_INTEGER,
    }
}

fn has_string_type(map: &Map<String, Value>) -> bool {
    match map.get("type") {
        Some(Value::String(name)) => name == "string",
        Some(Value::Array(names)) => names.iter().any(|name| name == "string"),
        _ => false,
    }
}

/// Decodifica `%XX` do fragmento de URI; bytes que não formam UTF-8 válido dão `None`.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Desce o ponteiro por `/<key>` e devolve a marca para o `truncate` de volta.
fn enter_key(pointer: &mut String, key: &str) -> usize {
    let mark = pointer.len();
    pointer.push('/');
    push_pointer_segment(pointer, key);
    mark
}

/// Desce o ponteiro por `/<index>` e devolve a marca para o `truncate` de volta.
fn enter_index(pointer: &mut String, index: usize) -> usize {
    let mark = pointer.len();
    let _ = write!(pointer, "/{index}");
    mark
}

/// `names` sem repetição vira `Some`; o `required` do metaesquema 2020-12 exige nomes únicos.
fn required_names(value: &Value) -> Option<Vec<String>> {
    let names: Vec<&str> = value
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<_>>()?;
    let unique: HashSet<&str> = names.iter().copied().collect();
    (unique.len() == names.len()).then(|| names.into_iter().map(str::to_owned).collect())
}

// ---------------------------------------------------------------- compilação

#[derive(Default)]
struct SchemaNode {
    /// Falso para o schema booleano `false`.
    deny: bool,
    types: Option<u8>,
    enum_values: Option<Vec<Value>>,
    const_value: Option<Value>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    min_items: Option<u64>,
    max_items: Option<u64>,
    format_attachment: bool,
    required: Vec<String>,
    /// Ordenadas por nome na compilação: a busca binária depende disso.
    properties: Vec<(String, NodeId)>,
    additional: Option<NodeId>,
    items: Option<NodeId>,
    all_of: Vec<NodeId>,
    any_of: Vec<NodeId>,
    one_of: Vec<NodeId>,
    not: Option<NodeId>,
    reference: Option<NodeId>,
}

pub(super) struct CompiledSchema {
    nodes: Vec<SchemaNode>,
}

/// Onde o schema está, para a regra de posição de `format`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Root,
    /// `properties/<campo>` da raiz.
    Field,
    /// `items` de um campo da raiz.
    FieldItems,
    Other,
}

struct Reference<'a> {
    holder: NodeId,
    pointer: String,
    text: &'a str,
}

struct Builder<'a> {
    nodes: Vec<SchemaNode>,
    by_pointer: HashMap<String, NodeId>,
    pointer: String,
    references: Vec<Reference<'a>>,
    /// Arestas de "mesma posição da instância": `allOf`, `anyOf`, `oneOf`, `not` e `$ref`.
    edges: Vec<(NodeId, NodeId)>,
    problems: Vec<Detail>,
}

/// Compila qualquer schema do subconjunto, sem a regra de raiz; os `path` são relativos à raiz do schema.
pub(super) fn compile(schema: &Value) -> Result<CompiledSchema, Vec<Detail>> {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_pointer: HashMap::new(),
        pointer: String::new(),
        references: Vec::new(),
        edges: Vec::new(),
        problems: Vec::new(),
    };
    builder.schema(schema, 0, Position::Root);
    builder.finish()
}

impl<'a> Builder<'a> {
    fn problem(&mut self, path: String, code: &str) {
        if self.problems.len() < PROBLEM_MAX {
            self.problems.push(detail(path, code));
        }
    }

    fn keyword_path(&self, key: &str) -> String {
        let mut path = self.pointer.clone();
        enter_key(&mut path, key);
        path
    }

    fn keyword_problem(&mut self, key: &str, code: &str) {
        let path = self.keyword_path(key);
        self.problem(path, code);
    }

    fn schema(&mut self, value: &'a Value, depth: usize, position: Position) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(SchemaNode::default());
        self.by_pointer.insert(self.pointer.clone(), id);
        if depth >= JSON_NESTING_MAX {
            let path = self.pointer.clone();
            self.problem(path, "max-depth-exceeded");
            return id;
        }
        match value {
            Value::Bool(allowed) => self.nodes[id].deny = !allowed,
            Value::Object(map) => self.keywords(id, map, depth, position),
            _ => {
                let path = self.pointer.clone();
                self.problem(path, "keyword-type");
            }
        }
        id
    }

    /// Desce em um schema filho sob `key`, deixando o ponteiro como estava.
    fn child(&mut self, key: &str, value: &'a Value, depth: usize, position: Position) -> NodeId {
        let mark = enter_key(&mut self.pointer, key);
        let id = self.schema(value, depth + 1, position);
        self.pointer.truncate(mark);
        id
    }

    /// Mapa de nome para schema (`properties`, `$defs`): as chaves são nomes, nunca palavras-chave.
    fn named_schemas(
        &mut self,
        key: &str,
        value: &'a Value,
        depth: usize,
        position: Position,
    ) -> Vec<(&'a str, NodeId)> {
        let Value::Object(entries) = value else {
            self.keyword_problem(key, "keyword-type");
            return Vec::new();
        };
        let mark = enter_key(&mut self.pointer, key);
        let found = entries
            .iter()
            .map(|(name, entry)| {
                let inner = enter_key(&mut self.pointer, name);
                let id = self.schema(entry, depth + 2, position);
                self.pointer.truncate(inner);
                (name.as_str(), id)
            })
            .collect();
        self.pointer.truncate(mark);
        found
    }

    /// `allOf`, `anyOf` e `oneOf`: lista não vazia de schemas aplicados à mesma posição.
    fn branches(
        &mut self,
        owner: NodeId,
        key: &str,
        value: &'a Value,
        depth: usize,
    ) -> Vec<NodeId> {
        let Value::Array(items) = value else {
            self.keyword_problem(key, "keyword-type");
            return Vec::new();
        };
        if items.is_empty() {
            self.keyword_problem(key, "keyword-type");
            return Vec::new();
        }
        let mark = enter_key(&mut self.pointer, key);
        let found: Vec<NodeId> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let inner = enter_index(&mut self.pointer, index);
                let id = self.schema(item, depth + 2, Position::Other);
                self.pointer.truncate(inner);
                id
            })
            .collect();
        self.pointer.truncate(mark);
        self.edges
            .extend(found.iter().map(|&branch| (owner, branch)));
        found
    }

    /// Valor de tipo errado dá `keyword-type` na palavra.
    fn require_type(&mut self, key: &str, ok: bool) -> bool {
        if !ok {
            self.keyword_problem(key, "keyword-type");
        }
        ok
    }

    fn bound(&mut self, key: &str, value: &Value) -> Option<i64> {
        let found = value.as_i64();
        self.require_type(key, found.is_some());
        found
    }

    fn count(&mut self, key: &str, value: &Value) -> Option<u64> {
        let found = value.as_u64();
        self.require_type(key, found.is_some());
        found
    }

    fn keywords(
        &mut self,
        id: NodeId,
        map: &'a Map<String, Value>,
        depth: usize,
        position: Position,
    ) {
        for (key, value) in map {
            match key.as_str() {
                "type" => {
                    let mask = type_mask(value);
                    self.require_type(key, mask.is_some());
                    self.nodes[id].types = mask;
                }
                "enum" => {
                    self.require_type(key, value.is_array());
                    self.nodes[id].enum_values = value.as_array().cloned();
                }
                "const" => self.nodes[id].const_value = Some(value.clone()),
                "minimum" => self.nodes[id].minimum = self.bound(key, value),
                "maximum" => self.nodes[id].maximum = self.bound(key, value),
                "minLength" => self.nodes[id].min_length = self.count(key, value),
                "maxLength" => self.nodes[id].max_length = self.count(key, value),
                "minItems" => self.nodes[id].min_items = self.count(key, value),
                "maxItems" => self.nodes[id].max_items = self.count(key, value),
                "required" => {
                    let names = required_names(value);
                    self.require_type(key, names.is_some());
                    self.nodes[id].required = names.unwrap_or_default();
                }
                "properties" => {
                    let fields_position = if position == Position::Root {
                        Position::Field
                    } else {
                        Position::Other
                    };
                    let mut found = self.named_schemas(key, value, depth, fields_position);
                    found.sort_unstable_by_key(|&(name, _)| name);
                    self.nodes[id].properties = found
                        .into_iter()
                        .map(|(name, child)| (name.to_owned(), child))
                        .collect();
                }
                "$defs" => {
                    self.named_schemas(key, value, depth, Position::Other);
                }
                "additionalProperties" => {
                    let found = self.child(key, value, depth, Position::Other);
                    self.nodes[id].additional = Some(found);
                }
                "items" => {
                    let items_position = if position == Position::Field {
                        Position::FieldItems
                    } else {
                        Position::Other
                    };
                    let found = self.child(key, value, depth, items_position);
                    self.nodes[id].items = Some(found);
                }
                "not" => {
                    let found = self.child(key, value, depth, Position::Other);
                    self.edges.push((id, found));
                    self.nodes[id].not = Some(found);
                }
                "allOf" => self.nodes[id].all_of = self.branches(id, key, value, depth),
                "anyOf" => self.nodes[id].any_of = self.branches(id, key, value, depth),
                "oneOf" => self.nodes[id].one_of = self.branches(id, key, value, depth),
                "$ref" => self.reference(id, key, value),
                "$schema" => match value {
                    Value::String(uri) if position == Position::Root && uri == SCHEMA_URI => {}
                    Value::String(_) => self.keyword_problem(key, "schema-uri"),
                    _ => self.keyword_problem(key, "keyword-type"),
                },
                "format" => self.format(id, key, value, map, position),
                "title" | "description" | "$comment" => {
                    self.require_type(key, value.is_string());
                }
                "default" => {}
                "examples" => {
                    self.require_type(key, value.is_array());
                }
                "deprecated" | "readOnly" | "writeOnly" => {
                    self.require_type(key, value.is_boolean());
                }
                "pattern" | "patternProperties" => self.keyword_problem(key, "pattern-not-allowed"),
                _ => self.keyword_problem(key, "keyword-not-allowed"),
            }
        }
    }

    fn reference(&mut self, holder: NodeId, key: &str, value: &'a Value) {
        let Value::String(text) = value else {
            self.keyword_problem(key, "keyword-type");
            return;
        };
        if text != "#" && !text.starts_with("#/") {
            self.keyword_problem(key, "ref-not-internal");
            return;
        }
        let pointer = self.keyword_path(key);
        self.references.push(Reference {
            holder,
            pointer,
            text,
        });
    }

    /// `attachment` só em `properties/<campo>` e `items` dele, e só em schema com `type` string (D-11).
    fn format(
        &mut self,
        id: NodeId,
        key: &str,
        value: &Value,
        siblings: &Map<String, Value>,
        position: Position,
    ) {
        let Value::String(name) = value else {
            self.keyword_problem(key, "keyword-type");
            return;
        };
        let allowed_here =
            matches!(position, Position::Field | Position::FieldItems) && has_string_type(siblings);
        if name == ATTACHMENT && allowed_here {
            self.nodes[id].format_attachment = true;
        } else {
            self.keyword_problem(key, "format-not-supported");
        }
    }

    /// Resolve `#` e `#/...` para uma posição de schema; qualquer outro alvo é `None`.
    fn target_of(&self, text: &str) -> Option<NodeId> {
        let decoded = percent_decode(text.strip_prefix('#')?)?;
        self.by_pointer.get(decoded.as_str()).copied()
    }

    /// Componente fortemente conexa de cada nó, sem recursão (`kosaraju_scc` é iterativo).
    fn components(&self) -> Vec<usize> {
        let mut graph =
            Graph::<(), (), Directed, usize>::with_capacity(self.nodes.len(), self.edges.len());
        for _ in 0..self.nodes.len() {
            graph.add_node(());
        }
        graph.extend_with_edges(self.edges.iter().copied());
        let mut component = vec![0; self.nodes.len()];
        for (number, members) in kosaraju_scc(&graph).into_iter().enumerate() {
            for member in members {
                component[member.index()] = number;
            }
        }
        component
    }

    fn finish(mut self) -> Result<CompiledSchema, Vec<Detail>> {
        let mut resolved = Vec::with_capacity(self.references.len());
        for reference in std::mem::take(&mut self.references) {
            match self.target_of(reference.text) {
                Some(target) => {
                    self.nodes[reference.holder].reference = Some(target);
                    self.edges.push((reference.holder, target));
                    resolved.push((reference.holder, target, reference.pointer));
                }
                None => self.problem(reference.pointer, "ref-unresolved"),
            }
        }
        let component = self.components();
        let mut cyclic: Vec<String> = resolved
            .into_iter()
            .filter(|&(holder, target, _)| component[holder] == component[target])
            .map(|(_, _, pointer)| pointer)
            .collect();
        cyclic.sort();
        for pointer in cyclic {
            self.problem(pointer, "ref-cycle");
        }
        if self.problems.is_empty() {
            Ok(CompiledSchema { nodes: self.nodes })
        } else {
            Err(self.problems)
        }
    }
}

// ----------------------------------------------------------------- avaliação

#[derive(Clone, Copy)]
enum Segment<'s> {
    Key(&'s str),
    Index(usize),
}

/// Estado de uma chamada de `validate`: nada é compartilhado entre chamadas.
struct Run<'a> {
    schema: &'a CompiledSchema,
    /// Resultado booleano por (nó, endereço do `&Value`): mantém o custo polinomial num DAG de `$ref`.
    memo: HashMap<(NodeId, usize), bool>,
    /// Pares já descritos, para o relatório não repetir o mesmo ramo.
    reported: HashSet<(NodeId, usize)>,
    pointer: String,
    found: Vec<DataViolation>,
    overflow: bool,
}

/// Códigos das palavras escalares que falham, na ordem do contrato, sem o `type`.
/// Precondição: números de `value` já passaram pela guarda de inteiro seguro.
fn scalar_failures(node: &SchemaNode, value: &Value) -> Vec<&'static str> {
    let mut failed = Vec::new();
    if let Some(options) = &node.enum_values
        && !options.contains(value)
    {
        failed.push("enum");
    }
    if let Some(expected) = &node.const_value
        && expected != value
    {
        failed.push("const");
    }
    if let Value::Number(number) = value
        && let Some(integer) = number.as_i64()
    {
        if node.minimum.is_some_and(|limit| integer < limit) {
            failed.push("minimum");
        }
        if node.maximum.is_some_and(|limit| integer > limit) {
            failed.push("maximum");
        }
    }
    if let Value::String(text) = value {
        let length = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
        if node.min_length.is_some_and(|limit| length < limit) {
            failed.push("min-length");
        }
        if node.max_length.is_some_and(|limit| length > limit) {
            failed.push("max-length");
        }
        if node.format_attachment && Hash::parse(text).is_err() {
            failed.push("format");
        }
    }
    if let Value::Array(items) = value {
        let length = u64::try_from(items.len()).unwrap_or(u64::MAX);
        if node.min_items.is_some_and(|limit| length < limit) {
            failed.push("min-items");
        }
        if node.max_items.is_some_and(|limit| length > limit) {
            failed.push("max-items");
        }
    }
    failed
}

impl CompiledSchema {
    /// Guarda de inteiro seguro e de profundidade, depois todas as violações (até 50 e o marcador).
    pub(super) fn validate(&self, data: &Value) -> Result<(), Vec<DataViolation>> {
        let issues = json_issues(data, REPORT_MAX + 1);
        if !issues.is_empty() {
            return Err(capped(
                issues
                    .into_iter()
                    .map(|(pointer, code)| violation(pointer, code))
                    .collect(),
            ));
        }
        let mut run = Run {
            schema: self,
            memo: HashMap::new(),
            reported: HashSet::new(),
            pointer: String::new(),
            found: Vec::new(),
            overflow: false,
        };
        let valid = run.eval(ROOT, data, 0, false);
        if !run.overflow && !valid {
            run.eval(ROOT, data, 0, true);
        }
        if run.overflow {
            return Err(vec![violation(String::new(), "max-depth-exceeded")]);
        }
        if valid {
            Ok(())
        } else {
            Err(capped(run.found))
        }
    }
}

impl Run<'_> {
    fn report(&mut self, code: &str) {
        let repeated = self
            .found
            .iter()
            .any(|known| known.pointer == self.pointer && known.code == code);
        if self.found.len() <= REPORT_MAX && !repeated {
            self.found.push(violation(self.pointer.clone(), code));
        }
    }

    fn report_at(&mut self, key: &str, code: &str) {
        let mark = enter_key(&mut self.pointer, key);
        self.report(code);
        self.pointer.truncate(mark);
    }

    /// `collect` falso só diz se `value` vale para o nó (com memo); verdadeiro também descreve as violações.
    fn eval(&mut self, id: NodeId, value: &Value, depth: usize, collect: bool) -> bool {
        if self.overflow {
            return false;
        }
        if depth > EVAL_DEPTH_MAX {
            self.overflow = true;
            return false;
        }
        let key = (id, std::ptr::from_ref(value).addr());
        if collect {
            if !self.reported.insert(key) {
                return false;
            }
        } else if let Some(&known) = self.memo.get(&key) {
            return known;
        }
        let valid = self.keywords(id, value, depth, collect);
        if !collect && !self.overflow {
            self.memo.insert(key, valid);
        }
        valid
    }

    /// Aplica o schema `id` a `value`. Em `collect`, desce só no que falha, sob `segment` quando há.
    fn apply(
        &mut self,
        id: NodeId,
        value: &Value,
        segment: Option<Segment<'_>>,
        depth: usize,
        collect: bool,
    ) -> bool {
        let valid = self.eval(id, value, depth + 1, false);
        if valid || !collect {
            return valid;
        }
        let mark = match segment {
            Some(Segment::Key(key)) => enter_key(&mut self.pointer, key),
            Some(Segment::Index(index)) => enter_index(&mut self.pointer, index),
            None => self.pointer.len(),
        };
        self.eval(id, value, depth + 1, true);
        self.pointer.truncate(mark);
        false
    }

    /// Cada grupo de palavras fica numa função própria: o quadro da recursão carrega só o grupo em que desce.
    fn keywords(&mut self, id: NodeId, value: &Value, depth: usize, collect: bool) -> bool {
        let node = &self.schema.nodes[id];
        if node.deny {
            if collect {
                self.report("false-schema");
            }
            return false;
        }
        if let Some(mask) = node.types
            && mask & type_bits_of(value) == 0
        {
            if collect {
                self.report("type");
            }
            return false;
        }
        let mut valid = true;
        for code in scalar_failures(node, value) {
            if !collect {
                return false;
            }
            valid = false;
            self.report(code);
        }
        let structure_valid = match value {
            Value::Object(map) => self.object_rules(node, map, depth, collect),
            Value::Array(elements) => self.array_rules(node, elements, depth, collect),
            _ => true,
        };
        if !structure_valid {
            if !collect {
                return false;
            }
            valid = false;
        }
        if !self.combinator_rules(node, value, depth, collect) {
            return false;
        }
        valid
    }

    /// `required`, `properties` e `additionalProperties`, nesta ordem.
    fn object_rules(
        &mut self,
        node: &SchemaNode,
        map: &Map<String, Value>,
        depth: usize,
        collect: bool,
    ) -> bool {
        let mut valid = true;
        for name in &node.required {
            if !map.contains_key(name) {
                if !collect {
                    return false;
                }
                valid = false;
                self.report_at(name, "required");
            }
        }
        for (name, child) in &node.properties {
            if let Some(inner) = map.get(name)
                && !self.apply(*child, inner, Some(Segment::Key(name)), depth, collect)
            {
                if !collect {
                    return false;
                }
                valid = false;
            }
        }
        let Some(extra) = node.additional else {
            return valid;
        };
        for (name, inner) in map {
            let declared = node
                .properties
                .binary_search_by(|(known, _)| known.as_str().cmp(name))
                .is_ok();
            if declared || self.found.len() > REPORT_MAX {
                continue;
            }
            let rejected = if self.schema.nodes[extra].deny {
                if collect {
                    self.report_at(name, "additional-properties");
                }
                true
            } else {
                !self.apply(extra, inner, Some(Segment::Key(name)), depth, collect)
            };
            if rejected {
                if !collect {
                    return false;
                }
                valid = false;
            }
        }
        valid
    }

    fn array_rules(
        &mut self,
        node: &SchemaNode,
        elements: &[Value],
        depth: usize,
        collect: bool,
    ) -> bool {
        let Some(child) = node.items else {
            return true;
        };
        let mut valid = true;
        for (index, element) in elements.iter().enumerate() {
            if !self.apply(child, element, Some(Segment::Index(index)), depth, collect) {
                if !collect {
                    return false;
                }
                valid = false;
            }
        }
        valid
    }

    /// `allOf`, `anyOf`, `oneOf`, `not` e `$ref`, nesta ordem.
    fn combinator_rules(
        &mut self,
        node: &SchemaNode,
        value: &Value,
        depth: usize,
        collect: bool,
    ) -> bool {
        let mut valid = true;
        for &branch in &node.all_of {
            if !self.apply(branch, value, None, depth, collect) {
                if !collect {
                    return false;
                }
                valid = false;
            }
        }
        if !node.any_of.is_empty()
            && !node
                .any_of
                .iter()
                .any(|&branch| self.eval(branch, value, depth + 1, false))
        {
            if !collect {
                return false;
            }
            valid = false;
            self.report("any-of");
        }
        if !node.one_of.is_empty() && !self.exactly_one(&node.one_of, value, depth) {
            if !collect {
                return false;
            }
            valid = false;
            self.report("one-of");
        }
        if let Some(negated) = node.not
            && self.eval(negated, value, depth + 1, false)
        {
            if !collect {
                return false;
            }
            valid = false;
            self.report("not");
        }
        if let Some(target) = node.reference
            && !self.apply(target, value, None, depth, collect)
        {
            valid = false;
        }
        valid
    }

    fn exactly_one(&mut self, branches: &[NodeId], value: &Value, depth: usize) -> bool {
        let mut passed = 0;
        for &branch in branches {
            if self.eval(branch, value, depth + 1, false) {
                passed += 1;
                if passed > 1 {
                    return false;
                }
            }
        }
        passed == 1
    }
}

#[cfg(test)]
mod tests;
