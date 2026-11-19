use ::omhl::adapters::text::{
    Entry, NodeContainer, Outcome, Profile, Region, Scope, Shape, WILDCARD,
};
use serde_json::Value;

// ---------------------------------------------------------------- folhas de um `Value`

#[derive(Debug, Clone, PartialEq)]
enum Scalar {
    Text(String),
    Number(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone)]
struct Leaf {
    path: String,
    region: Region,
    node_type: Option<String>,
    parent: Option<String>,
    key: String,
    scalar: Scalar,
}

/// Percorre o `Value` coletando os escalares com o caminho de chaves; array é transparente.
fn scalars(value: &Value, segments: &mut Vec<String>, out: &mut Vec<(Vec<String>, Scalar)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                segments.push(key.clone());
                scalars(inner, segments, out);
                segments.pop();
            }
        }
        Value::Array(items) => items.iter().for_each(|inner| scalars(inner, segments, out)),
        Value::String(text) => out.push((segments.clone(), Scalar::Text(text.clone()))),
        Value::Number(number) => out.push((segments.clone(), Scalar::Number(number.to_string()))),
        Value::Bool(flag) => out.push((segments.clone(), Scalar::Bool(*flag))),
        Value::Null => out.push((segments.clone(), Scalar::Null)),
    }
}

/// O pai de um nível: a chave nomeada logo antes da chave da folha.
fn parent_of(segments: &[String]) -> Option<String> {
    segments
        .len()
        .checked_sub(2)
        .map(|index| segments[index].clone())
}

fn plain_leaves(root_key: &str, value: &Value, leaves: &mut Vec<Leaf>) {
    let mut found = Vec::new();
    scalars(value, &mut vec![root_key.to_owned()], &mut found);
    for (segments, scalar) in found {
        let region = if segments.len() == 1 {
            Region::Root
        } else {
            Region::Data
        };
        leaves.push(Leaf {
            path: segments.join("."),
            region,
            node_type: None,
            parent: parent_of(&segments),
            key: segments.last().expect("a path has a key").clone(),
            scalar,
        });
    }
}

fn node_leaves(container: &str, node: &Value, leaves: &mut Vec<Leaf>) {
    let node_type = node["type"].as_str().map(str::to_owned);
    let mut found = Vec::new();
    scalars(node, &mut Vec::new(), &mut found);
    for (segments, scalar) in found {
        let (region, relative) = match segments[0].as_str() {
            "data" if segments.len() > 1 => (Region::Data, &segments[1..]),
            "snapshot" if segments.len() > 1 => (Region::Snapshot, &segments[1..]),
            "author" if segments.len() > 1 => (Region::Author, &segments[1..]),
            _ => (Region::Root, &segments[..]),
        };
        leaves.push(Leaf {
            path: format!("{container}.{}", segments.join(".")),
            region,
            node_type: node_type.clone(),
            parent: parent_of(relative),
            key: relative.last().expect("a path has a key").clone(),
            scalar,
        });
    }
}

/// As folhas do `Value` do desfecho: o erro lê o objeto sob `error`; o contêiner de nós do `Profile` separa os nós.
fn collect_leaves(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<Leaf> {
    let (root, nodes) = match outcome {
        Outcome::Success => (value, profile.nodes),
        Outcome::Failure => (&value["error"], None),
    };
    let container_key = nodes.map(|container| match container {
        NodeContainer::Array(key) | NodeContainer::Object(key) => key,
    });
    let mut leaves = Vec::new();
    for (key, inner) in root.as_object().expect("the value is an object") {
        if container_key != Some(key.as_str()) {
            plain_leaves(key, inner, &mut leaves);
            continue;
        }
        match (nodes, inner) {
            (Some(NodeContainer::Array(_)), Value::Array(items)) => items
                .iter()
                .for_each(|node| node_leaves(&format!("{key}[]"), node, &mut leaves)),
            (Some(NodeContainer::Object(_)), Value::Object(_)) => {
                node_leaves(key, inner, &mut leaves)
            }
            _ => plain_leaves(key, inner, &mut leaves),
        }
    }

    leaves
}

// ---------------------------------------------------------------- folhas distintas e formas

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Number,
    Bool,
}

/// Escalares de valor idêntico formam uma folha só.
struct Group<'a> {
    kind: Kind,
    text: String,
    leaves: Vec<&'a Leaf>,
    /// A forma mais exigente entre as chaves do grupo; `None` quando alguma chave está fora da tabela.
    shape: Option<Shape>,
}

impl Group<'_> {
    fn paths(&self) -> String {
        self.leaves
            .iter()
            .map(|leaf| leaf.path.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Literal, Closed, Hash, Derived, Noise: da forma mais exigente para a mais frouxa.
fn demand(shape: Shape) -> u8 {
    match shape {
        Shape::Literal => 0,
        Shape::Closed => 1,
        Shape::Hash(_) => 2,
        Shape::Derived => 3,
        Shape::Noise => 4,
    }
}

fn entry_of(profile: &Profile, outcome: Outcome, leaf: &Leaf) -> Option<&'static Entry> {
    profile.find(
        outcome,
        leaf.region,
        leaf.node_type.as_deref(),
        leaf.parent.as_deref(),
        &leaf.key,
    )
}

fn groups<'a>(profile: &Profile, outcome: Outcome, leaves: &'a [Leaf]) -> Vec<Group<'a>> {
    let mut groups: Vec<Group<'a>> = Vec::new();
    for leaf in leaves {
        let (kind, text) = match &leaf.scalar {
            Scalar::Text(text) => (Kind::Text, text.clone()),
            Scalar::Number(text) => (Kind::Number, text.clone()),
            Scalar::Bool(flag) => (Kind::Bool, flag.to_string()),
            Scalar::Null => continue,
        };
        let shape = entry_of(profile, outcome, leaf).map(|entry| entry.shape);
        match groups
            .iter_mut()
            .find(|group| group.kind == kind && group.text == text)
        {
            Some(group) => {
                group.leaves.push(leaf);
                group.shape = match (group.shape, shape) {
                    (Some(known), Some(new)) => Some(if demand(new) < demand(known) {
                        new
                    } else {
                        known
                    }),
                    _ => None,
                };
            }
            None => groups.push(Group {
                kind,
                text,
                leaves: vec![leaf],
                shape,
            }),
        }
    }
    // String vazia não é folha de `Literal`: é `Derived`, seja qual for a forma declarada (o caso afirma a ausência do trecho).
    for group in &mut groups {
        if group.kind == Kind::Text && group.text.is_empty() && group.shape.is_some() {
            group.shape = Some(Shape::Derived);
        }
    }

    groups
}

// ---------------------------------------------------------------- tokens

fn is_token_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

/// Onde `needle` ocorre com limite de token à esquerda e, se `bounded_right`, também à direita.
fn occurrences(text: &str, needle: &str, bounded_right: bool) -> Vec<(usize, usize)> {
    text.match_indices(needle)
        .map(|(start, found)| (start, start + found.len()))
        .filter(|(start, end)| {
            let before = text[..*start].chars().next_back();
            let after = text[*end..].chars().next();
            !before.is_some_and(is_token_char)
                && !(bounded_right && after.is_some_and(is_token_char))
        })
        .collect()
}

/// Troca por um espaço cada ocorrência de `needle` como token inteiro.
fn erase_token(text: &str, needle: &str) -> String {
    let mut erased = String::new();
    let mut cursor = 0;
    for (start, end) in occurrences(text, needle, true) {
        erased.push_str(&text[cursor..start]);
        erased.push(' ');
        cursor = end;
    }
    erased.push_str(&text[cursor..]);

    erased
}

// ---------------------------------------------------------------- as checagens do CA-2

/// (4) chave fora da tabela e booleano fora de `Derived`/`Noise`.
pub fn table_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    let mut problems: Vec<String> = leaves
        .iter()
        .filter(|leaf| leaf.scalar != Scalar::Null)
        .filter(|leaf| entry_of(profile, outcome, leaf).is_none())
        .map(|leaf| {
            format!(
                "{}: key not in the profile table (region {:?}, type {:?}, parent {:?}, key {})",
                leaf.path, leaf.region, leaf.node_type, leaf.parent, leaf.key
            )
        })
        .collect();
    for group in groups(profile, outcome, &leaves) {
        let demanding = matches!(
            group.shape,
            Some(Shape::Literal | Shape::Closed | Shape::Hash(_))
        );
        if group.kind == Kind::Bool && demanding {
            problems.push(format!(
                "{}: a boolean admits only Derived or Noise",
                group.paths()
            ));
        }
    }

    problems
}

/// (5) entrada não opcional da tabela do desfecho sem chave no `Value`; a de tipo de nó só vale se o `Value` tem um nó do tipo.
pub fn required_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    // Um `Value` sem nó (`{"node":null}` do `get`, página vazia do `list`) não tem de onde cobrar as chaves de nó.
    let node_profile = profile.nodes.is_some() && outcome == Outcome::Success;
    if node_profile && leaves.iter().all(|leaf| leaf.node_type.is_none()) {
        return Vec::new();
    }

    profile
        .entries(outcome)
        .iter()
        .filter(|entry| !entry.optional && entry.key != WILDCARD)
        .filter(|entry| match entry.scope {
            Scope::Any => true,
            Scope::NodeType(wanted) => leaves
                .iter()
                .any(|leaf| leaf.node_type.as_deref() == Some(wanted)),
        })
        .filter(|entry| {
            !leaves.iter().any(|leaf| {
                leaf.region == entry.region
                    && leaf.key == entry.key
                    && (entry.parent.is_none() || leaf.parent.as_deref() == entry.parent)
            })
        })
        .map(|entry| {
            format!(
                "declared key missing from the value: {:?} {:?} parent {:?} key {}",
                entry.region, entry.scope, entry.parent, entry.key
            )
        })
        .collect()
}

/// (6) pré-condição de fixture: o que a fixture escolhe livremente (`Literal`) tem de ser distinguível no texto.
pub fn fixture_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    let all = groups(profile, outcome, &leaves);
    let mut problems = Vec::new();
    for group in all.iter().filter(|g| g.shape == Some(Shape::Literal)) {
        if group.kind == Kind::Text && group.text.chars().count() < 4 {
            problems.push(format!(
                "{}: a free literal needs at least 4 characters",
                group.paths()
            ));
        }
        if group.text.contains('\n') {
            problems.push(format!("{}: a free literal is one line", group.paths()));
        }
        for other in all.iter().filter(|other| !same(group, other)) {
            if other.shape == Some(Shape::Literal)
                && !occurrences(&other.text, &group.text, true).is_empty()
                && other.text != group.text
            {
                problems.push(format!(
                    "{}: `{}` also occurs as a token inside the free leaf `{}`",
                    group.paths(),
                    group.text,
                    other.text
                ));
            }
            if other.shape == Some(Shape::Closed)
                && other.text != group.text
                && !occurrences(&group.text, &other.text, true).is_empty()
            {
                problems.push(format!(
                    "{}: contains the closed value `{}` as a token",
                    group.paths(),
                    other.text
                ));
            }
        }
    }

    problems
}

fn same(left: &Group<'_>, right: &Group<'_>) -> bool {
    left.kind == right.kind && left.text == right.text
}

/// (4) `Literal` e `Closed` aparecem como token inteiro; `Hash(n)` pelos `n` primeiros caracteres, com limite só à esquerda.
/// As ocorrências de uma folha contêiner (que contém o valor) são apagadas antes da busca.
pub fn presence_problems(
    profile: &Profile,
    outcome: Outcome,
    value: &Value,
    text: &str,
) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    let all = groups(profile, outcome, &leaves);
    let mut problems = Vec::new();
    for group in &all {
        let (needle, bounded_right) = match group.shape {
            Some(Shape::Literal | Shape::Closed) => (group.text.clone(), true),
            Some(Shape::Hash(size)) => (group.text.chars().take(size).collect(), false),
            _ => continue,
        };
        let mut remaining = text.to_owned();
        for container in all
            .iter()
            .filter(|other| !same(group, other) && other.text != needle)
            .filter(|other| !occurrences(&other.text, &needle, bounded_right).is_empty())
        {
            remaining = erase_token(&remaining, &container.text);
        }
        if occurrences(&remaining, &needle, bounded_right).is_empty() {
            problems.push(format!(
                "{}: `{needle}` is not shown as a whole token in the text",
                group.paths()
            ));
        }
    }

    problems
}

// ---------------------------------------------------------------- costura comando <-> chave (CA-2 (7))

/// Troca por `<dur>` cada duração (`12ms`, `2.6s`, `60s`): o comando real leva um tempo diferente a cada execução.
pub fn mask_durations(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut masked = String::new();
    let mut index = 0;
    while index < chars.len() {
        let starts_number =
            chars[index].is_ascii_digit() && (index == 0 || !is_token_char(chars[index - 1]));
        if starts_number {
            let digits_end = (index..chars.len())
                .find(|&at| !(chars[at].is_ascii_digit() || chars[at] == '.'))
                .unwrap_or(chars.len());
            let unit_len = if chars[digits_end..].starts_with(&['m', 's']) {
                2
            } else {
                usize::from(chars.get(digits_end) == Some(&'s'))
            };
            let end = digits_end + unit_len;
            if unit_len > 0 && !chars.get(end).is_some_and(|&next| is_token_char(next)) {
                masked.push_str("<dur>");
                index = end;
                continue;
            }
        }
        masked.push(chars[index]);
        index += 1;
    }

    masked
}
